//! What a sealed feed's wire format is, as settings: every path, header name, the signed string,
//! encodings and the payload's field names. Nothing here names a service; the defaults are neutral
//! placeholders, and the user's settings carry the values a given feed's spec fixes.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Hex,
    Base64,
    Base64Url,
}

impl Encoding {
    pub const ALL: [Encoding; 3] = [Encoding::Hex, Encoding::Base64, Encoding::Base64Url];

    pub fn label(self) -> &'static str {
        match self {
            Encoding::Hex => "hex",
            Encoding::Base64 => "base64",
            Encoding::Base64Url => "base64url",
        }
    }

    pub fn encode(self, b: &[u8]) -> String {
        use base64::Engine as _;
        match self {
            Encoding::Hex => b.iter().map(|x| format!("{x:02x}")).collect(),
            Encoding::Base64 => base64::engine::general_purpose::STANDARD.encode(b),
            Encoding::Base64Url => base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b),
        }
    }

    pub fn decode(self, s: &str) -> Option<Vec<u8>> {
        use base64::Engine as _;
        let s = s.trim();
        match self {
            Encoding::Hex => (s.len() % 2 == 0)
                .then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect::<Option<Vec<u8>>>())
                .flatten(),
            Encoding::Base64 => base64::engine::general_purpose::STANDARD.decode(s).ok(),
            Encoding::Base64Url => base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).ok(),
        }
    }
}

/// What the key id header carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyId {
    /// The key block's fingerprint, its spaces removed.
    Fingerprint,
    /// The fingerprint as the key block prints it.
    FingerprintSpaced,
    /// The Ed25519 public key in standard base64.
    SigningKey,
}

impl KeyId {
    pub const ALL: [KeyId; 3] = [KeyId::Fingerprint, KeyId::FingerprintSpaced, KeyId::SigningKey];

    pub fn label(self) -> &'static str {
        match self {
            KeyId::Fingerprint => "fingerprint",
            KeyId::FingerprintSpaced => "fingerprint, spaced",
            KeyId::SigningKey => "signing key",
        }
    }
}

/// The answer body: the sealed box's bytes as sent, or text in an encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    Raw,
    Base64,
}

/// The payload's field names, outer and per event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fields {
    pub cursor: String,
    pub more: String,
    pub events: String,
    pub id: String,
    pub timestamp: String,
    pub kind: String,
    pub location: String,
    pub additional_systems: String,
    pub system: String,
    pub system_id: String,
    pub region: String,
    pub hostiles: String,
    pub hostile_name: String,
    pub hostile_id: String,
    pub count: String,
    pub ships: String,
    pub ship_name: String,
    pub ship_type_id: String,
    pub keywords: String,
    pub gate: String,
    /// The `kind` values that mean an all-clear and a kill.
    pub kind_clear: String,
    pub kind_kill: String,
}

impl Default for Fields {
    fn default() -> Self {
        let s = |v: &str| v.to_owned();
        Self {
            cursor: s("cursor"),
            more: s("more"),
            events: s("events"),
            id: s("id"),
            timestamp: s("timestamp"),
            kind: s("kind"),
            location: s("location"),
            additional_systems: s("additional_systems"),
            system: s("system"),
            system_id: s("system_id"),
            region: s("region"),
            hostiles: s("hostiles"),
            hostile_name: s("name"),
            hostile_id: s("id"),
            count: s("count"),
            ships: s("ships"),
            ship_name: s("name"),
            ship_type_id: s("type_id"),
            keywords: s("keywords"),
            gate: s("gate"),
            kind_clear: s("clear"),
            kind_kill: s("kill"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Protocol {
    pub events_path: String,
    pub query_after: String,
    pub query_limit: String,
    pub page_limit: u32,
    pub header_key_id: String,
    pub header_timestamp: String,
    pub header_nonce: String,
    pub header_signature: String,
    /// The first part of the signed string.
    pub sign_prefix: String,
    /// Between the parts of the signed string. `\n` is written as an escape.
    pub sign_separator: String,
    pub nonce_bytes: u32,
    pub nonce_encoding: Encoding,
    pub signature_encoding: Encoding,
    pub key_id: KeyId,
    pub body: Body,
    /// How far either way the server accepts a timestamp, for the clock hint on a refusal.
    pub clock_tolerance_secs: i64,
    pub fields: Fields,
}

impl Default for Protocol {
    fn default() -> Self {
        Self {
            events_path: "/events".into(),
            query_after: "after".into(),
            query_limit: "limit".into(),
            page_limit: 200,
            header_key_id: "X-Key-Id".into(),
            header_timestamp: "X-Timestamp".into(),
            header_nonce: "X-Nonce".into(),
            header_signature: "X-Signature".into(),
            sign_prefix: String::new(),
            sign_separator: "\\n".into(),
            nonce_bytes: 16,
            nonce_encoding: Encoding::Hex,
            signature_encoding: Encoding::Base64,
            key_id: KeyId::Fingerprint,
            body: Body::Raw,
            clock_tolerance_secs: 60,
            fields: Fields::default(),
        }
    }
}

impl Protocol {
    /// The separator with `\n`, `\r` and `\t` read as escapes, since a settings field is one line.
    pub fn separator(&self) -> String {
        self.sign_separator.replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t")
    }

    pub fn nonce_len(&self) -> usize {
        self.nonce_bytes.clamp(8, 64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings_round_trip() {
        let b = [0u8, 1, 0xfe, 0xff, 7];
        for e in Encoding::ALL {
            assert_eq!(e.decode(&e.encode(&b)).as_deref(), Some(&b[..]), "{e:?}");
        }
        assert_eq!(Encoding::Hex.encode(&[0xab, 1]), "ab01");
        assert!(Encoding::Hex.decode("abc").is_none());
    }

    #[test]
    fn the_separator_reads_escapes() {
        let p = Protocol { sign_separator: "\\n".into(), ..Default::default() };
        assert_eq!(p.separator(), "\n");
    }
}
