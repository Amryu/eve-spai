//! This install's device keys: made once, kept in the OS keychain like the EVE refresh tokens, or
//! sealed to this machine when there is no keychain.

use anyhow::Result;

use super::crypto::DeviceKeys;

const SERVICE: &str = "eve-spai-share";
const ACCOUNT: &str = "device";
/// The sealed-file slot for the device keys. No EVE character has id 0.
const SEALED_SLOT: i64 = 0;

fn keychain() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()
}

fn stored() -> Option<String> {
    if let Some(text) = keychain().and_then(|e| e.get_password().ok()) {
        return Some(text);
    }
    crate::sealed::load(SEALED_SLOT).ok().flatten()
}

fn store(text: &str) -> Result<()> {
    if keychain().is_some_and(|e| e.set_password(text).is_ok()) {
        return Ok(());
    }
    crate::sealed::save(SEALED_SLOT, text)
}

/// The device keys, made on first use.
pub fn device() -> Result<&'static DeviceKeys> {
    static KEYS: std::sync::OnceLock<DeviceKeys> = std::sync::OnceLock::new();
    if let Some(k) = KEYS.get() {
        return Ok(k);
    }
    let keys = match stored().map(|t| DeviceKeys::from_text(&t)) {
        Some(Ok(k)) => k,
        // A stored key that does not parse is not silently replaced: the groups it was in would
        // be lost with it.
        Some(Err(e)) => return Err(e.context("the stored sharing key is damaged")),
        None => {
            let k = DeviceKeys::generate();
            store(&k.to_text())?;
            k
        }
    };
    Ok(KEYS.get_or_init(|| keys))
}
