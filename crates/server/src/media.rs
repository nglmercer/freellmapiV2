//! Validation for media references inside multimodal chat content.
//!
//! Nothing here fetches remote URLs: the proxy only checks their shape and
//! forwards them to providers. Data URLs are decoded just far enough to
//! confirm the declared MIME type against the leading magic bytes.

/// Image MIME types accepted in `image_url` parts.
pub const IMAGE_MIME_TYPES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

/// A parsed `data:<mime>;base64,<payload>` URL.
#[derive(Debug, PartialEq, Eq)]
pub struct DataUrl<'a> {
    pub mime: &'a str,
    pub base64: &'a str,
}

/// Splits a base64 data URL into its MIME type and payload.
pub fn parse_data_url(url: &str) -> Result<DataUrl<'_>, String> {
    let rest = url
        .strip_prefix("data:")
        .ok_or_else(|| "data URL must start with 'data:'".to_string())?;
    let (header, payload) = rest
        .split_once(',')
        .ok_or_else(|| "data URL is missing the ',' separator".to_string())?;
    let mime = header
        .strip_suffix(";base64")
        .ok_or_else(|| "data URL must be base64-encoded (';base64')".to_string())?;
    if mime.is_empty() || !mime.contains('/') {
        return Err("data URL has an invalid MIME type".to_string());
    }
    if payload.is_empty() {
        return Err("data URL payload is empty".to_string());
    }
    if !is_base64(payload) {
        return Err("data URL payload is not valid base64".to_string());
    }
    Ok(DataUrl {
        mime,
        base64: payload,
    })
}

/// Standard base64 alphabet, with up to two trailing `=` padding characters.
fn is_base64(s: &str) -> bool {
    let bytes = s.as_bytes();
    let pad = bytes.iter().rev().take_while(|&&b| b == b'=').count();
    if pad > 2 {
        return false;
    }
    let body = &bytes[..bytes.len() - pad];
    // A single leftover sextet cannot encode a whole byte.
    if body.len() % 4 == 1 {
        return false;
    }
    body.iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'/')
}

fn base64_value(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodes at most the first 16 base64 characters (12 bytes).
fn decode_head(b64: &str) -> Option<Vec<u8>> {
    let bytes = &b64.as_bytes()[..b64.len().min(16)];
    let mut out = Vec::with_capacity(12);
    for chunk in bytes.chunks(4) {
        let mut acc: u32 = 0;
        let mut count = 0usize;
        for &c in chunk {
            if c == b'=' {
                break;
            }
            acc = (acc << 6) | base64_value(c)?;
            count += 1;
        }
        if count < 2 {
            break;
        }
        acc <<= 6 * (4 - count);
        let produced = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        out.extend_from_slice(&produced[..count * 3 / 4]);
    }
    Some(out)
}

/// Identifies an image by its magic bytes. Returns `None` when unrecognised.
pub fn sniff_image_mime(b64: &str) -> Option<&'static str> {
    let head = decode_head(b64)?;
    if head.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Canonical form of an image MIME type (`image/jpg` is an alias of jpeg).
fn canonical_image_mime(mime: &str) -> &str {
    match mime.to_ascii_lowercase().as_str() {
        "image/jpg" => "image/jpeg",
        _ => mime,
    }
}

/// Validates the `url` of a `video_url` content part: an `http(s)` URL (shape
/// only) or a base64 `data:video/*` URL.
pub fn validate_video_url(url: &str) -> Result<(), String> {
    if url.starts_with("data:") {
        let parsed = parse_data_url(url)?;
        if !parsed.mime.starts_with("video/") {
            return Err(format!("unsupported video MIME type '{}'", parsed.mime));
        }
        return Ok(());
    }
    validate_http_url(url)
}

/// Validates an audio payload: base64 data, with a declared container format.
pub fn validate_audio_input(data: &str, format: &str) -> Result<(), String> {
    if data.is_empty() {
        return Err("audio data must not be empty".to_string());
    }
    if !is_base64(data) {
        return Err("audio data is not valid base64".to_string());
    }
    if format.trim().is_empty() {
        return Err("audio format must not be empty".to_string());
    }
    Ok(())
}

/// Validates the `url` of an `image_url` content part.
///
/// Accepts `http(s)` URLs (shape only, never fetched here) and base64 image
/// data URLs whose declared MIME type matches the decoded content.
pub fn validate_image_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("String must contain at least 1 character(s)".to_string());
    }
    if url.starts_with("data:") {
        let parsed = parse_data_url(url)?;
        let declared = canonical_image_mime(parsed.mime);
        if !IMAGE_MIME_TYPES.contains(&declared) {
            return Err(format!("unsupported image MIME type '{}'", parsed.mime));
        }
        let sniffed = sniff_image_mime(parsed.base64)
            .ok_or_else(|| "data URL content is not a recognised image format".to_string())?;
        if sniffed != declared {
            return Err(format!(
                "declared MIME type '{}' does not match content ({sniffed})",
                parsed.mime
            ));
        }
        return Ok(());
    }
    if has_http_scheme(url) {
        return validate_http_url(url);
    }
    Err("image_url must be an http(s) URL or a base64 data URL".to_string())
}

