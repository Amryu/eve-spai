//! Kills from zKillboard as the app stored them, and the battles it built from them.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&RECENT, &HISTORY, &BATTLES];

fn kill_schema(max_minutes: u64) -> Value {
    schema(
        json!({
            "system": {"type": "string", "description": "Only kills in or near this system"},
            "within_jumps": {"type": "integer", "minimum": 0, "maximum": 15},
            "region": {"type": "string"},
            "entity": {"type": "string", "description": "Only kills involving this alliance, corporation or pilot"},
            "since_minutes": {"type": "integer", "minimum": 1, "maximum": max_minutes},
            "limit": {"type": "integer", "minimum": 1, "maximum": 80}
        }),
        &[],
    )
}

static RECENT: ToolSpec = ToolSpec {
    name: "recent_kills",
    description: "Kills from the last few hours: when, where, the victim's ship, pilot and group, the ISK, and who killed it \
                  (groups and ships). Filter by system and radius, region, or a group or pilot taking part.",
    need: Need::All(&["kills.feed"]),
    kind: Kind::Read,
    schema: || kill_schema(360),
    run: |ctx, v| kills(ctx, v, 60, 360),
};

static HISTORY: ToolSpec = ToolSpec {
    name: "kill_history",
    description: "Like recent_kills, over the last 30 days of stored kills. Narrow it with filters: it is a lot of data.",
    need: Need::All(&["kills.history"]),
    kind: Kind::Read,
    schema: || kill_schema(43_200),
    run: |ctx, v| kills(ctx, v, 1440, 43_200),
};

fn kills(ctx: &mut Ctx, v: &Value, default_min: u64, max_min: u64) -> Result<Value, String> {
    let store = ctx.store.ok_or("the database is not open")?;
    let since = ctx.now - 60 * u64_arg(v, "since_minutes", default_min, max_min) as i64;
    let limit = u64_arg(v, "limit", 30, 80) as usize;
    let around = match str_arg(v, "system") {
        Some(n) => Some(ctx.geo()?.distances_from(ctx.system(n)?, u64_arg(v, "within_jumps", 0, 15) as u32)),
        None => None,
    };
    let region = str_arg(v, "region").map(str::to_lowercase);
    let who = str_arg(v, "entity").map(|e| {
        let mut m = vec![e.to_lowercase()];
        if let Some((n, _)) = crate::alliances::lookup(e) {
            m.push(n.to_lowercase());
        }
        m
    });
    let names = super::intel::ship_names(store);
    let ship = |id: i64| names.get(&id).cloned().unwrap_or_else(|| format!("type {id}"));
    let geo = ctx.geo()?;
    let mut all = store.load_engagements(since);
    all.reverse();
    let total = all.len();
    let mut out = Vec::new();
    for e in &all {
        if let Some(d) = &around {
            if !d.contains_key(&e.system_id) {
                continue;
            }
        }
        if let Some(r) = &region {
            if geo.info_of(e.system_id).is_none_or(|i| i.region.to_lowercase() != *r) {
                continue;
            }
        }
        if let Some(m) = &who {
            let named = |n: &str| {
                let n = n.to_lowercase();
                m.iter().any(|w| n == *w || (w.len() >= 4 && n.contains(w.as_str())))
            };
            if !(named(&e.victim.name) || named(&e.victim_pilot) || e.attackers.iter().any(|a| named(&a.party.name) || named(&a.pilot))) {
                continue;
            }
        }
        let mut groups: Vec<(String, usize)> = Vec::new();
        for a in &e.attackers {
            match groups.iter_mut().find(|(n, _)| *n == a.party.name) {
                Some(g) => g.1 += 1,
                None => groups.push((a.party.name.clone(), 1)),
            }
        }
        groups.sort_by(|a, b| b.1.cmp(&a.1));
        let mut hulls: Vec<String> = e.attackers.iter().map(|a| ship(a.ship)).filter(|s| !s.starts_with("type 0")).collect();
        hulls.sort();
        hulls.dedup();
        out.push(json!({
            "when": eve_time(e.time),
            "age": fmt_age(ctx.now, e.time),
            "system": e.system_name,
            "victim": {"ship": ship(e.victim_ship), "pilot": e.victim_pilot, "group": e.victim.name},
            "isk_millions": (e.isk / 1e6).round(),
            "attackers": e.attackers.len(),
            "attacker_groups": groups.iter().take(4).map(|(n, c)| format!("{n} ({c})")).collect::<Vec<_>>(),
            "attacker_ships": hulls.into_iter().take(12).collect::<Vec<_>>(),
            "kill_id": e.kill_id,
        }));
        if out.len() >= limit {
            break;
        }
    }
    Ok(json!({"in_window": total, "shown": out.len(), "kills": out}))
}

static BATTLES: ToolSpec = ToolSpec {
    name: "battle_reports",
    description: "Battles the app has clustered from kills in the watched area: systems, time span, sides with their groups, \
                  kills, losses and ISK lost.",
    need: Need::All(&["battles"]),
    kind: Kind::Read,
    schema: || schema(json!({"limit": {"type": "integer", "minimum": 1, "maximum": 20}}), &[]),
    run: battles,
};

fn battles(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let limit = u64_arg(v, "limit", 8, 20) as usize;
    let list = ctx.deps.battles.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut list: Vec<&br_core::battle::Battle> = list.iter().collect::<Vec<_>>();
    list.sort_by_key(|b| std::cmp::Reverse(b.end));
    let out: Vec<Value> = list
        .iter()
        .take(limit)
        .map(|b| {
            json!({
                "battle_id": b.engagements.iter().map(|e| e.kill_id).max(),
                "from": eve_time(b.start),
                "to": eve_time(b.end),
                "ended": fmt_age(ctx.now, b.end),
                "systems": b.systems.iter().map(|(_, n, _)| n.clone()).collect::<Vec<_>>(),
                "kills": b.kills,
                "isk_billions": (b.isk / 1e8).round() / 10.0,
                "sides": b.sides.iter().map(|s| json!({
                    "coalition": s.coalition,
                    "groups": s.parties.iter().take(6).map(|p| p.name.clone()).collect::<Vec<_>>(),
                    "kills": s.kills,
                    "losses": s.losses,
                    "isk_lost_billions": (s.isk_lost / 1e8).round() / 10.0,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!(out))
}
