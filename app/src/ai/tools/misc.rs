//! The user's own side: characters, fleets, the rescue, Jabber, the local scan and notes.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] =
    &[&MY_CHARACTERS, &FLEETS_CURRENT, &FLEETS_HISTORY, &FLEET_DETAIL, &FLEET_PRESETS, &RESCUE, &RESCUE_HISTORY, &JABBER_PINGS, &JABBER_CHAT, &LOCALSCAN, &NOTES];

static MY_CHARACTERS: ToolSpec = ToolSpec {
    name: "my_characters",
    description: "Where the user's logged-in characters are, and whether each is docked. The active one is marked.",
    need: Need::All(&["characters.locations"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let (active, locs) = {
            let p = ctx.deps.player.lock().unwrap_or_else(|e| e.into_inner());
            (p.active_name.clone(), p.locations.clone())
        };
        let mut out: Vec<Value> = locs
            .iter()
            .map(|(name, (sys, docked))| json!({"name": name, "system": ctx.system_name(*sys), "docked": docked, "active": *name == active}))
            .collect();
        out.sort_by(|a, b| b["active"].as_bool().cmp(&a["active"].as_bool()));
        Ok(json!(out))
    },
};

fn rows(r: &[crate::fleets::model::FleetRow]) -> Value {
    json!(r.iter().take(30).map(|f| json!({
        "id": f.id.0, "name": f.name, "doctrine": f.setup_name, "operation": f.operation_name, "group": f.group_name,
        "commander": f.commander, "started_by": f.started_by, "started": f.started_at, "closed": f.closed_at,
    })).collect::<Vec<_>>())
}

static FLEETS_CURRENT: ToolSpec = ToolSpec {
    name: "fleets_current",
    description: "Fleets running now on the fleet dashboard (strategic and other), and the fleet the user has open.",
    need: Need::All(&["fleets.current"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let live = ctx.facts.fleet_backend.clone().filter(|_| ctx.deps.online);
        let fetched = live.as_ref().map(|b| (b.active(true).ok(), b.active(false).ok()));
        let st = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner());
        let (strat, other) = match &fetched {
            Some((s, o)) => (s.as_deref().map(rows), o.as_deref().map(rows)),
            None => (st.active_strat.value.as_deref().map(rows), st.active_pct.value.as_deref().map(rows)),
        };
        Ok(json!({
            "strategic": strat,
            "other": other,
            "open_fleet": st.open.value.as_ref().map(|o| {
                let mut v = composition(ctx, &o.composition, &o.report);
                v["id"] = json!(o.fleet.id.0);
                v["name"] = json!(o.fleet.name);
                v["started"] = json!(o.fleet.started_at);
                v["closed"] = json!(o.fleet.closed_at);
                v["doctrine"] = json!(o.doctrine.as_ref().map(|d| d.setup_name.clone()));
                v["off_doctrine"] = json!(st.off_doctrine.iter().take(30).map(|p| json!({"pilot": p.name, "ship": p.ship, "for": fmt_age(ctx.now, p.since)})).collect::<Vec<_>>());
                v["boosts"] = json!(st.boosts.iter().map(|b| json!({"what": b.what, "pilots": b.pilots})).collect::<Vec<_>>());
                v
            }),
        }))
    },
};

/// A fleet's members summed up: ships, groups, roles, and pilots with where they are.
fn composition(ctx: &Ctx, c: &crate::fleets::model::Composition, r: &crate::fleets::model::FleetReport) -> Value {
    let mut ships: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut groups: std::collections::BTreeMap<&str, usize> = Default::default();
    for m in c.members() {
        *ships.entry(m.ship_type_name.as_str()).or_default() += 1;
        *groups.entry(m.ship_group.as_str()).or_default() += 1;
    }
    let pilots: Vec<Value> = c
        .members()
        .take(80)
        .map(|m| json!({"pilot": m.name, "ship": m.ship_type_name, "role": m.role, "system": (m.solar_system_id > 0).then(|| ctx.system_name(m.solar_system_id)), "paps": (m.pap_count > 0).then_some(m.pap_count)}))
        .collect();
    let mut v = json!({"members": c.total(), "ships": ships, "groups": groups, "pilots": pilots});
    // A closed fleet has no live roster; its report still counts who flew what.
    if c.total() == 0 && r.total_characters > 0 {
        v["members"] = json!(r.total_characters);
        v["ships"] = json!(r.ship_counts.iter().map(|s| (s.ship_type_name.clone(), s.count)).collect::<std::collections::BTreeMap<_, _>>());
        v["groups"] = json!(r.group_counts.iter().map(|g| (g.group_name.clone(), g.count)).collect::<std::collections::BTreeMap<_, _>>());
        v["pilots"] = json!(r.characters.iter().take(80).map(|p| json!({"pilot": p.name, "paps": p.pap_count})).collect::<Vec<_>>());
    }
    v
}

static FLEETS_HISTORY: ToolSpec = ToolSpec {
    name: "fleets_history",
    description: "Past fleets from the fleet dashboard, newest first, 25 a page. Search matches the fleet's name, doctrine, \
                  operation or commander, as the dashboard's own search box does.",
    need: Need::All(&["fleets.history"]),
    kind: Kind::Read,
    schema: || schema(json!({"search": {"type": "string"}, "page": {"type": "integer", "minimum": 0, "maximum": 40}}), &[]),
    run: |ctx, v| {
        let search = str_arg(v, "search").unwrap_or_default();
        let page = u64_arg(v, "page", 0, 40) as u32;
        let Some(b) = ctx.facts.fleet_backend.clone().filter(|_| ctx.deps.online) else {
            let st = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner());
            return Ok(json!({"fleets": st.history.value.as_ref().map(|p| rows(&p.items)), "total": st.history.value.as_ref().map(|p| p.total), "note": "as last loaded in the Fleet tab"}));
        };
        let p = b.history(search, page * 25).map_err(|e| format!("the fleet dashboard answered: {e}"))?;
        Ok(json!({"fleets": rows(&p.items), "total": p.total, "page": page}))
    },
};

