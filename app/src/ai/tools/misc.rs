//! The user's own side: characters, the rescue, Jabber, the local scan and notes.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] =
    &[&MY_CHARACTERS, &RESCUE, &RESCUE_HISTORY, &JABBER_PINGS, &JABBER_ROOMS, &JABBER_CHAT, &LOCALSCAN, &LOOKUP_PILOTS, &PILOT_INFO, &NOTES, &MY_SETUP];

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
pub(crate) fn ping_view(ctx: &Ctx, p: &crate::pings::Ping) -> Value {
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

static LOOKUP_PILOTS: ToolSpec = ToolSpec {
    name: "lookup_pilots",
    description: "Looks pilots up on zKillboard now, by name, up to 50 at once: corporation, alliance, danger, kills and \
                  losses, gang size, what they fly, cyno, FC and bait signs. Use it for any pilot not looked up yet; \
                  pilot_info adds what intel and saved local scans say of one. Results also land in the Lookup tab.",
    need: Need::All(&["pilots.lookup"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "names": {"type": "array", "items": {"type": "string"}, "maxItems": 50},
                "wait_seconds": {"type": "integer", "minimum": 1, "maximum": 25, "description": "How long to wait for slow lookups, 15 by default"}
            }),
            &["names"],
        )
    },
    run: |ctx, v| {
        let names: Vec<String> = v
            .get("names")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).take(50).collect())
            .unwrap_or_default();
        if names.is_empty() {
            return Err("which pilots?".into());
        }
        if !ctx.deps.online {
            return Err("offline: pilots cannot be looked up now".into());
        }
        let standings = ctx.deps.standings.lock().unwrap_or_else(|e| e.into_inner()).clone();
        // Not left out as blues: the user asked about these pilots by name.
        crate::localscan::request(&ctx.deps.lookup_table, &names, standings, false, &egui::Context::default());
        let wait = std::time::Duration::from_secs(u64_arg(v, "wait_seconds", 15, 25));
        let started = std::time::Instant::now();
        let lows: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        loop {
            let pending = {
                let t = ctx.deps.lookup_table.lock().unwrap_or_else(|e| e.into_inner());
                lows.iter().any(|n| matches!(t.rows.get(n), Some(crate::localscan::Row::Pending) | None))
            };
            if !pending || started.elapsed() >= wait {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let ships = ctx.store.map(super::intel::ship_names).unwrap_or_default();
        let t = ctx.deps.lookup_table.lock().unwrap_or_else(|e| e.into_inner());
        let out: Vec<Value> = names.iter().zip(&lows).map(|(n, l)| pilot_row(n, t.rows.get(l), &t.orgs, &ships)).collect();
        Ok(json!({"pilots": out}))
    },
};

static PILOT_INFO: ToolSpec = ToolSpec {
    name: "pilot_info",
    description: "What the app knows of one pilot: their zKillboard summary if looked up this session (lookup_pilots looks one up; corporation, alliance, \
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
            seen.retain(|r| ctx.sees_report(r));
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
mod rescue_tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    fn deps(perms: &[&str]) -> AiDeps {
        let mut f = facts(perms);
        f.unlocked = crate::ai::perms::Unlocked { rescue: true };
        let mut d = AiDeps::for_tests(f);
        d.online = true;
        d
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

#[cfg(test)]
mod lookup_tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    #[test]
    fn pilots_are_looked_up_by_name_and_offline_says_so() {
        let mut deps = AiDeps::for_tests(facts(&["pilots.lookup"]));
        let (r, err) = run(&deps, "lookup_pilots", json!({"names": ["Nobody Here"]}));
        assert!(err && r.to_string().contains("offline"), "{r}");
        deps.online = true;
        // Already known as missing, so nothing goes out and the answer comes at once.
        deps.lookup_table.lock().unwrap().rows.insert("nobody here".into(), crate::localscan::Row::Missing);
        let (r, err) = run(&deps, "lookup_pilots", json!({"names": ["Nobody Here"], "wait_seconds": 1}));
        assert!(!err, "{r}");
        assert_eq!(r["pilots"][0]["status"], "no such character");
        let (r, err) = run(&deps, "lookup_pilots", json!({"names": []}));
        assert!(err, "{r}");
    }
}
