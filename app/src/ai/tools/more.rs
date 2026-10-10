//! Capital jumps, routes through wormholes, killmail fits, doctrines, the user's current route and
//! last d-scan, battle report links, rats, the wormhole log, and joining Mumble.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] =
    &[&JUMP_ROUTE, &WH_ROUTE, &KILL_DETAIL, &DOCTRINES, &MY_ROUTE, &LAST_DSCAN, &BR_LINKS, &RATS, &WH_LOG, &MUMBLE_NOW, &JOIN_MUMBLE];

static JUMP_ROUTE: ToolSpec = ToolSpec {
    name: "jump_route",
    description: "A capital jump route between two systems: each midpoint, its distance in light years, the fuel, and the \
                  fatigue and reactivation timer after each jump. Uses the character's jump skills when known and prefers \
                  the user's cyno generators as midpoints.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "from": {"type": "string"},
                "to": {"type": "string"},
                "ship": {"type": "string", "enum": ["capital", "super", "black_ops", "jump_freighter", "rorqual", "command_carrier"]}
            }),
            &["from", "to"],
        )
    },
    run: |ctx, v| {
        use crate::jumproute::{hop_costs, jumpable, max_range_ly, shortest_path_pref, SHIP_CLASSES};
        let store = ctx.store.ok_or("the database is not open")?;
        let (from, to) = (ctx.system(str_arg(v, "from").unwrap_or_default())?, ctx.system(str_arg(v, "to").unwrap_or_default())?);
        let class = &SHIP_CLASSES[match str_arg(v, "ship").unwrap_or("capital") {
            "super" => 1,
            "black_ops" => 2,
            "jump_freighter" => 3,
            "rorqual" => 4,
            "command_carrier" => 5,
            _ => 0,
        }];
        let skills = *ctx.deps.jump_skills.lock().unwrap_or_else(|e| e.into_inner());
        let (jdc, jfc) = skills.unwrap_or((5, 4));
        let range = max_range_ly(class, jdc);
        let all = store.all_map_systems();
        // The start may be anywhere; every jump after it lands where a cyno can be lit.
        let systems: Vec<_> = all.iter().filter(|s| s.id == from || jumpable(s)).cloned().collect();
        let prefer: std::collections::HashSet<i64> = if ctx.facts.allowed("map.cyno") { ctx.facts.cyno_generators.iter().copied().collect() } else { Default::default() };
        let path = shortest_path_pref(&systems, range, from, to, &prefer).ok_or_else(|| format!("no jump route within {range:.1} ly a jump"))?;
        let hops = hop_costs(&systems, &path, class, jfc);
        let total_fuel: f64 = hops.iter().map(|h| h.fuel).sum();
        Ok(json!({
            "ship": class.name,
            "range_ly": (range * 10.0).round() / 10.0,
            "skills": if skills.is_some() { format!("JDC {jdc}, JFC {jfc}") } else { "unknown, assumed JDC 5, JFC 4".into() },
            "jumps": hops.len(),
            "fuel_isotopes": total_fuel.round(),
            "route": path.iter().skip(1).zip(&hops).map(|(id, h)| {
                let mut x = json!({
                    "to": ctx.system_name(*id),
                    "ly": (h.ly * 100.0).round() / 100.0,
                    "fatigue_min": h.fatigue_min.round(),
                    "wait_min": h.reactivation_min.round(),
                });
                if prefer.contains(id) {
                    x["cyno_generator"] = json!(true);
                }
                x
            }).collect::<Vec<_>>(),
        }))
    },
};

static WH_ROUTE: ToolSpec = ToolSpec {
    name: "route_via_wormholes",
    description: "The shortest route between two systems through gates, the user's jump bridges and the wormholes the app \
                  knows (Thera, Turnur, scanned holes), with the holes on it and their size and time left. Compare with \
                  route to see what the shortcut saves.",
    need: Need::All(&["wormholes"]),
    kind: Kind::Read,
    schema: || schema(json!({"from": {"type": "string"}, "to": {"type": "string"}}), &["from", "to"]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let geo = ctx.geo()?;
        let (from, to) = (ctx.system(str_arg(v, "from").unwrap_or_default())?, ctx.system(str_arg(v, "to").unwrap_or_default())?);
        let holes_list: Vec<_> = store.wormholes().into_iter().filter(|w| !w.is_expired(ctx.now) && w.dest_system_id.is_some()).collect();
        let mut holes: std::collections::HashMap<i64, Vec<i64>> = Default::default();
        for w in &holes_list {
            let b = w.dest_system_id.unwrap_or(0);
            holes.entry(w.system_id).or_default().push(b);
            holes.entry(b).or_default().push(w.system_id);
        }
        let path = geo.route_with(from, to, true, true, &holes, |_| true).ok_or("no route")?;
        let gates_only = geo.route(from, to, true, true, |_| true).map(|p| p.len().saturating_sub(1));
        let mut through = Vec::new();
        for w in path.windows(2) {
            if !geo.neighbors(w[0]).contains(&w[1]) {
                if let Some(h) = holes_list.iter().find(|h| (h.system_id == w[0] && h.dest_system_id == Some(w[1])) || (h.system_id == w[1] && h.dest_system_id == Some(w[0]))) {
                    through.push(json!({"from": ctx.system_name(w[0]), "to": ctx.system_name(w[1]), "signature": h.signature, "size": h.effective_size().map(|s| format!("{s:?}")), "hours_left": h.hours_left(ctx.now)}));
                }
            }
        }
        Ok(json!({
            "jumps": path.len().saturating_sub(1),
            "jumps_by_gates_only": gates_only,
            "wormholes_used": through,
            "systems": path.iter().map(|id| ctx.system_name(*id)).collect::<Vec<_>>(),
        }))
    },
};

