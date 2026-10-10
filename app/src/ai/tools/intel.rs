//! Intel: parsed reports, raw channel logs, and following a group across both and the kill feed.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&SEARCH_INTEL, &CHAT_LOG, &TRACK];

static SEARCH_INTEL: ToolSpec = ToolSpec {
    name: "search_intel",
    description: "Intel reports from the user's intel channels, newest first: text, reporter, channel, systems, ships, \
                  pilots, hostile count and severity. Filter by words in the text, by a system and a jump radius, and by age.",
    need: Need::All(&["intel.reports"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "query": {"type": "string", "description": "Words to find in the report, its pilots, ships or channel"},
                "system": {"type": "string"},
                "within_jumps": {"type": "integer", "minimum": 0, "maximum": 15},
                "kinds": {"type": "array", "items": {"type": "string", "enum": KINDS}, "description": "Only reports of any of these kinds"},
                "since_minutes": {"type": "integer", "minimum": 1, "maximum": 1440},
                "days": {"type": "integer", "minimum": 1, "maximum": 3650, "description": "Look back this many days instead, into the saved history"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 60}
            }),
            &[],
        )
    },
    run: search_intel,
};

const KINDS: [&str; 13] = ["cyno", "bubble", "camp", "tackle", "capital_tackled", "dropper", "wormhole", "spike", "help", "clear", "ess", "skyhook", "structure"];

/// The kinds a report is, by the names in [`KINDS`].
fn kinds_of(r: &crate::intel::IntelReport) -> Vec<&'static str> {
    [
        (r.cyno, "cyno"),
        (r.bubble, "bubble"),
        (r.camp, "camp"),
        (r.tackled, "tackle"),
        (r.cap_tackled, "capital_tackled"),
        (r.dropper, "dropper"),
        (r.wormhole, "wormhole"),
        (r.spike, "spike"),
        (r.help, "help"),
        (r.clear, "clear"),
        (r.ess, "ess"),
        (r.skyhook, "skyhook"),
        (!r.structures.is_empty(), "structure"),
    ]
    .into_iter()
    .filter_map(|(on, k)| on.then_some(k))
    .collect()
}

fn report_json(ctx: &Ctx, r: &crate::intel::IntelReport, jumps: Option<u32>) -> Value {
    let sev = crate::app::severity_of(r, &ctx.facts.severity);
    let mut v = json!({
        "when": eve_time(r.received),
        "age": fmt_age(ctx.now, r.received),
        "channel": r.channel,
        "reporter": r.reporter,
        "text": r.text,
        "systems": r.systems.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "ships": r.ships.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "pilots": r.pilots,
        "count": r.count,
        "severity": format!("{sev:?}"),
        "clear": r.clear,
    });
    if !r.alliances.is_empty() {
        v["alliances"] = json!(r.alliances.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
    }
    let kinds = kinds_of(r);
    if !kinds.is_empty() {
        v["kinds"] = json!(kinds);
    }
    if !r.gates.is_empty() {
        v["gates"] = json!(r.gates);
    }
    if let Some((c, d)) = &r.near_celestial {
        v["near"] = json!(format!("{c} ({d:.0} km)"));
    }
    if let Some(m) = &r.movement {
        v["came_from"] = json!({"system": m.from, "jumps": m.jumps});
    }
    if !r.tackled_targets.is_empty() {
        v["tackled"] = json!(r.tackled_targets);
    }
    if !r.structures.is_empty() {
        v["structures"] = json!(r.structures.iter().map(|(a, b)| b.as_ref().map_or(a.clone(), |b| format!("{a} {b}"))).collect::<Vec<_>>());
    }
    if r.wormhole {
        v["wormhole"] = json!({"type": r.wh_type, "signature": r.wh_sig, "end_of_life": r.wh_eol, "drifter": r.wh_drifter});
    }
    if let Some(i) = r.isk {
        v["isk"] = json!(i);
    }
    if !r.classes.is_empty() {
        v["classes"] = json!(r.classes);
    }
    if r.no_visual {
        v["no_visual"] = json!(true);
    }
    if let Some(j) = jumps {
        v["jumps_from_filter_system"] = json!(j);
    }
    v
}

fn search_intel(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let q = str_arg(v, "query").map(str::to_lowercase);
    let since = since_of(ctx, v, 120, 1440);
    let limit = u64_arg(v, "limit", 25, 60) as usize;
    let around = match str_arg(v, "system") {
        Some(n) => {
            let id = ctx.system(n)?;
            Some(ctx.geo()?.distances_from(id, u64_arg(v, "within_jumps", 0, 15) as u32))
        }
        None => None,
    };
    let area: Vec<i64> = around.as_ref().map(|d| d.keys().copied().take(400).collect()).unwrap_or_default();
    let reports = intel_since(ctx, since, q.as_deref(), &area);
    let want_kinds: Vec<String> = v.get("kinds").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    let mut out = Vec::new();
    for r in &reports {
        if !want_kinds.is_empty() && !kinds_of(r).iter().any(|k| want_kinds.iter().any(|w| w == k)) {
            continue;
        }
        let mut jumps = None;
        if let Some(d) = &around {
            match r.systems.iter().filter_map(|s| d.get(&s.id)).min() {
                Some(j) => jumps = Some(*j),
                None => continue,
            }
        }
        if let Some(q) = &q {
            let hay = format!(
                "{} {} {} {} {} {}",
                r.text,
                r.channel,
                r.pilots.join(" "),
                r.ships.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" "),
                r.systems.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" "),
                r.alliances.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>().join(" ")
            )
            .to_lowercase();
            if !q.split_whitespace().all(|w| hay.contains(w)) {
                continue;
            }
        }
        out.push(report_json(ctx, r, jumps));
        if out.len() >= limit {
            break;
        }
    }
    Ok(json!({"count": out.len(), "reports": out}))
}

