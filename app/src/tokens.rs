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

/// What to do about a keychain error, because the underlying error does not say.
///
/// The Secret Service error for "there is no keyring at all" is `result not returned from SS API`,
/// nested twice, which reads like a bug in this app. It means the D-Bus service answered but has no
/// collection to put anything in, and installing a provider fixes it.
#[cfg(target_os = "linux")]
const ADVICE: &str = "The system keychain could not be used. On Linux it needs a Secret Service \
     provider with a keyring created: GNOME Keyring, KWallet with its Secret Service module, or \
     KeePassXC.";

#[cfg(not(target_os = "linux"))]
const ADVICE: &str = "The system keychain could not be used. Make sure it is unlocked and \
     reachable.";

/// Store a character's refresh token.
///
/// The OS keychain first. Only the small refresh token lives there: the access-token JWT grows with
/// scopes, and the Windows Credential Manager rejects a password over 2560 UTF-16 chars, so it is
/// cached in the DB instead.
///
/// When the keychain cannot be used at all, the token is sealed into [`crate::sealed`], encrypted
/// under a key tied to this OS account on this machine. Never plaintext, and never reached while
/// the keychain works.
pub fn save_refresh(character_id: i64, refresh_token: &str) -> Result<()> {
    let Err(e) = entry(character_id).and_then(|e| {
        e.set_password(refresh_token).map_err(|e| anyhow::Error::new(e).context(ADVICE))
    }) else {
        // The keychain took it, so any earlier sealed copy is now a second place the token lives.
        // One home at a time.
        let _ = crate::sealed::delete(character_id);
        return Ok(());
    };
    eprintln!("keychain unavailable, sealing the refresh token instead: {e:#}");
    crate::sealed::save(character_id, refresh_token)
        .context("the system keychain could not be used, and the encrypted fallback failed too")
}

pub fn load_refresh(character_id: i64) -> Option<String> {
    try_load_refresh(character_id).ok().flatten()
}

/// The same read, keeping the difference between "no token saved for this character" and "neither
/// store could be read at all".
///
/// Collapsing the two into `None` would make a broken keyring look like a character that never
/// logged in, with a new login then failing to save and nothing saying why.
pub fn try_load_refresh(character_id: i64) -> Result<Option<String>> {
    match entry(character_id).and_then(|e| match e.get_password() {
        Ok(raw) => Ok(Some(raw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context(ADVICE)),
    }) {
        Ok(Some(raw)) => Ok(Some(unwrap_legacy(raw))),
        // The keychain works and has nothing for this character. A sealed copy can still exist from
        // a time when it did not, so it is promoted back and the fallback gives the character up.
        Ok(None) => match crate::sealed::load(character_id)? {
            Some(tok) => {
                promote(character_id, &tok);
                Ok(Some(unwrap_legacy(tok)))
            }
            None => Ok(None),
        },
        Err(keychain_err) => match crate::sealed::load(character_id) {
            Ok(Some(tok)) => Ok(Some(unwrap_legacy(tok))),
            Ok(None) => Err(keychain_err),
            // Both failed. The keychain's reason is the one that explains the situation; the
            // fallback's is what it could not do about it.
            Err(sealed_err) => Err(keychain_err.context(format!("{sealed_err:#}"))),
        },
    }
}

/// A token sealed while there was no keychain, put back where it belongs now that there is one.
///
/// Best effort on purpose: if this fails the sealed copy is still there and still works, and the
/// caller already has the token it asked for.
fn promote(character_id: i64, refresh_token: &str) {
    if entry(character_id)
        .and_then(|e| e.set_password(refresh_token).map_err(anyhow::Error::new))
        .is_ok()
    {
        let _ = crate::sealed::delete(character_id);
    }
}

/// Older versions stored the whole `Tokens` struct. The next save rewrites it as a bare refresh
/// token.
fn unwrap_legacy(raw: String) -> String {
    match serde_json::from_str::<Tokens>(&raw) {
        Ok(t) => t.refresh_token,
        Err(_) => raw,
    }
}

pub fn delete(character_id: i64) -> Result<()> {
    // Both stores, whatever either says: "forget this character" has to mean it even if one of them
    // is unreachable today and comes back tomorrow.
    let sealed = crate::sealed::delete(character_id);
    match entry(character_id).and_then(|e| match e.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context("deleting tokens from keychain")),
    }) {
        Ok(()) => sealed,
        // A keychain that cannot be reached has nothing in it to delete either.
        Err(e) => {
            if crate::sealed::load(character_id).ok().flatten().is_none() {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Whether any token is being kept in the encrypted fallback rather than the OS keychain. The user
/// is told in settings, because it is a real difference and not one they chose.
pub fn fallback_in_use() -> bool {
    crate::sealed::in_use()
}
