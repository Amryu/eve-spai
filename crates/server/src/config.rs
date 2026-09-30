pub const DEFAULT_CLIENT_ID: &str = "fef96bde615b450bba89c9414962ca38";
pub const DEFAULT_JWKS_URL: &str = "https://login.eveonline.com/oauth/jwks";

pub const DEFAULT_MAX_COMPRESSED: usize = 1024 * 1024;
pub const DEFAULT_MAX_DECOMPRESSED: usize = 8 * 1024 * 1024;
pub const DEFAULT_MAX_PER_CHAR: i64 = 1000;
pub const DEFAULT_UPLOADS_PER_HOUR: i64 = 60;
pub const DEFAULT_SESSION_TTL_SECS: i64 = 86_400;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub client_id: String,
    pub jwks_url: String,
    pub public_base_url: String,
    pub max_compressed: usize,
    pub max_decompressed: usize,
    pub max_per_char: i64,
    pub uploads_per_hour: i64,
    pub session_secret: String,
    pub session_ttl_secs: i64,
    /// Other SSO applications allowed to share wormholes, signing in without scopes.
    pub extra_client_ids: Vec<String>,
    /// Web origins allowed to call the session mint and the sharing routes from a browser.
    pub cors_origins: Vec<String>,
    pub wh_rate: WhRate,
}

/// Per-character request budgets on the sharing routes: a burst, refilled per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WhRate {
    pub read_burst: f64,
    pub read_per_sec: f64,
    pub write_burst: f64,
    pub write_per_sec: f64,
}

impl Default for WhRate {
    /// Well above what EVE Spai does: a poll per group every 15 s, and up to 200 entries sent in
    /// one round after joining a group.
    fn default() -> Self {
        WhRate { read_burst: 300.0, read_per_sec: 5.0, write_burst: 600.0, write_per_sec: 10.0 }
    }
}

fn env_list(key: &str) -> Vec<String> {
    std::env::var(key)
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().trim_end_matches('/').to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).filter(|v: &f64| *v > 0.0).unwrap_or(default)
}

/// The CORS layer for `origins`, or none (no CORS headers at all, the default).
pub fn cors(origins: &[String]) -> Option<tower_http::cors::CorsLayer> {
    use axum::http::{header, HeaderValue, Method};
    let allowed: Vec<HeaderValue> = origins.iter().filter_map(|o| HeaderValue::from_str(o).ok()).collect();
    if allowed.is_empty() {
        return None;
    }
    Some(
        tower_http::cors::CorsLayer::new()
            .allow_origin(allowed)
            .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::HeaderName::from_static("x-spai-device")])
            .max_age(std::time::Duration::from_secs(3600)),
    )
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
        let session_secret = std::env::var("BR_SESSION_SECRET")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("BR_SESSION_SECRET must be set"))?;
        Ok(Self {
            database_url,
            bind_addr: env_or("BIND_ADDR", "0.0.0.0:8080"),
            client_id: env_or("EVE_CLIENT_ID", DEFAULT_CLIENT_ID),
            jwks_url: env_or("EVE_JWKS_URL", DEFAULT_JWKS_URL),
            public_base_url: env_or("PUBLIC_BASE_URL", "https://eve-spai.com")
                .trim_end_matches('/')
                .to_string(),
            max_compressed: env_usize("BR_MAX_COMPRESSED", DEFAULT_MAX_COMPRESSED),
            max_decompressed: env_usize("BR_MAX_DECOMPRESSED", DEFAULT_MAX_DECOMPRESSED),
            max_per_char: env_i64("BR_MAX_PER_CHAR", DEFAULT_MAX_PER_CHAR),
            uploads_per_hour: env_i64("BR_UPLOADS_PER_HOUR", DEFAULT_UPLOADS_PER_HOUR),
            session_secret,
            session_ttl_secs: env_i64("BR_SESSION_TTL_SECS", DEFAULT_SESSION_TTL_SECS),
            extra_client_ids: env_list("EVE_EXTRA_CLIENT_IDS"),
            cors_origins: env_list("WH_CORS_ORIGINS"),
            wh_rate: {
                let d = WhRate::default();
                WhRate {
                    read_burst: env_f64("WH_RATE_READ_BURST", d.read_burst),
                    read_per_sec: env_f64("WH_RATE_READ_PER_SEC", d.read_per_sec),
                    write_burst: env_f64("WH_RATE_WRITE_BURST", d.write_burst),
                    write_per_sec: env_f64("WH_RATE_WRITE_PER_SEC", d.write_per_sec),
                }
            },
        })
    }

    pub fn report_url(&self, id: &str) -> String {
        format!("{}/br/{}", self.public_base_url, id)
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
