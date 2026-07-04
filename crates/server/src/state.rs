use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::PgPool;

use crate::auth::Verifier;
use crate::config::Config;
use crate::session::{SessionIssuer, SessionVerifier};

const VIEW_THROTTLE: Duration = Duration::from_secs(3600);

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub verifier: Arc<Verifier>,
    pub session_issuer: Arc<SessionIssuer>,
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
