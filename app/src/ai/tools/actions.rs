//! Things the assistant may ask to do. None of them happens here: each queues a card in the chat,
//! and the user applies it or not. The model is told the action is waiting for the user, and later
//! what the user chose.

use serde_json::{json, Value};

use super::{schema, str_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&HIGHLIGHT, &FOCUS, &PLAN_ROUTE, &SET_DESTINATION, &ADD_ALERT_RULE, &MAP_DATA, &EDIT_MAP_DATA];

#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    Highlight(Vec<i64>),
    Focus(i64),
    PlanRoute { from: i64, to: i64 },
    SetDestination { system: i64, character: Option<String> },
    AddAlertRule(Box<crate::settings::AlertRule>),
    /// Changes to the jump bridges, cyno generators or sov upgrades, all applied together.
    EditMapData(MapDataEdit),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapDataEdit {
    pub add_bridges: Vec<crate::settings::JumpBridge>,
    pub remove_bridges: Vec<crate::settings::JumpBridge>,
    pub add_cynos: Vec<i64>,
    pub remove_cynos: Vec<i64>,
    pub add_upgrades: Vec<crate::settings::SovUpgrade>,
    pub remove_upgrades: Vec<crate::settings::SovUpgrade>,
}

impl MapDataEdit {
    /// Applies the edit to the three lists; returns whether anything changed.
    pub fn apply(&self, bridges: &mut Vec<crate::settings::JumpBridge>, cynos: &mut Vec<i64>, upgrades: &mut Vec<crate::settings::SovUpgrade>) -> bool {
        let same = |a: &crate::settings::JumpBridge, b: &crate::settings::JumpBridge| (a.from == b.from && a.to == b.to) || (a.from == b.to && a.to == b.from);
        let before = (bridges.len(), cynos.len(), upgrades.len(), bridges.clone(), upgrades.clone());
        bridges.retain(|b| !self.remove_bridges.iter().any(|r| same(r, b)));
        for b in &self.add_bridges {
            if !bridges.iter().any(|k| same(k, b)) {
                bridges.push(b.clone());
            }
        }
        cynos.retain(|c| !self.remove_cynos.contains(c));
        for c in &self.add_cynos {
            if !cynos.contains(c) {
                cynos.push(*c);
            }
        }
        upgrades.retain(|u| !self.remove_upgrades.iter().any(|r| r.system == u.system && (r.upgrade.is_empty() || r.upgrade.eq_ignore_ascii_case(&u.upgrade))));
        for u in &self.add_upgrades {
            if !upgrades.contains(u) {
                upgrades.push(u.clone());
            }
        }
        (bridges.len(), cynos.len(), upgrades.len(), bridges.clone(), upgrades.clone()) != before
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingAction {
    pub id: u64,
    pub kind: ActionKind,
    /// What will happen, in words, for the card.
    pub summary: String,
}

fn queue(ctx: &mut Ctx, kind: ActionKind, summary: String) -> Result<Value, String> {
    // Unique across calls too: the MCP server queues each call's actions on their own.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = ctx.now as u64 * 1000 + SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 1000;
    ctx.actions.push(PendingAction { id, kind, summary: summary.clone() });
    Ok(json!({"status": "waiting for the user to confirm", "action": summary}))
}

static HIGHLIGHT: ToolSpec = ToolSpec {
    name: "highlight_systems",
    description: "Proposes marking systems on the user's map, e.g. a hostile gang's path. The user confirms first.",
    need: Need::All(&["actions.map"]),
    kind: Kind::Action,
    schema: || schema(json!({"systems": {"type": "array", "items": {"type": "string"}, "maxItems": 40}}), &["systems"]),
    run: |ctx, v| {
        let mut ids = Vec::new();
        for s in v.get("systems").and_then(Value::as_array).cloned().unwrap_or_default() {
            ids.push(ctx.system(s.as_str().unwrap_or_default())?);
        }
        if ids.is_empty() {
            return Err("no systems given".into());
        }
        let names: Vec<String> = ids.iter().map(|i| ctx.system_name(*i)).collect();
        queue(ctx, ActionKind::Highlight(ids), format!("Highlight {} on the map", names.join(", ")))
    },
};

static FOCUS: ToolSpec = ToolSpec {
    name: "focus_map",
    description: "Proposes centring the user's map on a system. The user confirms first.",
    need: Need::All(&["actions.map"]),
    kind: Kind::Action,
    schema: || schema(json!({"system": {"type": "string"}}), &["system"]),
    run: |ctx, v| {
        let id = ctx.system(str_arg(v, "system").unwrap_or_default())?;
        let name = ctx.system_name(id);
        queue(ctx, ActionKind::Focus(id), format!("Show {name} on the map"))
    },
};

static PLAN_ROUTE: ToolSpec = ToolSpec {
    name: "plan_route",
    description: "Proposes planning a route in the user's route planner, with its own avoid rules. The user confirms first.",
    need: Need::All(&["actions.route"]),
    kind: Kind::Action,
    schema: || schema(json!({"from": {"type": "string"}, "to": {"type": "string"}}), &["from", "to"]),
    run: |ctx, v| {
        let from = ctx.system(str_arg(v, "from").unwrap_or_default())?;
        let to = ctx.system(str_arg(v, "to").unwrap_or_default())?;
        let s = format!("Plan a route from {} to {}", ctx.system_name(from), ctx.system_name(to));
        queue(ctx, ActionKind::PlanRoute { from, to }, s)
    },
};

static SET_DESTINATION: ToolSpec = ToolSpec {
    name: "set_destination",
    description: "Proposes setting the in-game destination of the active character, or a named one. The user confirms first.",
    need: Need::All(&["actions.destination"]),
    kind: Kind::Action,
    schema: || schema(json!({"system": {"type": "string"}, "character": {"type": "string"}}), &["system"]),
    run: |ctx, v| {
        let system = ctx.system(str_arg(v, "system").unwrap_or_default())?;
        let character = str_arg(v, "character").map(str::to_owned);
        let who = character.clone().unwrap_or_else(|| "the active character".into());
        let s = format!("Set the destination of {who} to {}", ctx.system_name(system));
        queue(ctx, ActionKind::SetDestination { system, character }, s)
    },
};

static ADD_ALERT_RULE: ToolSpec = ToolSpec {
    name: "add_alert_rule",
    description: "Proposes a new intel alert rule: a name, the least severity, optional systems or regions, an optional jump \
                  radius from the user's characters, and report kinds it must mention (bubble, camp, cyno, dropper, \
                  captackled, kill, ess, spike, wormhole, help). The user confirms first.",
    need: Need::All(&["actions.settings"]),
    kind: Kind::Action,
    schema: || {
        schema(
            json!({
                "name": {"type": "string"},
                "min_severity": {"type": "string", "enum": ["Info", "Warning", "Danger", "Critical"]},
                "systems": {"type": "array", "items": {"type": "string"}},
                "regions": {"type": "array", "items": {"type": "string"}},
                "within_jumps": {"type": "integer", "minimum": 1, "maximum": 50},
                "require": {"type": "array", "items": {"type": "string"}}
            }),
            &["name"],
        )
    },
    run: add_alert_rule,
};

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn add_alert_rule(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    use crate::settings::Severity;
    let name = str_arg(v, "name").ok_or("the rule needs a name")?.to_owned();
    let min = match str_arg(v, "min_severity").unwrap_or("Warning") {
        "Info" => Severity::Info,
        "Danger" => Severity::Danger,
        "Critical" => Severity::Critical,
        _ => Severity::Warning,
    };
    let mut systems = Vec::new();
    for s in strings(v, "systems") {
        systems.push(ctx.system_name(ctx.system(&s)?));
    }
    const KINDS: [&str; 10] = ["bubble", "camp", "cyno", "dropper", "captackled", "kill", "ess", "spike", "wormhole", "help"];
    let require: Vec<String> = strings(v, "require").into_iter().map(|s| s.to_lowercase().replace(' ', "")).filter(|s| KINDS.contains(&s.as_str())).collect();
    let rule = crate::settings::AlertRule {
        name: name.clone(),
        enabled: true,
        min_severity: min,
        systems,
        regions: strings(v, "regions"),
        max_jumps: v.get("within_jumps").and_then(Value::as_u64).map(|n| n as u32),
        require,
        ..Default::default()
    };
    let mut parts = vec![format!("{min:?} or worse")];
    if !rule.systems.is_empty() {
        parts.push(format!("in {}", rule.systems.join(", ")));
    }
    if !rule.regions.is_empty() {
        parts.push(format!("in {}", rule.regions.join(", ")));
    }
    if let Some(j) = rule.max_jumps {
        parts.push(format!("within {j} jumps"));
    }
    if !rule.require.is_empty() {
        parts.push(format!("mentioning {}", rule.require.join(" or ")));
    }
    queue(ctx, ActionKind::AddAlertRule(Box::new(rule)), format!("Add alert rule \"{name}\": {}", parts.join(", ")))
}

static MAP_DATA: ToolSpec = ToolSpec {
    name: "map_data",
    description: "The user's own map data as it stands: jump bridges (Ansiblexes), friendly cyno generators and sov upgrades.",
    need: Need::Any(&["actions.settings", "map.cyno"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        Ok(json!({
            "jump_bridges": ctx.facts.jump_bridges.iter().map(|b| format!("{} <> {}", b.from, b.to)).collect::<Vec<_>>(),
            "cyno_generators": ctx.facts.cyno_generators.iter().map(|id| ctx.system_name(*id)).collect::<Vec<_>>(),
            "sov_upgrades": ctx.facts.sov_upgrades.iter().map(|u| json!({"system": u.system, "upgrade": u.upgrade})).collect::<Vec<_>>(),
        }))
    },
};

static EDIT_MAP_DATA: ToolSpec = ToolSpec {
    name: "edit_map_data",
    description: "Proposes changes to the user's jump bridges, cyno generators and sov upgrades in one go: entries to add and                   to remove, read from whatever the user pasted or said (lists, dotlan links, tables, sentences). To change an                   entry, remove the old and add the new. A sov upgrade removal without an upgrade name removes every upgrade                   in that system. The user confirms first. Check map_data for what is there now.",
    need: Need::All(&["actions.settings"]),
    kind: Kind::Action,
    schema: || {
        let bridge = json!({"type": "object", "properties": {"from": {"type": "string"}, "to": {"type": "string"}}, "required": ["from", "to"]});
        let upgrade = json!({"type": "object", "properties": {"system": {"type": "string"}, "upgrade": {"type": "string"}}, "required": ["system"]});
        let list = |item: Value| json!({"type": "array", "items": item, "maxItems": 200});
        schema(
            json!({
                "add_bridges": list(bridge.clone()), "remove_bridges": list(bridge),
                "add_cyno_generators": list(json!({"type": "string"})), "remove_cyno_generators": list(json!({"type": "string"})),
                "add_upgrades": list(upgrade.clone()), "remove_upgrades": list(upgrade),
            }),
            &[],
        )
    },
    run: edit_map_data,
};

fn edit_map_data(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let arr = |k: &str| v.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut unknown: Vec<String> = Vec::new();
    let mut name = |ctx: &Ctx, n: &str| -> Option<String> {
        match ctx.system(n) {
            Ok(id) => Some(ctx.system_name(id)),
            Err(_) => {
                unknown.push(n.to_owned());
                None
            }
        }
    };
    let mut e = MapDataEdit::default();
    for (key, out) in [("add_bridges", 0), ("remove_bridges", 1)] {
        for b in arr(key) {
            let (f, t) = (b["from"].as_str().unwrap_or_default(), b["to"].as_str().unwrap_or_default());
            if let (Some(from), Some(to)) = (name(ctx, f), name(ctx, t)) {
                let jb = crate::settings::JumpBridge { from, to };
                if out == 0 { e.add_bridges.push(jb) } else { e.remove_bridges.push(jb) }
            }
        }
    }
    for (key, out) in [("add_cyno_generators", 0), ("remove_cyno_generators", 1)] {
        for c in arr(key) {
            if let Some(n) = name(ctx, c.as_str().unwrap_or_default()) {
                if let Ok(id) = ctx.system(&n) {
                    if out == 0 { e.add_cynos.push(id) } else { e.remove_cynos.push(id) }
                }
            }
        }
    }
    for (key, out) in [("add_upgrades", 0), ("remove_upgrades", 1)] {
        for u in arr(key) {
            if let Some(system) = name(ctx, u["system"].as_str().unwrap_or_default()) {
                let up = crate::settings::SovUpgrade { system, upgrade: u["upgrade"].as_str().unwrap_or_default().trim().to_owned() };
                if out == 0 && up.upgrade.is_empty() {
                    continue;
                }
                if out == 0 { e.add_upgrades.push(up) } else { e.remove_upgrades.push(up) }
            }
        }
    }
    drop(name);
    let mut parts = Vec::new();
    let mut count = |n: usize, what: &str| {
        if n > 0 {
            parts.push(format!("{n} {what}"));
        }
    };
    count(e.add_bridges.len(), "bridges to add");
    count(e.remove_bridges.len(), "bridges to remove");
    count(e.add_cynos.len(), "cyno generators to add");
    count(e.remove_cynos.len(), "cyno generators to remove");
    count(e.add_upgrades.len(), "upgrades to add");
    count(e.remove_upgrades.len(), "upgrades to remove");
    if parts.is_empty() {
        return Err(if unknown.is_empty() { "nothing to change".into() } else { format!("no known systems among: {}", unknown.join(", ")) });
    }
    let mut summary = format!("Map data: {}", parts.join(", "));
    let detail: Vec<String> = e
        .add_bridges
        .iter()
        .take(6)
        .map(|b| format!("+ {} <> {}", b.from, b.to))
        .chain(e.remove_bridges.iter().take(6).map(|b| format!("- {} <> {}", b.from, b.to)))
        .chain(e.add_upgrades.iter().take(6).map(|u| format!("+ {} {}", u.system, u.upgrade)))
        .chain(e.remove_upgrades.iter().take(6).map(|u| format!("- {} {}", u.system, if u.upgrade.is_empty() { "(all)" } else { &u.upgrade })))
        .collect();
    if !detail.is_empty() {
        summary.push_str(&format!(" ({})", detail.join("; ")));
    }
    let mut out = queue(ctx, ActionKind::EditMapData(e), summary)?;
    if !unknown.is_empty() {
        out["skipped_unknown_systems"] = json!(unknown);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use crate::ai::deps::AiDeps;

    #[test]
    fn actions_only_queue_a_card() {
        let deps = AiDeps::for_tests(facts(&["actions"]));
        let f = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = Ctx { deps: &deps, facts: &f, store: None, now: 1_000, actions: &mut actions };
        let (out, err) = super::super::dispatch(&mut ctx, "focus_map", &json!({"system": "1DQ1-A"}));
        assert!(!err, "{out}");
        assert!(out.contains("waiting for the user"));
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, ActionKind::Focus(30_004_759));
        let mut ctx = Ctx { deps: &deps, facts: &f, store: None, now: 1_000, actions: &mut actions };
        let (_, err) = super::super::dispatch(&mut ctx, "add_alert_rule", &json!({"name": "Bubbles home", "require": ["Bubble", "nonsense"], "within_jumps": 5}));
        assert!(!err);
        match &actions[1].kind {
            ActionKind::AddAlertRule(r) => assert_eq!(r.require, vec!["bubble".to_owned()]),
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn map_data_edits_resolve_names_and_apply_together() {
        let deps = AiDeps::for_tests(facts(&["actions.settings"]));
        let f = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = Ctx { deps: &deps, facts: &f, store: None, now: 1, actions: &mut actions };
        let (out, err) = super::super::dispatch(
            &mut ctx,
            "edit_map_data",
            &json!({"add_bridges": [{"from": "1dq1-a", "to": "7-K5EL"}, {"from": "Nowhere", "to": "1DQ1-A"}], "add_upgrades": [{"system": "1DQ1-A", "upgrade": "Cynosural Suppression"}], "remove_upgrades": [{"system": "7-K5EL"}]}),
        );
        assert!(!err, "{out}");
        assert!(out.contains("Nowhere"), "unknown names are reported back: {out}");
        let ActionKind::EditMapData(e) = &actions[0].kind else { panic!() };
        assert_eq!(e.add_bridges, vec![crate::settings::JumpBridge { from: "1DQ1-A".into(), to: "7-K5EL".into() }]);
        let mut bridges = vec![crate::settings::JumpBridge { from: "7-K5EL".into(), to: "1DQ1-A".into() }];
        let mut cynos = vec![];
        let mut ups = vec![crate::settings::SovUpgrade { system: "7-K5EL".into(), upgrade: "Ore Prospecting 3".into() }];
        assert!(e.apply(&mut bridges, &mut cynos, &mut ups));
        assert_eq!(bridges.len(), 1, "the same bridge the other way round is not added twice");
        assert_eq!(ups, vec![crate::settings::SovUpgrade { system: "1DQ1-A".into(), upgrade: "Cynosural Suppression".into() }]);
    }

    #[test]
    fn actions_need_their_own_permission() {
        let deps = AiDeps::for_tests(facts(&["actions.map"]));
        let (_, err) = run(&deps, "set_destination", json!({"system": "1DQ1-A"}));
        assert!(err);
    }
}
