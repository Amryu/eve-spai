//! Searching the big piles: every battle the app can build from its stored kills, and the whole
//! Jabber history. Each answer says how many matched and pages through the rest, and can count
//! the matches by a field instead of listing them.

use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&SEARCH_BATTLES, &JABBER_SEARCH, &FIND_SYSTEMS, &FIND_SHIPS, &FIND_PILOTS];

/// One page of `items` from `offset`, with how many there are and where the next page starts.
fn page(items: Vec<Value>, v: &Value, default: u64, max: u64) -> Value {
    let total = items.len();
    let offset = v.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = u64_arg(v, "limit", default, max) as usize;
    let shown: Vec<Value> = items.into_iter().skip(offset).take(limit).collect();
    let next = (offset + shown.len() < total).then_some(offset + shown.len());
    json!({"matching": total, "offset": offset, "next_offset": next, "items": shown})
}

static FIND_SYSTEMS: ToolSpec = ToolSpec {
    name: "find_systems",
    description: "Finds solar systems by part of their name, region, constellation and security range, for when a name is \
                  only half known or a list is wanted (every lowsec system in a region). Paged.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "name": {"type": "string"}, "region": {"type": "string"}, "constellation": {"type": "string"},
                "min_security": {"type": "number"}, "max_security": {"type": "number"},
                "offset": {"type": "integer", "minimum": 0}, "limit": {"type": "integer", "minimum": 1, "maximum": 80}
            }),
            &[],
        )
    },
    run: |ctx, v| {
        let geo = ctx.geo()?;
        let low = |k: &str| str_arg(v, k).map(|x| x.trim().to_lowercase());
        let (name, region, cons) = (low("name"), low("region"), low("constellation"));
        let (lo, hi) = (v.get("min_security").and_then(Value::as_f64).unwrap_or(-1.0), v.get("max_security").and_then(Value::as_f64).unwrap_or(1.0));
        let mut hits: Vec<&crate::geo::SystemInfo> = geo
            .all_ids()
            .filter_map(|id| geo.info_of(id))
            .filter(|i| name.as_ref().is_none_or(|n| i.name.to_lowercase().contains(n.as_str())))
            .filter(|i| region.as_ref().is_none_or(|r| i.region.to_lowercase() == *r))
            .filter(|i| cons.as_ref().is_none_or(|c| i.constellation.to_lowercase() == *c))
            .filter(|i| (i.security * 10.0).round() / 10.0 >= lo && (i.security * 10.0).round() / 10.0 <= hi)
            .collect();
        hits.sort_by(|a, b| a.name.cmp(&b.name));
        let items: Vec<Value> = hits.iter().map(|i| json!({"name": i.name, "security": (i.security * 10.0).round() / 10.0, "constellation": i.constellation, "region": i.region})).collect();
        Ok(page(items, v, 30, 80))
    },
};

static FIND_SHIPS: ToolSpec = ToolSpec {
    name: "find_ships",
    description: "Finds ship types by part of their name or their class (Interdictor, Heavy Assault Cruiser...), for when a \
                  name is misheard or only half given, or to list a class. Paged; ship_info has the details.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"name": {"type": "string"}, "class": {"type": "string"}, "offset": {"type": "integer", "minimum": 0}, "limit": {"type": "integer", "minimum": 1, "maximum": 80}}), &[]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let name = str_arg(v, "name").map(|x| x.trim().to_lowercase());
        let class = str_arg(v, "class").map(|x| x.trim().to_lowercase());
        let names = super::intel::ship_names(store);
        let mut hits: Vec<(String, String)> = store
            .ship_index()
            .into_iter()
            .filter(|(n, (_, g))| name.as_ref().is_none_or(|w| n.contains(w.as_str())) && class.as_ref().is_none_or(|c| g.to_lowercase().contains(c.as_str())))
            .map(|(n, (id, g))| (names.get(&id).cloned().unwrap_or(n), g))
            .collect();
        hits.sort();
        hits.dedup();
        Ok(page(hits.into_iter().map(|(n, g)| json!({"ship": n, "class": g})).collect(), v, 30, 80))
    },
};

