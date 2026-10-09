//! A short picture of the moment, sent with every question so the first answer needs no lookups:
//! where the user's characters are, what intel and kills are near them, and the time. Only what
//! the user allowed goes in.

use std::fmt::Write;

use super::deps::{AiDeps, AiFacts};

pub fn summary(deps: &AiDeps, facts: &AiFacts, jumps: u32, now: i64) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "Now: {} EVE time (UTC).", super::tools::eve_time(now));
    let scopes: Vec<&str> = [
        ("intel.reports", "intel reports"),
        ("intel.chatlogs", "chat logs"),
        ("kills.feed", "kill feed"),
        ("kills.history", "kill history"),
        ("wormholes", "wormholes"),
        ("map.status", "sov and activity"),
        ("battles", "battle reports"),
        ("characters.locations", "character locations"),
        ("internet", "the web"),
    ]
    .iter()
    .filter(|(k, _)| facts.allowed(k))
    .map(|(_, l)| *l)
    .collect();
    let _ = writeln!(s, "You may read: static game data{}{}.", if scopes.is_empty() { "" } else { ", " }, scopes.join(", "));
    let Some(geo) = facts.systems.as_ref() else { return s };
    let mut homes: Vec<(String, i64)> = Vec::new();
    if facts.allowed("characters.locations") {
        let p = deps.player.lock().unwrap_or_else(|e| e.into_inner());
        let mut locs: Vec<(&String, &(i64, bool))> = p.locations.iter().collect();
        locs.sort_by_key(|(n, _)| (**n != p.active_name, (*n).clone()));
        for (name, (sys, docked)) in locs.into_iter().take(6) {
            let sys_name = geo.info_of(*sys).map(|i| i.name.clone()).unwrap_or_default();
            let _ = writeln!(s, "Character {name}{}: {sys_name}{}.", if *name == p.active_name { " (active)" } else { "" }, if *docked { ", docked" } else { "" });
            homes.push((name.clone(), *sys));
        }
    }
    if homes.is_empty() {
        return s;
    }
    let near = |sys: i64| homes.iter().filter_map(|(_, h)| geo.jumps(*h, sys, jumps)).min();
    if facts.allowed("intel.reports") {
        let st = deps.intel_state.lock().unwrap_or_else(|e| e.into_inner());
        let mut lines: Vec<(i64, String)> = Vec::new();
        for r in st.reports.iter().rev().filter(|r| now - r.received <= 900 && !r.clear) {
            if let Some((sys, j)) = r.systems.iter().find_map(|x| near(x.id).map(|j| (x.name.clone(), j))) {
                lines.push((r.received, format!("{} ({j}j, {}): {}", sys, super::tools::fmt_age(now, r.received), r.text.chars().take(120).collect::<String>())));
            }
            if lines.len() >= 8 {
                break;
            }
        }
        if !lines.is_empty() {
            let _ = writeln!(s, "Recent intel within {jumps} jumps:");
            for (_, l) in lines {
                let _ = writeln!(s, "- {l}");
            }
        }
    }
    if facts.allowed("kills.feed") {
        let feed = deps.killfeed.lock().unwrap_or_else(|e| e.into_inner());
        let mut by_sys: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
        for k in feed.iter().filter(|k| now - k.time <= 1800) {
            if let Some(j) = near(k.system_id) {
                let e = by_sys.entry(geo.info_of(k.system_id).map(|i| i.name.clone()).unwrap_or_default()).or_insert((0, j));
                e.0 += 1;
            }
        }
        if !by_sys.is_empty() {
            let mut v: Vec<_> = by_sys.into_iter().collect();
            v.sort_by_key(|(_, (n, j))| (*j, std::cmp::Reverse(*n)));
            let list: Vec<String> = v.iter().take(8).map(|(name, (n, j))| format!("{name} {n} ({j}j)")).collect();
            let _ = writeln!(s, "Kills in the last 30 minutes nearby: {}.", list.join(", "));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tools::testkit::facts;

    #[test]
    fn only_allowed_data_goes_in() {
        let deps = AiDeps::for_tests(facts(&[]));
        deps.player.lock().unwrap().locations.insert("Kestrel Vane".into(), (30_004_759, false));
        deps.player.lock().unwrap().active_name = "Kestrel Vane".into();
        let s = summary(&deps, &deps.facts(), 5, 1_000_000);
        assert!(!s.contains("Kestrel"), "locations were not allowed: {s}");
        let deps2 = AiDeps { facts: std::sync::Arc::new(std::sync::Mutex::new(facts(&["characters.locations"]))), ..deps };
        let s = summary(&deps2, &deps2.facts(), 5, 1_000_000);
        assert!(s.contains("Character Kestrel Vane (active): 1DQ1-A."), "{s}");
    }
}
