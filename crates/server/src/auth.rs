use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

/// How long a fetched JWKS is trusted before a refresh. An unknown `kid` also forces
/// a refresh regardless of age (key rotation).
const JWKS_TTL: Duration = Duration::from_secs(3600);

/// Current EVE tokens stamp the URI form `https://login.eveonline.com` without a trailing slash,
/// older ones the bare host. EVE's guidance is to match either, so all three forms are accepted.
const ISSUERS: [&str; 3] = [
    "login.eveonline.com",
    "https://login.eveonline.com",
    "https://login.eveonline.com/",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub char_id: i64,
    pub name: String,
    /// The SSO application of another client allowed to share wormholes; `None` for EVE Spai's.
    pub client: Option<String>,
}

impl Identity {
    /// Battle reports are EVE Spai's own feature: other clients reach wormhole sharing only.
    pub fn primary(&self) -> Result<(), crate::error::AppError> {
        match self.client {
            None => Ok(()),
            Some(_) => Err(crate::error::AppError::Forbidden),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing or malformed Authorization header")]
    MissingBearer,
    #[error("token header has no kid")]
    NoKid,
    #[error("unknown signing key")]
    UnknownKid,
    #[error("token rejected: {0}")]
    Invalid(String),
    #[error("malformed sub claim: {0}")]
    BadSub(String),
    #[error("could not fetch JWKS: {0}")]
    Jwks(String),
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    name: String,
    #[serde(default)]
    azp: Option<String>,
    #[serde(default)]
    aud: serde_json::Value,
    /// A string for one scope, a list for several; absent without any.
    #[serde(default)]
    scp: serde_json::Value,
}

fn no_scopes(scp: &serde_json::Value) -> bool {
    match scp {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.trim().is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

struct LiveKeys {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

enum Source {
    Static(HashMap<String, DecodingKey>),
    Live { url: String, http: reqwest::Client, cache: RwLock<Option<LiveKeys>> },
}

pub struct Verifier {
    source: Source,
    audience: String,
    /// Other applications allowed to sign in, and only without scopes.
    extras: Vec<String>,
}

impl Verifier {
    pub fn live(jwks_url: impl Into<String>, audience: impl Into<String>) -> Self {
        Self {
            source: Source::Live {
                url: jwks_url.into(),
                http: reqwest::Client::new(),
                cache: RwLock::new(None),
            },
            audience: audience.into(),
            extras: Vec::new(),
        }
    }

    pub fn with_extras(mut self, extras: Vec<String>) -> Self {
        self.extras = extras;
        self
    }

    pub fn from_jwks(jwks: &JwkSet, audience: impl Into<String>) -> anyhow::Result<Self> {
        Ok(Self { source: Source::Static(build_keys(jwks)?), audience: audience.into(), extras: Vec::new() })
    }

    fn validation(&self) -> Validation {
        let mut v = Validation::new(Algorithm::RS256);
        v.set_issuer(&ISSUERS);
        let audiences: Vec<&str> = std::iter::once(self.audience.as_str()).chain(self.extras.iter().map(String::as_str)).collect();
        v.set_audience(&audiences);
        // `exp` is in the default required-claims set and is validated automatically.
        v
    }

    pub async fn verify(&self, token: &str) -> Result<Identity, AuthError> {
        let header = decode_header(token).map_err(|e| AuthError::Invalid(e.to_string()))?;
        let kid = header.kid.ok_or(AuthError::NoKid)?;
        let key = self.decoding_key(&kid).await?;
        let data = decode::<Claims>(token, &key, &self.validation())
            .map_err(|e| AuthError::Invalid(e.to_string()))?;
        let char_id = parse_char_id(&data.claims.sub)?;
        let client = self.client_of(&data.claims)?;
        if client.is_some() && !no_scopes(&data.claims.scp) {
            return Err(AuthError::Invalid("this client may only sign in without scopes".into()));
        }
        Ok(Identity { char_id, name: data.claims.name, client })
    }

    /// Which application the token was issued to: `azp` when EVE sets it, else the audience.
    fn client_of(&self, claims: &Claims) -> Result<Option<String>, AuthError> {
        let aud: Vec<&str> = match &claims.aud {
            serde_json::Value::String(s) => vec![s.as_str()],
            serde_json::Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
            _ => Vec::new(),
        };
        let issued_to = match claims.azp.as_deref() {
            Some(azp) => azp,
            None if aud.contains(&self.audience.as_str()) => self.audience.as_str(),
            None => aud.iter().copied().find(|a| self.extras.iter().any(|e| e == a)).unwrap_or_default(),
        };
        if issued_to == self.audience {
            Ok(None)
        } else if self.extras.iter().any(|e| e == issued_to) {
            Ok(Some(issued_to.to_owned()))
        } else {
            Err(AuthError::Invalid("token issued to another application".into()))
        }
    }

    async fn decoding_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
        match &self.source {
            Source::Static(map) => map.get(kid).cloned().ok_or(AuthError::UnknownKid),
            Source::Live { url, http, cache } => {
                if let Some(k) = fresh_cached(cache, kid) {
                    return Ok(k);
                }
                let jwks: JwkSet = http
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| AuthError::Jwks(e.to_string()))?
                    .json()
                    .await
                    .map_err(|e| AuthError::Jwks(e.to_string()))?;
                let keys = build_keys(&jwks).map_err(|e| AuthError::Jwks(e.to_string()))?;
                let found = keys.get(kid).cloned();
                *cache.write().unwrap() = Some(LiveKeys { keys, fetched_at: Instant::now() });
                found.ok_or(AuthError::UnknownKid)
            }
        }
    }
}

fn fresh_cached(cache: &RwLock<Option<LiveKeys>>, kid: &str) -> Option<DecodingKey> {
    let guard = cache.read().unwrap();
    let live = guard.as_ref()?;
    if live.fetched_at.elapsed() >= JWKS_TTL {
        return None;
    }
    live.keys.get(kid).cloned()
}

fn build_keys(jwks: &JwkSet) -> anyhow::Result<HashMap<String, DecodingKey>> {
    let mut out = HashMap::new();
    for jwk in &jwks.keys {
        if let Some(kid) = jwk.common.key_id.clone() {
            let key = DecodingKey::from_jwk(jwk)?;
            out.insert(kid, key);
        }
    }
    Ok(out)
}

fn parse_char_id(sub: &str) -> Result<i64, AuthError> {
    sub.rsplit(':')
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AuthError::BadSub(sub.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    const TEST_KID: &str = "testkey";
    const TEST_PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCSvEmzmdMFgdXc\n\
8wVGqC67DrSx4ob4y7959e/FDw22Y7Vu6QlnOuD9wPJ3Ah5TlvIRWwHBx/1eBo5n\n\
f+iJu0pK+jqMw4QoaGxsb1pFlLfZMvg6q+LtCfTUkqg5zHl2VZVas60uOT4T5MP2\n\
Ek4FYlj8QDqyD7OYMGIQDTXWGuq8EP+u7exd33gGaafcI56EiOjBG6x+ySBUzJKS\n\
u6InORHoDj/UYvrRWIUTGawzeCug3zg5gv2kHwHq044HcBpGZXEmrBC1PWItDqZ7\n\
rfJ0yDuQMb2LYpbfnj6em0JDAUunKBQJzJKAcPezh8KHNTBekkTEoGprOYPcgELb\n\
AOEPAsqhAgMBAAECggEAMhWtrG2BXzxZZLDYqKzoQnX7DEqvWkWlZjohbKg+PHad\n\
K63ERWWN/V86A5AIDO0VVAI1v9CE9W6UddRtaXGxopT1ni1wMyCtfXemnuBrvmnM\n\
263m55TB6jriy9O008TTlWGF56SnQUAQ+TF3SxQuHm/H+RYt7XD6T9NKgHmwjJ9W\n\
KBfqZsoXy04DW0+93QwTCywgv6g8xUxSZksTLS8toehekmM+DvUTxEZiDyMETXLB\n\
gykhEFggKmE458YGC9kn3y72aSVPjxr5f+yu/8qj7k5sPNmFj7jdbjt4JHfrkSb6\n\
ND+CnBUMOnkd8STPhOv8vzV1gp9Q4rAsPsQbO+KWnwKBgQDOxX9L8WcEepP54IxN\n\
btdD7sARsTTCZYyyIfsEr0wRkljtTutQ919AtvQ4UuCyNabr4+6B+GWCZMrFIFH9\n\
03xcH9MekrwuUwXPZP+jhnr97TKKlrhvo7pkQFgNNGWslZlz+xMkklsdQCwgd38/\n\
/GO2Yv2USuIS6VgiMn+I8PcwowKBgQC1q6lXSCKox6ZWyrHe8up1EXKOjfYvyR3V\n\
dV1cPVJqcDOUiLVQUlGhaIhq+TlwOU45wup0aOfepy2Iz1N3QQRyxITeyv0cnYR4\n\
XdJFv0mcRN7ROyEKt9HTo0/drQpAaE2ln11SoQHW9EW6250wbB78Boy5TC4t4Z4v\n\
rHSG2YOX6wKBgQCad6IcWq/qAaSQNHa71gUMo8xqqyZN300XOhlrK4W5TsoOJjnX\n\
F6XaE5MojIl9uGUFrhZck/NJUQDF+NontBkgPUobeeUI+k7J25q6T9mL3uo17FjG\n\
VdsFz6e33Z/jKTMlGLj5Rji5BlqwunSemW7oLtVfNf3jwNxtV6o85D7V3wKBgEgM\n\
9PRw344g4I+7hB/wJ5yWduCi3OjG0tY93fEfQPiF128paP+aJlXlp3UFswoXMDco\n\
XuQcVxmvJBgGYgwB9UmvNyNFTm1y637xdtvCqecYSWaiFNCzZryRILPCVTaGJ4Vw\n\
VwrWYGxoJN+fChCSUReTYWx8EjSQLrSpqO1yhwZRAoGAOQlPp+YwsaEyZZq3Ruqc\n\
32Cz6GERvQuWdfdOG95C1lmi/NcBUqX1AmSEBv68YEnEQVJrRl35Gtf5bELyZyYV\n\
CrSigy78//1e/ZqnrDwX4m35XyNiB8qFaDrtHeviGrHZTV0nCgiMRX0FB7PKIBNP\n\
QxUVZGDK2388KblxKYy3YTE=\n\
-----END PRIVATE KEY-----\n";
    const TEST_N: &str = "krxJs5nTBYHV3PMFRqguuw60seKG-Mu_efXvxQ8NtmO1bukJZzrg_cDydwIeU5byEVsBwcf9XgaOZ3_oibtKSvo6jMOEKGhsbG9aRZS32TL4Oqvi7Qn01JKoOcx5dlWVWrOtLjk-E-TD9hJOBWJY_EA6sg-zmDBiEA011hrqvBD_ru3sXd94Bmmn3COehIjowRusfskgVMySkruiJzkR6A4_1GL60ViFExmsM3groN84OYL9pB8B6tOOB3AaRmVxJqwQtT1iLQ6me63ydMg7kDG9i2KW354-nptCQwFLpygUCcySgHD3s4fChzUwXpJExKBqazmD3IBC2wDhDwLKoQ";
    const TEST_E: &str = "AQAB";
    const AUD: &str = super::super::config::DEFAULT_CLIENT_ID;

    fn test_jwks() -> JwkSet {
        serde_json::from_value(json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": TEST_KID,
                "n": TEST_N,
                "e": TEST_E,
            }]
        }))
        .unwrap()
    }

