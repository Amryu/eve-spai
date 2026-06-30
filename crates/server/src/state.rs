//! Shared application state.
//!
//! Two token layers live here, deliberately distinct: the EVE [`Verifier`] (used ONLY
//! by the `/api/session` mint endpoint) and OUR [`SessionIssuer`]/[`SessionVerifier`]
//! (used by every protected battle-report route via the [`SessionIdentity`] extractor).
//!
//! [`SessionIdentity`]: crate::session::SessionIdentity

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::PgPool;

use crate::auth::Verifier;
use crate::config::Config;
use crate::session::{SessionIssuer, SessionVerifier};

/// A view counts once per (report, client) within this window — a cheap in-memory
/// throttle so a refresh loop can't inflate `views` (good enough for M3).
const VIEW_THROTTLE: Duration = Duration::from_secs(3600);

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// EVE SSO verifier — only the `/api/session` mint endpoint uses it.
    pub verifier: Arc<Verifier>,
    /// Mints OUR session tokens at `/api/session`.
    pub session_issuer: Arc<SessionIssuer>,
    /// Validates OUR session tokens on every protected BR route.
    pub session_verifier: Arc<SessionVerifier>,
    pub cfg: Arc<Config>,
    views: Arc<Mutex<HashMap<(String, String), Instant>>>,
}

impl AppState {
    pub fn new(db: PgPool, verifier: Verifier, cfg: Config) -> Self {
        let secret = cfg.session_secret.as_bytes();
        let session_issuer = SessionIssuer::new(secret, cfg.session_ttl_secs);
        let session_verifier = SessionVerifier::new(secret);
        Self {
            db,
            verifier: Arc::new(verifier),
            session_issuer: Arc::new(session_issuer),
            session_verifier: Arc::new(session_verifier),
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
