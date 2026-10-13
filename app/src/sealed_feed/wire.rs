//! A signed request and its sealed answer, built and read by the feed's `Protocol`.

use super::proto::{Body, KeyId, Protocol};
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
    pub nonce: String,
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
        nonce: nonce.clone(),
        headers: vec![
            (p.header_key_id.clone(), key_id(p, keys)),
            (p.header_timestamp.clone(), timestamp.to_string()),
            (p.header_nonce.clone(), nonce),
            (p.header_signature.clone(), sig),
        ],
    }
}

/// One item of an answer, read by the names such items usually have.
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
    pub text: Option<String>,
    /// The item as the feed sent it.
    pub raw: Value,
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
    /// The feed says events were missed: the cursor was older than what it keeps.
    pub gap: bool,
    /// The answer's top-level text values, where a feed echoes the request's nonce.
    pub top_strings: Vec<String>,
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
        Value::String(s) => {
            let s = s.trim();
            chrono::DateTime::parse_from_rfc3339(s)
                .or_else(|_| chrono::DateTime::parse_from_rfc2822(s))
                .map(|t| t.timestamp())
                .ok()
                .or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok().map(|t| t.and_utc().timestamp()))
                .or_else(|| s.parse::<i64>().ok().map(|t| if t > 100_000_000_000 { t / 1000 } else { t }))
        }
        _ => None,
    }
}

fn get<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.get(name).filter(|x| !x.is_null())
}

/// The first of the usual names the item has.
fn pick<'a>(v: &'a Value, usual: &[&str]) -> Option<&'a Value> {
    usual.iter().find_map(|n| get(v, n))
}

/// A list, or one value standing for a list of one.
fn many(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(a)) => a.clone(),
        Some(x) => vec![x.clone()],
        None => Vec::new(),
    }
}

const SYSTEM: &[&str] = &["system", "system_name", "solar_system", "solar_system_name", "solarSystemName", "name"];
const SYSTEM_ID: &[&str] = &["system_id", "solar_system_id", "solarSystemID", "systemId", "id"];

fn location(v: &Value) -> Option<Location> {
    let l = match v {
        Value::String(s) => Location { system: s.clone(), system_id: None },
        Value::Number(_) => Location { system: String::new(), system_id: int(v) },
        Value::Object(_) => match pick(v, SYSTEM) {
            Some(inner @ Value::Object(_)) => return location(inner),
            s => Location { system: s.and_then(text).unwrap_or_default(), system_id: pick(v, SYSTEM_ID).and_then(int) },
        },
        _ => return None,
    };
    (!l.system.trim().is_empty() || l.system_id.is_some()).then_some(l)
}

fn named(v: &Value, usual_name: &[&str], usual_id: &[&str]) -> (String, Option<i64>) {
    match v {
        Value::String(s) => (s.clone(), None),
        _ => (pick(v, usual_name).and_then(text).unwrap_or_default(), pick(v, usual_id).and_then(int).filter(|&i| i != 0)),
    }
}

/// Killmails named in an item, as "victim (ship)".
fn kills(v: &Value) -> Option<String> {
    let k: Vec<String> = many(get(v, "kills"))
        .iter()
        .filter_map(|k| {
            let victim = pick(k, &["victim", "name", "character"]).and_then(text).unwrap_or_default();
            let ship = pick(k, &["ship", "ship_name", "type"]).and_then(text).unwrap_or_default();
            match (victim.is_empty(), ship.is_empty()) {
                (true, true) => None,
                (false, false) => Some(format!("{victim} ({ship})")),
                _ => Some(format!("{victim}{ship}")),
            }
        })
        .collect();
    (!k.is_empty()).then(|| k.join(", "))
}

fn digest(v: &Value) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(v.to_string().as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect()
}