static FLEET_DETAIL: ToolSpec = ToolSpec {
    name: "fleet_detail",
    description: "One fleet from the dashboard, running or past, by its id from fleets_current or fleets_history: who \
                  commanded, the doctrine, and who flew what (and where they are, while it runs).",
    need: Need::Any(&["fleets.current", "fleets.history"]),
    kind: Kind::Read,
    schema: || schema(json!({"id": {"type": "string"}}), &["id"]),
    run: |ctx, v| {
        let id = crate::fleets::model::FleetId(str_arg(v, "id").ok_or("which fleet?")?.to_owned());
        let b = ctx.facts.fleet_backend.clone().filter(|_| ctx.deps.online).ok_or("the fleet dashboard is not signed in")?;
        let f = b.fleet(&id).map_err(|e| format!("the fleet dashboard answered: {e}"))?;
        let running = f.closed_at.is_none();
        if !ctx.facts.allowed(if running { "fleets.current" } else { "fleets.history" }) {
            return Err(format!("{} fleets are not allowed", if running { "running" } else { "past" }));
        }
        let report = b.report(&id).unwrap_or_default();
        let comp = if running { b.composition(&id).unwrap_or_default() } else { Default::default() };
        let mut out = composition(ctx, &comp, &report);
        out["name"] = json!(f.name);
        out["description"] = json!(f.description);
        out["operation"] = json!(f.operation_name);
        out["group"] = json!(f.group_name);
        out["commander"] = json!(f.commander.as_ref().map(|c| c.label.clone()));
        out["started"] = json!(f.started_at);
        out["closed"] = json!(f.closed_at);
        out["doctrine"] = json!(b.doctrine(&id).ok().flatten().map(|d| d.setup_name));
        Ok(out)
    },
};

