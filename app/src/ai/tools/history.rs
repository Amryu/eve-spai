//! Lookbacks into what the app has kept: a system's traffic and owners over time, kills anywhere in
//! EVE, the user's own travels and the locals they looked up.

use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&SYSTEM_HISTORY, &KILLS_ANYWHERE, &MY_MOVES, &LOCAL_SCANS];

fn days(v: &Value, default: u64) -> u64 {
    u64_arg(v, "days", default, 3650)
}

/// Alliance names for ids, from what the app knows and, when online, from ESI.
fn alliance_names(ctx: &Ctx, ids: &[i64]) -> HashMap<i64, String> {
    let mut out: HashMap<i64, String> = HashMap::new();
    // Sov holders are named on the map already; the rest come from ESI when online.
    {
        let st = ctx.deps.system_status.lock().unwrap_or_else(|e| e.into_inner());
        for f in st.values() {
            if let (Some(id), Some(n)) = (f.sov_alliance, f.sov.as_ref()) {
                if ids.contains(&id) {
                    out.insert(id, n.clone());
                }
            }
        }
    }
    let rest: Vec<i64> = ids.iter().copied().filter(|i| !out.contains_key(i)).collect();
    if ctx.deps.online && !rest.is_empty() {
        out.extend(crate::universe::lookup_names(&rest));
    }
    out
}

static SYSTEM_HISTORY: ToolSpec = ToolSpec {
    name: "system_history",
    description: "How a system has been over the past days, from the saved history: ship and pod kills, NPC kills and \
                  jumps per day, its busiest hours (EVE time), who held its sov and when that changed, and how many kills \
                  there looked like a gate camp. For 'is this system usually busy', 'is it camped', 'who owns it lately'.",
    need: Need::All(&["map.status"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}, "days": {"type": "integer", "minimum": 1, "maximum": 365}}), &["system"]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let id = ctx.system(str_arg(v, "system").unwrap_or_default())?;
        let d = days(v, 7);
        let since = ctx.now - d as i64 * 86_400;
        let hours = store.system_stats_history(id, since / 3600);
        let mut per_day: BTreeMap<String, [u32; 4]> = BTreeMap::new();
        let mut by_hour = [[0u32; 2]; 24];
        for (h, s) in &hours {
            let day = chrono::DateTime::from_timestamp(h * 3600, 0).map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default();
            let e = per_day.entry(day).or_default();
            e[0] += s.ship_kills as u32;
            e[1] += s.pod_kills as u32;
            e[2] += s.npc_kills as u32;
            e[3] += s.jumps as u32;
            let hod = (h % 24) as usize;
            by_hour[hod][0] += s.ship_kills as u32;
            by_hour[hod][1] += s.jumps as u32;
        }
        let mut busiest: Vec<(usize, u32)> = by_hour.iter().enumerate().map(|(h, v)| (h, v[1])).filter(|(_, j)| *j > 0).collect();
        busiest.sort_by_key(|(_, j)| std::cmp::Reverse(*j));
        let mut deadliest: Vec<(usize, u32)> = by_hour.iter().enumerate().map(|(h, v)| (h, v[0])).filter(|(_, k)| *k > 0).collect();
        deadliest.sort_by_key(|(_, k)| std::cmp::Reverse(*k));
        let sov = store.sov_history(id);
        let sov_ids: Vec<i64> = sov.iter().filter_map(|(_, a)| *a).collect();
        let kills = store.kill_history(since, &[id], None, 5000);
        let names = alliance_names(ctx, &sov_ids);
        let name = |a: Option<i64>| a.map(|a| names.get(&a).cloned().unwrap_or_else(|| format!("alliance {a}"))).unwrap_or_else(|| "nobody".into());
        let mut out = json!({
            "system": ctx.system_name(id),
            "days": d,
            "hours_recorded": hours.len(),
            "per_day": per_day.iter().rev().take(30).map(|(day, v)| json!({"day": day, "ship_kills": v[0], "pod_kills": v[1], "npc_kills": v[2], "jumps": v[3]})).collect::<Vec<_>>(),
            "busiest_hours_by_jumps": busiest.iter().take(4).map(|(h, j)| format!("{h:02}:00 ({j} jumps)")).collect::<Vec<_>>(),
            "deadliest_hours": deadliest.iter().take(4).map(|(h, k)| format!("{h:02}:00 ({k} kills)")).collect::<Vec<_>>(),
            "sov_changes": sov.iter().rev().take(10).map(|(t, a)| json!({"when": eve_time(*t), "holder": name(*a)})).collect::<Vec<_>>(),
            "kills_logged": kills.len(),
            "kills_like_a_gate_camp": kills.iter().filter(|k| k.on_gate && (k.camp_gear || k.attackers >= 3)).count(),
        });
        if hours.is_empty() && kills.is_empty() {
            out["note"] = json!("nothing saved for this span; the history only covers what the app saw while it ran");
        }
        Ok(out)
    },
};

