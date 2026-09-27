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
    /// Sharing request budgets by (character, write): tokens left and when last topped up.
    buckets: Arc<Mutex<HashMap<(i64, bool), (f64, Instant)>>>,
}

impl AppState {
    pub fn new(db: PgPool, verifier: Verifier, cfg: Config) -> Self {
        let secret = cfg.session_secret.as_bytes();
        let session_issuer = SessionIssuer::new(secret, cfg.session_ttl_secs);
        let session_verifier = SessionVerifier::new(secret).with_extras(cfg.extra_client_ids.clone());
        Self {
            db,
            verifier: Arc::new(verifier),
            session_issuer: Arc::new(session_issuer),
            session_verifier: Arc::new(session_verifier),
            cfg: Arc::new(cfg),
            views: Arc::new(Mutex::new(HashMap::new())),
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Takes one request from `char_id`'s budget. False when it is spent.
    pub fn take_request(&self, char_id: i64, write: bool) -> bool {
        let r = self.cfg.wh_rate;
        let (burst, per_sec) = if write { (r.write_burst, r.write_per_sec) } else { (r.read_burst, r.read_per_sec) };
        let now = Instant::now();
        let mut map = self.buckets.lock().unwrap();
        // A budget idle long enough to be full again says nothing a fresh one would not.
        let full_after = Duration::from_secs_f64(burst / per_sec);
        if map.len() > 10_000 {
            map.retain(|_, (_, at)| now.duration_since(*at) < full_after);
        }
        let (left, at) = map.entry((char_id, write)).or_insert((burst, now));
        *left = (*left + now.duration_since(*at).as_secs_f64() * per_sec).min(burst);
        *at = now;
        if *left < 1.0 {
            return false;
        }
        *left -= 1.0;
        true
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
