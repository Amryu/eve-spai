//! Running fleets for the user: starting one from a preset, and every dashboard action on the fleet
//! open in the app. Each is proposed as a card and read out; it happens only on the user's yes, said
//! in a later message (confirm_fleet_action) or clicked.

use serde_json::{json, Value};

use super::actions::{queue_pub, ActionKind, FleetOp};
use super::{schema, str_arg, Ctx, Kind, Need, ToolSpec};
use crate::fleets::backend::Action;
use crate::fleets::model::{ChannelItem, SnowflakeType};

pub static TOOLS: &[&ToolSpec] = &[&FLEET_FORM, &FLEET_START, &FLEET_KICK, &FLEET_COMMAND, &FLEET_EDIT, &FLEET_BR, &FLEET_LINKS, &CONFIRM];

/// What a proposal returns: the action waits for a yes in the user's next message.
const ASK: &str = "Not done yet. Say in one short sentence what will happen and ask for a yes. When the user's next \
                   message agrees, call confirm_fleet_action with this action_id; anything else, leave it.";

fn propose(ctx: &mut Ctx, op: FleetOp, summary: String) -> Result<Value, String> {
    let mut v = queue_pub(ctx, ActionKind::Fleet(op), summary)?;
    v["action_id"] = json!(ctx.actions.last().map(|a| a.id));
    v["status"] = json!(ASK);
    Ok(v)
}

fn lower(s: &str) -> String {
    s.trim().to_lowercase()
}

/// A channel from a list by its name, or the first free one for "free".
fn channel(list: &[ChannelItem], want: &str) -> Result<Option<crate::fleets::model::ChannelId>, String> {
    let w = lower(want);
    if w == "free" {
        let pick = crate::fleets::state::pick_free(list, None);
        return pick.id.map(Some).ok_or_else(|| "every channel of that kind is in use".to_owned());
    }
    if w == "none" {
        return Ok(None);
    }
    let found = list.iter().find(|c| lower(&c.name) == w).or_else(|| list.iter().find(|c| lower(&c.name).contains(&w)));
    found.map(|c| Some(c.id)).ok_or_else(|| format!("no channel called {want}; channels: {}", list.iter().map(|c| c.name.trim()).collect::<Vec<_>>().join(", ")))
}

fn channel_name(list: &[ChannelItem], id: Option<crate::fleets::model::ChannelId>) -> Option<String> {
    id.and_then(|id| list.iter().find(|c| c.id == id)).map(|c| c.name.trim().to_owned())
}

fn in_use(list: &[ChannelItem], id: Option<crate::fleets::model::ChannelId>) -> Option<String> {
    id.and_then(|id| list.iter().find(|c| c.id == id && c.is_in_use)).map(|c| c.name.trim().to_owned())
}

static FLEET_FORM: ToolSpec = ToolSpec {
    name: "fleet_form",
    description: "The fleet start form as it stands: presets to start from, the name, FC, doctrine, formup, comms, logi \
                  and boost channels (and whether another fleet uses them), tags, snowflakes, whether the FC is fleet \
                  boss, and what is still missing before it can be tracked. Read it before fleet_start.",
    need: Need::All(&["actions.fleet"]),
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let st = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner());
        let f = &st.draft.form;
        let s = &st.seed;
        let tag = |id: &crate::fleets::model::TagId| s.tags.iter().find(|t| t.id == *id).map(|t| t.name.clone());
        let boss = st.fc().and_then(|(id, _)| st.boss.as_ref().filter(|(who, _)| *who == id)).map(|(_, c)| c.verdict());
        Ok(json!({
            "presets": ctx.facts.fleet_presets.iter().map(|p| json!({"preset": p.label, "folder": p.folder})).collect::<Vec<_>>(),
            "name": f.name,
            "fc": st.fc().map(|(_, n)| n),
            "fc_is_boss": boss.as_ref().map(|b| b.0),
            "boss_note": boss.map(|b| b.1).filter(|n| !n.is_empty()),
            "waiting_for_boss": st.track_waiting,
            "doctrine": s.setups.iter().find(|x| x.id.0 == f.setup_id).map(|x| x.name.clone()),
            "doctrine_notes": f.doctrine_notes,
            "formup": st.draft.formup.as_ref().map(|l| l.label.clone()),
            "comms": channel_name(&s.mumble_channels, f.mumble_channel_id),
            "logi": channel_name(&s.logi_channels, f.logi_channel_id),
            "boost": channel_name(&s.boost_channels, f.boost_channel_id),
            "channels_in_use": st.channels_in_use().into_iter().map(|(w, n)| format!("{w}: {n}")).collect::<Vec<_>>(),
            "tags": st.draft.tags.iter().filter_map(tag).collect::<Vec<_>>(),
            "snowflakes": st.draft.snowflakes.iter().map(|x| format!("{} ({})", x.character_name, x.kind.label())).collect::<Vec<_>>(),
            "missing": st.missing(),
            "already_tracked_as": st.already_tracking().map(|(_, n)| n),
            "doctrines": s.setups.iter().map(|x| x.name.clone()).collect::<Vec<_>>(),
        }))
    },
};