static KILLS_ANYWHERE: ToolSpec = ToolSpec {
    name: "kills_anywhere",
    description: "Every kill in EVE the app saw, from the saved history, not just those near the user: by system or region, \
                  by an alliance as victim or attacker, over days. Gives totals, the systems and alliances most involved, \
                  the ships the attackers flew, and the latest kills. For 'where is this alliance active', 'what does \
                  Fraternity fly this week', 'what died in this region'.",
    need: Need::All(&["kills.history"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "system": {"type": "string"},
                "within_jumps": {"type": "integer", "minimum": 0, "maximum": 10},
                "region": {"type": "string"},
                "alliance": {"type": "string", "description": "Name or shorthand"},
                "days": {"type": "integer", "minimum": 1, "maximum": 365},
                "limit": {"type": "integer", "minimum": 1, "maximum": 40},
                "offset": {"type": "integer", "minimum": 0, "description": "Skip this many of the latest kills, to page"}
            }),
            &[],
        )
    },
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let since = ctx.now - days(v, 1) as i64 * 86_400;
        let geo = ctx.geo()?;
        let mut systems: Vec<i64> = Vec::new();
        if let Some(n) = str_arg(v, "system") {
            let id = ctx.system(n)?;
            systems.extend(geo.distances_from(id, u64_arg(v, "within_jumps", 0, 10) as u32).into_keys());
        }
        if let Some(r) = str_arg(v, "region") {
            let r = r.trim().to_lowercase();
            systems.extend(geo.all_ids().filter(|id| geo.info_of(*id).is_some_and(|i| i.region.to_lowercase() == r)));
            if systems.is_empty() {
                return Err(format!("no region called {r}"));
            }
        }
        let alliance = match str_arg(v, "alliance") {
            Some(a) => Some(super::intel::alliance_of(ctx, a).ok_or_else(|| format!("no alliance called {a}"))?),
            None => None,
        };
        let kills = store.kill_history(since, &systems, alliance.as_ref().map(|a| a.0), 20_000);
        let ships = super::intel::ship_names(store);
        let mut by_sys: HashMap<i64, usize> = HashMap::new();
        let mut by_ally: HashMap<i64, usize> = HashMap::new();
        let mut isk = 0.0;
        for k in &kills {
            *by_sys.entry(k.system_id).or_default() += 1;
            for a in &k.attacker_alliances {
                *by_ally.entry(*a).or_default() += 1;
            }
            isk += k.value;
        }
        let mut top_sys: Vec<_> = by_sys.into_iter().collect();
        top_sys.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let mut hulls: HashMap<i64, usize> = HashMap::new();
        for k in &kills {
            for s in &k.attacker_ships {
                *hulls.entry(*s).or_default() += 1;
            }
        }
        let mut top_hulls: Vec<_> = hulls.into_iter().collect();
        top_hulls.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let mut top_ally: Vec<_> = by_ally.into_iter().collect();
        top_ally.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let ally_ids: Vec<i64> = top_ally.iter().take(8).map(|(a, _)| *a).collect();
        let names = alliance_names(ctx, &ally_ids);
        let limit = u64_arg(v, "limit", 15, 40) as usize;
        let offset = v.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        Ok(json!({
            "alliance": alliance.map(|a| a.1),
            "kills": kills.len(),
            "isk_billions": (isk / 1e8).round() / 10.0,
            "top_systems": top_sys.iter().take(8).map(|(s, n)| json!({"system": ctx.system_name(*s), "kills": n})).collect::<Vec<_>>(),
            "attacker_ships": top_hulls.iter().take(12).map(|(s, n)| json!({"ship": ships.get(s).cloned().unwrap_or_else(|| format!("type {s}")), "kills": n})).collect::<Vec<_>>(),
            "top_attacking_alliances": top_ally.iter().take(8).map(|(a, n)| json!({"alliance": names.get(a).cloned().unwrap_or_else(|| format!("alliance {a}")), "kills": n})).collect::<Vec<_>>(),
            "next_offset": (offset + limit < kills.len()).then_some(offset + limit),
            "latest": kills.iter().skip(offset).take(limit).map(|k| json!({
                "kill_id": k.kill_id, "when": eve_time(k.time), "age": fmt_age(ctx.now, k.time), "system": ctx.system_name(k.system_id),
                "ship": ships.get(&k.ship_type_id).cloned().unwrap_or_else(|| format!("type {}", k.ship_type_id)),
                "isk_millions": (k.value / 1e6).round(), "attackers": k.attackers, "on_gate": k.on_gate,
            })).collect::<Vec<_>>(),
        }))
    },
};

static MY_MOVES: ToolSpec = ToolSpec {
    name: "my_moves",
    description: "Where the user's own characters have been, newest first: each system change with the ship and whether \
                  they docked. For 'where was I yesterday', 'where did I leave my Rorqual'.",
    need: Need::All(&["characters.locations"]),
    kind: Kind::Read,
    schema: || schema(json!({"character": {"type": "string"}, "days": {"type": "integer", "minimum": 1, "maximum": 365}, "limit": {"type": "integer", "minimum": 1, "maximum": 100}}), &[]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let since = ctx.now - days(v, 7) as i64 * 86_400;
        let ships = super::intel::ship_names(store);
        let moves = store.move_history(since, str_arg(v, "character"), u64_arg(v, "limit", 40, 100) as usize);
        Ok(json!({"moves": moves.iter().map(|m| json!({
            "when": eve_time(m.time), "character": m.character, "from": ctx.system_name(m.from), "to": ctx.system_name(m.to),
            "ship": m.ship.map(|s| ships.get(&s).cloned().unwrap_or_else(|| format!("type {s}"))), "docked": m.docked,
        })).collect::<Vec<_>>()}))
    },
};

