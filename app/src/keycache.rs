//! Short-lived answers to "is this secret in the keychain", for code that asks every frame.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an answer stands. Another process can change the keychain, so it is not kept for good.
const FRESH: Duration = Duration::from_secs(30);

pub struct KeyCache(Mutex<Option<HashMap<String, (bool, Instant)>>>);

impl KeyCache {
    pub const fn new() -> Self {
        KeyCache(Mutex::new(None))
    }

    pub fn get(&self, key: &str, read: impl FnOnce() -> bool) -> bool {
        if let Some(&(v, at)) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(key)) {
            if at.elapsed() < FRESH {
                return v;
            }
        }
        // Read outside the lock: it can block for as long as the Secret Service takes.
        let v = read();
        self.remember(key, v);
        v
    }

    pub fn remember(&self, key: &str, v: bool) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key.to_owned(), (v, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_kept_and_a_change_replaces_it() {
        let c = KeyCache::new();
        let mut reads = 0;
        assert!(c.get("a", || {
            reads += 1;
            true
        }));
        assert!(c.get("a", || {
            reads += 1;
            false
        }), "kept, not read again");
        assert_eq!(reads, 1);
        c.remember("a", false);
        assert!(!c.get("a", || true));
        assert!(!c.get("b", || false), "per key");
    }
}
