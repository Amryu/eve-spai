//! The user's own side: characters, fleets, the rescue, Jabber, the local scan and notes.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] =
    &[&MY_CHARACTERS, &FLEETS_CURRENT, &FLEETS_HISTORY, &FLEET_DETAIL, &FLEET_TRACKING, &FLEET_PRESETS, &RESCUE, &RESCUE_HISTORY, &JABBER_PINGS, &JABBER_ROOMS, &JABBER_CHAT, &LOCALSCAN, &PILOT_INFO, &NOTES, &MY_SETUP];

static MY_CHARACTERS: ToolSpec = ToolSpec {
    name: "my_characters",
    description: "Where the user's logged-in characters are, whether each is docked, and the ship each last moved in. The \
                  active one is marked.",
    need: Need::All(&["characters.locations"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let (active, locs) = {
            let p = ctx.deps.player.lock().unwrap_or_else(|e| e.into_inner());
            (p.active_name.clone(), p.locations.clone())
        };
        let ship_names = ctx.store.map(super::intel::ship_names).unwrap_or_default();
        let mut out: Vec<Value> = locs
            .iter()
            .map(|(name, (sys, docked))| {
                let mut c = json!({"name": name, "system": ctx.system_name(*sys), "docked": docked, "active": *name == active});
                // The ship as of the last move the app saw; it does not know of a reship without one.
                if let Some(store) = ctx.store {
                    if let Some(m) = store.move_history(0, Some(name), 1).into_iter().next() {
                        if let Some(id) = m.ship {
                            c["ship"] = json!(ship_names.get(&id).cloned().unwrap_or_else(|| format!("type {id}")));
                            c["ship_as_of"] = json!(fmt_age(ctx.now, m.time));
                        }
                    }
                }
                c
            })
            .collect();
        out.sort_by(|a, b| b["active"].as_bool().cmp(&a["active"].as_bool()));
        Ok(json!(out))
    },
};

fn rows(r: &[crate::fleets::model::FleetRow]) -> Value {
    json!(r.iter().take(30).map(|f| json!({
        "id": f.id.0, "name": f.name,
        "started_by": f.started_by.as_ref().filter(|s| Some(*s) != f.commander.as_ref()),
        "started": super::iso_time(&f.started_at),
        "closed": f.closed_at.as_deref().map(super::iso_time), "doctrine": f.setup_name, "operation": f.operation_name, "group": f.group_name,
        "commander": f.commander,
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

static FLEET_TRACKING: ToolSpec = ToolSpec {
    name: "fleet_tracking",
    description: "What the app recorded while it tracked a fleet (by fleet id): where the fleet went and how (gate, \
                  Ansiblex, wormhole, jump), pilots joining, leaving and reshipping, and its kills and losses with value.",
    need: Need::All(&["fleets.history"]),
    kind: Kind::Read,
    schema: || schema(json!({"id": {"type": "string"}, "limit": {"type": "integer", "minimum": 5, "maximum": 200}}), &["id"]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let id = str_arg(v, "id").ok_or("which fleet?")?;
        let n = u64_arg(v, "limit", 60, 200) as usize;
        let moves = store.fleet_moves(id);
        let kills = store.fleet_kills(id);
        if moves.is_empty() && kills.is_empty() {
            return Err("nothing recorded for that fleet; only fleets tracked in the app are".into());
        }
        let names = super::intel::ship_names(store);
        let ship = |id: i64| names.get(&id).cloned().unwrap_or_else(|| format!("type {id}"));
        let (lost, killed): (Vec<_>, Vec<_>) = kills.iter().partition(|k| k.loss);
        Ok(json!({
            "losses": lost.len(),
            "isk_lost_billions": (lost.iter().map(|k| k.value).sum::<f64>() / 1e8).round() / 10.0,
            "kills": killed.len(),
            "isk_killed_billions": (killed.iter().map(|k| k.value).sum::<f64>() / 1e8).round() / 10.0,
            "kill_list": kills.iter().rev().take(30).map(|k| json!({"when": eve_time(k.at), "system": ctx.system_name(k.system_id), "lost": k.loss, "victim": k.victim_name, "ship": ship(k.ship_type_id), "isk_millions": (k.value / 1e6).round(), "kill_id": k.kill_id})).collect::<Vec<_>>(),
            "moves": moves.iter().rev().take(n).map(|m| {
                let mut x = json!({"when": eve_time(m.at), "what": format!("{:?}", m.kind)});
                if m.count > 0 { x["fleet_count"] = json!(m.count); }
                if !m.name.is_empty() { x["pilot"] = json!(m.name); }
                if m.system_id > 0 { x["system"] = json!(ctx.system_name(m.system_id)); }
                if m.from_system > 0 && m.from_system != m.system_id { x["from"] = json!(ctx.system_name(m.from_system)); }
                if let Some(via) = &m.via { x["via"] = json!(format!("{via:?}")); }
                if !m.ship_name.is_empty() { x["ship"] = json!(m.ship_name); }
                x
            }).collect::<Vec<_>>(),
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
    description: "Fleet pings received over Jabber, newest first: the FC, doctrine, formup, comms and text. Search them by \
                  words (an FC, a doctrine, a fleet name) and how many days back.",
    need: Need::All(&["jabber.pings"]),
    kind: Kind::Read,
    schema: || schema(json!({"query": {"type": "string"}, "days": {"type": "integer", "minimum": 1, "maximum": 365}, "limit": {"type": "integer", "minimum": 1, "maximum": 30}}), &[]),
    run: |ctx, v| {
        let n = u64_arg(v, "limit", 10, 30) as usize;
        let since = v.get("days").and_then(Value::as_u64).map(|d| ctx.now - d as i64 * 86_400);
        let words: Vec<String> = str_arg(v, "query").unwrap_or_default().to_lowercase().split_whitespace().map(str::to_owned).collect();
        let st = ctx.deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
        let hits: Vec<&crate::pings::Ping> = st
            .pings
            .iter()
            .rev()
            .filter(|p| since.is_none_or(|s| p.timestamp() >= s))
            .filter(|p| {
                if words.is_empty() {
                    return true;
                }
                let hay = serde_json::to_string(p).unwrap_or_default().to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .take(n)
            .collect();
        Ok(json!(hits.iter().map(|p| ping_view(ctx, p)).collect::<Vec<_>>()))
    },
};

/// A ping as the assistant reads it: who, when, and each fleet it calls, with systems by name; the
/// raw text only when nothing could be read out of it.
fn ping_view(ctx: &Ctx, p: &crate::pings::Ping) -> Value {
    use crate::pings::{Comms, Formup, Ping};
    let fleet = |f: &crate::pings::FleetInfo| {
        json!({
            "fc": f.fc,
            "fleet": f.fleet,
            "doctrine": f.doctrine,
            "formup": f.formup.iter().map(|x| match x { Formup::System(id) => ctx.system_name(*id), Formup::Text(t) => t.clone() }).collect::<Vec<_>>(),
            "pap": f.pap.as_ref().map(|x| format!("{x:?}")),
            "comms": f.comms.as_ref().map(|c| match c { Comms::Mumble { channel, .. } => format!("{channel} (Mumble)"), Comms::Text(t) => t.clone() }),
            "comms_link": f.comms.as_ref().and_then(|c| match c { Comms::Mumble { link, .. } => Some(link.clone()), Comms::Text(_) => None }),
        })
    };
    let t = p.timestamp();
    match p {
        Ping::Plain { text, sender, target, raw, .. } => json!({
            "when": eve_time(t), "age": fmt_age(ctx.now, t), "from": sender, "to": target,
            "text": if text.trim().is_empty() { raw } else { text },
        }),
        Ping::Fleet { description, source, target, raw, .. } => {
            let fleets: Vec<Value> = p.fleets().iter().map(fleet).collect();
            json!({
                "when": eve_time(t), "age": fmt_age(ctx.now, t), "from": source, "to": target,
                "text": description,
                "fleets": fleets,
                "raw": if fleets.is_empty() && description.trim().is_empty() { Some(raw) } else { None },
            })
        }
    }
}

static JABBER_ROOMS: ToolSpec = ToolSpec {
    name: "jabber_rooms",
    description: "The user's Jabber rooms with their message of the day (often the standing fleet, formup and comms), unread \
                  counts and whether the user was mentioned, plus which contacts are online.",
    need: Need::All(&["jabber.chats"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let st = ctx.deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
        let local = |j: &str| j.split('@').next().unwrap_or(j).to_owned();
        let rooms: Vec<Value> = st
            .rooms
            .iter()
            .map(|r| json!({"room": local(r), "address": r, "motd": st.room_subjects.get(r), "unread": st.unread_counts.get(r).copied().unwrap_or(0), "mentioned": st.mentions.contains(r)}))
            .collect();
        let online: Vec<Value> = st
            .roster
            .iter()
            .filter(|(_, c)| c.presence != crate::jabber::Presence::Offline)
            .map(|(j, c)| json!({"name": c.name.clone().unwrap_or_else(|| local(j)), "status": format!("{:?}", c.presence), "says": c.status_text}))
            .collect();
        let dms: Vec<Value> = st
            .chats
            .keys()
            .filter(|k| !st.rooms.contains(*k) && st.unread_counts.get(*k).copied().unwrap_or(0) > 0)
            .map(|k| json!({"from": local(k), "unread": st.unread_counts.get(k)}))
            .collect();
        Ok(json!({"connected": st.ever_online, "rooms": rooms, "unread_direct_messages": dms, "contacts_online": online}))
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
        let t = super::actions::resolve_jabber(&st, &ctx.facts.jabber_domain, &want, None)?;
        let (jid, msgs) = st.chats.get_key_value(&t.jid).ok_or_else(|| format!("no messages with {} yet", t.label))?;
        let tail: Vec<Value> = msgs.iter().rev().take(n).rev().map(|m| json!({"when": eve_time(m.time), "from": m.from, "text": m.body})).collect();
        Ok(json!({"conversation": jid, "messages": tail}))
    },
};

static LOCALSCAN: ToolSpec = ToolSpec {
    name: "local_scan",
    description: "The pilots in the local the user is looking up now, each with corporation and alliance, zKillboard danger, \
                  kills and losses, gang size, what they fly, and whether they look like a cyno pilot, an FC or bait. Blues \
                  the user leaves out are listed as such.",
    need: Need::All(&["pilots.localscan"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let names: Vec<String> = ctx.facts.lookup_current.iter().map(|n| n.to_lowercase()).collect();
        if names.is_empty() {
            return Ok(json!({"pilots": [], "note": "no local is being looked up"}));
        }
        let ships = ctx.store.map(super::intel::ship_names).unwrap_or_default();
        let t = ctx.deps.lookup_table.lock().unwrap_or_else(|e| e.into_inner());
        let out: Vec<Value> = names.iter().map(|n| pilot_row(n, t.rows.get(n), &t.orgs, &ships)).collect();
        Ok(json!({"pilots": out}))
    },
};

/// One looked-up pilot as the assistant reads it.
pub(crate) fn pilot_row(name: &str, row: Option<&crate::localscan::Row>, orgs: &std::collections::HashMap<i64, crate::localscan::Org>, ships: &std::collections::HashMap<i64, String>) -> Value {
    use crate::localscan::Row;
    let org = |id: i64| orgs.get(&id).map(|o| format!("{} [{}]", o.name, o.ticker)).or_else(|| (id > 0).then(|| format!("#{id}")));
    match row {
        Some(Row::Done(s)) => {
            let mut top: Vec<&crate::localscan::Ship> = s.ships.iter().collect();
            top.sort_by_key(|x| std::cmp::Reverse(x.kills + x.losses));
            json!({
                "name": s.name,
                "corporation": org(s.corp_id),
                "alliance": org(s.alliance_id),
                "security": s.security,
                "age_days": s.birthday.map(|b| (crate::clock::utc().timestamp() - b) / 86_400),
                "danger": s.danger, "kills": s.kills, "losses": s.losses, "solo": s.solo, "avg_gang": s.avg_gang,
                "isk_destroyed_billions": (s.isk_destroyed / 1e8).round() / 10.0,
                "flies": top.iter().take(6).map(|x| json!({"ship": ships.get(&x.type_id).cloned().unwrap_or_else(|| format!("type {}", x.type_id)), "kills": x.kills, "losses": x.losses})).collect::<Vec<_>>(),
                "cyno": s.cyno.as_ref().map(|c| json!({"standard": c.standard, "covert": c.covert, "industrial": c.industrial})),
                "fc": s.fc.as_ref().map(|f| f.level.clone()),
                "bait": s.bait.as_ref().map(|b| b.level.clone()),
                "awox_kills": s.awox.iter().sum::<u32>(),
                "tags": s.tags.iter().map(|(t, n)| if *n > 0 { format!("{t:?} {n}") } else { format!("{t:?}") }).collect::<Vec<_>>(),
            })
        }
        Some(Row::Blue(st)) => json!({"name": name, "blue": st}),
        Some(Row::Missing) => json!({"name": name, "status": "no such character"}),
        Some(Row::Failed(e)) => json!({"name": name, "status": format!("lookup failed: {e}")}),
        Some(Row::Pending) => json!({"name": name, "status": "still being looked up"}),
        None => json!({"name": name, "status": "not looked up"}),
    }
}

static PILOT_INFO: ToolSpec = ToolSpec {
    name: "pilot_info",
    description: "What the app knows of one pilot: their zKillboard summary if looked up this session (corporation, alliance, \
                  danger, what they fly, cyno, FC or bait signs), when and where intel last named them, and in which of the \
                  user's saved local scans they were.",
    need: Need::Any(&["pilots.lookup", "pilots.localscan"]),
    kind: Kind::Read,
    schema: || schema(json!({"name": {"type": "string"}, "days": {"type": "integer", "minimum": 1, "maximum": 365}}), &["name"]),
    run: |ctx, v| {
        let name = str_arg(v, "name").ok_or("which pilot?")?.trim().to_owned();
        let low = name.to_lowercase();
        let since = ctx.now - u64_arg(v, "days", 30, 365) as i64 * 86_400;
        let ships = ctx.store.map(super::intel::ship_names).unwrap_or_default();
        let summary = {
            let t = ctx.deps.lookup_table.lock().unwrap_or_else(|e| e.into_inner());
            pilot_row(&name, t.rows.get(&low), &t.orgs, &ships)
        };
        let mut out = json!({"pilot": summary});
        if ctx.facts.allowed("intel.reports") {
            let mut seen: Vec<crate::intel::IntelReport> = {
                let st = ctx.deps.intel_state.lock().unwrap_or_else(|e| e.into_inner());
                st.reports.iter().filter(|r| r.pilots.iter().any(|p| p.to_lowercase() == low)).cloned().collect()
            };
            if let Some(store) = ctx.store {
                seen.extend(store.intel_history(since, ctx.now, &[], Some(&low), None, 20));
            }
            seen.sort_by_key(|r| std::cmp::Reverse(r.received));
            seen.dedup_by_key(|r| (r.received, r.text.clone()));
            out["intel"] = json!(seen.iter().take(10).map(|r| json!({"when": eve_time(r.received), "age": fmt_age(ctx.now, r.received), "systems": r.systems.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), "text": r.text})).collect::<Vec<_>>());
        }
        if let (true, Some(store)) = (ctx.facts.allowed("pilots.localscan"), ctx.store) {
            let scans: Vec<Value> = store
                .local_scan_history(since, 500)
                .into_iter()
                .filter_map(|(t, sys, j)| {
                    let s: crate::localscan::SavedLookup = serde_json::from_str(&j).ok()?;
                    s.names.iter().any(|n| n.to_lowercase() == low).then(|| json!({"when": eve_time(t), "system": sys.map(|x| ctx.system_name(x))}))
                })
                .take(10)
                .collect();
            out["in_local_scans"] = json!(scans);
        }
        Ok(out)
    },
};

static MY_SETUP: ToolSpec = ToolSpec {
    name: "my_setup",
    description: "The user's own setup: staging system, the alliance capital Ansiblex zones are counted from, the route \
                  planner's avoid lists and security limits, and their saved routes.",
    need: Need::Any(&["actions.route", "actions.settings"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let s = &ctx.facts.setup;
        Ok(json!({
            "staging": (!s.staging.is_empty()).then_some(&s.staging),
            "ansiblex_capital": (!s.capital.is_empty()).then_some(&s.capital),
            "avoid_by_gate": s.avoid_gate.iter().map(|id| ctx.system_name(*id)).collect::<Vec<_>>(),
            "avoid_jumping_into": s.avoid_jump.iter().map(|id| ctx.system_name(*id)).collect::<Vec<_>>(),
            "avoid_sov_of": s.avoid_sov,
            "allowed_security": {"high": s.sec[0], "low": s.sec[1], "null": s.sec[2]},
            "saved_routes": s.routes.iter().map(|r| json!({"name": r.name, "through": r.anchors.iter().map(|id| ctx.system_name(*id)).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        }))
    },
};

static NOTES: ToolSpec = ToolSpec {
    name: "notes",
    description: "The user's own notes and tags: on one system or pilot, or every system and pilot carrying a tag or words.",
    need: Need::All(&["notes"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}, "pilot": {"type": "string"}, "search": {"type": "string", "description": "A tag or words, to list every system and pilot noted with them"}}), &[]),
    run: notes,
};

fn notes(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let view = ctx.facts.notes_view.clone().ok_or("no notes")?;
    if let Some(q) = str_arg(v, "search") {
        let q = q.trim().to_lowercase();
        let row = |m: &crate::notes::Merged, kind: &str| {
            let tags: Vec<String> = m.tags.iter().filter_map(|id| view.tag(id).map(|t| t.name.clone())).collect();
            json!({"kind": kind, "name": m.name, "tags": tags, "notes": m.parts.iter().filter(|p| !p.note.is_empty()).map(|p| p.note.clone()).collect::<Vec<_>>()})
        };
        let hits: Vec<Value> = view
            .systems
            .values()
            .filter(|m| view.matches(m, &q))
            .map(|m| row(m, "system"))
            .chain(view.pilots.values().filter(|m| view.matches(m, &q)).map(|m| row(m, "pilot")))
            .take(60)
            .collect();
        return Ok(json!({"matches": hits}));
    }
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
