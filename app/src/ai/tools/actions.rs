//! Things the assistant may ask to do. None of them happens here: each queues a card in the chat,
//! and the user applies it or not. The model is told the action is waiting for the user, and later
//! what the user chose.

use serde_json::{json, Value};

use super::{schema, str_arg, Ctx, Kind, Need, ToolSpec};

pub static TOOLS: &[&ToolSpec] = &[&HIGHLIGHT, &FOCUS, &PLAN_ROUTE, &SET_DESTINATION, &ADD_ALERT_RULE, &MAP_DATA, &EDIT_MAP_DATA, &SEND_JABBER, &OPEN_CHAT, &JABBER_WINDOWS];

#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    /// The watch asked whether to carry on: apply keeps it going, dismiss stops it.
    KeepWatching(u64),
    Highlight(Vec<i64>),
    Focus(i64),
    PlanRoute { from: i64, to: i64 },
    SetDestination { system: i64, character: Option<String> },
    AddAlertRule(Box<crate::settings::AlertRule>),
    /// Changes to the jump bridges, cyno generators or sov upgrades, all applied together.
    EditMapData(MapDataEdit),
    /// A Jabber message, to a room or a person, sent only on the user's click.
    SendJabber { to: String, room: bool, join: bool, body: String, broadcast: bool },
    /// Shows a conversation: in the Jabber tab, a new window, or a window already open, brought to
    /// the front. Only shows what is on the user's screen already, so it happens without a click.
    OpenChat { jid: String, window: ChatWindowPick },
    /// Moves the user's Mumble into a channel.
    JoinMumble { url: String },
    /// The user's own Mumble: mute, deafen, how it transmits.
    MumbleSet { mute: Option<bool>, deaf: Option<bool>, transmit: Option<u32> },
    /// Anything done to a fleet. Never done without the user's yes, said or clicked.
    Fleet(FleetOp),
}