static FIND_PILOTS: ToolSpec = ToolSpec {
    name: "find_pilots",
    description: "Finds pilots the app has met by part of their name, for when a name is misspelled or half remembered; \
                  pilot_info has what is known of one. Paged.",
    need: Need::Any(&["pilots.lookup", "intel.reports"]),
    kind: Kind::Read,
    schema: || schema(json!({"name": {"type": "string"}, "offset": {"type": "integer", "minimum": 0}, "limit": {"type": "integer", "minimum": 1, "maximum": 80}}), &["name"]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let want = str_arg(v, "name").unwrap_or_default().trim().to_lowercase();
        if want.len() < 2 {
            return Err("give at least two letters".into());
        }
        let mut hits: Vec<String> = store.known_pilot_names().into_iter().map(|(n, _)| n).filter(|n| n.to_lowercase().contains(&want)).collect();
        // Closest first: a name starting with the words before one merely holding them.
        hits.sort_by_key(|n| (!n.to_lowercase().starts_with(&want), n.len()));
        Ok(page(hits.into_iter().map(|n| json!(n)).collect(), v, 30, 80))
    },
};

/// Every battle since `since`: the live ones and the Battles page's history. The history is the
/// page's own, built once from the stored kills; when the page has not built it yet the app is asked
/// to, and this waits a little for it.
pub(crate) fn all_battles(ctx: &Ctx, since: i64) -> Result<Vec<br_core::battle::Battle>, String> {
    use std::sync::atomic::Ordering;
    let mut out: Vec<br_core::battle::Battle> = ctx.deps.battles.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let empty = || ctx.deps.battle_history.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
    if empty() {
        if !ctx.deps.battle_history_loading.load(Ordering::SeqCst) {
            ctx.deps.want_battle_history.store(true, Ordering::SeqCst);
        }
        // A few seconds for the app to take the request up, then the page's own build time.
        let started = std::time::Instant::now();
        let secs = |s: u64| std::time::Duration::from_secs(s);
        while empty() {
            let picked_up = !ctx.deps.want_battle_history.load(Ordering::SeqCst);
            let loading = ctx.deps.battle_history_loading.load(Ordering::SeqCst);
            let waited = started.elapsed();
            if (!picked_up && waited >= secs(3)) || (picked_up && !loading) || waited >= secs(20) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        ctx.deps.want_battle_history.store(false, Ordering::SeqCst);
        if empty() && ctx.deps.battle_history_loading.load(Ordering::SeqCst) {
            return Err("the battle history is still being built; ask again in a moment".into());
        }
    }
    let history = ctx.deps.battle_history.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // The live list wins for a battle in both: it may hold kills the store has not caught up on.
    let live: std::collections::HashSet<i64> = out.iter().flat_map(|b| b.engagements.iter().map(|e| e.kill_id)).collect();
    out.extend(history.into_iter().filter(|b| !b.engagements.iter().any(|e| live.contains(&e.kill_id))));
    out.retain(|b| b.end >= since);
    Ok(out)
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
    let mut hits: Vec<br_core::battle::Battle> = all_battles(ctx, since)?
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
        // The Battles page's history, as it builds it from those kills.
        let geo = deps.facts().systems.clone().unwrap();
        let built: Vec<_> = br_core::battle::cluster(
            &store.load_engagements(0),
            br_core::battle::BATTLE_WINDOW_SECS,
            br_core::battle::BATTLE_MAX_JUMPS,
            br_core::battle::BATTLE_WINDOW_SECS,
            &store.load_battle_overrides(),
            |a, b| geo.jumps(a, b, br_core::battle::BATTLE_MAX_JUMPS),
        )
        .into_iter()
        .filter(|b| b.is_anchored() && b.is_two_sided())
        .collect();
        *deps.battle_history.lock().unwrap() = built;
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
    fn systems_and_pilots_are_found_by_part_of_a_name_and_paged() {
        let store = crate::store::Store::mem();
        for (n, id) in [("Xenuria Thrax", 1), ("Xenu Other", 2), ("Somebody Else", 3)] {
            store.add_known_pilot(n, id);
        }
        let deps = AiDeps::for_tests(facts(&["intel.reports"]));
        let f = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = super::super::Ctx { deps: &deps, facts: &f, store: Some(&store), now: 0, actions: &mut actions };
        let (t, err) = super::super::dispatch(&mut ctx, "find_systems", &json!({"name": "1dq"}));
        assert!(!err && t.contains("1DQ1-A"), "{t}");
        let (t, _) = super::super::dispatch(&mut ctx, "find_systems", &json!({"limit": 1}));
        assert!(t.contains("next_offset"), "a long list pages: {t}");
        let (t, err) = super::super::dispatch(&mut ctx, "find_pilots", &json!({"name": "xenu"}));
        assert!(!err && t.contains("Xenuria Thrax") && t.contains("Xenu Other") && !t.contains("Somebody"), "{t}");
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

