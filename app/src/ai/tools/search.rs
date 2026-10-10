//! Searching the big piles: every battle the app can build from its stored kills, and the whole
//! Jabber history. Each answer says how many matched and pages through the rest, and can count
//! the matches by a field instead of listing them.

use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&SEARCH_BATTLES, &JABBER_SEARCH];

/// How long a clustering of the stored kills is reused before it is built again.
const CACHE_SECS: i64 = 300;

/// Every battle since `since`: the live ones and those clustered from the stored kills, as the
/// Battles tab builds its history (same window, reach, quiet gap and the user's edits).
pub(crate) fn all_battles(ctx: &Ctx, since: i64) -> Vec<br_core::battle::Battle> {
    let mut out: Vec<br_core::battle::Battle> = ctx.deps.battles.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (Some(store), Some(geo)) = (ctx.store, ctx.facts.systems.clone()) else { return out };
    let cached = {
        let c = ctx.deps.battle_cache.lock().unwrap_or_else(|e| e.into_inner());
        c.as_ref().filter(|(at, from, _)| ctx.now - at < CACHE_SECS && *from <= since).map(|(_, _, b)| b.clone())
    };
    let history = match cached {
        Some(b) => b,
        None => {
            let engs = store.load_engagements(since);
            let overrides = store.load_battle_overrides();
            let gap = if ctx.facts.battle_break_secs > 0 { ctx.facts.battle_break_secs } else { br_core::battle::BATTLE_WINDOW_SECS };
            let built: Vec<br_core::battle::Battle> = br_core::battle::cluster(
                &engs,
                br_core::battle::BATTLE_WINDOW_SECS,
                br_core::battle::BATTLE_MAX_JUMPS,
                gap,
                &overrides,
                |a, b| geo.jumps(a, b, br_core::battle::BATTLE_MAX_JUMPS),
            )
            .into_iter()
            .filter(|b| b.is_anchored() && b.is_two_sided())
            .collect();
            *ctx.deps.battle_cache.lock().unwrap_or_else(|e| e.into_inner()) = Some((ctx.now, since, built.clone()));
            built
        }
    };
    // The live list wins for a battle in both: it may hold kills the store has not caught up on.
    let live: std::collections::HashSet<i64> = out.iter().flat_map(|b| b.engagements.iter().map(|e| e.kill_id)).collect();
    out.extend(history.into_iter().filter(|b| !b.engagements.iter().any(|e| live.contains(&e.kill_id))));
    out.retain(|b| b.end >= since);
    out
}

fn battle_id(b: &br_core::battle::Battle) -> Option<i64> {
    b.engagements.iter().map(|e| e.kill_id).max()
}

static SEARCH_BATTLES: ToolSpec = ToolSpec {
    name: "search_battles",
    description: "Searches every battle the app knows, the live ones and those built from the stored kills, as the Battles tab \
                  shows them: by words (a group, pilot, system or ship), system or region, size and time. Sort by recent, \
                  kills or ISK, page with offset, or count the matches by region, system, group or day instead of listing them. \
                  Each battle has a battle_id for battle_detail.",
    need: Need::All(&["battles"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "query": {"type": "string", "description": "Every word must match a group, pilot, system or ship in the battle"},
                "system": {"type": "string"},
                "region": {"type": "string"},
                "days": {"type": "integer", "minimum": 1, "maximum": 3650},
                "min_kills": {"type": "integer", "minimum": 1},
                "min_isk_billions": {"type": "number"},
                "sort": {"type": "string", "enum": ["recent", "kills", "isk"]},
                "group_by": {"type": "string", "enum": ["region", "system", "group", "day"]},
                "offset": {"type": "integer", "minimum": 0},
                "limit": {"type": "integer", "minimum": 1, "maximum": 30}
            }),
            &[],
        )
    },
    run: search_battles,
};