static FLEET_START: ToolSpec = ToolSpec {
    name: "fleet_start",
    description: "Fills the fleet start form (from a preset and/or the fields given) and proposes tracking the fleet. \
                  Channels take a name, \"free\" for an unused one, or \"none\". When a picked channel is in use by another \
                  fleet it does not propose anything: ask the user whether to keep it anyway, take free ones, or which to \
                  use, then call again with in_use set. Missing fields come back as an error to ask about. Tracking \
                  waits for the FC to become fleet boss when they are not yet.",
    need: Need::All(&["actions.fleet"]),
    kind: Kind::Action,
    schema: || {
        schema(
            json!({
                "preset": {"type": "string"},
                "name": {"type": "string"},
                "doctrine": {"type": "string"},
                "doctrine_notes": {"type": "string", "description": "Optional extra doctrine text"},
                "formup": {"type": "string", "description": "Formup system"},
                "comms": {"type": "string"},
                "logi": {"type": "string"},
                "boost": {"type": "string"},
                "description": {"type": "string"},
                "in_use": {"type": "string", "enum": ["ask", "keep", "pick_free"], "description": "What to do with channels another fleet uses; ask by default"}
            }),
            &[],
        )
    },
    run: fleet_start,
};

fn fleet_start(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let formup = match str_arg(v, "formup") {
        Some(s) => {
            let id = ctx.system(s)?;
            Some(crate::fleets::model::Labelled { id, label: ctx.system_name(id) })
        }
        None => None,
    };
    let summary = {
        let mut st = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, name)) = st.already_tracking() {
            return Err(format!("this FC is already boss of a tracked fleet: {name}"));
        }
        if let Some(p) = str_arg(v, "preset") {
            let w = lower(p);
            let preset = ctx.facts.fleet_presets.iter().find(|x| lower(&x.label) == w).or_else(|| ctx.facts.fleet_presets.iter().find(|x| lower(&x.label).contains(&w)));
            let preset = preset.ok_or_else(|| format!("no preset called {p}"))?.clone();
            st.apply_preset(&preset);
        }
        if let Some(n) = str_arg(v, "name") {
            st.draft.form.name = n.trim().to_owned();
        }
        if let Some(d) = str_arg(v, "doctrine") {
            let w = lower(d);
            let setup = st.seed.setups.iter().find(|x| lower(&x.name) == w).or_else(|| st.seed.setups.iter().find(|x| lower(&x.name).contains(&w)));
            st.draft.form.setup_id = setup.ok_or_else(|| format!("no doctrine called {d}"))?.id.0;
        }
        if let Some(n) = str_arg(v, "doctrine_notes") {
            st.draft.form.doctrine_notes = Some(n.trim().to_owned()).filter(|s| !s.is_empty());
        }
        if let Some(d) = str_arg(v, "description") {
            st.draft.form.description = d.trim().to_owned();
        }
        if formup.is_some() {
            st.draft.formup = formup.clone();
        }
        if let Some(c) = str_arg(v, "comms") {
            st.draft.form.mumble_channel_id = channel(&st.seed.mumble_channels, c)?;
        }
        if let Some(c) = str_arg(v, "logi") {
            st.draft.form.logi_channel_id = channel(&st.seed.logi_channels, c)?;
        }
        if let Some(c) = str_arg(v, "boost") {
            st.draft.form.boost_channel_id = channel(&st.seed.boost_channels, c)?;
        }
        let missing = st.missing();
        if !missing.is_empty() {
            return Err(format!("the form still needs {}; ask the user for it", missing.join(", ")));
        }
        let busy = st.channels_in_use();
        if !busy.is_empty() {
            match str_arg(v, "in_use").unwrap_or("ask") {
                "keep" => {}
                "pick_free" => st.free_channels(),
                _ => {
                    let list: Vec<String> = busy.iter().map(|(w, n)| format!("{w} {n}")).collect();
                    return Ok(json!({
                        "needs_answer": format!("Another fleet already uses {}. Ask the user: keep them anyway, take free ones, or which to use instead.", list.join(" and ")),
                    }));
                }
            }
        }
        let f = &st.draft.form;
        let doctrine = st.seed.setups.iter().find(|x| x.id.0 == f.setup_id).map(|x| x.name.clone()).unwrap_or_default();
        let comms = channel_name(&st.seed.mumble_channels, f.mumble_channel_id).unwrap_or_default();
        let fc = st.fc().map(|(_, n)| n).unwrap_or_default();
        let at = st.draft.formup.as_ref().map(|l| l.label.clone()).unwrap_or_default();
        format!("Track fleet \"{}\": {doctrine}, formup {at}, comms {comms}, FC {fc}", f.name)
    };
    propose(ctx, FleetOp::Start, summary)
}

