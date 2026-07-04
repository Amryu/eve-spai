//! The EVE SSO access token carries write scopes (`write_waypoint`, `write_fittings`)
//! and is audienced to EVE — not to us. So it is verified exactly ONCE, at the
//! [`POST /api/session`](crate::routes) mint endpoint, and is never logged or persisted.
//! Every battle-report call then authenticates with one of OUR own short-lived HS256
//! tokens, audienced to `eve-spai.com` and carrying no EVE scopes.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::auth::Identity;
use crate::error::AppError;
use crate::state::AppState;

/// `iss`/`aud` of our own tokens — we issue to ourselves and accept only ourselves.
pub const SESSION_ISS: &str = "eve-spai.com";
pub const SESSION_AUD: &str = "eve-spai.com";

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub name: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Clone)]
pub struct SessionIssuer {
    key: EncodingKey,
    ttl_secs: i64,
}

impl SessionIssuer {
    pub fn new(secret: &[u8], ttl_secs: i64) -> Self {
        Self { key: EncodingKey::from_secret(secret), ttl_secs }
    }

    pub fn issue(&self, id: &Identity) -> anyhow::Result<(String, i64)> {
        let now = chrono::Utc::now().timestamp();
        let exp = now + self.ttl_secs;
        let claims = SessionClaims {
            iss: SESSION_ISS.to_string(),
            aud: SESSION_AUD.to_string(),
            sub: id.char_id.to_string(),
            name: id.name.clone(),
            exp,
            iat: now,
        };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.key)?;
        Ok((token, exp))
    }
}

pub struct SessionVerifier {
    key: DecodingKey,
    validation: Validation,
}

impl SessionVerifier {
    pub fn new(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[SESSION_ISS]);
        validation.set_audience(&[SESSION_AUD]);
        // `exp` is in the default required-claims set and is validated automatically.
        Self { key: DecodingKey::from_secret(secret), validation }
    }

    pub fn verify(&self, token: &str) -> Result<Identity, AppError> {
        let data = decode::<SessionClaims>(token, &self.key, &self.validation)
            .map_err(|e| AppError::Unauthorized(e.to_string()))?;
        let char_id = data
            .claims
            .sub
            .parse::<i64>()
            .map_err(|_| AppError::Unauthorized("malformed sub claim".into()))?;
        Ok(Identity { char_id, name: data.claims.name })
    }
}

pub fn bearer(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or_else(|| AppError::Unauthorized("missing bearer token".into()))
}

/// Extractor for the protected BR routes: validates OUR session token (never the EVE
/// token) and yields the caller's [`Identity`]. Anything else becomes a 401.
pub struct SessionIdentity(pub Identity);

impl<S> FromRequestParts<S> for SessionIdentity
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);
        let token = bearer(&parts.headers)?;
        app.session_verifier.verify(token).map(SessionIdentity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"unit-test-session-secret";

    fn id() -> Identity {
        Identity { char_id: 2112000001, name: "Spai Pilot".into() }
    }

    fn issuer(ttl: i64) -> SessionIssuer {
        SessionIssuer::new(SECRET, ttl)
    }

    fn verifier() -> SessionVerifier {
        SessionVerifier::new(SECRET)
    }

    #[test]
    fn round_trip_yields_identity() {
        let (tok, exp) = issuer(3600).issue(&id()).unwrap();
        assert!(exp > chrono::Utc::now().timestamp());
        let got = verifier().verify(&tok).unwrap();
        assert_eq!(got, id());
    }

    #[test]
    fn wrong_secret_rejected() {
        let (tok, _) = issuer(3600).issue(&id()).unwrap();
        let other = SessionVerifier::new(b"a-different-secret");
        assert!(other.verify(&tok).is_err());
    }

    #[test]
    fn expired_token_rejected() {
        let (tok, _) = issuer(-3600).issue(&id()).unwrap();
        assert!(verifier().verify(&tok).is_err());
    }

    fn sign_claims(claims: serde_json::Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(SECRET),
        )
        .unwrap()
    }

    #[test]
    fn wrong_audience_rejected() {
        let tok = sign_claims(serde_json::json!({
            "iss": SESSION_ISS, "aud": "someone-else.example",
            "sub": "1", "name": "X",
            "exp": chrono::Utc::now().timestamp() + 3600, "iat": 0,
        }));
        assert!(verifier().verify(&tok).is_err());
    }

    #[test]
    fn wrong_issuer_rejected() {
        let tok = sign_claims(serde_json::json!({
            "iss": "evil.example", "aud": SESSION_AUD,
            "sub": "1", "name": "X",
            "exp": chrono::Utc::now().timestamp() + 3600, "iat": 0,
        }));
        assert!(verifier().verify(&tok).is_err());
    }

    #[test]
    fn missing_bearer_is_error() {
        assert!(bearer(&HeaderMap::new()).is_err());
    }
}
