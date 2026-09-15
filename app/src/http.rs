//! The one way this app builds an HTTP client.
//!
//! ESI and zKillboard both ask API users to identify themselves, so every request carries the same
//! user agent, and a contact point a site operator can follow.

pub const USER_AGENT: &str =
    concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool; +github.com/Amryu/eve-spai)");

pub fn client(timeout_secs: u64) -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
}
