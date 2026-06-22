//! OAuth token storage in the OS keychain (docs/DESIGN.md §11 D5).
//!
//! Secrets (refresh + access tokens) live in the platform credential store —
//! Secret Service on Linux, Keychain on macOS, Credential Manager on Windows —
//! keyed by character id. Only non-secret metadata (name, scopes, expiry) is kept
//! in the local SQLite DB.

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

/// Store a character's tokens in the keychain.
pub fn save(character_id: i64, tokens: &Tokens) -> Result<()> {
    let json = serde_json::to_string(tokens)?;
    entry(character_id)?
        .set_password(&json)
        .context("writing tokens to keychain")
}

/// Load a character's tokens from the keychain, if present.
#[allow(dead_code)]
pub fn load(character_id: i64) -> Option<Tokens> {
    let json = entry(character_id).ok()?.get_password().ok()?;
    serde_json::from_str(&json).ok()
}

/// Delete a character's tokens from the keychain (no-op if already absent).
pub fn delete(character_id: i64) -> Result<()> {
    match entry(character_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("deleting tokens from keychain"),
    }
}