pub fn read_event(v: &Value) -> Event {
    let mut locations: Vec<Location> = many(pick(v, &["location", "where", "solar_system", "system"])).iter().filter_map(|l| location(l)).collect();
    if locations.is_empty() {
        if let Some(id) = pick(v, &SYSTEM_ID[..4]).and_then(int) {
            locations.push(Location { system: String::new(), system_id: Some(id) });
        }
    }
    locations.extend(many(pick(v, &["systems"])).iter().filter_map(|l| location(l)));
    let id = match v {
        Value::Object(_) => pick(v, &["id", "_id", "uuid", "guid", "event_id", "key"]).and_then(text),
        _ => None,
    };
    Event {
        id: id.filter(|i| !i.is_empty()).unwrap_or_else(|| digest(v)),
        timestamp: pick(v, &["timestamp", "time", "ts", "at", "created_at", "created", "date", "reported_at"]).and_then(time),
        kind: pick(v, &["kind", "type", "event", "category"]).and_then(text).unwrap_or_default(),
        locations,
        hostiles: many(pick(v, &["hostiles", "pilots", "characters", "attackers"]))
            .iter()
            .map(|h| named(h, &["name", "character", "pilot", "character_name"], &["id", "character_id", "characterID"]))
            .collect(),
        count: pick(v, &["count", "number", "size"]).and_then(int).and_then(|c| u32::try_from(c).ok()).filter(|&c| c > 0),
        ships: many(pick(v, &["ships", "hulls"]))
            .iter()
            .map(|s| named(s, &["name", "ship", "ship_name", "type"], &["type_id", "ship_type_id", "shipTypeID", "id"]))
            .collect(),
        keywords: many(pick(v, &["keywords", "tags", "flags"])).iter().filter_map(text).collect(),
        gate: pick(v, &["gate", "stargate"]).and_then(|g| text(g).or_else(|| location(g).map(|l| l.system)).filter(|s| !s.is_empty())),
        text: match v {
            Value::String(s) => Some(s.clone()),
            _ => pick(v, &["text", "message", "msg", "body", "description", "summary", "note", "content"]).and_then(text).or_else(|| kills(v)),
        },
        raw: v.clone(),
    }
}

/// Where the items are: the answer itself when it is one, a usual name, or
/// the first list of objects. Anything else is one item.
fn items(v: &Value) -> Vec<&Value> {
    if let Value::Array(a) = v {
        return a.iter().collect();
    }
    let list = pick(v, &["events", "items", "data", "results", "reports", "entries"])
        .filter(|l| l.is_array())
        .or_else(|| v.as_object()?.values().find(|x| x.as_array().is_some_and(|a| a.first().is_some_and(Value::is_object))));
    match list {
        Some(Value::Array(a)) => a.iter().collect(),
        _ if v.as_object().is_some_and(|o| o.is_empty()) => Vec::new(),
        _ if pick(v, &["cursor", "next_cursor"]).is_some() => Vec::new(),
        _ => vec![v],
    }
}

