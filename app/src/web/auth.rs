//! Pairing token for the web view.

use base64::Engine;

/// Bytes of entropy behind a pairing token. The socket can be on the LAN, so the token is the thing
/// standing between a stranger on the same wifi and a live intel feed.
const TOKEN_BYTES: usize = 32;

/// A fresh pairing token, or `None` if the OS rng failed. A caller that gets `None` must leave the
/// server disabled rather than fall back to anything weaker.
pub fn new_token() -> Option<String> {
    let mut buf = [0u8; TOKEN_BYTES];
    getrandom::getrandom(&mut buf).ok()?;
    Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

/// Constant-time comparison. A token check that short-circuits on the first wrong byte leaks the
/// prefix to anyone who can time it, and this one answers requests from the network.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_43_chars_of_url_safe_base64() {
        let t = new_token().expect("rng");
        assert_eq!(t.len(), 43, "32 bytes unpadded base64");
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{t}");
    }

    #[test]
    fn two_tokens_differ() {
        assert_ne!(new_token().expect("rng"), new_token().expect("rng"));
    }

    #[test]
    fn ct_eq_matches_only_identical_strings() {
        let t = "abcdefghijklmnopqrstuvwxyz0123456789-_ABCDE";
        assert!(ct_eq(t, t));
        assert!(!ct_eq(t, ""));
        assert!(!ct_eq(t, &t[..t.len() - 1]), "a prefix must not pass");
        assert!(!ct_eq(&format!("{t}x"), t), "an extension must not pass");
    }

    /// One bit wrong at any position must fail. A `starts_with` or a truncating compare passes the
    /// equality test above and fails this one.
    #[test]
    fn ct_eq_rejects_a_single_flipped_bit_anywhere() {
        let base = new_token().expect("rng");
        for i in 0..base.len() {
            let mut bytes = base.clone().into_bytes();
            bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
            let other = String::from_utf8(bytes).expect("ascii");
            assert!(!ct_eq(&base, &other), "byte {i} ignored");
        }
    }
}