/// The fleet open in the app, which every action on a running fleet works on.
fn open_fleet(ctx: &Ctx) -> Result<crate::fleets::state::OpenFleet, String> {
    let st = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner());
    st.open.value.clone().ok_or_else(|| "no fleet is open in the app; open the fleet first".to_owned())
}

static FLEET_KICK: ToolSpec = ToolSpec {
    name: "fleet_kick",
    description: "Proposes kicking pilots from the fleet open in the app: by name, by hull, or both (\"kick Bob in the Hulk\" \
                  is pilot Bob, ship Hulk; \"kick everyone in Hulks\" is ship Hulk alone).",
    need: Need::All(&["actions.fleet"]),
    kind: Kind::Action,
    schema: || schema(json!({"pilots": {"type": "array", "items": {"type": "string"}}, "ship": {"type": "string"}}), &[]),
    run: |ctx, v| {
        let open = open_fleet(ctx)?;
        let names: Vec<String> = v.get("pilots").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(lower).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
        let ship = str_arg(v, "ship").map(lower).filter(|s| !s.is_empty());
        if names.is_empty() && ship.is_none() {
            return Err("who? give pilots, a ship, or both".into());
        }
        let hits: Vec<&crate::fleets::model::Member> = open
            .composition
            .members()
            .filter(|m| names.is_empty() || names.iter().any(|n| lower(&m.name) == *n || lower(&m.name).contains(n.as_str())))
            .filter(|m| ship.as_ref().is_none_or(|s| lower(&m.ship_type_name) == *s || lower(&m.ship_type_name).contains(s.as_str()) || lower(&m.ship_group).contains(s.as_str())))
            .collect();
        if hits.is_empty() {
            return Err("nobody in the fleet matches that".into());
        }
        let who: Vec<String> = hits.iter().map(|m| format!("{} ({})", m.name, m.ship_type_name)).collect();
        let action = if hits.len() == 1 {
            Action::Kick { character_id: hits[0].character_id, exclude: false }
        } else {
            Action::KickMany { character_ids: hits.iter().map(|m| m.character_id).collect(), exclude: false }
        };
        propose(ctx, FleetOp::Act(open.fleet.id.clone(), action), format!("Kick {} from {}", who.join(", "), open.fleet.name))
    },
};

