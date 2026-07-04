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