static LOCAL_SCANS: ToolSpec = ToolSpec {
    name: "local_scan_history",
    description: "The locals the user looked up before, newest first: when, where they were, and who was in local. \
                  Filter by a pilot to see where and when they were seen.",
    need: Need::All(&["pilots.localscan"]),
    kind: Kind::Read,
    schema: || schema(json!({"pilot": {"type": "string"}, "days": {"type": "integer", "minimum": 1, "maximum": 365}, "limit": {"type": "integer", "minimum": 1, "maximum": 30}}), &[]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let since = ctx.now - days(v, 30) as i64 * 86_400;
        let want = str_arg(v, "pilot").map(|p| p.trim().to_lowercase());
        let mut out = Vec::new();
        for (t, sys, json) in store.local_scan_history(since, 500) {
            let Ok(saved) = serde_json::from_str::<crate::localscan::SavedLookup>(&json) else { continue };
            if let Some(w) = &want {
                if !saved.names.iter().any(|n| n.to_lowercase() == *w) {
                    continue;
                }
            }
            out.push(json!({
                "when": eve_time(t), "age": fmt_age(ctx.now, t), "system": sys.map(|s| ctx.system_name(s)),
                "pilots": saved.names.len(), "names": saved.names.iter().take(60).collect::<Vec<_>>(),
            }));
            if out.len() >= u64_arg(v, "limit", 10, 30) as usize {
                break;
            }
        }
        Ok(json!({"scans": out}))
    },
};

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use crate::store::history::{HourStats, KillRow, MoveRow};
    use serde_json::json;

    #[test]
    fn a_systems_past_its_kills_anywhere_and_the_users_moves_come_from_the_history() {
        let store = crate::store::Store::mem();
        let now = crate::clock::utc().timestamp();
        let id = 30_004_759;
        store.log_system_stats(now / 3600 - 2, &[HourStats { system_id: id, ship_kills: 4, jumps: 120, ..Default::default() }]);
        store.log_system_stats(now / 3600 - 1, &[HourStats { system_id: id, ship_kills: 1, jumps: 30, ..Default::default() }]);
        store.log_kill(&KillRow { kill_id: 9, time: now - 600, system_id: id, ship_type_id: 587, attackers: 6, on_gate: true, attacker_alliances: vec![1354830081], ..Default::default() });
        store.log_move(&MoveRow { time: now - 60, character: "Me".into(), from: 30_003_704, to: id, ship: None, docked: true });
        let deps = AiDeps::for_tests(facts(&["map.status", "kills.history", "characters.locations"]));
        let facts = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = super::super::Ctx { deps: &deps, facts: &facts, store: Some(&store), now, actions: &mut actions };
        let mut call = |name: &str, args: serde_json::Value| {
            let (t, err) = super::super::dispatch(&mut ctx, name, &args);
            assert!(!err, "{name}: {t}");
            serde_json::from_str::<serde_json::Value>(&t).unwrap()["untrusted_data"].clone()
        };
        let sys = call("system_history", json!({"system": "1DQ1-A", "days": 2}));
        assert_eq!(sys["hours_recorded"], 2);
        assert_eq!(sys["kills_like_a_gate_camp"], 1);
        assert!(sys["busiest_hours_by_jumps"][0].as_str().unwrap().contains("120 jumps"));
        let k = call("kills_anywhere", json!({"system": "1DQ1-A", "days": 1}));
        assert_eq!(k["kills"], 1);
        assert_eq!(k["latest"][0]["kill_id"], 9);
        let m = call("my_moves", json!({}));
        assert_eq!(m["moves"][0]["to"], "1DQ1-A");
    }

    #[test]
    fn intel_older_than_the_live_hour_comes_from_the_history() {
        let store = crate::store::Store::mem();
        let now = crate::clock::utc().timestamp();
        let old = crate::intel::IntelReport { received: now - 86_400, channel: "Delve.Imperium".into(), reporter: "Scout".into(), text: "frat gang +20 QX-LIJ".into(), ..Default::default() };
        store.replace_intel_window(0, &[old]);
        let deps = AiDeps::for_tests(facts(&["intel.reports"]));
        let facts = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = super::super::Ctx { deps: &deps, facts: &facts, store: Some(&store), now, actions: &mut actions };
        let (t, err) = super::super::dispatch(&mut ctx, "search_intel", &json!({"query": "frat", "days": 2}));
        assert!(!err, "{t}");
        assert!(t.contains("frat gang +20"), "{t}");
        let (t, _) = super::super::dispatch(&mut ctx, "search_intel", &json!({"query": "frat"}));
        assert!(!t.contains("frat gang +20"), "two hours back by default: {t}");
    }
}
