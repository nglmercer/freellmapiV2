//! AES-256-GCM key encryption used by the server.
//!
//! The 16-byte IV, 16-byte authentication tag, and hex encoding are retained
//! for compatibility with existing databases and their encrypted provider
//! keys.

use std::sync::{Mutex, OnceLock};

use aes_gcm::aead::generic_array::typenum::U16;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::aes::Aes256;
use aes_gcm::{AesGcm, Key, Nonce};
use rusqlite::Connection;

type Cipher = AesGcm<Aes256, U16, U16>;

const KEY_BYTES: usize = 32;
const KEY_HEX_LEN: usize = KEY_BYTES * 2;

static CACHED_KEY: OnceLock<Mutex<Option<[u8; KEY_BYTES]>>> = OnceLock::new();

fn cached_key() -> &'static Mutex<Option<[u8; KEY_BYTES]>> {
    CACHED_KEY.get_or_init(|| Mutex::new(None))
}

fn parse_hex_key(value: &str, source: &str) -> Result<[u8; KEY_BYTES], String> {
    if value.len() != KEY_HEX_LEN || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "Invalid ENCRYPTION_KEY ({}): expected {} hex chars (32 bytes), got {} chars. \
             Generate one with: node -e \"console.log(require('crypto').randomBytes(32).toString('hex'))\"",
            source,
            KEY_HEX_LEN,
            value.len()
        ));
    }
    let mut key = [0u8; KEY_BYTES];
    hex::decode_to_slice(value, &mut key)
        .map_err(|_| "Invalid ENCRYPTION_KEY: not valid hex".to_string())?;
    Ok(key)
}

fn setting_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

fn upsert_setting(conn: &Connection, key: &str, value: &str) {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )
    .ok();
}

/// Initialize encryption key from env, DB, or generate a new one.
///
/// Priority: ENCRYPTION_KEY env var > DB-stored key > random generate.
/// Auto-fix: If ENCRYPTION_KEY is set, the DB copy is synced to match.
pub fn init_encryption_key(conn: &Connection) {
    let mut cached = cached_key().lock().unwrap();

    // 1. Check ENCRYPTION_KEY env var first (auto-fix priority)
    let env_key = std::env::var("ENCRYPTION_KEY").ok();
    if let Some(ref raw) = env_key {
        let raw = raw.trim().to_string();
        if !raw.is_empty() && raw != "your-64-char-hex-key-here" {
            let parsed = parse_hex_key(&raw, "env").unwrap_or_else(|e| panic!("{e}"));
            *cached = Some(parsed);
            upsert_setting(conn, "encryption_key", &hex::encode(parsed));
            return;
        }
    }

    // 2. No ENCRYPTION_KEY. Check DB for stored key.
    if let Some(value) = setting_value(conn, "encryption_key") {
        let parsed = parse_hex_key(&value, "db").unwrap_or_else(|e| panic!("{e}"));
        *cached = Some(parsed);
        return;
    }

    // 3. Generate and persist a new random key.
    let mut bytes = [0u8; KEY_BYTES];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    upsert_setting(conn, "encryption_key", &hex::encode(bytes));
    *cached = Some(bytes);
}

fn get_encryption_key() -> [u8; KEY_BYTES] {
    let cached = cached_key().lock().unwrap();
    match *cached {
        Some(k) => k,
        None => panic!("Encryption key not initialized. Call init_encryption_key() first."),
    }
}

/// Encrypt `text` → (encrypted, iv, auth_tag), all hex strings.
pub fn encrypt(text: &str) -> (String, String, String) {
    let key = get_encryption_key();
    let key = Key::<Cipher>::from_slice(&key);
    let cipher = Cipher::new(key);

    let mut iv_bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut iv_bytes);
    let nonce = Nonce::<U16>::from_slice(&iv_bytes);

    let ct = cipher
        .encrypt(nonce, text.as_bytes())
        .expect("encryption failed");
    // aes-gcm appends the 16-byte tag to the ciphertext
    let (ciphertext, tag) = ct.split_at(ct.len() - 16);

    (
        hex::encode(ciphertext),
        hex::encode(iv_bytes),
        hex::encode(tag),
    )
}

fn decrypt_with_key(
    key: &[u8; KEY_BYTES],
    encrypted: &str,
    iv: &str,
    auth_tag: &str,
) -> Result<String, String> {
    let key = Key::<Cipher>::from_slice(key);
    let cipher = Cipher::new(key);

    let ct_hex = hex::decode(encrypted).map_err(|_| "invalid ciphertext hex".to_string())?;
    let iv_bytes = hex::decode(iv).map_err(|_| "invalid iv hex".to_string())?;
    let tag_bytes = hex::decode(auth_tag).map_err(|_| "invalid tag hex".to_string())?;
    if iv_bytes.len() != 16 || tag_bytes.len() != 16 {
        return Err("invalid iv/tag length".to_string());
    }

    let mut ct = ct_hex;
    ct.extend_from_slice(&tag_bytes);
    let nonce = Nonce::<U16>::from_slice(&iv_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|_| "decryption failed".to_string())?;
    String::from_utf8(pt).map_err(|_| "decrypted data not utf8".to_string())
}

/// Decrypt hex-encoded (encrypted, iv, auth_tag) back to plaintext.
/// Returns Err on auth failure or mismatched key.
pub fn decrypt(encrypted: &str, iv: &str, auth_tag: &str) -> Result<String, String> {
    let key = get_encryption_key();
    decrypt_with_key(&key, encrypted, iv, auth_tag)
}

/// `key.slice(0, 4) + '...' + key.slice(-4)`, or `'****' + last 4` when short.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        let start = chars.len().saturating_sub(4);
        format!("****{}", chars[start..].iter().collect::<String>())
    } else {
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}...{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        std::env::set_var("ENCRYPTION_KEY", "a".repeat(64));
        init_encryption_key(&conn);
        let (ct, iv, tag) = encrypt("sk-test-1234567890");
        assert_eq!(decrypt(&ct, &iv, &tag).unwrap(), "sk-test-1234567890");
        assert!(decrypt(&ct, &iv, "00".repeat(16).as_str()).is_err());
    }

    #[test]
    fn decrypts_legacy_node_golden_vector() {
        let key = parse_hex_key(&"aa".repeat(32), "test").unwrap();
        let plaintext = decrypt_with_key(
            &key,
            "45fe82ca573343475996890256d5220bcff28c",
            "00112233445566778899aabbccddeeff",
            "fd1669cf1d462b755ca503cd74347d3c",
        )
        .unwrap();
        assert_eq!(plaintext, "legacy-provider-key");
    }

    #[test]
    fn wrong_key_cannot_decrypt_legacy_ciphertext() {
        let mut wrong_key = parse_hex_key(&"aa".repeat(32), "test").unwrap();
        wrong_key[0] ^= 0xff;
        assert!(decrypt_with_key(
            &wrong_key,
            "45fe82ca573343475996890256d5220bcff28c",
            "00112233445566778899aabbccddeeff",
            "fd1669cf1d462b755ca503cd74347d3c",
        )
        .is_err());
    }

    #[test]
    fn mask_key_matches_ts() {
        assert_eq!(mask_key("12345678"), "****5678");
        assert_eq!(mask_key("abcdefghijklmnop"), "abcd...mnop");
    }
}
