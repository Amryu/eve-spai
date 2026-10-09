//! The user's own side: characters, fleets, the rescue, Jabber, the local scan and notes.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] =
    &[&MY_CHARACTERS, &FLEETS_CURRENT, &FLEETS_HISTORY, &FLEET_PRESETS, &RESCUE, &JABBER_PINGS, &JABBER_CHAT, &LOCALSCAN, &NOTES];

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
        "name": f.name, "doctrine": f.setup_name, "operation": f.operation_name, "group": f.group_name,
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
        let st = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner());
        Ok(json!({
            "strategic": st.active_strat.value.as_deref().map(rows),
            "other": st.active_pct.value.as_deref().map(rows),
            "open_fleet": st.open.value.as_ref().map(|o| json!({"name": o.fleet.name, "started": o.fleet.started_at})),
        }))
    },
};

static FLEETS_HISTORY: ToolSpec = ToolSpec {
    name: "fleets_history",
    description: "Past fleets from the fleet dashboard, newest first, as far as the dashboard view has loaded them.",
    need: Need::All(&["fleets.history"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let st = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner());
        Ok(json!({"fleets": st.history.value.as_ref().map(|p| rows(&p.items)), "total": st.history.value.as_ref().map(|p| p.total)}))
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
