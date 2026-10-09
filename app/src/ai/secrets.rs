//! API keys and feed credentials, in the OS keychain only, never in settings or logs.
//!
//! NOTE: the uitest guard only redirects `EVE_SPAI_DATA_DIR`. The keychain is outside it, so tests
//! and headless builds get a [`MemSecrets`] and never a [`Keychain`].

use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE: &str = "eve-spai-ai";

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Option<String>;
    fn set(&self, account: &str, value: &str) -> anyhow::Result<()>;
    fn delete(&self, account: &str);
    fn has(&self, account: &str) -> bool {
        self.get(account).is_some()
    }
}

pub struct Keychain;

impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Option<String> {
        let v = keyring::Entry::new(SERVICE, account).ok()?.get_password().ok()?;
        (!v.trim().is_empty()).then_some(v)
    }

    fn set(&self, account: &str, value: &str) -> anyhow::Result<()> {
        use anyhow::Context;
        keyring::Entry::new(SERVICE, account)
            .context("opening keychain entry")?
            .set_password(value.trim())
            .context("writing to the keychain")?;
        Ok(())
    }

    fn delete(&self, account: &str) {
        if let Ok(e) = keyring::Entry::new(SERVICE, account) {
            let _ = e.delete_credential();
        }
    }
}

#[derive(Default)]
pub struct MemSecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemSecrets {
    fn get(&self, account: &str) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).get(account).cloned()
    }

    fn set(&self, account: &str, value: &str) -> anyhow::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(account.to_owned(), value.trim().to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(account);
    }
}

/// Drops anything that looks like a key from text shown to the user or logged: provider error bodies
/// sometimes echo the request.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_owned();
    for s in secrets.iter().filter(|s| s.len() >= 8) {
        out = out.replace(s.as_str(), "[key]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_secrets_round_trip_and_redact() {
        let m = MemSecrets::default();
        m.set("anthropic", " sk-ant-123456789 ").unwrap();
        assert_eq!(m.get("anthropic").as_deref(), Some("sk-ant-123456789"));
        assert_eq!(redact("bad key sk-ant-123456789 given", &["sk-ant-123456789".into()]), "bad key [key] given");
        m.delete("anthropic");
        assert!(!m.has("anthropic"));
    }
}