static FLEET_COMMAND: ToolSpec = ToolSpec {
    name: "fleet_command",
    description: "Proposes one of the fleet-wide actions on the fleet open in the app: close it, set the MOTD from the \
                  fleet's ping, add a wing, kick the pods, kick everyone, or kick the pilots off doctrine.",
    need: Need::All(&["actions.fleet"]),
    kind: Kind::Action,
    schema: || schema(json!({"action": {"type": "string", "enum": ["close", "set_motd", "add_wing", "kick_pods", "kick_everyone", "kick_off_doctrine"]}}), &["action"]),
    run: |ctx, v| {
        let open = open_fleet(ctx)?;
        let name = open.fleet.name.clone();
        let (action, summary) = match str_arg(v, "action").unwrap_or_default() {
            "close" => (Action::Close, format!("Close {name}")),
            "set_motd" => (Action::SetMotd, format!("Set the MOTD of {name}")),
            "add_wing" => (Action::AddWing, format!("Add a wing to {name}")),
            "kick_pods" => (Action::KickCapsules, format!("Kick every pod from {name}")),
            "kick_everyone" => (Action::KickAll, format!("Kick everyone from {name}")),
            "kick_off_doctrine" => {
                let locked = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner()).locked.clone();
                let off = crate::fleets::doctrine::off_doctrine_kickable(&open.composition, open.doctrine.as_ref(), &locked);
                if off.is_empty() {
                    return Err("nobody is off doctrine".into());
                }
                let who: Vec<String> = off.iter().map(|m| format!("{} ({})", m.name, m.ship_type_name)).collect();
                (Action::KickMany { character_ids: off.iter().map(|m| m.character_id).collect(), exclude: false }, format!("Kick {} off doctrine: {}", off.len(), who.join(", ")))
            }
            other => return Err(format!("no fleet action {other}")),
        };
        propose(ctx, FleetOp::Act(open.fleet.id.clone(), action), summary)
    },
};

static FLEET_EDIT: ToolSpec = ToolSpec {
    name: "fleet_edit",
    description: "Proposes changing the fleet open in the app: its doctrine, its comms, logi or boost channel (a name, \
                  \"free\" or \"none\"), and snowflakes added (with a role: fc, vip, logi_anchor, backseat, hunter) or \
                  removed. A channel another fleet uses is asked about first, as with fleet_start.",
    need: Need::All(&["actions.fleet"]),
    kind: Kind::Action,
    schema: || {
        schema(
            json!({
                "doctrine": {"type": "string"},
                "comms": {"type": "string"},
                "logi": {"type": "string"},
                "boost": {"type": "string"},
                "add_snowflakes": {"type": "array", "items": {"type": "object", "properties": {"name": {"type": "string"}, "role": {"type": "string", "enum": ["fc", "vip", "logi_anchor", "backseat", "hunter"]}}, "required": ["name", "role"]}},
                "remove_snowflakes": {"type": "array", "items": {"type": "string"}},
                "in_use": {"type": "string", "enum": ["ask", "keep"]}
            }),
            &[],
        )
    },
    run: fleet_edit,
};

