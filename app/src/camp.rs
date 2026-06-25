//! Gate-camp detection from the live zKillboard feed.
//!
//! Heuristic: a system is "camped" when several kills land there inside a short rolling
//! window with at least one very recent. We track kill timestamps per system (fed by the
//! RedisQ feed in `zkill`) and derive the flag on demand. System-level clustering is a
//! proxy for an actual gate camp — good enough for a travel-warning + map cue.

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

/// Camp-relevant type-id sets resolved from the SDE: interdictors + HICs, smartbombs, and
/// anchorable warp-disruption bubbles. Used to flag "camp equipment" on a kill.
#[derive(Default, Clone)]
pub struct CampTypes {
    pub dic_hic: std::collections::HashSet<i64>,
    pub smartbomb: std::collections::HashSet<i64>,
    pub bubble: std::collections::HashSet<i64>,
}

#[derive(Default)]
struct SysKills {
    /// Kill timestamps (pruned/capped on insert).
    times: Vec<i64>,
    /// Timestamps of kills that landed on a stargate.
    on_gate: Vec<i64>,
    /// Most recent kill involving camp equipment (dic/hic/smartbomb/bubble).
    last_equip: i64,
}

#[derive(Default)]
pub struct CampState {
    kills: std::collections::HashMap<i64, SysKills>,
}

impl CampState {
    /// Record a kill in `system` at `time`. `on_gate`: it landed near a stargate. `equip`: an
    /// interdictor/HIC/smartbomb/anchorable-bubble was involved.
    pub fn record(&mut self, system: i64, time: i64, on_gate: bool, equip: bool) {
        let e = self.kills.entry(system).or_default();
        e.times.push(time);
        if e.times.len() > CAP {
            let drop = e.times.len() - CAP;
            e.times.drain(0..drop);
        }
        if on_gate {
            e.on_gate.push(time);
            if e.on_gate.len() > CAP {
                let drop = e.on_gate.len() - CAP;
                e.on_gate.drain(0..drop);
            }
        }
        if equip {
            e.last_equip = e.last_equip.max(time);
        }
    }

    /// Camp status for a system, or None if it doesn't currently qualify.
    pub fn camp(&self, system: i64, now: i64) -> Option<Camp> {
        let e = self.kills.get(&system)?;
        let recent: Vec<i64> = e.times.iter().copied().filter(|&t| now - t <= WINDOW).collect();
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
        let on_gate = e.on_gate.iter().filter(|&&t| now - t <= WINDOW).count();
        let has_equip = e.last_equip > 0 && now - e.last_equip <= WINDOW;

        // Base read from clustering + span; on-gate kills and camp equipment push it up — a
        // gate camp is kills *on the gate*, usually with bubbles/dics over a longer span.
        let mut level = if kills >= 6 || (kills >= 3 && span >= SUSTAINED) {
            CampLevel::Likely
        } else if kills >= 3 {
            CampLevel::Possible
        } else {
            CampLevel::Flag
        };
        if on_gate >= 1 {
            if has_equip || on_gate >= 2 || span >= SUSTAINED {
                level = CampLevel::Likely;
            } else {
                level = level.max(CampLevel::Possible);
            }
        } else if has_equip {
            level = level.max(CampLevel::Possible);
        }
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
        s.record(30000142, now - 100, false, false);
        assert!(s.camp(30000142, now).is_none());
        // Two off-gate kills: a flag, not yet a camp.
        s.record(30000142, now - 80, false, false);
        assert_eq!(s.camp(30000142, now).unwrap().level, CampLevel::Flag);
        // A third in a tight burst (<8min span): a possible camp.
        s.record(30000142, now - 60, false, false);
        let c = s.camp(30000142, now).unwrap();
        assert_eq!(c.kills, 3);
        assert_eq!(c.level, CampLevel::Possible);
        // A kill 20 minutes back makes the span long → a likely camp.
        s.record(30000142, now - 20 * 60, false, false);
        assert_eq!(s.camp(30000142, now).unwrap().level, CampLevel::Likely);
        // Far in the future: the last kill is stale, not a camp.
        assert!(s.camp(30000142, now + 3600).is_none());

        // On-gate kills with camp equipment read as a likely camp with only a couple kills.
        let mut g = CampState::default();
        g.record(30000144, now - 100, true, true);
        g.record(30000144, now - 60, true, false);
        assert_eq!(g.camp(30000144, now).unwrap().level, CampLevel::Likely);
    }
}