static CHAT_LOG: ToolSpec = ToolSpec {
    name: "chat_log",
    description: "Raw lines from one of the user's EVE chat channels, as written in game, for the last minutes. Use when the \
                  parsed intel reports miss something. Only channels the user allowed can be read.",
    need: Need::AnyUnder("intel.chatlogs"),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "channel": {"type": "string"},
                "since_minutes": {"type": "integer", "minimum": 1, "maximum": 720},
                "grep": {"type": "string", "description": "Only lines containing this, case-insensitive"}
            }),
            &["channel"],
        )
    },
    run: chat_log,
};

fn chat_log(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let channel = str_arg(v, "channel").unwrap_or_default().to_owned();
    if !ctx.allowed(&crate::ai::perms::channel_key(&channel)) {
        return Err(format!("the user has not allowed reading the {channel} channel"));
    }
    let dir = ctx.facts.chat_dir.clone().ok_or("the EVE chat log folder is not set")?;
    let since = ctx.now - 60 * u64_arg(v, "since_minutes", 30, 720) as i64;
    let cutoff = chrono::DateTime::from_timestamp(since, 0).map(|d| d.format("%Y.%m.%d %H:%M:%S").to_string()).unwrap_or_default();
    let grep = str_arg(v, "grep").map(str::to_lowercase);
    let prefix = format!("{channel}_").to_lowercase();
    let mut lines: Vec<(String, String, String)> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.starts_with(&prefix) || !name.ends_with(".txt") {
            continue;
        }
        let fresh = entry.metadata().ok().and_then(|m| m.modified().ok()).is_some_and(|t| {
            t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64 >= since).unwrap_or(false)
        });
        if !fresh {
            continue;
        }
        if let Some((_, msgs)) = crate::chatlog::read(&entry.path()) {
            for m in msgs {
                if m.timestamp.as_str() >= cutoff.as_str() && grep.as_ref().is_none_or(|g| m.text.to_lowercase().contains(g)) {
                    lines.push((m.timestamp, m.author, m.text));
                }
            }
        }
    }
    // Several characters in the channel log the same lines.
    lines.sort();
    lines.dedup();
    let total = lines.len();
    let tail: Vec<Value> = lines.into_iter().rev().take(150).rev().map(|(t, a, x)| json!(format!("[{t}] {a} > {x}"))).collect();
    Ok(json!({"channel": channel, "lines": total, "log": tail}))
}

static TRACK: ToolSpec = ToolSpec {
    name: "track_movement",
    description: "Follows a group or pilot (alliance, corporation, shorthand like 'frat', or a pilot name) through the kill \
                  feed and intel reports: every sighting in time order with the system, what happened and the jumps from \
                  the sighting before, then the last place seen. The fastest way to answer 'where did they go'. Uses whichever \
                  of kills and intel the user allowed.",
    need: Need::Any(&["kills.feed", "kills.history", "intel.reports"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "entity": {"type": "string", "description": "Alliance, corporation or pilot name, or a common shorthand"},
                "since_minutes": {"type": "integer", "minimum": 5, "maximum": 2880},
                "days": {"type": "integer", "minimum": 1, "maximum": 3650, "description": "Look back this many days instead"}
            }),
            &["entity"],
        )
    },
    run: track,
};

