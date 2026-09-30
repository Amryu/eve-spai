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
}

impl Pending {
    pub fn new() -> Self {
        Pending { state: b64(&random32()[..16]), verifier: b64(&random32()) }
    }

    fn challenge(&self) -> String {
        b64(&sha2::Sha256::digest(self.verifier.as_bytes()))
    }

    /// Where to send the browser. `redirect` is the callback registered with EVE.
    pub fn authorize_url(&self, redirect: &str) -> String {
        format!(
            "{AUTHORIZE}?response_type=code&redirect_uri={}&client_id={CLIENT_ID}&state={}&code_challenge={}&code_challenge_method=S256",
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
        let p = Pending { state: "s".into(), verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into() };
        assert_eq!(p.challenge(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let url = p.authorize_url("https://eve-spai.com/wh/callback");
        assert!(url.contains("redirect_uri=https%3A%2F%2Feve-spai.com%2Fwh%2Fcallback"), "{url}");
        assert!(url.contains("code_challenge_method=S256") && !url.contains("scope"), "{url}");
    }

    #[test]
    fn a_callback_counts_only_with_our_state() {
        let p = Pending { state: "abc".into(), verifier: "v".into() };
        assert_eq!(callback_code("?code=xy%2Fz&state=abc", &p), Ok("xy/z".into()));
        assert!(callback_code("?code=xyz&state=other", &p).is_err());
        assert!(callback_code("?error=access_denied&state=abc", &p).unwrap_err().contains("access_denied"));
        assert!(callback_code("?state=abc", &p).is_err());
    }

    #[test]
    fn fresh_pairs_differ() {
        assert_ne!(Pending::new(), Pending::new());
    }
}