fn search_battles(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let since = ctx.now - u64_arg(v, "days", 7, 3650) as i64 * 86_400;
    let geo = ctx.geo()?;
    let names = ctx.store.map(super::intel::ship_names).unwrap_or_default();
    let words: Vec<String> = str_arg(v, "query").unwrap_or_default().to_lowercase().split_whitespace().map(str::to_owned).collect();
    let system = match str_arg(v, "system") {
        Some(s) => Some(ctx.system(s)?),
        None => None,
    };
    let region = str_arg(v, "region").map(|r| r.trim().to_lowercase());
    let min_kills = v.get("min_kills").and_then(Value::as_u64).unwrap_or(0) as usize;
    let min_isk = v.get("min_isk_billions").and_then(Value::as_f64).unwrap_or(0.0) * 1e9;
    let region_of = |b: &br_core::battle::Battle| b.systems.first().and_then(|(id, _, _)| geo.info_of(*id)).map(|i| i.region.clone()).unwrap_or_default();
    let mut hits: Vec<br_core::battle::Battle> = all_battles(ctx, since)
        .into_iter()
        .filter(|b| b.kills >= min_kills && b.isk >= min_isk)
        .filter(|b| system.is_none_or(|s| b.systems.iter().any(|(id, _, _)| *id == s)))
        .filter(|b| region.as_ref().is_none_or(|r| region_of(b).to_lowercase() == *r))
        .filter(|b| {
            if words.is_empty() {
                return true;
            }
            let mut hay = String::new();
            for s in &b.sides {
                for p in &s.parties {
                    hay.push_str(&p.name);
                    hay.push(' ');
                }
            }
            for (_, n, _) in &b.systems {
                hay.push_str(n);
                hay.push(' ');
            }
            for e in &b.engagements {
                hay.push_str(&e.victim_pilot);
                hay.push(' ');
                if let Some(n) = names.get(&e.victim_ship) {
                    hay.push_str(n);
                    hay.push(' ');
                }
                for a in &e.attackers {
                    hay.push_str(&a.pilot);
                    hay.push(' ');
                }
            }
            let hay = hay.to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    match str_arg(v, "sort").unwrap_or("recent") {
        "kills" => hits.sort_by_key(|b| std::cmp::Reverse(b.kills)),
        "isk" => hits.sort_by(|a, b| b.isk.partial_cmp(&a.isk).unwrap_or(std::cmp::Ordering::Equal)),
        _ => hits.sort_by_key(|b| std::cmp::Reverse(b.end)),
    }
    let total = hits.len();
    if let Some(by) = str_arg(v, "group_by") {
        let mut counts: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
        for b in &hits {
            let keys: Vec<String> = match by {
                "system" => b.systems.iter().map(|(_, n, _)| n.clone()).collect(),
                "group" => b.sides.iter().flat_map(|s| s.parties.iter().map(|p| p.name.clone())).collect(),
                "day" => vec![eve_time(b.start).split(' ').take(2).collect::<Vec<_>>().join(" ")],
                _ => vec![region_of(b)],
            };
            for k in keys {
                let e = counts.entry(k).or_default();
                e.0 += 1;
                e.1 += b.kills;
                e.2 += b.isk;
            }
        }
        let mut rows: Vec<_> = counts.into_iter().collect();
        rows.sort_by_key(|(_, (n, _, _))| std::cmp::Reverse(*n));
        return Ok(json!({
            "matching": total,
            "by": by,
            "counts": rows.iter().take(40).map(|(k, (n, kills, isk))| json!({"key": k, "battles": n, "kills": kills, "isk_billions": (isk / 1e8).round() / 10.0})).collect::<Vec<_>>(),
        }));
    }
    let offset = v.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = u64_arg(v, "limit", 10, 30) as usize;
    let page: Vec<Value> = hits
        .iter()
        .skip(offset)
        .take(limit)
        .map(|b| {
            json!({
                "battle_id": battle_id(b),
                "when": eve_time(b.start),
                "ended": fmt_age(ctx.now, b.end),
                "systems": b.systems.iter().map(|(_, n, _)| n.clone()).collect::<Vec<_>>(),
                "region": region_of(b),
                "kills": b.kills,
                "isk_billions": (b.isk / 1e8).round() / 10.0,
                "sides": b.sides.iter().map(|s| json!({
                    "groups": s.parties.iter().take(6).map(|p| p.name.clone()).collect::<Vec<_>>(),
                    "losses": s.losses,
                    "isk_lost_billions": (s.isk_lost / 1e8).round() / 10.0,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let next = (offset + page.len() < total).then_some(offset + page.len());
    Ok(json!({"matching": total, "offset": offset, "next_offset": next, "battles": page}))
}

static JABBER_SEARCH: ToolSpec = ToolSpec {
    name: "jabber_search",
    description: "Searches the whole stored Jabber history, rooms and direct messages: messages holding every word, in one \
                  conversation if named, over days; pages with offset, or counts the matches by conversation or sender.",
    need: Need::All(&["jabber.chats"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "query": {"type": "string"},
                "conversation": {"type": "string", "description": "Part of a room's or person's address"},
                "days": {"type": "integer", "minimum": 1, "maximum": 3650},
                "group_by": {"type": "string", "enum": ["conversation", "sender"]},
                "offset": {"type": "integer", "minimum": 0},
                "limit": {"type": "integer", "minimum": 1, "maximum": 60}
            }),
            &[],
        )
    },
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let words: Vec<String> = str_arg(v, "query").unwrap_or_default().split_whitespace().map(str::to_owned).collect();
        let since = ctx.now - u64_arg(v, "days", 30, 3650) as i64 * 86_400;
        let conv = str_arg(v, "conversation").map(|c| super::actions::jabber_norm(c).replace('_', "%"));
        let local = |j: &str| j.split('@').next().unwrap_or(j).to_owned();
        if let Some(by) = str_arg(v, "group_by") {
            let (total, rows) = store.search_chats(&words, conv.as_deref(), since, 0, 20_000);
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for (jid, sender, _, _) in &rows {
                *counts.entry(if by == "sender" { sender.clone() } else { local(jid) }).or_default() += 1;
            }
            let mut rows: Vec<_> = counts.into_iter().collect();
            rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            return Ok(json!({"matching": total, "by": by, "counts": rows.iter().take(40).map(|(k, n)| json!({"key": k, "messages": n})).collect::<Vec<_>>()}));
        }
        let offset = v.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let limit = u64_arg(v, "limit", 25, 60) as usize;
        let (total, rows) = store.search_chats(&words, conv.as_deref(), since, offset, limit);
        let next = (offset + rows.len() < total).then_some(offset + rows.len());
        Ok(json!({
            "matching": total,
            "offset": offset,
            "next_offset": next,
            "messages": rows.iter().map(|(jid, from, body, t)| json!({"in": local(jid), "from": from, "when": eve_time(*t), "text": body})).collect::<Vec<_>>(),
        }))
    },
};

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    #[test]
    fn stored_kills_become_battles_to_search_count_and_open() {
        use br_core::battle::{Attacker, Engagement, Party, PartyKind};
        let store = crate::store::Store::mem();
        let now = crate::clock::utc().timestamp();
        let party = |id: i64, n: &str| Party { id, name: n.into(), kind: PartyKind::Alliance };
        for i in 0..6i64 {
            let (v, a) = if i % 2 == 0 { (party(1, "Fraternity."), party(2, "Goonswarm Federation")) } else { (party(2, "Goonswarm Federation"), party(1, "Fraternity.")) };
            store.save_engagement(&Engagement {
                kill_id: 500 + i,
                time: now - 3 * 86_400 + i * 60,
                system_id: 30_004_759,
                system_name: "1DQ1-A".into(),
                security: -0.4,
                victim: v,
                victim_char: 900 + i,
                victim_pilot: format!("Pilot {i}"),
                victim_ship: 12015,
                attackers: vec![Attacker { party: a, char_id: 800 + i, ship: 22456, pilot: format!("Shooter {i}"), final_blow: true, damage: 1000 }],
                isk: 2e8,
                anchored: true,
            });
        }
        let deps = AiDeps::for_tests(facts(&["battles"]));
        let f = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = super::super::Ctx { deps: &deps, facts: &f, store: Some(&store), now, actions: &mut actions };
        let (t, err) = super::super::dispatch(&mut ctx, "search_battles", &json!({"query": "fraternity", "days": 7}));
        assert!(!err, "{t}");
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["untrusted_data"]["matching"], 1, "{t}");
        let id = v["untrusted_data"]["battles"][0]["battle_id"].as_i64().unwrap();
        let (t, _) = super::super::dispatch(&mut ctx, "search_battles", &json!({"days": 7, "group_by": "region"}));
        assert!(t.contains("\"battles\":1"), "{t}");
        let (t, err) = super::super::dispatch(&mut ctx, "battle_detail", &json!({"battle_id": id}));
        assert!(!err && t.contains("Goonswarm"), "the battle opens from the stored history: {t}");
        let (t, _) = super::super::dispatch(&mut ctx, "search_battles", &json!({"query": "nobody here", "days": 7}));
        assert!(t.contains("\"matching\":0"), "{t}");
    }

    #[test]
    fn jabber_history_is_searched_paged_and_counted() {
        let store = crate::store::Store::mem();
        let now = crate::clock::utc().timestamp();
        for i in 0..30 {
            store.add_chat("ops@conference.example.invalid", if i % 2 == 0 { "FC One" } else { "Scout" }, &format!("frat gang {i} in QX-LIJ"), now - i * 60, false);
        }
        store.add_chat("someone@example.invalid", "Someone", "unrelated", now, false);
        let deps = AiDeps::for_tests(facts(&["jabber.chats"]));
        let f = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = super::super::Ctx { deps: &deps, facts: &f, store: Some(&store), now, actions: &mut actions };
        let (t, err) = super::super::dispatch(&mut ctx, "jabber_search", &json!({"query": "frat", "limit": 10}));
        assert!(!err, "{t}");
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["untrusted_data"]["matching"], 30);
        assert_eq!(v["untrusted_data"]["next_offset"], 10);
        let (t, _) = super::super::dispatch(&mut ctx, "jabber_search", &json!({"query": "frat", "group_by": "sender"}));
        assert!(t.contains("FC One") && t.contains("\"messages\":15"), "{t}");
        assert!(deps.opsec.load(std::sync::atomic::Ordering::Relaxed), "reading Jabber closes the web for this chat");
    }
}