/// What a name matches: the name itself, plus the full alliance name for a known shorthand.
fn matchers(entity: &str) -> Vec<String> {
    let mut m = vec![entity.trim().to_lowercase()];
    if let Some((name, _)) = crate::alliances::lookup(entity.trim()) {
        m.push(name.to_lowercase());
    }
    m
}

fn hit(name: &str, m: &[String]) -> bool {
    let n = name.to_lowercase();
    !n.is_empty() && m.iter().any(|w| n == *w || (w.len() >= 4 && n.contains(w.as_str())))
}

fn track(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let entity = str_arg(v, "entity").ok_or("which group or pilot?")?.to_owned();
    let m = matchers(&entity);
    let since = since_of(ctx, v, 120, 2880);
    // (time, system, what)
    let mut seen: Vec<(i64, i64, String)> = Vec::new();
    if ctx.allowed("kills.feed") || ctx.allowed("kills.history") {
        if let Some(store) = ctx.store {
            let names = ship_names(store);
            for e in store.load_engagements(since) {
                let ship = |id: i64| names.get(&id).cloned().unwrap_or_else(|| format!("type {id}"));
                let lost = hit(&e.victim.name, &m) || hit(&e.victim_pilot, &m);
                let ours: Vec<&br_core::battle::Attacker> = e.attackers.iter().filter(|a| hit(&a.party.name, &m) || hit(&a.pilot, &m)).collect();
                if lost {
                    seen.push((e.time, e.system_id, format!("lost a {} ({}, {})", ship(e.victim_ship), e.victim_pilot, e.victim.name)));
                }
                if !ours.is_empty() {
                    let mut hulls: Vec<String> = ours.iter().map(|a| ship(a.ship)).collect();
                    hulls.sort();
                    hulls.dedup();
                    seen.push((
                        e.time,
                        e.system_id,
                        format!("{} of them killed a {} ({}); ships: {}", ours.len(), ship(e.victim_ship), e.victim.name, hulls.join(", ")),
                    ));
                }
            }
        }
    }
    if ctx.allowed("kills.history") {
        if let (Some(store), Some((aid, _))) = (ctx.store, alliance_of(ctx, &entity)) {
            let names = ship_names(store);
            let known: std::collections::HashSet<i64> = store.load_engagements(since).iter().map(|e| e.kill_id).collect();
            for k in store.kill_history(since, &[], Some(aid), 400).into_iter().filter(|k| !known.contains(&k.kill_id)) {
                let ship = names.get(&k.ship_type_id).cloned().unwrap_or_else(|| format!("type {}", k.ship_type_id));
                let what = if k.victim_alliance == Some(aid) { format!("lost a {ship}") } else { format!("killed a {ship} ({} attackers)", k.attackers) };
                seen.push((k.time, k.system_id, what));
            }
        }
    }
    if ctx.allowed("intel.reports") {
        // Matched on the entity's own words in the store, then exactly here.
        let word = m.iter().max_by_key(|w| w.len()).cloned().unwrap_or_default();
        let reports = intel_since(ctx, since, Some(&word), &[]);
        let reports = if reports.is_empty() { intel_since(ctx, since, None, &[]) } else { reports };
        for r in reports.iter().filter(|r| !r.clear) {
            let named = r.pilots.iter().any(|p| hit(p, &m)) || r.alliances.iter().any(|(a, _)| hit(a, &m)) || m.iter().any(|w| w.len() >= 3 && r.text.to_lowercase().contains(w.as_str()));
            if let (true, Some(s)) = (named, r.primary_system()) {
                seen.push((r.received, s.id, format!("intel in {}: \"{}\"", r.channel, r.text)));
            }
        }
    }
    seen.sort_by_key(|(t, _, _)| *t);
    let geo = ctx.geo()?;
    let mut trail = Vec::new();
    let mut prev: Option<i64> = None;
    for (t, sys, what) in &seen {
        let jumps = prev.and_then(|p| geo.jumps(p, *sys, 40));
        trail.push(json!({
            "when": eve_time(*t),
            "age": fmt_age(ctx.now, *t),
            "system": ctx.system_name(*sys),
            "region": geo.info_of(*sys).map(|i| i.region.clone()),
            "jumps_from_previous": jumps,
            "what": what,
        }));
        prev = Some(*sys);
    }
    let last = seen.last().map(|(t, s, _)| (*t, *s));
    let mut out = json!({
        "entity": entity,
        "matched_as": m,
        "sightings": trail.len(),
        "trail": trail.into_iter().rev().take(60).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>(),
    });
    if let Some((t, s)) = last {
        out["last_seen"] = json!({"system": ctx.system_name(s), "age": fmt_age(ctx.now, t)});
        if ctx.allowed("map.jove") {
            let mut jove: Vec<(i64, u32)> = geo.distances_from(s, 6).into_iter().filter(|(x, _)| crate::jove::has(*x)).collect();
            jove.sort_by_key(|(_, j)| *j);
            out["jove_observatories_near_last_seen"] =
                json!(jove.iter().take(5).map(|(x, j)| json!({"system": ctx.system_name(*x), "jumps": j})).collect::<Vec<_>>());
        }
    }
    if seen.is_empty() {
        out["note"] = json!("nothing found; try a longer since_minutes, the full name, or a member pilot");
    }
    Ok(out)
}