fn fleet_edit(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let open = open_fleet(ctx)?;
    let seed = ctx.deps.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.clone();
    let mut edit = crate::fleets::state::FleetEdit::default();
    edit.seed(&open.fleet);
    let mut what = Vec::new();
    if let Some(d) = str_arg(v, "doctrine") {
        let w = lower(d);
        let setup = seed.setups.iter().find(|x| lower(&x.name) == w).or_else(|| seed.setups.iter().find(|x| lower(&x.name).contains(&w))).ok_or_else(|| format!("no doctrine called {d}"))?;
        edit.setup_id = setup.id;
        what.push(format!("doctrine {}", setup.name));
    }
    let mut busy = Vec::new();
    for (key, list, slot) in [("comms", &seed.mumble_channels, &mut edit.mumble), ("logi", &seed.logi_channels, &mut edit.logi), ("boost", &seed.boost_channels, &mut edit.boost)] {
        if let Some(c) = str_arg(v, key) {
            *slot = channel(list, c)?;
            if let Some(n) = in_use(list, *slot) {
                busy.push(format!("{key} {n}"));
            }
            what.push(format!("{key} {}", channel_name(list, *slot).unwrap_or_else(|| "none".into())));
        }
    }
    if !busy.is_empty() && str_arg(v, "in_use") != Some("keep") {
        return Ok(json!({"needs_answer": format!("Another fleet already uses {}. Ask the user: keep it anyway, take a free one, or which to use instead.", busy.join(" and "))}));
    }
    for gone in v.get("remove_snowflakes").and_then(Value::as_array).cloned().unwrap_or_default() {
        let n = lower(gone.as_str().unwrap_or_default());
        let before = edit.snowflakes.len();
        edit.snowflakes.retain(|s| lower(&s.character_name) != n);
        if edit.snowflakes.len() < before {
            what.push(format!("remove snowflake {}", gone.as_str().unwrap_or_default()));
        }
    }
    for add in v.get("add_snowflakes").and_then(Value::as_array).cloned().unwrap_or_default() {
        let name = add.get("name").and_then(Value::as_str).unwrap_or_default().trim().to_owned();
        let kind = match add.get("role").and_then(Value::as_str).unwrap_or_default() {
            "vip" => SnowflakeType::Vip,
            "logi_anchor" => SnowflakeType::LogiAnchor,
            "backseat" => SnowflakeType::Backseat,
            "hunter" => SnowflakeType::Hunter,
            _ => SnowflakeType::Fc,
        };
        // A snowflake is a character that exists, not a typed name.
        let found = if ctx.deps.online { crate::http::client(10).ok().and_then(|c| crate::universe::character(&c, &name).ok().flatten()) } else { None };
        let (id, real) = found.ok_or_else(|| format!("no character called {name}"))?;
        edit.snowflakes.push(crate::fleets::model::Snowflake { id: 0, character_id: id, character_name: real.clone(), kind });
        what.push(format!("add {real} as {}", kind.label()));
    }
    if !edit.differs(&open.fleet) {
        return Err("that changes nothing".into());
    }
    let changed = edit.applied(&open.fleet);
    propose(ctx, FleetOp::Act(open.fleet.id.clone(), Action::Update(Box::new(changed))), format!("Change {}: {}", open.fleet.name, what.join(", ")))
}

static FLEET_BR: ToolSpec = ToolSpec {
    name: "fleet_battle_report",
    description: "Proposes making the br.evetools battle report of the fleet open in the app. Once made, fleet_links has its link.",
    need: Need::All(&["actions.fleet"]),
    kind: Kind::Action,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let open = open_fleet(ctx)?;
        propose(ctx, FleetOp::CreateBr(open.fleet.id.0.clone()), format!("Make the battle report of {}", open.fleet.name))
    },
};

static FLEET_LINKS: ToolSpec = ToolSpec {
    name: "fleet_links",
    description: "The dashboard link of a fleet (the open one, or by fleet id) and its battle report link once one was made.",
    need: Need::Any(&["actions.fleet", "fleets.current"]),
    kind: Kind::Read,
    schema: || schema(json!({"fleet_id": {"type": "string"}}), &[]),
    run: |ctx, v| {
        let id = match str_arg(v, "fleet_id") {
            Some(i) => i.trim().to_owned(),
            None => open_fleet(ctx)?.fleet.id.0,
        };
        let br = ctx.store.and_then(|s| s.fleet_br(&id));
        Ok(json!({"dashboard": format!("{}/fleet/overview/{id}", crate::fleets::http::BASE), "battle_report": br}))
    },
};

