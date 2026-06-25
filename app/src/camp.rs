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
/// Minimum kills in the window to flag anything.
const MIN_KILLS: usize = 2;
/// Kills spanning at least this long read as a sustained camp rather than a passing burst.
const SUSTAINED: i64 = 8 * 60;
/// Cap stored timestamps per system so memory stays bounded.
const CAP: usize = 64;

/// How strongly a system looks like an actual gate camp (vs. a one-off or a roam passing
/// through). Drives the map icon/highlight colour and the travel warning.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum CampLevel {
    /// A kill or two recently — flagworthy, but could be a one-off.
    Flag,
    /// A burst of kills — possibly a camp, possibly a roam passing through.
    Possible,
    /// Sustained kills over a longer span (or many kills) — a real camp.
    Likely,
}

#[derive(Clone, Copy)]
pub struct Camp {
    /// How camp-like this looks.
    pub level: CampLevel,
    /// Kills in the rolling window.
    pub kills: usize,
    /// Seconds since the most recent kill.
    pub age: i64,
    /// Seconds between the first and last kill in the window (sustained vs. burst).
    pub span: i64,
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
        let recent: Vec<i64> = v.iter().copied().filter(|&t| now - t <= WINDOW).collect();
        let kills = recent.len();
        if kills < MIN_KILLS {
            return None;
        }
        let last = *recent.iter().max()?;
        let age = now - last;
        if age > FRESH {
            return None;
        }
        let span = last - *recent.iter().min()?;
        // Sustained over a longer span (or simply many kills) reads as a real camp; a tight
        // burst of a few is only "possible"; the bare minimum is a flag.
        let level = if kills >= 6 || (kills >= 3 && span >= SUSTAINED) {
            CampLevel::Likely
        } else if kills >= 3 {
            CampLevel::Possible
        } else {
            CampLevel::Flag
        };
        Some(Camp { level, kills, age, span })
    }

    /// All currently-flagged systems with their camp level.
    pub fn camped(&self, now: i64) -> Vec<(i64, CampLevel)> {
        self.kills
            .keys()
            .copied()
            .filter_map(|s| self.camp(s, now).map(|c| (s, c.level)))
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
        // One kill: nothing yet.
        s.record(30000142, now - 600);
        assert!(s.camp(30000142, now).is_none());
        // Two kills: a flag, not yet a camp.
        s.record(30000142, now - 300);
        assert_eq!(s.camp(30000142, now).unwrap().level, CampLevel::Flag);
        // Third recent kill in a tight span: a possible camp.
        s.record(30000142, now - 60);
        let c = s.camp(30000142, now).unwrap();
        assert_eq!(c.kills, 3);
        assert_eq!(c.level, CampLevel::Possible);
        // Kills sustained over a long span read as a likely camp.
        s.record(30000142, now - 20 * 60);
        assert_eq!(s.camp(30000142, now).unwrap().level, CampLevel::Likely);
        // Far in the future: the last kill is stale, not a camp.
        assert!(s.camp(30000142, now + 3600).is_none());
    }
}
