//! Things the assistant may ask to do. None of them happens here: each queues a card in the chat,
//! and the user applies it or not. The model is told the action is waiting for the user, and later
//! what the user chose.

use serde_json::{json, Value};

use super::{schema, str_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&HIGHLIGHT, &FOCUS, &PLAN_ROUTE, &SET_DESTINATION, &ADD_ALERT_RULE];

#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    Highlight(Vec<i64>),
    Focus(i64),
    PlanRoute { from: i64, to: i64 },
    SetDestination { system: i64, character: Option<String> },
    AddAlertRule(Box<crate::settings::AlertRule>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingAction {
    pub id: u64,
    pub kind: ActionKind,
    /// What will happen, in words, for the card.
    pub summary: String,
}

fn queue(ctx: &mut Ctx, kind: ActionKind, summary: String) -> Result<Value, String> {
    let id = ctx.now as u64 * 1000 + ctx.actions.len() as u64;
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
    fn actions_need_their_own_permission() {
        let deps = AiDeps::for_tests(facts(&["actions.map"]));
        let (_, err) = run(&deps, "set_destination", json!({"system": "1DQ1-A"}));
        assert!(err);
    }
}
