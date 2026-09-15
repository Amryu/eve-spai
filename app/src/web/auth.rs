//! Pairing token for the web view.

use base64::Engine;

/// Bytes of entropy behind a pairing token. The socket can be on the LAN.
const TOKEN_BYTES: usize = 32;

/// `None` if the OS rng failed. The caller must then leave the server disabled rather than fall back
/// to anything weaker.
pub fn new_token() -> Option<String> {
    let mut buf = [0u8; TOKEN_BYTES];
    getrandom::getrandom(&mut buf).ok()?;
    Some(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

/// Constant-time, because a short-circuiting compare leaks the prefix to a network timing attack.
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

    /// A `starts_with` or truncating compare passes the equality test above and fails this one.
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

/// The pairing link as a QR, rasterised for egui. `scale` is pixels per module; at 1 a camera cannot
/// read it on a modern display.
pub fn qr_image(url: &str, scale: usize) -> Option<egui::ColorImage> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    let modules = code.to_colors();
    let side = (modules.len() as f64).sqrt() as usize;
    if side == 0 || side * side != modules.len() {
        return None;
    }
    // The spec requires a quiet zone, or a reader cannot find the code against its background.
    const QUIET: usize = 4;
    let px = (side + QUIET * 2) * scale;
    let mut pixels = vec![egui::Color32::WHITE; px * px];
    for (i, m) in modules.iter().enumerate() {
        if *m != qrcode::Color::Dark {
            continue;
        }
        let (mx, my) = (i % side + QUIET, i / side + QUIET);
        for dy in 0..scale {
            for dx in 0..scale {
                pixels[(my * scale + dy) * px + mx * scale + dx] = egui::Color32::BLACK;
            }
        }
    }
    Some(egui::ColorImage { size: [px, px], pixels, source_size: egui::vec2(px as f32, px as f32) })
}

#[cfg(test)]
mod qr_tests {
    use super::*;

    #[test]
    fn a_pairing_url_becomes_a_square_code() {
        let url = format!("http://192.168.1.5:6767/?t={}", new_token().expect("rng"));
        let img = qr_image(&url, 4).expect("encodes");
        assert_eq!(img.size[0], img.size[1], "a QR is square");
        assert!(img.size[0] > 100, "too small to scan: {}px", img.size[0]);

        let w = img.size[0];
        for i in 0..w {
            assert_eq!(img.pixels[i], egui::Color32::WHITE, "top row {i} is not quiet");
            assert_eq!(img.pixels[(w - 1) * w + i], egui::Color32::WHITE, "bottom row {i}");
            assert_eq!(img.pixels[i * w], egui::Color32::WHITE, "left column {i}");
        }
        assert!(img.pixels.iter().any(|p| *p == egui::Color32::BLACK), "no modules were drawn");
    }

    #[test]
    fn scale_multiplies_the_raster() {
        let url = "http://10.0.0.2:6767/?t=abc";
        let small = qr_image(url, 2).expect("encodes");
        let big = qr_image(url, 6).expect("encodes");
        assert_eq!(big.size[0], small.size[0] / 2 * 6);
    }

    #[test]
    fn a_url_too_long_to_encode_is_refused_rather_than_panicking() {
        assert!(qr_image(&"x".repeat(10_000), 4).is_none());
    }
}
