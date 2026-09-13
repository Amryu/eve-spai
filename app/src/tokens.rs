use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "eve-spai";

#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub refresh_token: String,
    pub access_token: String,
}

fn entry(character_id: i64) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &character_id.to_string()).map_err(|e| anyhow::Error::new(e).context(ADVICE))
}

/// What to actually do about it, because the underlying error does not say.
///
/// The Secret Service error for "there is no keyring at all" is `result not returned from SS API`,
/// nested twice, which reads like a bug in this app. It is not: it means the D-Bus service answered
/// but has no collection to put anything in. A user hitting this had to work out on their own that
/// installing a provider fixed it.
#[cfg(target_os = "linux")]
const ADVICE: &str = "The system keychain could not be used. EVE Spai keeps refresh tokens there \
     and will not write them anywhere less safe. On Linux this needs a Secret Service provider with \
     a keyring created: GNOME Keyring, KWallet with its Secret Service module, or KeePassXC. Start \
     or install one, make sure it is unlocked, then log in again.";

#[cfg(not(target_os = "linux"))]
const ADVICE: &str = "The system keychain could not be used. EVE Spai keeps refresh tokens there \
     and will not write them anywhere less safe. Make sure the keychain is unlocked and reachable, \
     then log in again.";

/// Store a character's refresh token in the keychain. Only the (small) refresh token lives
/// here — the access-token JWT is short-lived and grows with scopes, and on Windows the
/// Credential Manager rejects a password over 2560 UTF-16 chars; it's cached in the DB instead.
pub fn save_refresh(character_id: i64, refresh_token: &str) -> Result<()> {
    entry(character_id)?.set_password(refresh_token).map_err(|e| anyhow::Error::new(e).context(ADVICE))
}

/// (the next save rewrites it as a plain refresh token).
pub fn load_refresh(character_id: i64) -> Option<String> {
    try_load_refresh(character_id).ok().flatten()
}

/// The same read, keeping the difference between "no token saved for this character" and "the
/// keychain could not be reached at all".
///
/// Collapsing those two into `None` is what made a broken keyring look like a character that had
/// never been logged in: the app asked for a login, the login then failed to save, and nothing
/// said why.
pub fn try_load_refresh(character_id: i64) -> Result<Option<String>> {
    let raw = match entry(character_id)?.get_password() {
        Ok(raw) => raw,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(anyhow::Error::new(e).context(ADVICE)),
    };
    Ok(Some(match serde_json::from_str::<Tokens>(&raw) {
        Ok(t) => t.refresh_token,
        Err(_) => raw,
    }))
}

pub fn delete(character_id: i64) -> Result<()> {
    match entry(character_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("deleting tokens from keychain"),
    }
}
