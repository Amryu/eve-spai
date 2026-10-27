//! EVE SSO from the browser, without a secret: authorization code with PKCE, then the EVE token is
//! traded for an EVE Spai session. Both browser apps share the one EVE application and its
//! callback under `/wh/`; a sign-in started elsewhere says where to return to.

use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use spai_share::crypto::{b64, random32};

/// The scopeless EVE application registered for the web apps, calling back to `/wh/callback`.
pub const CLIENT_ID: &str = "c0e78d5da88849ac94775034063ec68f";
const AUTHORIZE: &str = "https://login.eveonline.com/v2/oauth/authorize";
pub const TOKEN: &str = "https://login.eveonline.com/v2/oauth/token";

/// Keys the apps share in the browser's storage.
pub const SESSION_KEY: &str = "spai.session";
/// Why the last sign-in got no EVE Spai session, for the page it returns to to say.
pub const SESSION_ERROR_KEY: &str = "spai.session.error";
pub const PENDING_KEY: &str = "spai.sso";
pub const ACCOUNTS_KEY: &str = "spai.accounts";

/// What a sign-in in progress keeps across the trip to EVE and back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub state: String,
    pub verifier: String,
    /// ESI scopes asked for: none for the sign-in to EVE Spai, some for a character added for
    /// its location, waypoints or skills.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Where to go once the callback under `/wh/` has finished: the page that started it.
    #[serde(default)]
    pub return_to: Option<String>,
    /// The same login also signs in to EVE Spai: one trip to EVE for the scopes and the session.
    #[serde(default)]
    pub session: bool,
}

impl Pending {
    pub fn new() -> Self {
        Pending { state: b64(&random32()[..16]), verifier: b64(&random32()), scopes: Vec::new(), return_to: None, session: false }
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

/// The form that renews a login from its refresh token.
pub fn refresh_form(refresh_token: &str) -> String {
    format!("grant_type=refresh_token&refresh_token={}&client_id={CLIENT_ID}", encode(refresh_token))
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

pub fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() + 0 && i + 2 <= bytes.len() - 1 => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Trades the code for EVE's tokens.
#[cfg(target_arch = "wasm32")]
pub async fn token(p: &Pending, code: &str) -> Result<TokenReply, String> {
    let mut req = ehttp::Request::post(TOKEN, p.token_form(code).into_bytes());
    req.headers = ehttp::Headers::new(&[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")]);
    let r = ehttp::fetch_async(req).await?;
    if !r.ok {
        return Err(format!("EVE refused the sign-in ({} {})", r.status, r.status_text));
    }
    serde_json::from_slice(&r.bytes).map_err(|e| format!("EVE's answer: {e}"))
}

/// Trades an EVE token for an EVE Spai session at `origin`.
#[cfg(target_arch = "wasm32")]
pub async fn session(origin: &str, access_token: &str) -> Result<Session, String> {
    let mut req = ehttp::Request::post(format!("{origin}/api/session"), Vec::new());
    req.headers = ehttp::Headers::new(&[("Authorization", &format!("Bearer {access_token}")), ("Accept", "application/json")]);
    let r = ehttp::fetch_async(req).await?;
    if !r.ok {
        let msg = serde_json::from_slice::<serde_json::Value>(&r.bytes).ok().and_then(|v| v["error"].as_str().map(str::to_owned));
        return Err(format!("the EVE Spai server refused the sign-in: {}", msg.unwrap_or_else(|| r.status.to_string())));
    }
    serde_json::from_slice(&r.bytes).map_err(|e| format!("the server's answer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_authorize_url_carries_pkce_state_scopes_and_the_redirect() {
        let p = Pending { state: "s".into(), verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(), scopes: Vec::new(), return_to: None, session: false };
        let url = p.authorize_url("https://eve-spai.com/wh/callback");
        assert!(url.contains("redirect_uri=https%3A%2F%2Feve-spai.com%2Fwh%2Fcallback"));
        assert!(url.contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"), "{url}");
        assert!(!url.contains("scope="));
        let p = Pending { scopes: vec!["esi-skills.read_skills.v1".into()], ..p };
        assert!(p.authorize_url("x").contains("&scope=esi-skills.read_skills.v1"));
        assert!(p.token_form("c o").starts_with("grant_type=authorization_code&code=c%20o&client_id="));
        assert!(refresh_form("r/t").contains("refresh_token=r%2Ft"));
    }

    #[test]
    fn the_callback_is_checked_against_the_pending_state() {
        let p = Pending { state: "abc".into(), verifier: "v".into(), scopes: Vec::new(), return_to: Some("/sim/".into()), session: true };
        assert_eq!(callback_code("?code=xyz&state=abc", &p).unwrap(), "xyz");
        assert!(callback_code("?code=xyz&state=nope", &p).is_err());
        assert!(callback_code("?error=access_denied&state=abc", &p).unwrap_err().contains("access_denied"));
        assert!(callback_code("?state=abc", &p).is_err());
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Pending>(&json).unwrap(), p);
        let old: Pending = serde_json::from_str(r#"{"state":"a","verifier":"b"}"#).unwrap();
        assert_eq!((old.return_to, old.session), (None, false), "a sign-in started before the field reads as scopes only");
    }

    #[test]
    fn percent_coding_round_trips() {
        assert_eq!(decode(&encode("a b/c?d=ä")), "a b/c?d=ä");
        assert_eq!(decode("x+y%2"), "x y%2");
    }

    #[test]
    fn token_facts_come_from_the_jwt_body() {
        use base64::Engine as _;
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"CHARACTER:EVE:90000777","name":"Test Pilot","scp":["esi-skills.read_skills.v1"]}"#);
        let f = token_facts(&format!("h.{body}.s")).unwrap();
        assert_eq!((f.char_id, f.name.as_str()), (90000777, "Test Pilot"));
        assert_eq!(f.scopes, vec!["esi-skills.read_skills.v1"]);
        assert!(token_facts("nope").is_none());
    }
}
