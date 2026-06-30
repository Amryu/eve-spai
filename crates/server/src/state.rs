//! Shared application state and the authenticated-identity extractor.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::header::AUTHORIZATION;
use sqlx::PgPool;

use crate::auth::{Identity, Verifier};
use crate::config::Config;
use crate::error::AppError;

/// A view counts once per (report, client) within this window — a cheap in-memory
/// throttle so a refresh loop can't inflate `views` (good enough for M3).
const VIEW_THROTTLE: Duration = Duration::from_secs(3600);

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub verifier: Arc<Verifier>,
    pub cfg: Arc<Config>,
    views: Arc<Mutex<HashMap<(String, String), Instant>>>,
}

impl AppState {
    pub fn new(db: PgPool, verifier: Verifier, cfg: Config) -> Self {
        Self {
            db,
            verifier: Arc::new(verifier),
            cfg: Arc::new(cfg),
            views: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns true if this (id, ip) view should be counted now, recording it. Also
    /// opportunistically evicts stale entries so the map can't grow unbounded.
    pub fn should_count_view(&self, id: &str, ip: &str) -> bool {
        let mut map = self.views.lock().unwrap();
        let now = Instant::now();
        map.retain(|_, t| now.duration_since(*t) < VIEW_THROTTLE);
        let key = (id.to_string(), ip.to_string());
        if map.contains_key(&key) {
            return false;
        }
        map.insert(key, now);
        true
    }
}

/// Extractor that verifies the `Authorization: Bearer <jwt>` token and yields the
/// caller's [`Identity`]. Any problem becomes a 401.
impl<S> FromRequestParts<S> for Identity
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or_else(|| AppError::Unauthorized("missing bearer token".into()))?;
        app.verifier
            .verify(token.trim())
            .await
            .map_err(|e| AppError::Unauthorized(e.to_string()))
    }
}
