//! Systems, routes, distances and what sits where: static data plus the map's own layers.

use serde_json::{json, Value};

use super::{schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&SYSTEM_INFO, &ROUTE, &SYSTEMS_WITHIN, &JOVE_NEAR, &CAMPS, &WORMHOLES_NEAR, &SHIP_INFO];

static SYSTEM_INFO: ToolSpec = ToolSpec {
    name: "system_info",
    description: "Facts about one solar system: security, region, constellation, neighbouring systems; plus sov holder, ADM, \
                  kills and jumps in the last hour, gate camps, Jove observatory and cyno generator where those are allowed.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string", "description": "System name, e.g. 1DQ1-A"}}), &["system"]),
    run: system_info,
};

fn sys_brief(ctx: &Ctx, id: i64) -> Value {
    match ctx.facts.systems.as_ref().and_then(|g| g.info_of(id)) {
        Some(i) => json!({"name": i.name, "security": (i.security * 10.0).round() / 10.0, "region": i.region}),
        None => json!({"id": id}),
    }
}

fn system_info(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let id = ctx.system(str_arg(v, "system").unwrap_or_default())?;
    let geo = ctx.geo()?;
    let info = geo.info_of(id).ok_or("unknown system")?;
    let neighbours: Vec<Value> = geo.neighbors(id).iter().map(|n| sys_brief(ctx, *n)).collect();
    let mut out = json!({
        "name": info.name,
        "security": (info.security * 10.0).round() / 10.0,
        "constellation": info.constellation,
        "region": info.region,
        "neighbours": neighbours,
    });
    if ctx.allowed("map.status") {
        if let Some(f) = ctx.deps.system_status.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
            out["sov_holder"] = json!(f.sov);
            out["adm"] = json!(f.adm);
            out["last_hour"] = json!({"ship_kills": f.ship_kills, "pod_kills": f.pod_kills, "npc_kills": f.npc_kills, "jumps": f.jumps});
            out["incursion"] = json!(f.incursion);
        }
    }
    if ctx.allowed("map.camps") {
        if let Some(c) = ctx.deps.camps.lock().unwrap_or_else(|e| e.into_inner()).camp(id, ctx.now) {
            out["gate_camp"] = json!({"level": format!("{:?}", c.level), "kills": c.kills, "age_s": c.age});
        }
    }
    if ctx.allowed("map.jove") {
        out["jove_observatory"] = json!(crate::jove::has(id));
    }
    if ctx.allowed("map.cyno") {
        out["friendly_cyno_generator"] = json!(ctx.facts.cyno_generators.contains(&id));
    }
    Ok(out)
}

static ROUTE: ToolSpec = ToolSpec {
    name: "route",
    description: "The shortest gate route between two systems, every system on it with its security. Jump bridges are \
                  used unless use_bridges is false. It knows nothing of the user's avoid lists, camps or security limits; \
                  plan_route takes those into account.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || {
        schema(
            json!({"from": {"type": "string"}, "to": {"type": "string"}, "use_bridges": {"type": "boolean"}}),
            &["from", "to"],
        )
    },
    run: route,
};

fn route(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let from = ctx.system(str_arg(v, "from").unwrap_or_default())?;
    let to = ctx.system(str_arg(v, "to").unwrap_or_default())?;
    let bridges = v.get("use_bridges").and_then(Value::as_bool).unwrap_or(true);
    let geo = ctx.geo()?;
    let path = geo.route(from, to, true, bridges, |_| true).ok_or("no route between them")?;
    let hops: Vec<Value> = path.iter().map(|id| sys_brief(ctx, *id)).collect();
    Ok(json!({"jumps": path.len().saturating_sub(1), "systems": hops}))
}

static SYSTEMS_WITHIN: ToolSpec = ToolSpec {
    name: "systems_within",
    description: "Systems within a number of jumps of one, nearest first, with their distance in jumps.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}, "jumps": {"type": "integer", "minimum": 1, "maximum": 10}}), &["system", "jumps"]),
    run: systems_within,
};

fn systems_within(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let id = ctx.system(str_arg(v, "system").unwrap_or_default())?;
    let jumps = u64_arg(v, "jumps", 3, 10) as u32;
    let mut d: Vec<(i64, u32)> = ctx.geo()?.distances_from(id, jumps).into_iter().collect();
    d.sort_by_key(|(id, j)| (*j, *id));
    let list: Vec<Value> = d.iter().take(200).map(|(s, j)| {
        let mut b = sys_brief(ctx, *s);
        b["jumps"] = json!(j);
        b
    }).collect();
    Ok(json!({"count": d.len(), "systems": list}))
}

static JOVE_NEAR: ToolSpec = ToolSpec {
    name: "jove_systems_near",
    description: "Systems with a Jove observatory within a number of jumps. Drifter and wormhole traffic often runs \
                  through these, so they help explain where a gang came from.",
    need: Need::All(&["map.jove"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}, "jumps": {"type": "integer", "minimum": 1, "maximum": 15}}), &["system"]),
    run: jove_near,
};

