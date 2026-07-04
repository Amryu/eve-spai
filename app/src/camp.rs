use std::sync::{Arc, Mutex};

const WINDOW: i64 = 35 * 60;
const FRESH: i64 = 12 * 60;
const MIN_KILLS: usize = 2;
const SUSTAINED: i64 = 8 * 60;
const CAP: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum CampLevel {
    Flag,
    Possible,
    Likely,
}

#[derive(Clone, Copy)]
pub struct Camp {
    pub level: CampLevel,
    pub kills: usize,
    pub age: i64,
    pub span: i64,
}

#[derive(Default, Clone)]
pub struct CampTypes {
    pub dic_hic: std::collections::HashSet<i64>,
    pub smartbomb: std::collections::HashSet<i64>,
    pub bubble: std::collections::HashSet<i64>,
}

#[derive(Default)]
struct SysKills {
    times: Vec<i64>,
    on_gate: Vec<i64>,
    last_equip: i64,
}

#[derive(Default)]
pub struct CampState {
    kills: std::collections::HashMap<i64, SysKills>,
}

impl CampState {
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
        s.record(30000142, now - 100, false, false);
        assert!(s.camp(30000142, now).is_none());
        s.record(30000142, now - 80, false, false);
        assert_eq!(s.camp(30000142, now).unwrap().level, CampLevel::Flag);
        s.record(30000142, now - 60, false, false);
        let c = s.camp(30000142, now).unwrap();
        assert_eq!(c.kills, 3);
        assert_eq!(c.level, CampLevel::Possible);
        s.record(30000142, now - 20 * 60, false, false);
        assert_eq!(s.camp(30000142, now).unwrap().level, CampLevel::Likely);
        assert!(s.camp(30000142, now + 3600).is_none());

        let mut g = CampState::default();
        g.record(30000144, now - 100, true, true);
        g.record(30000144, now - 60, true, false);
        assert_eq!(g.camp(30000144, now).unwrap().level, CampLevel::Likely);
    }
}
