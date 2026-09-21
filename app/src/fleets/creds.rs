//! The dashboard session, at rest.
//!
//! `fleets.gnf.lt` authenticates with a session cookie and nothing else, so the cookie is the
//! credential. It goes in the OS keychain, following `jabber.rs`: keyring only, no `sealed.rs`
//! fallback, because `sealed.rs` is keyed by character id with a per-character AAD and this secret
//! belongs to the account. A machine with no Secret Service provider signs in again each run.
//!
//! NOTE: the uitest guard only redirects `EVE_SPAI_DATA_DIR`. The keychain is outside it, so a
//! developer with a live cookie here would hand it to anything that builds an `HttpBackend`.
//! Nothing under test may construct one.

const KEYCHAIN_SERVICE: &str = "eve-spai-fleet";
const ACCOUNT: &str = "fleets.gnf.lt";

pub fn save(cookies: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    keyring::Entry::new(KEYCHAIN_SERVICE, ACCOUNT)
        .context("opening keychain entry")?
        .set_password(cookies)
        .context("writing the fleet dashboard session")?;
    Ok(())
}

pub fn load() -> Option<String> {
    let text = keyring::Entry::new(KEYCHAIN_SERVICE, ACCOUNT).ok()?.get_password().ok()?;
    (!text.trim().is_empty()).then_some(text)
}

pub fn has() -> bool {
    load().is_some()
}

/// Best effort: a keychain that will not delete is the same problem as one that will not read, and
/// the caller has already dropped the backend either way.
pub fn forget() {
    if let Ok(e) = keyring::Entry::new(KEYCHAIN_SERVICE, ACCOUNT) {
        let _ = e.delete_credential();
    }
}
