use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "eve-spai";

#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub refresh_token: String,
    pub access_token: String,
}

fn entry(character_id: i64) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &character_id.to_string())
        .context("opening keychain entry (is a Secret Service / keychain available?)")
}

/// Store a character's refresh token in the keychain. Only the (small) refresh token lives
/// here — the access-token JWT is short-lived and grows with scopes, and on Windows the
/// Credential Manager rejects a password over 2560 UTF-16 chars; it's cached in the DB instead.
pub fn save_refresh(character_id: i64, refresh_token: &str) -> Result<()> {
    entry(character_id)?
        .set_password(refresh_token)
        .context("writing refresh token to keychain")
}

/// (the next save rewrites it as a plain refresh token).
pub fn load_refresh(character_id: i64) -> Option<String> {
    let raw = entry(character_id).ok()?.get_password().ok()?;
    match serde_json::from_str::<Tokens>(&raw) {
        Ok(t) => Some(t.refresh_token),
        Err(_) => Some(raw),
    }
}

pub fn delete(character_id: i64) -> Result<()> {
    match entry(character_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("deleting tokens from keychain"),
    }
}