fn validate_http_url(url: &str) -> Result<(), String> {
    if !has_http_scheme(url) {
        return Err("URL must be http(s)".to_string());
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("URL must not contain whitespace or control characters".to_string());
    }
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    let host = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        return Err("URL is missing a host".to_string());
    }
    Ok(())
}

fn has_http_scheme(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1x1 PNG, base64.
    const PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    const JPEG_B64: &str = "/9j/4AAQSkZJRgABAQEAYABgAAD/2Q==";

    #[test]
    fn parses_base64_data_url() {
        let p = parse_data_url("data:image/png;base64,AAAA").unwrap();
        assert_eq!(p.mime, "image/png");
        assert_eq!(p.base64, "AAAA");
    }

    #[test]
    fn rejects_malformed_data_urls() {
        assert!(parse_data_url("data:image/png,AAAA").is_err());
        assert!(parse_data_url("data:image/png;base64,").is_err());
        assert!(parse_data_url("data:png;base64,AAAA").is_err());
        assert!(parse_data_url("data:image/png;base64,AA!A").is_err());
        assert!(parse_data_url("data:image/png;base64,A").is_err());
        assert!(parse_data_url("data:image/png;base64,AAAA===").is_err());
    }

    #[test]
    fn sniffs_known_image_formats() {
        assert_eq!(sniff_image_mime(PNG_B64), Some("image/png"));
        assert_eq!(sniff_image_mime(JPEG_B64), Some("image/jpeg"));
        // "GIF89a" -> base64 "R0lGODlh"
        assert_eq!(sniff_image_mime("R0lGODlhAQABAA=="), Some("image/gif"));
        // "RIFF....WEBP" -> "UklGRgAAAABXRUJQ"
        assert_eq!(sniff_image_mime("UklGRgAAAABXRUJQVlA4"), Some("image/webp"));
        assert_eq!(sniff_image_mime("aGVsbG8gd29ybGQ="), None);
    }

    #[test]
    fn validates_image_urls() {
        assert!(validate_image_url("https://example.com/cat.png").is_ok());
        assert!(validate_image_url(&format!("data:image/png;base64,{PNG_B64}")).is_ok());
        assert!(validate_image_url(&format!("data:image/jpg;base64,{JPEG_B64}")).is_ok());
        assert!(validate_image_url(&format!("data:image/png;base64,{JPEG_B64}")).is_err());
        assert!(validate_image_url("data:text/plain;base64,aGVsbG8=").is_err());
        assert!(validate_image_url("ftp://example.com/a.png").is_err());
        assert!(validate_image_url("https://").is_err());
        assert!(validate_image_url("https://exa mple.com/a.png").is_err());
        assert!(validate_image_url("").is_err());
    }
}
