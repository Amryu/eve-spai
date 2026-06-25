//! Gate-camp detection from the live zKillboard feed.
//!
//! Heuristic: a system is "camped" when several kills land there inside a short rolling
//! window with at least one very recent. We track kill timestamps per system (fed by the
//! RedisQ feed in `zkill`) and derive the flag on demand. System-level clustering is a
//! proxy for an actual gate camp — good enough for a travel-warning + map cue.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Keep kills for this long (seconds).
const WINDOW: i64 = 35 * 60;
/// A camp counts as active only if a kill landed within this many seconds.
const FRESH: i64 = 12 * 60;
/// Minimum kills in the window to call it a camp.
const MIN_KILLS: usize = 3;
/// Cap stored timestamps per system so memory stays bounded.
const CAP: usize = 64;

#[derive(Clone, Copy)]
pub struct Camp {
    /// Kills in the rolling window.
    pub kills: usize,
    /// Seconds since the most recent kill.
    pub age: i64,
}

#[derive(Default)]
pub struct CampState {
    /// system id -> kill unix timestamps (ascending-ish; pruned/capped on insert).
    kills: HashMap<i64, Vec<i64>>,
}

impl CampState {
    /// Record a kill in `system` at `time` (unix seconds).
    pub fn record(&mut self, system: i64, time: i64) {
        let v = self.kills.entry(system).or_default();
        v.push(time);
        if v.len() > CAP {
            let drop = v.len() - CAP;
            v.drain(0..drop);
        }
    }

    /// Camp status for a system, or None if it doesn't currently qualify.
    pub fn camp(&self, system: i64, now: i64) -> Option<Camp> {
        let v = self.kills.get(&system)?;
        let kills = v.iter().filter(|&&t| now - t <= WINDOW).count();
        if kills < MIN_KILLS {
            return None;
        }
        let last = *v.iter().max()?;
        let age = now - last;
        if age > FRESH {
            return None;
        }
        Some(Camp { kills, age })
    }

    /// All currently-camped systems.
    pub fn camped(&self, now: i64) -> Vec<i64> {
        self.kills
            .keys()
            .copied()
            .filter(|&s| self.camp(s, now).is_some())
            .collect()
    }
}

pub type SharedCamps = Arc<Mutex<CampState>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_expires() {
        let mut s = CampState::default();
        let now = 1_000_000;
        // Two kills: not a camp yet.
        s.record(30000142, now - 600);
        s.record(30000142, now - 300);
        assert!(s.camp(30000142, now).is_none());
        // Third recent kill: camp.
        s.record(30000142, now - 60);
        let c = s.camp(30000142, now).unwrap();
        assert_eq!(c.kills, 3);
        // Far in the future: the last kill is stale, not a camp.
        assert!(s.camp(30000142, now + 3600).is_none());
    }
}