static FLEET_PRESETS: ToolSpec = ToolSpec {
    name: "fleet_templates",
    description: "The fleet templates (presets) the user keeps for starting fleets.",
    need: Need::All(&["fleets.presets"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| Ok(serde_json::to_value(&ctx.facts.fleet_presets).unwrap_or_default()),
};

static RESCUE: ToolSpec = ToolSpec {
    name: "rescue_status",
    description: "The delve911 capital rescue: the ping being worked (capital, pilot, system, cyno) and recent pings.",
    need: Need::All(&["rescue"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let r = ctx.deps.rescue.lock().unwrap_or_else(|e| e.into_inner());
        let pings: Vec<Value> = r
            .recent_pings(10)
            .iter()
            .map(|e| json!({"age": fmt_age(ctx.now, e.received), "from": e.author, "text": e.raw, "system": e.system_name, "pilot": e.pilot, "class": e.cap_class.map(|c| format!("{c:?}"))}))
            .collect();
        Ok(json!({
            "selected": {
                "system": r.capital_system_name, "pilot": r.capital_pilot, "cyno": r.cyno_pilot,
                "class": r.cap_class.map(|c| format!("{c:?}")), "anomaly": r.anomaly,
            },
            "recent_pings": pings,
        }))
    },
};

static RESCUE_HISTORY: ToolSpec = ToolSpec {
    name: "rescue_history",
    description: "Past delve911 capital rescue pings, newest first, with what the FC did: coord pinged, invited, called \
                  into comms, resolved. Filter by a system, pilot or ship class word, and how many days back.",
    need: Need::All(&["rescue"]),
    kind: Kind::Read,
    schema: || schema(json!({"filter": {"type": "string"}, "days": {"type": "integer", "minimum": 1, "maximum": 365}, "limit": {"type": "integer", "minimum": 1, "maximum": 100}}), &[]),
    run: |ctx, v| {
        let f = str_arg(v, "filter").unwrap_or_default().to_lowercase();
        let since = ctx.now - u64_arg(v, "days", 30, 365) as i64 * 86_400;
        let n = u64_arg(v, "limit", 30, 100) as usize;
        let r = ctx.deps.rescue.lock().unwrap_or_else(|e| e.into_inner());
        let hits: Vec<&crate::rescue::RescueRecord> = r
            .history
            .iter()
            .rev()
            .filter(|h| h.received >= since)
            .filter(|h| {
                f.is_empty() || [Some(&h.raw), h.system.as_ref(), h.pilot.as_ref(), h.class.as_ref()].into_iter().flatten().any(|x| x.to_lowercase().contains(&f))
            })
            .collect();
        let mut by_system: std::collections::BTreeMap<&str, usize> = Default::default();
        for h in &hits {
            if let Some(s) = &h.system {
                *by_system.entry(s.as_str()).or_default() += 1;
            }
        }
        Ok(json!({
            "count": hits.len(),
            "by_system": by_system,
            "pings": hits.iter().take(n).map(|h| json!({
                "at": eve_time(h.received), "from": h.author, "text": h.raw, "system": h.system, "pilot": h.pilot, "class": h.class,
                "worked": h.worked, "coord_pinged": h.coord_pinged, "invited": h.invited, "comms": h.comms,
                "resolved_after": h.resolved_at.map(|t| fmt_age(t, h.received).replace(" ago", "")),
            })).collect::<Vec<_>>(),
        }))
    },
};

static JABBER_PINGS: ToolSpec = ToolSpec {
    name: "jabber_pings",
    description: "Recent fleet pings received over Jabber, newest first.",
    need: Need::All(&["jabber.pings"]),
    kind: Kind::Read,
    schema: || schema(json!({"limit": {"type": "integer", "minimum": 1, "maximum": 30}}), &[]),
    run: |ctx, v| {
        let n = u64_arg(v, "limit", 10, 30) as usize;
        let st = ctx.deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
        Ok(serde_json::to_value(st.pings.iter().rev().take(n).collect::<Vec<_>>()).unwrap_or_default())
    },
};

static JABBER_CHAT: ToolSpec = ToolSpec {
    name: "jabber_chat",
    description: "Recent messages of one Jabber conversation (a room or a person), by part of its address or name.",
    need: Need::All(&["jabber.chats"]),
    kind: Kind::Read,
    schema: || schema(json!({"conversation": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 80}}), &["conversation"]),
    run: |ctx, v| {
        let want = str_arg(v, "conversation").unwrap_or_default().to_lowercase();
        let n = u64_arg(v, "limit", 30, 80) as usize;
        let st = ctx.deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
        let (jid, msgs) = st.chats.iter().find(|(j, _)| j.to_lowercase().contains(&want)).ok_or_else(|| format!("no conversation matching {want:?}"))?;
        let tail: Vec<Value> = msgs.iter().rev().take(n).rev().map(|m| json!({"when": eve_time(m.time), "from": m.from, "text": m.body})).collect();
        Ok(json!({"conversation": jid, "messages": tail}))
    },
};

static LOCALSCAN: ToolSpec = ToolSpec {
    name: "local_scan",
    description: "The pilots in the user's last local scan with their zKillboard summary: kills, losses, danger, gang size, \
                  and alliance and corporation ids.",
    need: Need::All(&["pilots.localscan"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let t = ctx.deps.lookup_table.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for row in t.rows.values() {
            if let crate::localscan::Row::Done(s) = row {
                out.push(json!({"name": s.name, "danger": s.danger, "kills": s.kills, "losses": s.losses, "avg_gang": s.avg_gang, "solo": s.solo, "alliance_id": s.alliance_id, "corp_id": s.corp_id}));
            }
        }
        Ok(json!({"pilots": out}))
    },
};

static NOTES: ToolSpec = ToolSpec {
    name: "notes",
    description: "The user's own notes and tags on a system or a pilot.",
    need: Need::All(&["notes"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}, "pilot": {"type": "string"}}), &[]),
    run: notes,
};

fn notes(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let view = ctx.facts.notes_view.clone().ok_or("no notes")?;
    let merged = if let Some(sys) = str_arg(v, "system") {
        view.system(ctx.system(sys)?).cloned()
    } else if let Some(p) = str_arg(v, "pilot") {
        view.pilot_by_name(p).cloned()
    } else {
        return Err("name a system or a pilot".into());
    };
    Ok(match merged {
        Some(m) => {
            let tags: Vec<String> = m.tags.iter().filter_map(|id| view.tag(id).map(|t| t.name.clone())).collect();
            json!({"name": m.name, "tags": tags, "notes": m.parts.iter().filter(|p| !p.note.is_empty()).map(|p| p.note.clone()).collect::<Vec<_>>()})
        }
        None => json!({"note": "nothing noted"}),
    })
}

#[cfg(test)]
mod fleet_rescue_tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    fn deps(perms: &[&str]) -> AiDeps {
        let mut f = facts(perms);
        f.unlocked = crate::ai::perms::Unlocked { fleet: true, rescue: true };
        f.fleet_backend = Some(std::sync::Arc::new(crate::fleets::spoof::SpoofBackend::with(crate::fleets::seed::invented(), std::time::Duration::ZERO)));
        let mut d = AiDeps::for_tests(f);
        d.online = true;
        d
    }

    #[test]
    fn past_fleets_are_searched_and_opened_with_who_flew_what() {
        let d = deps(&["fleets.history", "fleets.current"]);
        let (h, err) = run(&d, "fleets_history", json!({}));
        assert!(!err, "{h}");
        assert!(h["fleets"][0]["id"].is_string(), "{h}");
        // The spoof dashboard opens only its running fleets.
        let (cur, _) = run(&d, "fleets_current", json!({}));
        let id = cur["strategic"][0]["id"].as_str().expect("a fleet id").to_owned();
        let (f, err) = run(&d, "fleet_detail", json!({"id": id}));
        assert!(!err, "{f}");
        assert!(f["members"].as_i64().unwrap_or(0) > 0, "{f}");
        assert!(f["ships"].as_object().is_some_and(|s| !s.is_empty()));
    }

    #[test]
    fn rescue_history_filters_and_counts_by_system() {
        let d = deps(&["rescue"]);
        {
            let mut r = d.rescue.lock().unwrap();
            for (i, sys) in ["1DQ1-A", "1DQ1-A", "7-K5EL"].iter().enumerate() {
                r.history.push(crate::rescue::RescueRecord { received: crate::clock::utc().timestamp() - 3600 + i as i64, raw: format!("rorq tackled {sys}"), system: Some((*sys).into()), class: Some("Rorqual".into()), coord_pinged: i == 0, ..Default::default() });
            }
        }
        let (out, err) = run(&d, "rescue_history", json!({"filter": "1dq1", "days": 365}));
        assert!(!err, "{out}");
        assert_eq!(out["count"], 2);
        assert_eq!(out["by_system"]["1DQ1-A"], 2);
        let (none, _) = run(&deps(&[]), "rescue_history", json!({}));
        assert!(none.to_string().contains("not allowed"), "{none}");
    }
}
