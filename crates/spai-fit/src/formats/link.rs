//! A fit inside a link: EFT text, deflated and in URL-safe base64 behind `z:`, or as plain text
//! with its line breaks percent-encoded. Reading takes either, and unpacked base64 too.

use std::io::{Read as _, Write as _};

use base64::Engine as _;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// The value for a link's `fit=`.
pub fn encode(text: &str) -> String {
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    let _ = z.write_all(text.as_bytes());
    format!("z:{}", B64.encode(z.finish().unwrap_or_default()))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unbase64(s: &str) -> Option<Vec<u8>> {
    let s = s.trim().trim_end_matches('=');
    B64.decode(s.replace('+', "-").replace('/', "_")).ok()
}

/// The fit text a link's `fit=` value holds, as given (still percent-encoded or not).
pub fn decode(value: &str) -> Option<String> {
    let v = percent_decode(value.trim());
    let v = v.trim();
    if let Some(packed) = v.strip_prefix("z:") {
        let bytes = unbase64(packed)?;
        let mut out = String::new();
        flate2::read::DeflateDecoder::new(bytes.as_slice()).read_to_string(&mut out).ok()?;
        return Some(out);
    }
    if let Some(plain) = v.strip_prefix("b:").and_then(unbase64) {
        return String::from_utf8(plain).ok();
    }
    // Plain text: a fit's own first character, or unmarked base64 of one.
    if v.starts_with('[') || v.starts_with('{') || v.starts_with('<') || v.contains(':') {
        return Some(v.to_owned());
    }
    unbase64(v).and_then(|b| String::from_utf8(b).ok())
}

/// The `fit` parameter of a page address's query or fragment.
pub fn param(url_part: &str) -> Option<&str> {
    url_part.trim_start_matches(['?', '#']).split('&').find_map(|kv| kv.strip_prefix("fit="))
}