/// The answer opened and read, whatever shape it has. A sealed box is opened first; an answer
/// that is not one is read as it came.
pub fn open_payload(p: &Protocol, keys: &crate::feedkeys::FeedKeys, body: &[u8]) -> Result<Payload> {
    let b64 = || super::proto::Encoding::Base64.decode(std::str::from_utf8(body).ok()?.trim());
    let opened = match p.body {
        Body::Raw => keys.open_sealed(body).ok().or_else(|| keys.open_sealed(&b64()?).ok()),
        Body::Base64 => b64().and_then(|s| keys.open_sealed(&s).ok()).or_else(|| keys.open_sealed(body).ok()),
    };
    let plain = match opened {
        Some(p) => p,
        None => match std::str::from_utf8(body) {
            Ok(_) => body.to_vec(),
            Err(_) => return Err(anyhow!("the answer could not be opened with your feed keys")),
        },
    };
    let s = String::from_utf8_lossy(&plain);
    let v: Value = serde_json::from_str(&s).unwrap_or_else(|_| Value::String(s.trim().to_owned()));
    let top_strings = v.as_object().map(|o| o.values().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();
    if v.as_str().is_some_and(str::is_empty) {
        return Ok(Payload { cursor: None, more: false, gap: false, top_strings, events: Vec::new() });
    }
    Ok(Payload {
        cursor: pick(&v, &["cursor", "next_cursor", "next", "after"]).and_then(text),
        more: pick(&v, &["more", "has_more"]).and_then(Value::as_bool).unwrap_or(false),
        gap: pick(&v, &["gap", "missed"]).and_then(Value::as_bool).unwrap_or(false),
        events: items(&v).into_iter().map(read_event).collect(),
        top_strings,
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

    /// A feed spec's published example: its test keys, request and signature, set up through the
    /// settings alone. Ed25519 is deterministic, so the signature must match byte for byte.
    #[test]
    fn a_spec_example_signs_exactly() {
        let hex = |s: &str| -> [u8; 32] { super::super::proto::Encoding::Hex.decode(s).unwrap().try_into().unwrap() };
        let keys = crate::feedkeys::FeedKeys::from_secrets(
            hex("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"),
            hex("2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40"),
        );
        let b64 = super::super::proto::Encoding::Base64;
        assert_eq!(b64.encode(&keys.ed25519_pub()), "ebVWLo/mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ=");
        assert_eq!(b64.encode(&keys.x25519_pub()), "WGmv9FBUlzLLqu1eXfmzCm2jHLDldCutWtShp2jxpns=");
        let p = Protocol {
            events_path: "/intel-feed/v1/events".into(),
            page_limit: 100,
            header_key_id: "X-Feed-Key".into(),
            header_timestamp: "X-Feed-Timestamp".into(),
            header_nonce: "X-Feed-Nonce".into(),
            header_signature: "X-Feed-Signature".into(),
            sign_prefix: "DLFEED1".into(),
            nonce_encoding: super::super::proto::Encoding::Base64Url,
            ..Default::default()
        };
        assert_eq!(key_id(&p, &keys), "3d12d88257c8d4d66f3ec1cb754c98e2");
        let nonce = "n0nce-Example_0123456789";
        let msg = canonical_string(&p, "GET", &p.events_path, &q(&[("after", "AAAA-bbbb_CCCC"), ("limit", "100")]), 1_760_356_800, nonce);
        assert_eq!(msg, "DLFEED1\nGET\n/intel-feed/v1/events\nafter=AAAA-bbbb_CCCC&limit=100\n1760356800\nn0nce-Example_0123456789");
        assert_eq!(b64.encode(&keys.sign(msg.as_bytes())), "VftNK04Nl6Wp5hs2Ak+ng8NNX80KFjyoRaoyNR7QFCwL+x4WQEXIiowZpER+l17xwFDLEoSlyMJXf5iuxCvpBw==");
        // The nonce bytes the poller draws, base64url without padding: 18 bytes give the 24 characters.
        let r = sign_events_request(&p, "https://feed.example.invalid", Some("AAAA-bbbb_CCCC"), &keys, 1_760_356_800, &super::super::proto::Encoding::Base64Url.decode(nonce).unwrap());
        assert_eq!(r.nonce, nonce);
        assert_eq!(r.url, "https://feed.example.invalid/intel-feed/v1/events?after=AAAA-bbbb_CCCC&limit=100");
        assert!(r.headers.contains(&("X-Feed-Signature".into(), "VftNK04Nl6Wp5hs2Ak+ng8NNX80KFjyoRaoyNR7QFCwL+x4WQEXIiowZpER+l17xwFDLEoSlyMJXf5iuxCvpBw==".into())));
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

        let plain = open_payload(&Protocol::default(), &keys, br#"{"version":2,"note":"plain"}"#).unwrap();
        assert_eq!(plain.events[0].text.as_deref(), Some("plain"), "an answer that is no sealed box is read as it came");
    }

    /// No fixed shape: a bare list, usual names, nested systems, plain text, each item kept whole.
    #[test]
    fn any_answer_shape_is_read() {
        let keys = crate::feedkeys::FeedKeys::for_test(3, 4);
        let read = |body: &str| open_payload(&Protocol::default(), &keys, &seal(&keys, body.as_bytes())).unwrap();
        let pl = read(r#"[{"type":"report","solar_system":{"name":"1DQ1-A","id":30004759},"pilots":"Some Pilot","message":"cyno up","time":"2026-10-12 19:00:00"}]"#);
        let e = &pl.events[0];
        assert_eq!((e.kind.as_str(), e.locations[0].system_id, e.text.as_deref()), ("report", Some(30004759), Some("cyno up")));
        assert_eq!((e.hostiles[0].0.as_str(), e.timestamp), ("Some Pilot", Some(1_791_831_600)));
        assert_eq!(e.raw["message"], "cyno up");
        assert!(!e.id.is_empty(), "an item without an id gets one from its content");

        let pl = read(r#"{"next_cursor":"c2","has_more":true,"data":[{"system_id":30004758},{"x":1}]}"#);
        assert_eq!((pl.cursor.as_deref(), pl.more, pl.events.len()), (Some("c2"), true, 2));
        assert_eq!(pl.events[0].locations[0].system_id, Some(30004758));
        assert_ne!(pl.events[0].id, pl.events[1].id);

        let pl = read("hostiles in 1DQ1-A");
        assert_eq!((pl.events.len(), pl.events[0].text.as_deref()), (1, Some("hostiles in 1DQ1-A")));
        assert!(read(r#"{"cursor":"c3","events":[]}"#).events.is_empty());
    }

    #[test]
    fn other_names_and_a_base64_body_are_read() {
        let keys = crate::feedkeys::FeedKeys::for_test(3, 4);
        let p = Protocol { body: Body::Base64, ..Default::default() };
        let body = br#"{"items":[{"id":7,"where":{"solar_system_id":30004759},"pilots":["Some Pilot",{"name":"Other","id":91}]}]}"#;
        let wire = super::super::proto::Encoding::Base64.encode(&seal(&keys, body));
        let pl = open_payload(&p, &keys, wire.as_bytes()).unwrap();
        let e = &pl.events[0];
        assert_eq!(e.id, "7");
        assert_eq!(e.locations[0].system_id, Some(30004759));
        assert_eq!(e.hostiles, vec![("Some Pilot".to_owned(), None), ("Other".to_owned(), Some(91))]);
    }
}
