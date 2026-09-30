//! EVE SSO from the browser, without a secret: authorization code with PKCE, then the EVE token is
//! traded for an EVE Spai session. No scopes: the server only needs to know who the character is.

use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use spai_share::crypto::{b64, random32};

/// The scopeless EVE application registered for the web app.
pub const CLIENT_ID: &str = "c0e78d5da88849ac94775034063ec68f";
const AUTHORIZE: &str = "https://login.eveonline.com/v2/oauth/authorize";
pub const TOKEN: &str = "https://login.eveonline.com/v2/oauth/token";

/// What a sign-in in progress keeps across the trip to EVE and back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub state: String,
    pub verifier: String,
    /// ESI scopes asked for: none for the sign-in to EVE Spai, some for a character added for
    /// its location, waypoints or skills.
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl Pending {
    pub fn new() -> Self {
        Pending { state: b64(&random32()[..16]), verifier: b64(&random32()), scopes: Vec::new() }
    }

    fn challenge(&self) -> String {
        b64(&sha2::Sha256::digest(self.verifier.as_bytes()))
    }

    /// Where to send the browser. `redirect` is the callback registered with EVE.
    pub fn authorize_url(&self, redirect: &str) -> String {
        let scope = if self.scopes.is_empty() { String::new() } else { format!("&scope={}", encode(&self.scopes.join(" "))) };
        format!(
            "{AUTHORIZE}?response_type=code&redirect_uri={}&client_id={CLIENT_ID}&state={}&code_challenge={}&code_challenge_method=S256{scope}",
            encode(redirect),
            self.state,
            self.challenge()
        )
    }

    /// The form that trades the code EVE sent back for a token.
    pub fn token_form(&self, code: &str) -> String {
        format!("grant_type=authorization_code&code={}&client_id={CLIENT_ID}&code_verifier={}", encode(code), self.verifier)
    }
}

impl Default for Pending {
    fn default() -> Self {
        Self::new()
    }
}

/// An EVE Spai session, as `/api/session` hands it out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub expires_at: i64,
    pub character_id: i64,
    pub character_name: String,
}

impl Session {
    /// Still good for a while: a session about to expire is renewed by signing in again.
    pub fn usable(&self, now: i64) -> bool {
        self.expires_at > now + 300
    }
}

#[derive(Deserialize)]
pub struct TokenReply {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

/// Who an EVE access token is for and what it may do, read from the token itself (a JWT). Only
/// ever shown and used against ESI, which checks it properly.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenFacts {
    pub char_id: i64,
    pub name: String,
    pub scopes: Vec<String>,
}

pub fn token_facts(jwt: &str) -> Option<TokenFacts> {
    use base64::Engine as _;
    let body = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(body.trim_end_matches('=')).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let char_id = v["sub"].as_str()?.rsplit(':').next()?.parse().ok()?;
    let scopes = match &v["scp"] {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    };
    Some(TokenFacts { char_id, name: v["name"].as_str()?.to_owned(), scopes })
}

/// The `code` from EVE's redirect back, once `state` shows it answers our own sign-in.
pub fn callback_code(query: &str, pending: &Pending) -> Result<String, String> {
    let q = query.trim_start_matches('?');
    let get = |k: &str| q.split('&').find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('='))).map(decode);
    if let Some(e) = get("error") {
        return Err(format!("EVE sign-in failed: {e}"));
    }
    if get("state").as_deref() != Some(pending.state.as_str()) {
        return Err("this sign-in was not started here; try again".into());
    }
    get("code").filter(|c| !c.is_empty()).ok_or_else(|| "EVE sent no code back".into())
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_the_verifiers_sha256() {
        // RFC 7636, appendix B.
        let p = Pending { state: "s".into(), verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(), scopes: Vec::new() };
        assert_eq!(p.challenge(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let url = p.authorize_url("https://eve-spai.com/wh/callback");
        assert!(url.contains("redirect_uri=https%3A%2F%2Feve-spai.com%2Fwh%2Fcallback"), "{url}");
        assert!(url.contains("code_challenge_method=S256") && !url.contains("scope"), "{url}");
    }

    #[test]
    fn a_callback_counts_only_with_our_state() {
        let p = Pending { state: "abc".into(), verifier: "v".into(), scopes: Vec::new() };
        assert_eq!(callback_code("?code=xy%2Fz&state=abc", &p), Ok("xy/z".into()));
        assert!(callback_code("?code=xyz&state=other", &p).is_err());
        assert!(callback_code("?error=access_denied&state=abc", &p).unwrap_err().contains("access_denied"));
        assert!(callback_code("?state=abc", &p).is_err());
    }

    #[test]
    fn scopes_go_in_the_url_and_come_back_out_of_the_token() {
        let p = Pending { scopes: vec!["esi-location.read_location.v1".into(), "esi-ui.write_waypoint.v1".into()], ..Pending::new() };
        assert!(p.authorize_url("https://x/wh/callback").contains("&scope=esi-location.read_location.v1%20esi-ui.write_waypoint.v1"));
        use base64::Engine as _;
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"CHARACTER:EVE:2112345678","name":"Some Pilot","scp":["esi-location.read_location.v1"]}"#);
        let f = token_facts(&format!("h.{body}.s")).unwrap();
        assert_eq!((f.char_id, f.name.as_str(), f.scopes.len()), (2_112_345_678, "Some Pilot", 1));
        let one = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"CHARACTER:EVE:1","name":"A","scp":"esi-ui.write_waypoint.v1"}"#);
        assert_eq!(token_facts(&format!("h.{one}.s")).unwrap().scopes, vec!["esi-ui.write_waypoint.v1".to_owned()]);
    }

    #[test]
    fn fresh_pairs_differ() {
        assert_ne!(Pending::new(), Pending::new());
    }
}