static CONFIRM: ToolSpec = ToolSpec {
    name: "confirm_fleet_action",
    description: "Carries out a fleet action proposed earlier, by its action_id, once the user said yes to it in a later \
                  message. Refused when the user has not answered since it was proposed.",
    need: Need::All(&["actions.fleet"]),
    kind: Kind::Read,
    schema: || schema(json!({"action_id": {"type": "integer"}}), &["action_id"]),
    run: |ctx, v| {
        let id = v.get("action_id").and_then(Value::as_u64).ok_or("which action?")?;
        let mut view = ctx.deps.view.lock().unwrap_or_else(|e| e.into_inner());
        let at = view.turns.iter().position(|t| t.cards.iter().any(|c| c.action.id == id)).ok_or("no such action")?;
        // The yes has to come from the user after the proposal, not from the same answer.
        if !view.turns[at + 1..].iter().any(|t| t.user) {
            return Err("the user has not answered since this was proposed; ask them".into());
        }
        let card = view.turns[at].cards.iter_mut().find(|c| c.action.id == id).ok_or("no such action")?;
        if !matches!(card.action.kind, ActionKind::Fleet(_)) {
            return Err("not a fleet action".into());
        }
        if card.state != crate::ai::session::CardState::Pending {
            return Err("that one was already applied or dismissed".into());
        }
        card.confirmed = true;
        Ok(json!({"status": "confirmed; it happens now", "action": card.action.summary}))
    },
};

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use crate::ai::session::{ActionCard, CardState, Turn};
    use crate::fleets::model::{Composition, Member, Squad, Wing};
    use serde_json::json;

    fn member(id: i64, name: &str, ship: &str) -> Member {
        Member { character_id: id, name: name.into(), ship_type_name: ship.into(), ..Default::default() }
    }

    fn deps_with_fleet() -> AiDeps {
        let deps = AiDeps::for_tests(facts(&["actions.fleet"]));
        let mut st = deps.fleet.lock().unwrap();
        let mut open = crate::fleets::state::OpenFleet::default();
        open.fleet.id = crate::fleets::model::FleetId("f1".into());
        open.fleet.name = "Home Defence".into();
        open.composition = Composition {
            commander: None,
            wings: vec![Wing { squads: vec![Squad { members: vec![member(1, "Bob Miner", "Hulk"), member(2, "Bob Hauler", "Orca"), member(3, "Ann", "Hulk")], ..Default::default() }], ..Default::default() }],
            flat: false,
        };
        st.open.put(open);
        drop(st);
        deps
    }

    #[test]
    fn a_kick_by_pilot_and_hull_picks_that_pilot_only() {
        let deps = deps_with_fleet();
        let (r, err) = run(&deps, "fleet_kick", json!({"pilots": ["Bob"], "ship": "Hulk"}));
        assert!(!err, "{r}");
        assert!(r["action"].as_str().unwrap().contains("Bob Miner") && !r["action"].as_str().unwrap().contains("Hauler"), "{r}");
        let (r, err) = run(&deps, "fleet_kick", json!({"ship": "Hulk"}));
        assert!(!err && r["action"].as_str().unwrap().contains("Ann"), "every Hulk: {r}");
    }

    #[test]
    fn a_fleet_action_needs_a_later_yes_from_the_user() {
        let deps = deps_with_fleet();
        let (r, _) = run(&deps, "fleet_command", json!({"action": "close"}));
        let id = r["action_id"].as_u64().unwrap();
        let action = crate::ai::tools::PendingAction { id, kind: crate::ai::tools::ActionKind::Fleet(crate::ai::tools::FleetOp::Start), summary: "Close Home Defence".into() };
        assert!(!action.kind.immediate(&["actions.fleet".into()]), "never let through without asking");
        let mut proposal = Turn::default();
        proposal.cards.push(ActionCard { action, state: CardState::Pending, confirmed: false });
        deps.view.lock().unwrap().turns = vec![Turn { user: true, ..Default::default() }, proposal];
        let (r, err) = run(&deps, "confirm_fleet_action", json!({"action_id": id}));
        assert!(err, "no answer from the user yet: {r}");
        deps.view.lock().unwrap().turns.push(Turn { user: true, text: "yes".into(), ..Default::default() });
        let (r, err) = run(&deps, "confirm_fleet_action", json!({"action_id": id}));
        assert!(!err, "{r}");
        assert!(deps.view.lock().unwrap().turns[1].cards[0].confirmed);
    }
}
