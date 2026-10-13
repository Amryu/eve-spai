//! A signed request and its sealed answer, built and read by the feed's `Protocol`.

use super::proto::{Body, Fields, KeyId, Protocol};
use anyhow::{anyhow, Result};
use serde_json::Value;

/// One query parameter as the wire and the signed string have it.
pub fn encode_query_part(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The query in signed order: sorted by key, then value, each part encoded.
pub fn canonical_query(query: &[(String, String)]) -> String {
    let mut q: Vec<(String, String)> = query.iter().map(|(k, v)| (encode_query_part(k), encode_query_part(v))).collect();
    q.sort();
    q.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

pub fn canonical_string(p: &Protocol, method: &str, path: &str, query: &[(String, String)], timestamp: i64, nonce: &str) -> String {
    let parts = [p.sign_prefix.as_str(), method, path, &canonical_query(query), &timestamp.to_string(), nonce];
    // An empty prefix is no part at all, not an empty first line.
    let parts: Vec<&str> = if p.sign_prefix.is_empty() { parts[1..].to_vec() } else { parts.to_vec() };
    parts.join(&p.separator())
}

pub struct Signed {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

pub fn key_id(p: &Protocol, keys: &crate::feedkeys::FeedKeys) -> String {
    match p.key_id {
        KeyId::Fingerprint => keys.fingerprint().replace(' ', ""),
        KeyId::FingerprintSpaced => keys.fingerprint(),
        KeyId::SigningKey => super::proto::Encoding::Base64.encode(&keys.ed25519_pub()),
    }
}

/// One events request after `cursor`, signed at `timestamp` with `nonce`.
pub fn sign_events_request(p: &Protocol, base: &str, cursor: Option<&str>, keys: &crate::feedkeys::FeedKeys, timestamp: i64, nonce: &[u8]) -> Signed {
    let mut query: Vec<(String, String)> = Vec::new();
    if !p.query_limit.is_empty() {
        query.push((p.query_limit.clone(), p.page_limit.to_string()));
    }
    if let (Some(c), false) = (cursor, p.query_after.is_empty()) {
        query.push((p.query_after.clone(), c.to_owned()));
    }
    let nonce = p.nonce_encoding.encode(nonce);
    let path = if p.events_path.starts_with('/') { p.events_path.clone() } else { format!("/{}", p.events_path) };
    let msg = canonical_string(p, "GET", &path, &query, timestamp, &nonce);
    let sig = p.signature_encoding.encode(&keys.sign(msg.as_bytes()));
    let q = canonical_query(&query);
    Signed {
        url: format!("{}{}{}{}", base.trim().trim_end_matches('/'), path, if q.is_empty() { "" } else { "?" }, q),
        headers: vec![
            (p.header_key_id.clone(), key_id(p, keys)),
            (p.header_timestamp.clone(), timestamp.to_string()),
            (p.header_nonce.clone(), nonce),
            (p.header_signature.clone(), sig),
        ],
    }
}

/// One event, read through the configured field names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Event {
    pub id: String,
    pub timestamp: Option<i64>,
    pub kind: String,
    pub locations: Vec<Location>,
    pub hostiles: Vec<(String, Option<i64>)>,
    pub count: Option<u32>,
    pub ships: Vec<(String, Option<i64>)>,
    pub keywords: Vec<String>,
    pub gate: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Location {
    pub system: String,
    pub system_id: Option<i64>,
}

#[derive(Debug)]
pub struct Payload {
    pub cursor: Option<String>,
    pub more: bool,
    pub events: Vec<Event>,
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn int(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn time(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().map(|t| if t > 100_000_000_000 { t / 1000 } else { t }),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s.trim()).ok().map(|t| t.timestamp()).or_else(|| s.trim().parse().ok()),
        _ => None,
    }
}

fn get<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    if name.is_empty() {
        return None;
    }
    v.get(name).filter(|x| !x.is_null())
}

fn location(v: &Value, f: &Fields) -> Location {
    match v {
        Value::String(s) => Location { system: s.clone(), system_id: None },
        _ => Location { system: get(v, &f.system).and_then(text).unwrap_or_default(), system_id: get(v, &f.system_id).and_then(int) },
    }
}

fn named(v: &Value, name: &str, id: &str) -> (String, Option<i64>) {
    match v {
        Value::String(s) => (s.clone(), None),
        _ => (get(v, name).and_then(text).unwrap_or_default(), get(v, id).and_then(int)),
    }
}

pub fn read_event(v: &Value, f: &Fields) -> Event {
    let list = |name: &str| get(v, name).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut locations = Vec::new();
    if let Some(l) = get(v, &f.location) {
        locations.push(location(l, f));
    }
    locations.extend(list(&f.additional_systems).iter().map(|l| location(l, f)));
    Event {
        id: get(v, &f.id).and_then(text).unwrap_or_default(),
        timestamp: get(v, &f.timestamp).and_then(time),
        kind: get(v, &f.kind).and_then(text).unwrap_or_default(),
        locations,
        hostiles: list(&f.hostiles).iter().map(|h| named(h, &f.hostile_name, &f.hostile_id)).collect(),
        count: get(v, &f.count).and_then(int).and_then(|c| u32::try_from(c).ok()),
        ships: list(&f.ships).iter().map(|s| named(s, &f.ship_name, &f.ship_type_id)).collect(),
        keywords: list(&f.keywords).iter().filter_map(text).collect(),
        gate: get(v, &f.gate).and_then(text),
    }
}

/// The decrypted, parsed payload of one answer.
pub fn open_payload(p: &Protocol, keys: &crate::feedkeys::FeedKeys, body: &[u8]) -> Result<Payload> {
    let sealed = match p.body {
        Body::Raw => body.to_vec(),
        Body::Base64 => super::proto::Encoding::Base64
            .decode(std::str::from_utf8(body).unwrap_or_default())
            .ok_or_else(|| anyhow!("the answer is not base64"))?,
    };
    let plain = keys.open_sealed(&sealed)?;
    let v: Value = serde_json::from_slice(&plain).map_err(|e| anyhow!("the decrypted answer is not JSON: {e}"))?;
    let f = &p.fields;
    if let Some(ver) = get(&v, &f.version).and_then(int) {
        if ver != i64::from(p.payload_version) {
            return Err(anyhow!("the feed sent payload version {ver}, this client is set to read {}", p.payload_version));
        }
    }
    Ok(Payload {
        cursor: get(&v, &f.cursor).and_then(text),
        more: get(&v, &f.more).and_then(Value::as_bool).unwrap_or(false),
        events: get(&v, &f.events).and_then(Value::as_array).map(|a| a.iter().map(|e| read_event(e, f)).collect()).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made-up protocol values: the test is that each setting is what the wire carries.
    fn proto() -> Protocol {
        Protocol {
            events_path: "/v9/things".into(),
            query_after: "since".into(),
            query_limit: "n".into(),
            page_limit: 50,
            header_key_id: "X-Who".into(),
            header_timestamp: "X-When".into(),
            header_nonce: "X-Once".into(),
            header_signature: "X-Proof".into(),
            sign_prefix: "example-feed-v9".into(),
            ..Default::default()
        }
    }

    fn q(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn the_query_is_signed_sorted_and_encoded() {
        assert_eq!(canonical_query(&q(&[("n", "200"), ("since", "a b/c")])), "n=200&since=a%20b%2Fc");
        let s = canonical_string(&proto(), "GET", "/p", &q(&[("n", "1")]), 1_791_000_000, "00ff");
        assert_eq!(s, "example-feed-v9\nGET\n/p\nn=1\n1791000000\n00ff");
        let bare = canonical_string(&Protocol::default(), "GET", "/p", &[], 5, "ab");
        assert_eq!(bare, "GET\n/p\n\n5\nab", "no prefix, no empty first line");
    }

    #[test]
    fn a_signed_request_carries_the_configured_names_and_verifies() {
        let keys = crate::feedkeys::FeedKeys::for_test(3, 4);
        let p = proto();
        let r = sign_events_request(&p, "https://feed.example.invalid/", Some("c1"), &keys, 1_791_000_000, &[0xab; 16]);
        assert_eq!(r.url, "https://feed.example.invalid/v9/things?n=50&since=c1");
        let h = |n: &str| r.headers.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(h("X-Who"), keys.fingerprint().replace(' ', ""));
        assert_eq!(h("X-When"), "1791000000");
        assert_eq!(h("X-Once"), "ab".repeat(16));
        let msg = canonical_string(&p, "GET", "/v9/things", &q(&[("since", "c1"), ("n", "50")]), 1_791_000_000, &h("X-Once"));
        let sig: [u8; 64] = p.signature_encoding.decode(&h("X-Proof")).unwrap().try_into().unwrap();
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&keys.ed25519_pub()).unwrap();
        assert!(vk.verify_strict(msg.as_bytes(), &ed25519_dalek::Signature::from_bytes(&sig)).is_ok());
    }

    fn seal(keys: &crate::feedkeys::FeedKeys, body: &[u8]) -> Vec<u8> {
        crypto_box::PublicKey::from(keys.x25519_pub()).seal(&mut crypto_box::aead::OsRng, body).unwrap()
    }

    /// Sealed with libsodium's construction by the crate's own `seal`, opened by ours.
    #[test]
    fn a_sealed_answer_opens_and_a_foreign_or_tampered_one_does_not() {
        let keys = crate::feedkeys::FeedKeys::for_test(3, 4);
        let body = br#"{"version":1,"cursor":"c9","more":true,"events":[{"id":"e1","timestamp":"2026-10-12T19:00:00Z","kind":"report","location":{"system":"1DQ1-A","system_id":30004759},"count":3}]}"#;
        let sealed = seal(&keys, body);
        let pl = open_payload(&Protocol::default(), &keys, &sealed).unwrap();
        assert_eq!((pl.cursor.as_deref(), pl.more, pl.events.len()), (Some("c9"), true, 1));
        assert_eq!(pl.events[0].locations[0].system_id, Some(30004759));
        assert_eq!(pl.events[0].timestamp, Some(1_791_831_600));

        let other = crate::feedkeys::FeedKeys::for_test(5, 6);
        assert!(open_payload(&Protocol::default(), &keys, &seal(&other, body)).is_err());
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(open_payload(&Protocol::default(), &keys, &tampered).is_err());
        assert!(open_payload(&Protocol::default(), &keys, &seal(&keys, br#"{"version":2}"#)).is_err());
    }

    #[test]
    fn renamed_fields_and_a_base64_body_are_read() {
        let keys = crate::feedkeys::FeedKeys::for_test(3, 4);
        let mut p = Protocol { body: Body::Base64, ..Default::default() };
        p.fields.events = "items".into();
        p.fields.location = "where".into();
        p.fields.system_id = "sid".into();
        p.fields.hostiles = "who".into();
        let body = br#"{"items":[{"id":7,"where":{"sid":30004759},"who":["Some Pilot",{"name":"Other","id":91}]}]}"#;
        let wire = super::super::proto::Encoding::Base64.encode(&seal(&keys, body));
        let pl = open_payload(&p, &keys, wire.as_bytes()).unwrap();
        let e = &pl.events[0];
        assert_eq!(e.id, "7");
        assert_eq!(e.locations[0].system_id, Some(30004759));
        assert_eq!(e.hostiles, vec![("Some Pilot".to_owned(), None), ("Other".to_owned(), Some(91))]);
    }
}