fn jove_near(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let id = ctx.system(str_arg(v, "system").unwrap_or_default())?;
    let jumps = u64_arg(v, "jumps", 8, 15) as u32;
    let mut d: Vec<(i64, u32)> = ctx.geo()?.distances_from(id, jumps).into_iter().filter(|(s, _)| crate::jove::has(*s)).collect();
    d.sort_by_key(|(id, j)| (*j, *id));
    Ok(json!(d.iter().map(|(s, j)| {
        let mut b = sys_brief(ctx, *s);
        b["jumps"] = json!(j);
        b
    }).collect::<Vec<_>>()))
}

static CAMPS: ToolSpec = ToolSpec {
    name: "gate_camps",
    description: "Gate camps the kill feed suggests right now, optionally only within some jumps of a system.",
    need: Need::All(&["map.camps"]),
    kind: Kind::Read,
    schema: || schema(json!({"near": {"type": "string"}, "jumps": {"type": "integer", "minimum": 1, "maximum": 20}}), &[]),
    run: camps,
};

fn camps(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let near = match str_arg(v, "near") {
        Some(n) => Some((ctx.system(n)?, u64_arg(v, "jumps", 10, 20) as u32)),
        None => None,
    };
    let list = ctx.deps.camps.lock().unwrap_or_else(|e| e.into_inner()).camped(ctx.now);
    let geo = ctx.geo()?;
    let mut out = Vec::new();
    for (sys, level) in list {
        let jumps = match near {
            Some((c, max)) => match geo.jumps(c, sys, max) {
                Some(j) => Some(j),
                None => continue,
            },
            None => None,
        };
        let mut b = sys_brief(ctx, sys);
        b["level"] = json!(format!("{level:?}"));
        if let Some(j) = jumps {
            b["jumps"] = json!(j);
        }
        out.push(b);
    }
    Ok(json!(out))
}

static WORMHOLES_NEAR: ToolSpec = ToolSpec {
    name: "wormholes_near",
    description: "Known wormholes in or near a system: where each leads, its size, mass and time left, and when it was \
                  reported. Without a system, every known hole.",
    need: Need::All(&["wormholes"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}, "jumps": {"type": "integer", "minimum": 0, "maximum": 15}}), &[]),
    run: wormholes_near,
};

fn wormholes_near(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let store = ctx.store.ok_or("the database is not open")?;
    let around = match str_arg(v, "system") {
        Some(n) => Some(ctx.geo()?.distances_from(ctx.system(n)?, u64_arg(v, "jumps", 5, 15) as u32)),
        None => None,
    };
    let mut out = Vec::new();
    for w in store.wormholes().into_iter().filter(|w| !w.is_expired(ctx.now)) {
        if let Some(d) = &around {
            let near = d.contains_key(&w.system_id) || w.dest_system_id.is_some_and(|b| d.contains_key(&b));
            if !near {
                continue;
            }
        }
        out.push(json!({
            "in": ctx.system_name(w.system_id),
            "signature": w.signature,
            "type": w.wh_type,
            "leads_to": w.dest_system_id.map(|b| ctx.system_name(b)).unwrap_or_else(|| format!("{:?}", w.dest)),
            "size": w.size.map(|s| format!("{s:?}")),
            "mass": w.mass.map(|m| format!("{m:?}")),
            "size_for_ships": w.effective_size().map(|s| format!("{s:?}")),
            "life": w.life.map(|l| format!("{l:?}")),
            "hours_left": w.hours_left(ctx.now),
            "reported": super::fmt_age(ctx.now, w.reported_at),
            "source": format!("{:?}", w.source),
            "note": w.note,
            "drifter": w.is_drifter,
        }));
    }
    Ok(json!(out))
}

static SHIP_INFO: ToolSpec = ToolSpec {
    name: "ship_info",
    description: "A ship type's class and its hit points and resists from the static data.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"ship": {"type": "string", "description": "Ship name, e.g. Muninn"}}), &["ship"]),
    run: ship_info,
};

fn ship_info(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let store = ctx.store.ok_or("the database is not open")?;
    let name = str_arg(v, "ship").unwrap_or_default().to_lowercase();
    let index = store.ship_index();
    let (id, group) = index.get(&name).cloned().ok_or_else(|| format!("no ship called {name:?}"))?;
    let d = store.ship_details(id);
    Ok(json!({
        "name": name,
        "class": group,
        "details": d.map(|d| json!({
            "shield_hp": d.shield_hp, "armor_hp": d.armor_hp, "hull_hp": d.hull_hp,
            "shield_resists": d.shield_resist, "armor_resists": d.armor_resist, "hull_resists": d.hull_resist,
        })),
    }))
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    #[test]
    fn routes_and_neighbourhoods_come_from_the_static_data() {
        let deps = AiDeps::for_tests(facts(&[]));
        let (r, err) = run(&deps, "route", json!({"from": "1DQ1-A", "to": "7-K5EL"}));
        assert!(!err, "{r}");
        assert_eq!(r["jumps"], 2);
        assert_eq!(r["systems"][0]["name"], "1DQ1-A");
        let (w, err) = run(&deps, "systems_within", json!({"system": "1dq1-a", "jumps": 1}));
        assert!(!err, "{w}");
        assert!(w["count"].as_u64().unwrap() >= 2);
        let (s, _) = run(&deps, "system_info", json!({"system": "1DQ1-A"}));
        assert!(s.get("adm").is_none(), "status is not allowed");
        let (e, err) = run(&deps, "route", json!({"from": "Nowhere", "to": "1DQ1-A"}));
        assert!(err && e.as_str().unwrap().contains("no system"));
    }
}