/// Where a fitted item sits, by its killmail flag.
fn slot(flag: i64) -> &'static str {
    match flag {
        11..=18 => "low",
        19..=26 => "mid",
        27..=34 => "high",
        92..=99 => "rig",
        125..=132 => "subsystem",
        87 => "drone bay",
        158 => "fighter bay",
        5 => "cargo",
        _ => "other",
    }
}

static KILL_DETAIL: ToolSpec = ToolSpec {
    name: "kill_detail",
    description: "One killmail by its kill_id: the victim and their fit by slot (what dropped and what was destroyed), \
                  where it happened, the attackers with their ships and who did the most damage, and the value.",
    need: Need::Any(&["kills.feed", "kills.history", "battles"]),
    kind: Kind::Read,
    schema: || schema(json!({"kill_id": {"type": "integer"}}), &["kill_id"]),
    run: |ctx, v| {
        if !ctx.deps.online {
            return Err("killmails are fetched from zKillboard, which is not reachable now".into());
        }
        let id = v.get("kill_id").and_then(Value::as_i64).ok_or("which kill?")?;
        let d = crate::killmail::fetch(id, None)?;
        let mut types: Vec<i64> = d.items.iter().map(|i| i.type_id).chain(d.attackers.iter().map(|a| a.ship)).chain([d.victim.ship]).filter(|t| *t > 0).collect();
        types.sort_unstable();
        types.dedup();
        let tn = crate::universe::lookup_names(&types);
        let name = |id: i64| tn.get(&id).cloned().or_else(|| d.names.get(&id).cloned()).unwrap_or_else(|| format!("#{id}"));
        let mut fit: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
        for it in d.items.iter().filter(|i| i.depth == 0) {
            let n = it.dropped + it.destroyed;
            let fate = if it.dropped > 0 { "dropped" } else { "destroyed" };
            fit.entry(slot(it.flag)).or_default().push(if n > 1 { format!("{} x{n} ({fate})", name(it.type_id)) } else { format!("{} ({fate})", name(it.type_id)) });
        }
        let mut attackers: Vec<&crate::killmail::Who> = d.attackers.iter().collect();
        attackers.sort_by_key(|a| std::cmp::Reverse(a.damage));
        let who = |w: &crate::killmail::Who| {
            json!({
                "pilot": d.names.get(&w.char_id),
                "corporation": d.names.get(&w.corp_id),
                "alliance": d.names.get(&w.alliance_id),
                "ship": (w.ship > 0).then(|| name(w.ship)),
                "damage": (w.damage > 0).then_some(w.damage),
            })
        };
        Ok(json!({
            "when": eve_time(d.time),
            "system": ctx.system_name(d.system_id),
            "near": d.near.as_ref().map(|(c, m)| format!("{c} ({:.0} km)", m / 1000.0)),
            "victim": who(&d.victim),
            "fit": fit,
            "attackers": d.attackers.len(),
            "top_attackers": attackers.iter().take(10).map(|a| who(a)).collect::<Vec<_>>(),
            "isk_millions": (d.zkb.total / 1e6).round(),
            "zkillboard": crate::killmail::zkill_url(id),
        }))
    },
};

