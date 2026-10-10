//! What the user set up and was told: alerts and their rules, who is friend or foe, and the static
//! facts of wormhole systems and hole types.

use serde_json::{json, Value};

use super::{eve_time, fmt_age, schema, str_arg, u64_arg, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&ALERTS, &ALERT_RULES, &STANDING, &WH_INFO];

static ALERTS: ToolSpec = ToolSpec {
    name: "alerts",
    description: "The alerts the app raised, newest first: what the user was warned about, for 'what did I miss while I was AFK'.",
    need: Need::All(&["intel.reports"]),
    kind: Kind::Read,
    schema: || schema(json!({"since_minutes": {"type": "integer", "minimum": 1, "maximum": 1440}, "query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 60}}), &[]),
    run: |ctx, v| {
        let since = ctx.now - 60 * u64_arg(v, "since_minutes", 120, 1440) as i64;
        let words: Vec<String> = str_arg(v, "query").unwrap_or_default().to_lowercase().split_whitespace().map(str::to_owned).collect();
        let n = u64_arg(v, "limit", 25, 60) as usize;
        let log = ctx.deps.alerts.lock().unwrap_or_else(|e| e.into_inner());
        let out: Vec<Value> = log.iter().rev().filter(|(t, _)| *t >= since).filter(|(_, s)| { let l = s.to_lowercase(); words.iter().all(|w| l.contains(w.as_str())) }).take(n).map(|(t, s)| json!({"when": eve_time(*t), "age": fmt_age(ctx.now, *t), "alert": s})).collect();
        Ok(json!({"alerts": out}))
    },
};

static ALERT_RULES: ToolSpec = ToolSpec {
    name: "alert_rules",
    description: "The user's alert rules: which ones are on, the least severity, systems, regions, range, channels and words \
                  each one watches. Check them before proposing a new rule, so it is not a copy.",
    need: Need::Any(&["actions.settings", "intel.reports"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        Ok(json!(ctx
            .facts
            .alert_rules
            .iter()
            .map(|r| {
                let mut v = json!({"name": r.name, "on": r.enabled, "min_severity": format!("{:?}", r.min_severity)});
                let put = |v: &mut Value, k: &str, list: &[String]| {
                    if !list.is_empty() {
                        v[k] = json!(list);
                    }
                };
                put(&mut v, "systems", &r.systems);
                put(&mut v, "constellations", &r.constellations);
                put(&mut v, "regions", &r.regions);
                put(&mut v, "channels", &r.channels);
                put(&mut v, "mentioning", &r.require);
                put(&mut v, "characters", &r.characters);
                if let Some(j) = r.max_jumps {
                    v["within_jumps"] = json!(j);
                }
                if let Some(c) = r.min_count {
                    v["at_least_hostiles"] = json!(c);
                }
                v
            })
            .collect::<Vec<_>>()))
    },
};

static STANDING: ToolSpec = ToolSpec {
    name: "standing_of",
    description: "Whether an alliance or a pilot is friend or foe to the user: the standing from the user's contacts, and \
                  which of the user's coalitions an alliance belongs to.",
    need: Need::Any(&["pilots.lookup", "pilots.localscan", "characters.locations"]),
    kind: Kind::Read,
    schema: || schema(json!({"name": {"type": "string", "description": "Alliance (name or shorthand) or pilot"}}), &["name"]),
    run: |ctx, v| {
        let name = str_arg(v, "name").ok_or("who?")?.trim().to_owned();
        let low = name.to_lowercase();
        let alliance = super::intel::alliance_of(ctx, &name);
        let character = match (&alliance, ctx.deps.online) {
            (None, true) => crate::http::client(10).ok().and_then(|c| crate::universe::character(&c, &name).ok().flatten()),
            _ => None,
        };
        let (id, shown, kind) = match (&alliance, &character) {
            (Some((id, n)), _) => (*id, n.clone(), "alliance"),
            (None, Some((id, n))) => (*id, n.clone(), "pilot"),
            (None, None) => return Err(format!("no alliance or pilot called {name}")),
        };
        let standing = ctx.deps.standings.lock().unwrap_or_else(|e| e.into_inner()).get(&id).copied();
        let shown_low = shown.to_lowercase();
        let coalitions: Vec<&str> = ctx
            .facts
            .coalitions
            .iter()
            .filter(|c| c.alliances.iter().any(|a| a.to_lowercase() == shown_low || a.to_lowercase() == low))
            .map(|c| c.name.as_str())
            .collect();
        let verdict = match standing {
            Some(s) if s > 0.0 => "friendly",
            Some(s) if s < 0.0 => "hostile",
            _ if !coalitions.is_empty() => "in a coalition the user tracks",
            _ => "no standing set",
        };
        Ok(json!({"name": shown, "is": kind, "standing": standing, "coalitions": coalitions, "verdict": verdict}))
    },
};

static WH_INFO: ToolSpec = ToolSpec {
    name: "wormhole_info",
    description: "Static facts of wormhole space: for a J-space system its class, effect, statics, sun and planets; for a hole \
                  type code (like B274 or K162) where it leads, how long it lives, and its total and per-jump mass.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"system": {"type": "string"}, "type": {"type": "string", "description": "A hole type code"}}), &[]),
    run: |ctx, v| {
        let mut out = json!({});
        if let Some(code) = str_arg(v, "type") {
            let t = crate::whdata::hole_type(code).ok_or_else(|| format!("no hole type {code}"))?;
            out["type"] = json!({
                "code": t.code,
                "from": t.src.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
                "leads_to": format!("{:?}", t.dest),
                "static": t.is_static,
                "lifetime_hours": t.lifetime_h,
                "total_mass_kt": t.total_mass / 1000,
                "max_ship_mass_kt": t.jump_mass / 1000,
            });
        }
        if let Some(n) = str_arg(v, "system") {
            let id = ctx.system(n)?;
            let s = crate::whdata::jsystem(id).ok_or_else(|| format!("{} is not a wormhole system", ctx.system_name(id)))?;
            out["system"] = json!({
                "name": ctx.system_name(id),
                "class": format!("{:?}", s.class),
                "effect": s.effect,
                "statics": s.statics.iter().map(|code| {
                    let t = crate::whdata::hole_type(code);
                    json!({"code": code, "leads_to": t.map(|t| format!("{:?}", t.dest)), "max_ship_mass_kt": t.map(|t| t.jump_mass / 1000)})
                }).collect::<Vec<_>>(),
                "sun": s.sun,
                "planets": s.planets,
                "moons": s.moons,
            });
        }
        if out.as_object().is_some_and(|o| o.is_empty()) {
            return Err("give a system or a hole type".into());
        }
        Ok(out)
    },
};

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    #[test]
    fn alerts_rules_and_standings_come_from_the_users_own_setup() {
        let mut f = facts(&["intel.reports", "pilots.lookup", "actions.settings"]);
        f.alert_rules = vec![crate::settings::default_rule()];
        f.coalitions = vec![crate::settings::Coalition { name: "Imperium".into(), alliances: vec!["Goonswarm Federation".into()], color: None }];
        let deps = AiDeps::for_tests(f);
        let now = crate::clock::utc().timestamp();
        deps.alerts.lock().unwrap().push((now - 60, "Hostiles 2 jumps out".into()));
        deps.alerts.lock().unwrap().push((now - 99_999, "old".into()));
        let (a, err) = run(&deps, "alerts", json!({}));
        assert!(!err);
        assert_eq!(a["alerts"].as_array().unwrap().len(), 1);
        let (r, err) = run(&deps, "alert_rules", json!({}));
        assert!(!err && r.as_array().is_some_and(|x| x.len() == 1), "{r}");
        deps.standings.lock().unwrap().insert(1354830081, 10.0);
        let (s, err) = run(&deps, "standing_of", json!({"name": "goons"}));
        assert!(!err, "{s}");
        assert_eq!(s["verdict"], "friendly");
        assert_eq!(s["coalitions"][0], "Imperium");
    }
}