/// Where a lookback starts: `days` when given, else `since_minutes` (default and cap in minutes).
fn since_of(ctx: &Ctx, v: &Value, default_min: u64, max_min: u64) -> i64 {
    match v.get("days").and_then(Value::as_u64) {
        Some(d) => ctx.now - d.clamp(1, 3650) as i64 * 86_400,
        None => ctx.now - 60 * u64_arg(v, "since_minutes", default_min, max_min) as i64,
    }
}

/// Intel since `since`, newest first: the live hour from memory, older from the saved history,
/// narrowed there to reports holding every word of `words`.
pub(crate) fn intel_since(ctx: &Ctx, since: i64, words: Option<&str>, systems: &[i64]) -> Vec<crate::intel::IntelReport> {
    let mut out: Vec<crate::intel::IntelReport> = {
        let st = ctx.deps.intel_state.lock().unwrap_or_else(|e| e.into_inner());
        st.reports.iter().rev().filter(|r| r.received >= since).take(2000).cloned().collect()
    };
    let live_from = out.iter().map(|r| r.received).min().unwrap_or(ctx.now).min(ctx.now - 3600);
    if since < live_from {
        if let Some(store) = ctx.store {
            out.extend(store.intel_history(since, live_from - 1, systems, None, words, 3000));
        }
    }
    out
}

/// An alliance by shorthand or full name, as (id, name).
pub(crate) fn alliance_of(ctx: &Ctx, name: &str) -> Option<(i64, String)> {
    if let Some((n, id)) = crate::alliances::lookup(name.trim()) {
        return Some((id, n.to_owned()));
    }
    if !ctx.deps.online {
        return None;
    }
    crate::http::client(10).ok().and_then(|c| crate::universe::alliance(&c, name))
}

pub(crate) fn ship_names(store: &crate::store::Store) -> std::collections::HashMap<i64, String> {
    store.ship_index().into_iter().map(|(lc, (id, _))| (id, lc)).collect()
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use crate::ai::deps::AiDeps;

    #[test]
    fn intel_is_found_by_words_and_by_distance() {
        let deps = AiDeps::for_tests(facts(&["intel.reports"]));
        let now = crate::clock::utc().timestamp();
        for mut r in [crate::uitest::fixtures::intel_typical(), crate::uitest::fixtures::intel_next_door()] {
            r.received = now - 60;
            deps.intel_state.lock().unwrap().reports.push(r);
        }
        let (all, err) = run(&deps, "search_intel", json!({"since_minutes": 1440}));
        assert!(!err, "{all}");
        let n = all["count"].as_u64().unwrap();
        assert!(n > 0, "{all}");
        let (none, _) = run(&deps, "search_intel", json!({"query": "zzzz-not-there", "since_minutes": 1440}));
        assert_eq!(none["count"], 0);
    }

    #[test]
    fn shorthand_names_match_the_full_alliance() {
        let m = matchers("frat");
        assert!(hit("Fraternity.", &m));
        assert!(!hit("Fraternal Order Holdings", &matchers("xyz")));
        assert!(hit("Some Pilot", &matchers("some pilot")));
    }
}
