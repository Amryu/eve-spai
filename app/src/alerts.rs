//! Alert evaluation (docs/DESIGN.md §7.1 E9).
//!
//! M1 subset: alert when a non-clear sighting appears within N jumps of the active
//! character. Pure rule evaluation here; the app handles cooldown + firing the
//! desktop notification.

use crate::geo::Systems;
use crate::intel::IntelReport;

#[derive(Clone, Copy)]
pub struct AlertConfig {
    pub enabled: bool,
    /// Alert on hostiles within this many jumps of you (0 = off).
    pub within_jumps: u32,
}

/// Returns notification text if `report` should raise an alert.
pub fn evaluate(
    report: &IntelReport,
    player_sys: Option<i64>,
    systems: Option<&Systems>,
    cfg: &AlertConfig,
) -> Option<String> {
    if !cfg.enabled || cfg.within_jumps == 0 || report.clear {
        return None;
    }
    let sys = report.primary_system()?;
    let player = player_sys?;
    let systems = systems?;
    let jumps = systems.jumps(sys.id, player, cfg.within_jumps)?;

    let count = report.count.map(|n| format!("{n}x ")).unwrap_or_default();
    Some(if jumps == 0 {
        format!("Hostiles in your system — {count}{}", sys.name)
    } else {
        format!("Hostiles {jumps}j — {count}{}", sys.name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{SystemInfo, Systems};
    use crate::intel::{DetectedSystem, IntelReport};
    use std::collections::HashMap;

    fn systems() -> Systems {
        // 1 - 2 - 3 line.
        let by_name: HashMap<String, SystemInfo> = [1i64, 2, 3]
            .map(|id| {
                (
                    format!("s{id}"),
                    SystemInfo {
                        id,
                        name: format!("S{id}"),
                        security: 0.0,
                        constellation: String::new(),
                        region: String::new(),
                        faction: String::new(),
                    },
                )
            })
            .into_iter()
            .collect();
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for (a, b) in [(1, 2), (2, 3)] {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        Systems::new(by_name, adj)
    }

    fn report(system_id: i64, clear: bool) -> IntelReport {
        IntelReport {
            received: 0,
            channel: "c".into(),
            reporter: "r".into(),
            text: "t".into(),
            systems: vec![DetectedSystem {
                id: system_id,
                name: format!("S{system_id}"),
                security: 0.0,
            }],
            count: Some(3),
            clear,
            no_visual: false,
            spike: false,
            camp: false,
            bubble: false,
            killmail: false,
            gate: None,
            movement: None,
        }
    }

    #[test]
    fn alerts_within_range_only() {
        let s = systems();
        let cfg = AlertConfig { enabled: true, within_jumps: 3 };
        // player in system 1; sighting in 3 = 2 jumps -> alert.
        assert!(evaluate(&report(3, false), Some(1), Some(&s), &cfg).is_some());
        // clear sighting never alerts.
        assert!(evaluate(&report(3, true), Some(1), Some(&s), &cfg).is_none());
        // disabled / no player / out of range.
        let off = AlertConfig { enabled: false, within_jumps: 3 };
        assert!(evaluate(&report(3, false), Some(1), Some(&s), &off).is_none());
        assert!(evaluate(&report(3, false), None, Some(&s), &cfg).is_none());
        let near = AlertConfig { enabled: true, within_jumps: 1 };
        assert!(evaluate(&report(3, false), Some(1), Some(&s), &near).is_none());
    }
}
