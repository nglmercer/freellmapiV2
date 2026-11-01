//! The owner account: created with a username and password, signed into again
//! with the same pair, or joined from another browser with a join code.
//!
//! Better Auth stores accounts by email. The dashboard never asks for one, so
//! the owner's internal email is derived from the username and is not shown.
//!
//! A join code is a random credential equivalent to the owner's sign-in. It is
//! shown once when generated, stored only as a SHA-256 hash, and can be
//! regenerated or revoked from the dashboard.

use better_auth::{
    core::{
        adapter::{DbAdapter, Query},
        error::AuthError,
    },
    email_password::SignUpInput,
    username::normalize_username,
    AuthResult, User,
};
use rand::RngCore;
use sha2::{Digest, Sha256};

use super::{constant_time_eq, has_account, state};

const OWNER_EMAIL_DOMAIN: &str = "accounts.freellmapi.local";
const JOIN_CODE_SETTING: &str = "join_code_hash";
/// Uppercase letters and digits without look-alikes (0, O, 1, I, L).
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
const CODE_LEN: usize = 16;

#[derive(Debug)]
pub enum AccountError {
    /// The request is well formed but not allowed right now.
    Forbidden(String),
    /// The input does not meet the rules.
    Invalid(String),
    /// The join code or password does not match.
    Unauthorized,
    Internal(String),
}

impl AccountError {
    pub fn message(&self) -> &str {
        match self {
            Self::Forbidden(message) | Self::Invalid(message) | Self::Internal(message) => message,
            Self::Unauthorized => "Invalid join code.",
        }
    }
}

fn map_auth(error: AuthError) -> AccountError {
    match error {
        AuthError::InvalidRequest(message) => AccountError::Invalid(message),
        AuthError::Forbidden(message) => AccountError::Forbidden(message),
        AuthError::Unauthorized => AccountError::Unauthorized,
        other => AccountError::Internal(other.to_string()),
    }
}

/// Creates the owner account. Refused once an account exists.
pub async fn create_owner(
    username: &str,
    password: &str,
    secure_cookie: bool,
) -> Result<AuthResult, AccountError> {
    let state = state();
    if has_account(state.adapter.as_ref())
        .await
        .map_err(map_auth)?
    {
        return Err(AccountError::Forbidden(
            "an owner account already exists; sign in instead".into(),
        ));
    }
    let username = normalize_username(username).map_err(map_auth)?;
    let result = state
        .email_password
        .sign_up(
            SignUpInput {
                email: format!("{username}@{OWNER_EMAIL_DOMAIN}"),
                name: username.clone(),
                password: password.to_owned(),
            },
            secure_cookie,
        )
        .await
        .map_err(map_auth)?;
    state
        .usernames
        .set(&result.user.id, &username)
        .await
        .map_err(map_auth)?;
    Ok(result)
}

/// Gives every account that has no username one, taken from its display name
/// or, failing that, from the local part of its email. Accounts created before
/// usernames existed otherwise cannot sign in with a username. A name that is
/// already taken or not a valid username is skipped, and the account keeps
/// working with the email path.
pub async fn backfill_usernames(adapter: &dyn DbAdapter) -> Result<(), AuthError> {
    for row in adapter.find_many("user", Query::new()).await? {
        let has_username = row.get("username").is_some_and(|value| !value.is_null());
        let Some(id) = row.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        if has_username {
            continue;
        }
        let email_local = row
            .get("email")
            .and_then(|value| value.as_str())
            .and_then(|email| email.split('@').next())
            .unwrap_or_default();
        let candidates = [
            row.get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            email_local,
        ];
        for candidate in candidates {
            let Ok(username) = normalize_username(candidate) else {
                continue;
            };
            let set = adapter
                .update_where(
                    "user",
                    Query::new().eq("id", id.to_owned()),
                    serde_json::json!({ "username": username }),
                )
                .await;
            if set.is_ok() {
                break;
            }
        }
    }
    Ok(())
}

/// Whether a join code is currently set.
pub async fn join_code_enabled() -> bool {
    let conn = crate::db::connection::db().lock().await;
    stored_hash(&conn).is_some()
}

/// Generates a new join code, replacing any previous one, and returns it.
pub async fn rotate_join_code() -> String {
    let code = generate_code();
    let hash = hash_code(&normalize_code(&code));
    let conn = crate::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![JOIN_CODE_SETTING, hash],
    )
    .expect("failed to store the join code");
    code
}

/// Removes the join code. Existing sessions are unaffected.
pub async fn revoke_join_code() {
    let conn = crate::db::connection::db().lock().await;
    conn.execute(
        "DELETE FROM settings WHERE key = ?1",
        rusqlite::params![JOIN_CODE_SETTING],
    )
    .expect("failed to remove the join code");
}

/// Signs in as the owner when `code` matches the current join code.
pub async fn join_with_code(code: &str, secure_cookie: bool) -> Result<AuthResult, AccountError> {
    let presented = hash_code(&normalize_code(code));
    let stored = {
        let conn = crate::db::connection::db().lock().await;
        stored_hash(&conn)
    };
    let matches = stored.is_some_and(|stored| constant_time_eq(&stored, &presented));
    if !matches {
        return Err(AccountError::Unauthorized);
    }

    let state = state();
    let row = state
        .adapter
        .find_one("user", Query::new())
        .await
        .map_err(map_auth)?
        .ok_or_else(|| AccountError::Forbidden("no owner account exists yet".into()))?;
    let owner: User = serde_json::from_value(row)
        .map_err(|error| AccountError::Internal(format!("owner record is unreadable: {error}")))?;
    state
        .sessions
        .create(owner, secure_cookie)
        .await
        .map_err(map_auth)
}

fn stored_hash(conn: &rusqlite::Connection) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        rusqlite::params![JOIN_CODE_SETTING],
        |row| row.get(0),
    )
    .ok()
}

/// Accepts codes typed with any case, spaces or dashes.
fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn hash_code(normalized: &str) -> String {
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

/// Rejection sampling keeps every alphabet character equally likely.
fn generate_code() -> String {
    let mut chars = Vec::with_capacity(CODE_LEN);
    let limit = 256 - (256 % CODE_ALPHABET.len());
    while chars.len() < CODE_LEN {
        let mut byte = [0u8; 1];
        rand::rng().fill_bytes(&mut byte);
        if (byte[0] as usize) < limit {
            chars.push(CODE_ALPHABET[byte[0] as usize % CODE_ALPHABET.len()] as char);
        }
    }
    chars
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_grouped_and_use_the_alphabet() {
        let code = generate_code();
        assert_eq!(code.len(), CODE_LEN + 3);
        assert_eq!(code.matches('-').count(), 3);
        assert!(code
            .chars()
            .filter(|c| *c != '-')
            .all(|c| CODE_ALPHABET.contains(&(c as u8))));
    }

    #[test]
    fn normalization_ignores_case_spacing_and_dashes() {
        assert_eq!(normalize_code(" abcd-efgh 2345 "), "ABCDEFGH2345");
    }
}