static DOCTRINES: ToolSpec = ToolSpec {
    name: "doctrines",
    description: "The fleet doctrines: each one's hulls (main and support), how it tanks, the forum topic with its fits, \
                  its ping line, whether only its own hulls belong, and the boosts it wants. The forum topics need the user's \
                  forum login; link them for the user to open.",
    need: Need::Any(&["fleets.presets", "fleets.current"]),
    kind: Kind::Read,
    schema: || schema(json!({"name": {"type": "string", "description": "Part of a doctrine's name"}}), &[]),
    run: |ctx, v| {
        let want = str_arg(v, "name").map(|n| n.to_lowercase());
        Ok(json!(ctx
            .facts
            .doctrines
            .iter()
            .filter(|d| want.as_ref().is_none_or(|w| d.name.to_lowercase().contains(w.as_str())))
            .map(|d| json!({
                "name": d.name, "main_hulls": d.main, "support_hulls": d.support, "tank": d.tank,
                "fits_forum_topic": d.url, "ping_line": d.line, "only_its_own_hulls": d.strict.then_some(true),
                "boosts_wanted": d.boosts.iter().map(|(c, p)| format!("{c} ({p})")).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()))
    },
};

static MY_ROUTE: ToolSpec = ToolSpec {
    name: "my_route",
    description: "The route the user has laid out on the map now and the destination set, with how many jumps are left \
                  from where the active character is.",
    need: Need::Any(&["actions.route", "characters.locations"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let geo = ctx.geo()?;
        let here = ctx.deps.player.lock().unwrap_or_else(|e| e.into_inner()).system_id;
        let dest = ctx.facts.route_destination;
        Ok(json!({
            "planned_through": ctx.facts.route_anchors.iter().map(|id| ctx.system_name(*id)).collect::<Vec<_>>(),
            "destination": dest.map(|d| ctx.system_name(d)),
            "you_are_in": here.map(|h| ctx.system_name(h)),
            "jumps_left": match (here, dest) {
                (Some(h), Some(d)) => geo.jumps(h, d, 200),
                _ => None,
            },
        }))
    },
};

static LAST_DSCAN: ToolSpec = ToolSpec {
    name: "last_dscan",
    description: "The d-scan the user last opened in the app: its link and the ships on it with counts.",
    need: Need::Any(&["pilots.localscan", "intel.reports"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let Some((url, ships)) = &ctx.facts.last_dscan else { return Ok(json!({"note": "no d-scan opened"})) };
        Ok(json!({"link": url, "ships": ships.iter().map(|(n, c)| json!({"ship": n, "count": c})).collect::<Vec<_>>()}))
    },
};

static BR_LINKS: ToolSpec = ToolSpec {
    name: "battle_report_links",
    description: "Battle reports the user published to br.evetools.org, newest first, and the one for a tracked fleet.",
    need: Need::All(&["battles"]),
    kind: Kind::Read,
    schema: || schema(json!({"fleet_id": {"type": "string"}}), &[]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let mut out = json!({"published": store.evetools_all().iter().rev().take(20).map(|(anchor, s)| json!({"battle_id": anchor, "link": s.url()})).collect::<Vec<_>>()});
        if let Some(f) = str_arg(v, "fleet_id") {
            out["fleet_report"] = json!(store.fleet_br(f));
        }
        Ok(out)
    },
};

static RATS: ToolSpec = ToolSpec {
    name: "rats",
    description: "The NPC pirates of a region: their faction, the damage to deal and to tank against, and their e-war.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"region": {"type": "string"}, "system": {"type": "string"}}), &[]),
    run: |ctx, v| {
        let region = match (str_arg(v, "region"), str_arg(v, "system")) {
            (Some(r), _) => r.to_owned(),
            (None, Some(s)) => ctx.geo()?.info_of(ctx.system(s)?).map(|i| i.region.clone()).ok_or("unknown system")?,
            _ => return Err("name a region or a system".into()),
        };
        let p = crate::rats::rat_profile(&region).ok_or_else(|| format!("no pirate data for {region}"))?;
        Ok(json!({"region": region, "faction": p.faction, "deal_damage": p.deal, "they_deal": p.weak, "ewar": p.ewar}))
    },
};

static WH_LOG: ToolSpec = ToolSpec {
    name: "wormhole_log",
    description: "The edit history of the wormholes in a system: who added or changed each, what, and when.",
    need: Need::All(&["wormholes"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}}), &["system"]),
    run: |ctx, v| {
        let store = ctx.store.ok_or("the database is not open")?;
        let id = ctx.system(str_arg(v, "system").unwrap_or_default())?;
        let holes: Vec<_> = store.wormholes().into_iter().filter(|w| w.system_id == id || w.dest_system_id == Some(id)).collect();
        Ok(json!(holes
            .iter()
            .map(|w| json!({
                "signature": w.signature,
                "leads_to": w.dest_system_id.map(|b| ctx.system_name(b)),
                "changes": store.wormhole_audit(&w.uid).iter().rev().take(10).map(|(t, who, src, field, val)| json!({"when": eve_time(*t), "age": fmt_age(ctx.now, *t), "by": who, "via": src, "field": field, "to": val})).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()))
    },
};

static MUMBLE_NOW: ToolSpec = ToolSpec {
    name: "mumble_channel",
    description: "The Mumble channel the user is in now, if Mumble is running.",
    need: Need::Any(&["actions.mumble", "jabber.pings"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |_, _| Ok(json!({"channel": crate::mumble::current_url().and_then(|u| crate::mumble::channel_path(&u))})),
};

static JOIN_MUMBLE: ToolSpec = ToolSpec {
    name: "join_mumble",
    description: "Moves the user's Mumble into a channel, for the user to confirm with a click: by a mumble:// or comms link, \
                  by an op number (Op 4), or by words matching a fleet ping (its FC, fleet or doctrine), whose comms link is used.",
    need: Need::All(&["actions.mumble"]),
    kind: Kind::Action,
    schema: || schema(json!({"link": {"type": "string"}, "op": {"type": "integer"}, "ping": {"type": "string"}}), &[]),
    run: join_mumble,
};

fn join_mumble(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    use crate::pings::Comms;
    let mut link = str_arg(v, "link").map(str::to_owned);
    let mut label = String::new();
    if link.is_none() {
        let op = v.get("op").and_then(Value::as_u64);
        let words: Vec<String> = str_arg(v, "ping").unwrap_or_default().to_lowercase().split_whitespace().map(str::to_owned).collect();
        if op.is_none() && words.is_empty() {
            return Err("give a link, an op number or words from a fleet ping".into());
        }
        let st = ctx.deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
        'pings: for p in st.pings.iter().rev() {
            for f in p.fleets() {
                let Some(Comms::Mumble { channel, link: l }) = &f.comms else { continue };
                let hay = format!("{} {} {} {}", f.fc, f.fleet.clone().unwrap_or_default(), f.doctrine.clone().unwrap_or_default(), channel).to_lowercase();
                let op_ok = op.is_none_or(|n| {
                    let c = channel.to_lowercase();
                    c == format!("op {n}") || c.starts_with(&format!("op {n} "))
                });
                if op_ok && words.iter().all(|w| hay.contains(w.as_str())) {
                    link = Some(l.clone());
                    label = format!("{channel}, {}'s fleet", f.fc);
                    break 'pings;
                }
            }
        }
        drop(st);
    }
    let link = link.ok_or("no fleet ping names that channel; ask the user for the link")?;
    let url = if link.starts_with("mumble://") {
        link.clone()
    } else if link.starts_with("http") {
        if !ctx.deps.online {
            return Err("the comms link has to be opened to find the channel, and the network is not reachable".into());
        }
        let client = crate::http::client(10).map_err(|e| e.to_string())?;
        let body = client.get(&link).send().and_then(|r| r.text()).map_err(|e| e.to_string())?;
        crate::pings::extract_mumble_url(&body).ok_or("that link does not lead to a Mumble channel")?
    } else {
        return Err("not a Mumble or comms link".into());
    };
    let path = crate::mumble::channel_path(&url).unwrap_or_else(|| url.clone());
    if label.is_empty() {
        label = path.rsplit('/').next().unwrap_or(&path).to_owned();
    }
    super::actions::queue_pub(ctx, super::ActionKind::JoinMumble { url }, format!("Join Mumble: {label}"))
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    #[test]
    fn mumble_is_joined_by_link_or_by_a_pings_op_and_waits_for_a_click() {
        let deps = AiDeps::for_tests(facts(&["actions.mumble"]));
        let (out, err) = run(&deps, "join_mumble", json!({"link": "mumble://mumble.example.invalid/Ops/Op%204?title=x"}));
        assert!(!err, "{out}");
        deps.jabber.lock().unwrap().pings = vec![crate::uitest::fixtures::ping_fleet_multi()];
        let (out, err) = run(&deps, "join_mumble", json!({"op": 5}));
        assert!(err && out.to_string().contains("not reachable"), "the ping's comms link needs the network: {out}");
        let (out, err) = run(&deps, "join_mumble", json!({"op": 42}));
        assert!(err && out.to_string().contains("no fleet ping"), "{out}");
        let (_, err) = run(&AiDeps::for_tests(facts(&[])), "join_mumble", json!({"link": "mumble://x/y"}));
        assert!(err, "joining needs its own tick");
    }

    #[test]
    fn rats_and_doctrines_answer_from_what_is_known() {
        let mut f = facts(&["fleets.presets"]);
        f.doctrines = vec![crate::ai::deps::DoctrineFacts { name: "Muninn Fleet".into(), main: vec!["Muninn".into()], support: vec!["Scimitar".into()], tank: Some("armor".into()), url: Some("https://example.invalid/fits".into()), line: None, strict: false, boosts: vec![] }];
        let deps = AiDeps::for_tests(f);
        let (r, err) = run(&deps, "rats", json!({"system": "1DQ1-A"}));
        assert!(!err, "{r}");
        assert!(r["faction"].is_string());
        let (d, err) = run(&deps, "doctrines", json!({"name": "muninn"}));
        assert!(!err);
        assert_eq!(d[0]["main_hulls"][0], "Muninn");
    }
}