    fn verifier() -> Verifier {
        Verifier::from_jwks(&test_jwks(), AUD).unwrap()
    }

    fn sign(claims: serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_string());
        let key = EncodingKey::from_rsa_pem(TEST_PRIV_PEM.as_bytes()).unwrap();
        encode(&header, &claims, &key).unwrap()
    }

    fn future() -> i64 {
        chrono::Utc::now().timestamp() + 3600
    }

    fn valid_claims() -> serde_json::Value {
        json!({
            "sub": "CHARACTER:EVE:2112000001",
            "name": "Spai Pilot",
            "iss": "https://login.eveonline.com",
            "aud": [AUD, "EVE Online"],
            "exp": future(),
        })
    }

    #[tokio::test]
    async fn valid_token_accepted_with_identity() {
        let id = verifier().verify(&sign(valid_claims())).await.unwrap();
        assert_eq!(id.char_id, 2112000001);
        assert_eq!(id.name, "Spai Pilot");
    }

    #[tokio::test]
    async fn tampered_signature_rejected() {
        let mut token = sign(valid_claims());
        let last = token.pop().unwrap();
        token.push(if last == 'A' { 'B' } else { 'A' });
        assert!(verifier().verify(&token).await.is_err());
    }

    #[tokio::test]
    async fn expired_token_rejected() {
        let mut c = valid_claims();
        c["exp"] = json!(chrono::Utc::now().timestamp() - 3600);
        assert!(verifier().verify(&sign(c)).await.is_err());
    }

    #[tokio::test]
    async fn wrong_audience_rejected() {
        let mut c = valid_claims();
        c["aud"] = json!(["someone-elses-client", "EVE Online"]);
        assert!(verifier().verify(&sign(c)).await.is_err());
    }

    #[tokio::test]
    async fn wrong_issuer_rejected() {
        let mut c = valid_claims();
        c["iss"] = json!("https://evil.example.com");
        assert!(verifier().verify(&sign(c)).await.is_err());
    }

    #[tokio::test]
    async fn unknown_kid_rejected() {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("not-the-test-key".to_string());
        let key = EncodingKey::from_rsa_pem(TEST_PRIV_PEM.as_bytes()).unwrap();
        let token = encode(&header, &valid_claims(), &key).unwrap();
        assert!(matches!(verifier().verify(&token).await, Err(AuthError::UnknownKid)));
    }

    const EXTRA: &str = "third-party-client";

    fn with_extra() -> Verifier {
        Verifier::from_jwks(&test_jwks(), AUD).unwrap().with_extras(vec![EXTRA.to_owned()])
    }

    fn extra_claims(scp: serde_json::Value) -> serde_json::Value {
        let mut c = valid_claims();
        c["aud"] = json!([EXTRA, "EVE Online"]);
        c["azp"] = json!(EXTRA);
        if !scp.is_null() {
            c["scp"] = scp;
        }
        c
    }

    #[tokio::test]
    async fn eve_spai_keeps_its_scopes_and_needs_no_client_mark() {
        let mut c = valid_claims();
        c["azp"] = json!(AUD);
        c["scp"] = json!(["esi-location.read_location.v1", "esi-fleets.read_fleet.v1"]);
        let id = with_extra().verify(&sign(c)).await.unwrap();
        assert_eq!(id.client, None);
        // Tokens as live clients send them, without azp, still count as EVE Spai's.
        assert_eq!(with_extra().verify(&sign(valid_claims())).await.unwrap().client, None);
    }

    #[tokio::test]
    async fn another_client_signs_in_only_without_scopes() {
        let v = with_extra();
        let id = v.verify(&sign(extra_claims(json!(null)))).await.unwrap();
        assert_eq!(id.client.as_deref(), Some(EXTRA));
        assert!(v.verify(&sign(extra_claims(json!([])))).await.is_ok());
        assert!(v.verify(&sign(extra_claims(json!("esi-location.read_location.v1")))).await.is_err(), "one scope");
        assert!(v.verify(&sign(extra_claims(json!(["esi-location.read_location.v1"])))).await.is_err(), "a list");
        // Not configured: refused like any other stranger.
        assert!(verifier().verify(&sign(extra_claims(json!(null)))).await.is_err());
    }

    #[tokio::test]
    async fn a_token_claiming_one_client_for_another_audience_is_refused() {
        let mut c = extra_claims(json!(null));
        c["azp"] = json!("someone-else");
        assert!(with_extra().verify(&sign(c)).await.is_err());
    }

    #[test]
    fn parse_char_id_works() {
        assert_eq!(parse_char_id("CHARACTER:EVE:90000001").unwrap(), 90000001);
        assert!(parse_char_id("CHARACTER:EVE:notanumber").is_err());
    }
}