/// A fleet action the assistant proposed.
#[derive(Clone, Debug, PartialEq)]
pub enum FleetOp {
    /// Track the fleet the start form now describes; waits for the FC to be boss if they are not.
    Start,
    /// A dashboard action on a tracked fleet: kicks, close, MOTD, wings, or an update of its
    /// doctrine, channels and snowflakes.
    Act(crate::fleets::model::FleetId, crate::fleets::backend::Action),
    /// Make the br.evetools report of the fleet open in the app.
    CreateBr(String),
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

pub(crate) fn queue_pub(ctx: &mut Ctx, kind: ActionKind, summary: String) -> Result<Value, String> {
    queue(ctx, kind, summary)
}

fn queue(ctx: &mut Ctx, kind: ActionKind, summary: String) -> Result<Value, String> {
    let now = kind.immediate(&ctx.facts.ai.auto_actions);
    // Unique across calls too: the MCP server queues each call's actions on their own.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = ctx.now as u64 * 1000 + SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 1000;
    ctx.actions.push(PendingAction { id, kind, summary: summary.clone() });
    Ok(json!({"status": if now { "done; the user lets you do this without asking" } else { "waiting for the user to confirm" }, "action": summary}))
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
    need: Need::Any(&["map.bridges", "map.cyno"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        // Each part only as far as it is allowed.
        let mut out = json!({});
        if ctx.facts.allowed("map.bridges") {
            out["jump_bridges"] = json!(ctx.facts.jump_bridges.iter().map(|b| format!("{} <> {}", b.from, b.to)).collect::<Vec<_>>());
            out["sov_upgrades"] = json!(ctx.facts.sov_upgrades.iter().map(|u| json!({"system": u.system, "upgrade": u.upgrade})).collect::<Vec<_>>());
        }
        if ctx.facts.allowed("map.cyno") {
            out["cyno_generators"] = json!(ctx.facts.cyno_generators.iter().map(|id| ctx.system_name(*id)).collect::<Vec<_>>());
        }
        Ok(out)
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

#[derive(Clone, Debug, PartialEq)]
pub enum ChatWindowPick {
    Main,
    New,
    Existing(u64),
}

impl ActionKind {
    /// The Data access key the action falls under, for doing it without asking. None for what
    /// always asks.
    pub fn perm_key(&self) -> Option<&'static str> {
        match self {
            ActionKind::Highlight(_) | ActionKind::Focus(_) => Some("actions.map"),
            ActionKind::PlanRoute { .. } => Some("actions.route"),
            ActionKind::SetDestination { .. } => Some("actions.destination"),
            ActionKind::AddAlertRule(_) | ActionKind::EditMapData(_) => Some("actions.settings"),
            ActionKind::SendJabber { broadcast: false, .. } => Some("actions.jabber"),
            ActionKind::JoinMumble { .. } | ActionKind::MumbleSet { .. } => Some("actions.mumble"),
            // Fleet actions are never let through without asking, so they have no key to allow.
            ActionKind::SendJabber { broadcast: true, .. } | ActionKind::KeepWatching(_) | ActionKind::OpenChat { .. } | ActionKind::Fleet(_) => None,
        }
    }

    /// Carried out as soon as it is proposed: opening a window always, the rest when the user said so.
    pub fn immediate(&self, auto: &[String]) -> bool {
        matches!(self, ActionKind::OpenChat { .. }) || self.perm_key().is_some_and(|k| auto.iter().any(|a| a == k))
    }
}

static OPEN_CHAT: ToolSpec = ToolSpec {
    name: "open_chat",
    description: "Opens a Jabber conversation for the user and brings it to the front: in the Jabber tab, in a new window, or \
                  in one of the chat windows already open (see jabber_windows). Done at once, it sends nothing.",
    need: Need::Any(&["jabber.chats", "actions.jabber"]),
    kind: Kind::Action,
    schema: || {
        schema(
            json!({
                "conversation": {"type": "string", "description": "Room or person, by name, contact name or address"},
                "kind": {"type": "string", "enum": ["room", "person"]},
                "window": {"type": "string", "description": "main, new, or the number of an open chat window"}
            }),
            &["conversation"],
        )
    },
    run: |ctx, v| {
        let want = str_arg(v, "conversation").ok_or("which conversation?")?.to_owned();
        let t = {
            let st = ctx.deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
            resolve_jabber(&st, &ctx.facts.jabber_domain, &want, str_arg(v, "kind"))?
        };
        let window = match str_arg(v, "window").map(str::trim).unwrap_or("main") {
            "new" => ChatWindowPick::New,
            "main" | "" => ChatWindowPick::Main,
            n => {
                let id: u64 = n.trim_start_matches('#').parse().map_err(|_| format!("no chat window {n}; see jabber_windows"))?;
                if !ctx.facts.chat_windows.iter().any(|(w, _)| *w == id) {
                    return Err(format!("no chat window {id} is open; see jabber_windows"));
                }
                ChatWindowPick::Existing(id)
            }
        };
        let summary = format!("Opened {}", t.label);
        queue(ctx, ActionKind::OpenChat { jid: t.jid, window }, summary)
    },
};

static JABBER_WINDOWS: ToolSpec = ToolSpec {
    name: "jabber_windows",
    description: "The chat windows the user has popped out, by number, with the conversations open in each.",
    need: Need::Any(&["jabber.chats", "actions.jabber"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        Ok(json!(ctx
            .facts
            .chat_windows
            .iter()
            .map(|(id, tabs)| json!({"window": id, "conversations": tabs.iter().map(|t| t.split('@').next().unwrap_or(t)).collect::<Vec<_>>()}))
            .collect::<Vec<_>>()))
    },
};

/// The commands that reach a whole coalition. Never written unless the user asked for them by name.
const BROADCAST: [&str; 2] = ["!bping", "!bcast"];

static SEND_JABBER: ToolSpec = ToolSpec {
    name: "send_jabber",
    description: "Writes a Jabber message to a room or a person, for the user to check and send with a click: a room not \
                  joined yet is joined first, and a person with no conversation yet gets a new one. Only when the user clearly \
                  asked you to write or send a message; if in doubt, ask them first. Never use the !bping or !bcast commands \
                  unless the user asked for that command by name.",
    need: Need::All(&["actions.jabber"]),
    kind: Kind::Action,
    schema: || {
        schema(
            json!({
                "conversation": {"type": "string", "description": "Room or person, by name, contact name or address; spacing, underscores and case do not matter"},
                "kind": {"type": "string", "enum": ["room", "person"], "description": "Say which when the name could be either, or for a room not joined yet"},
                "text": {"type": "string"}
            }),
            &["conversation", "text"],
        )
    },
    run: send_jabber,
};

/// A name as Jabber spells it: lower case, with spaces, hyphens and dots as underscores.
pub fn jabber_norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.trim().chars() {
        let c = c.to_lowercase().next().unwrap_or(c);
        out.push(if matches!(c, ' ' | '-' | '.' | '_') { '_' } else { c });
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_owned()
}

/// Where a message goes: a room or a person, whether the room has to be joined first, and whether
/// there is no conversation with them yet.
#[derive(Clone, Debug, PartialEq)]
pub struct JabberTarget {
    pub jid: String,
    pub room: bool,
    pub join: bool,
    pub new: bool,
    pub label: String,
}

/// Finds a room or a person by address, by the part before the @, or by a contact's name; spaces,
/// underscores, hyphens, dots and case do not matter. Unknown names become a new conversation on
/// the user's own server, or a room on its conference service, for the user to check on the card.
pub fn resolve_jabber(st: &crate::jabber::JabberState, own_domain: &str, want: &str, kind: Option<&str>) -> Result<JabberTarget, String> {
    let local = |jid: &str| jid.split('@').next().unwrap_or(jid).to_owned();
    let is_room = |jid: &str| st.rooms.contains(jid) || st.rooms_left.contains(jid) || st.rooms_inaccessible.contains(jid) || jid.split('@').nth(1).is_some_and(|d| d.starts_with("conference."));
    // (jid, shown name)
    let mut known: Vec<(String, String)> = Vec::new();
    for r in st.rooms.iter().chain(&st.rooms_left).chain(&st.rooms_inaccessible).chain(st.chats.keys()) {
        if !known.iter().any(|(j, _)| j == r) {
            known.push((r.clone(), local(r)));
        }
    }
    for (jid, c) in &st.roster {
        match known.iter_mut().find(|(j, _)| j == jid) {
            Some(k) => k.1 = c.name.clone().unwrap_or_else(|| local(jid)),
            None => known.push((jid.clone(), c.name.clone().unwrap_or_else(|| local(jid)))),
        }
    }
    let wanted_room = kind.map(|k| k.eq_ignore_ascii_case("room"));
    let fits_kind = |jid: &str| wanted_room.is_none_or(|r| r == is_room(jid));
    let target = |jid: &str, label: &str| JabberTarget {
        jid: jid.to_owned(),
        room: is_room(jid),
        join: is_room(jid) && !st.rooms.contains(jid),
        new: !st.chats.contains_key(jid) && !st.rooms.contains(jid),
        label: label.to_owned(),
    };
    let w = want.trim();
    if w.contains('@') {
        let jid = w.to_lowercase();
        let label = known.iter().find(|(j, _)| *j == jid).map_or_else(|| local(&jid), |(_, l)| l.clone());
        return Ok(target(&jid, &label));
    }
    let n = jabber_norm(w);
    if n.is_empty() {
        return Err("to whom?".into());
    }
    let exact: Vec<&(String, String)> = known.iter().filter(|(j, l)| fits_kind(j) && (jabber_norm(&local(j)) == n || jabber_norm(l) == n)).collect();
    let hits: Vec<&(String, String)> =
        if exact.is_empty() { known.iter().filter(|(j, l)| fits_kind(j) && (jabber_norm(&local(j)).contains(&n) || jabber_norm(l).contains(&n))).collect() } else { exact };
    match hits.as_slice() {
        [one] => Ok(target(&one.0, &one.1)),
        [] => {
            let domain = own_domain.trim();
            if domain.is_empty() {
                return Err(format!("no Jabber room or contact matches {want:?}, and the user's own server is not set up"));
            }
            if wanted_room == Some(true) {
                // A room on the conference service the joined rooms use, else the server's usual one.
                let conf = st.rooms.iter().chain(st.chats.keys()).find_map(|r| r.split('@').nth(1).filter(|d| d.starts_with("conference.")).map(str::to_owned)).unwrap_or_else(|| format!("conference.{domain}"));
                let jid = format!("{n}@{conf}");
                Ok(JabberTarget { jid: jid.clone(), room: true, join: true, new: true, label: n })
            } else {
                let jid = format!("{n}@{domain}");
                Ok(JabberTarget { jid, room: false, join: false, new: true, label: w.to_owned() })
            }
        }
        many => Err(format!("several match {want:?}: {}; ask which", many.iter().take(8).map(|(j, l)| format!("{l} ({j})")).collect::<Vec<_>>().join(", "))),
    }
}

fn send_jabber(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let want = str_arg(v, "conversation").ok_or("to whom?")?.to_owned();
    let body = str_arg(v, "text").ok_or("what should it say?")?.trim().to_owned();
    if body.is_empty() {
        return Err("the message is empty".into());
    }
    let lower = body.to_lowercase();
    let broadcast = BROADCAST.iter().any(|c| lower.contains(c));
    if broadcast {
        let asked = ctx.deps.last_question.lock().unwrap_or_else(|e| e.into_inner()).to_lowercase();
        let own = ctx.facts.ai.instructions.to_lowercase();
        // Asked for by name in this question, or allowed in the user's own instructions at their risk.
        let named = BROADCAST.iter().filter(|c| lower.contains(*c)).all(|c| asked.contains(&c[1..]) || own.contains(&c[1..]));
        if !named {
            return Err("broadcast commands (!bping, !bcast) are only written when the user asks for that command by name; ask them".into());
        }
    }
    let t = {
        let st = ctx.deps.jabber.lock().unwrap_or_else(|e| e.into_inner());
        resolve_jabber(&st, &ctx.facts.jabber_domain, &want, str_arg(v, "kind"))?
    };
    let what = match (t.room, t.join, t.new) {
        (true, true, _) => format!("Join room {} ({}) and send", t.label, t.jid),
        (true, false, _) => format!("Send to room {}", t.label),
        (false, _, true) => format!("Start a conversation with {} ({})", t.label, t.jid),
        (false, _, false) => format!("Send to {}", t.label),
    };
    let summary = if broadcast { format!("BROADCAST. {what}: \"{body}\"") } else { format!("{what}: \"{body}\"") };
    queue(ctx, ActionKind::SendJabber { to: t.jid, room: t.room, join: t.join, body, broadcast }, summary)
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
    fn jabber_messages_wait_for_a_click_and_broadcasts_need_asking_by_name() {
        let deps = AiDeps::for_tests(facts(&["actions.jabber"]));
        {
            let mut j = deps.jabber.lock().unwrap();
            j.rooms.insert("ops@conference.example.invalid".into());
            j.rooms.insert("ops-chat@conference.example.invalid".into());
            j.chats.insert("someone@example.invalid".into(), Vec::new());
        }
        let f = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = Ctx { deps: &deps, facts: &f, store: None, now: 1, actions: &mut actions };
        let (out, err) = super::super::dispatch(&mut ctx, "send_jabber", &json!({"conversation": "ops", "text": "x up for the Muninn fleet"}));
        assert!(!err, "{out}");
        let (out, err) = super::super::dispatch(&mut ctx, "send_jabber", &json!({"conversation": "op", "text": "hi"}));
        assert!(err && out.contains("several"), "a vague name is asked about: {out}");
        {
            let mut j = deps.jabber.lock().unwrap();
            j.roster.insert("xenuria_thrax@example.invalid".into(), crate::jabber::Contact { name: Some("Xenuria Thrax".into()), groups: vec![], presence: Default::default(), status_text: String::new(), sub: Default::default() });
            j.rooms_left.insert("skirmish_commanders@conference.example.invalid".into());
        }
        let st = deps.jabber.lock().unwrap();
        let t = resolve_jabber(&st, "example.invalid", "Skirmish Commanders", None).unwrap();
        assert!(t.room && t.join, "a room left is joined again: {t:?}");
        let t = resolve_jabber(&st, "example.invalid", "xenuria thrax", None).unwrap();
        assert_eq!((t.jid.as_str(), t.room, t.new), ("xenuria_thrax@example.invalid", false, true));
        let t = resolve_jabber(&st, "example.invalid", "Some New-Pilot", Some("person")).unwrap();
        assert_eq!(t.jid, "some_new_pilot@example.invalid", "a new person, spelled the Jabber way");
        let t = resolve_jabber(&st, "example.invalid", "capital ops", Some("room")).unwrap();
        assert_eq!((t.jid.as_str(), t.join), ("capital_ops@conference.example.invalid", true));
        assert!(resolve_jabber(&st, "", "nobody here", None).is_err());
        drop(st);
        *deps.last_question.lock().unwrap() = "tell the ops room the fleet is up".into();
        let (out, err) = super::super::dispatch(&mut ctx, "send_jabber", &json!({"conversation": "ops", "text": "!bping fleet up"}));
        assert!(err && out.contains("by name"), "{out}");
        *deps.last_question.lock().unwrap() = "send a bping to ops: fleet up".into();
        let (out, err) = super::super::dispatch(&mut ctx, "send_jabber", &json!({"conversation": "ops", "text": "!bping fleet up"}));
        assert!(!err, "{out}");
        assert!(matches!(&actions[0].kind, ActionKind::SendJabber { room: true, broadcast: false, .. }));
        assert!(matches!(&actions[1].kind, ActionKind::SendJabber { broadcast: true, .. }));
        assert!(actions[1].summary.starts_with("BROADCAST"));
    }

    #[test]
    fn kinds_the_user_lets_through_run_at_once_and_broadcasts_never_do() {
        let mut f = facts(&["actions"]);
        f.ai.auto_actions = vec!["actions.destination".into(), "actions.jabber".into()];
        let deps = AiDeps::for_tests(f);
        let (out, err) = run(&deps, "set_destination", json!({"system": "1DQ1-A"}));
        assert!(!err && out["status"].as_str().unwrap().starts_with("done"), "{out}");
        let (out, _) = run(&deps, "focus_map", json!({"system": "1DQ1-A"}));
        assert!(out["status"].as_str().unwrap().starts_with("waiting"), "map actions still ask: {out}");
        let auto = vec!["actions.jabber".to_owned()];
        assert!(ActionKind::SendJabber { to: "x".into(), room: true, join: false, body: "hi".into(), broadcast: false }.immediate(&auto));
        assert!(!ActionKind::SendJabber { to: "x".into(), room: true, join: false, body: "!bping".into(), broadcast: true }.immediate(&auto), "a broadcast always asks");
        assert!(!ActionKind::KeepWatching(1).immediate(&["actions.settings".into()]));
    }

    #[test]
    fn actions_need_their_own_permission() {
        let deps = AiDeps::for_tests(facts(&["actions.map"]));
        let (_, err) = run(&deps, "set_destination", json!({"system": "1DQ1-A"}));
        assert!(err);
    }
}
