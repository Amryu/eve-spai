//! The assistant's tools: what it can look up and what it can ask to do.
//!
//! A tool is offered only when its permissions are allowed, and checked again when called, so a
//! made-up or replayed call to a denied tool fails. Results are data from chat logs, killmails and
//! the web, which anyone can write into: they are wrapped as untrusted and never read as orders.
//! Nothing here can send a message anywhere; `no_tool_sends_messages` keeps it that way.

mod actions;
mod fleet;
pub(crate) mod intel;
mod kills;
mod map;
mod memory;
mod misc;
mod history;
mod watch;
mod extra;
mod more;
mod search;
#[cfg(test)]
mod audit;
mod web;

use serde_json::{json, Value};

use super::deps::{AiDeps, AiFacts};
use super::provider::ToolDef;

pub use actions::{ActionKind, ChatWindowPick, FleetOp, PendingAction};

/// Results longer than this are cut, and say so.
pub const RESULT_CAP: usize = 8_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Read,
    /// Asks the user first: queues an action card and answers "pending confirmation".
    Action,
}

#[derive(Clone, Copy)]
pub enum Need {
    All(&'static [&'static str]),
    /// At least one: a tool that combines several sources uses whichever are allowed.
    Any(&'static [&'static str]),
    /// Any key under this prefix (per-channel chat logs, per-feed items).
    AnyUnder(&'static str),
}

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub need: Need,
    pub kind: Kind,
    pub schema: fn() -> Value,
    pub run: fn(&mut Ctx, &Value) -> Result<Value, String>,
}

pub struct Ctx<'a> {
    pub deps: &'a AiDeps,
    pub facts: &'a AiFacts,
    /// This worker's own database handle; `Store` cannot be shared across threads.
    pub store: Option<&'a crate::store::Store>,
    pub now: i64,
    pub actions: &'a mut Vec<PendingAction>,
}

impl Ctx<'_> {
    pub fn geo(&self) -> Result<&crate::geo::Systems, String> {
        self.facts.systems.as_deref().ok_or_else(|| "the map is still loading".to_owned())
    }

    pub fn system(&self, name: &str) -> Result<i64, String> {
        let geo = self.geo()?;
        super::deps::resolve_system(geo, name).ok_or_else(|| format!("no system called {name:?}"))
    }

    pub fn system_name(&self, id: i64) -> String {
        self.facts.systems.as_ref().and_then(|g| g.info_of(id).map(|i| i.name.clone())).unwrap_or_else(|| id.to_string())
    }

    pub fn allowed(&self, key: &str) -> bool {
        self.facts.allowed(key)
    }
}

fn registry() -> Vec<&'static ToolSpec> {
    let mut v: Vec<&'static ToolSpec> = Vec::new();
    v.extend(map::TOOLS);
    v.extend(intel::TOOLS);
    v.extend(kills::TOOLS);
    v.extend(misc::TOOLS);
    v.extend(history::TOOLS);
    v.extend(watch::TOOLS);
    v.extend(extra::TOOLS);
    v.extend(more::TOOLS);
    v.extend(search::TOOLS);
    v.extend(web::TOOLS);
    v.extend(actions::TOOLS);
    v.extend(memory::TOOLS);
    v.extend(fleet::TOOLS);
    v
}

fn permitted(spec: &ToolSpec, facts: &AiFacts) -> bool {
    match spec.need {
        Need::All(keys) => keys.iter().all(|k| facts.allowed(k)),
        Need::Any(keys) => keys.iter().any(|k| facts.allowed(k)),
        Need::AnyUnder(prefix) => {
            let under = format!("{prefix}.");
            facts.allowed(prefix) || facts.perms.keys().any(|k| k.starts_with(&under) && facts.allowed(k))
        }
    }
}

/// The tools on offer under the current permissions.
/// The Data access ticks that would open tools the user has switched off, by their names in the
/// dialog, so the assistant can say which one a question needs.
pub fn switched_off(facts: &AiFacts) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for t in registry().into_iter().filter(|t| !permitted(t, facts)) {
        let under;
        let keys: &[&str] = match t.need {
            Need::All(k) | Need::Any(k) => k,
            Need::AnyUnder(p) => {
                under = [p];
                &under
            }
        };
        if let Some(l) = keys.first().and_then(|k| crate::ai::perms::label_of(k)) {
            if !out.contains(&l) {
                out.push(l);
            }
        }
    }
    out
}

pub fn tools_for(facts: &AiFacts) -> Vec<ToolDef> {
    registry()
        .into_iter()
        .filter(|t| permitted(t, facts))
        .map(|t| ToolDef { name: t.name.to_owned(), description: t.description.to_owned(), schema: (t.schema)() })
        .collect()
}

/// Runs a tool call. The answer is the text handed back to the model, and whether it is an error.
pub fn dispatch(ctx: &mut Ctx, name: &str, input: &Value) -> (String, bool) {
    let Some(spec) = registry().into_iter().find(|t| t.name == name) else {
        return (format!("there is no tool called {name}"), true);
    };
    if !permitted(spec, ctx.facts) {
        return (format!("{name} is not allowed: the user has not given access to that data"), true);
    }
    if WEB.contains(&name) && ctx.deps.opsec.load(std::sync::atomic::Ordering::Relaxed) {
        return (
            "Web lookups are off for this conversation: it has read Jabber, rescue or feed messages, which must not leave the app. \
             Answer without the web, or tell the user to start a new chat for web questions."
                .into(),
            true,
        );
    }
    // A memory outlives the conversation, and a later one has the web again.
    if ["remember", "update_memory"].contains(&name) && ctx.deps.opsec.load(std::sync::atomic::Ordering::Relaxed) {
        return ("Memories are not saved in a conversation that read Jabber, rescue or feed messages, so they cannot leave the app later.".into(), true);
    }
    match (spec.run)(ctx, input) {
        Ok(mut v) => {
            if OPSEC.contains(&name) {
                ctx.deps.opsec.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            compact(&mut v);
            let dropped = shrink(&mut v);
            let mut out = json!({"untrusted_data": v});
            if dropped > 0 {
                out["left_out"] = json!(format!("{dropped} items did not fit; ask for fewer, a shorter time span or a narrower filter"));
            }
            (cap(&out.to_string()), false)
        }
        Err(e) => (e, true),
    }
}

/// Tools whose results are opsec: Jabber messages, the rescue's pings, and outside feeds. Once one
/// has been read, the conversation may not reach the web.
pub const OPSEC: &[&str] = &["chat_log", "jabber_chat", "jabber_pings", "jabber_rooms", "jabber_search", "rescue_status", "rescue_history", "feed_items"];
/// Tools that reach the web with words the model chose.
pub const WEB: &[&str] = &["web_search", "web_fetch"];

fn cap(s: &str) -> String {
    if s.len() <= RESULT_CAP {
        return s.to_owned();
    }
    let mut end = RESULT_CAP;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated: ask for less, e.g. a shorter time window or a narrower filter]", &s[..end])
}

/// The tool's kind, for showing a call as a read or as an action.
pub fn kind_of(name: &str) -> Option<Kind> {
    registry().into_iter().find(|t| t.name == name).map(|t| t.kind)
}

// Helpers the tools share.

pub(crate) fn str_arg<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

pub(crate) fn u64_arg(v: &Value, key: &str, default: u64, max: u64) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(default).min(max)
}

pub(crate) fn schema(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

pub(crate) fn fmt_age(now: i64, t: i64) -> String {
    let s = (now - t).max(0);
    if s < 90 {
        format!("{s}s ago")
    } else if s < 5400 {
        format!("{}m ago", s / 60)
    } else {
        format!("{:.1}h ago", s as f64 / 3600.0)
    }
}

/// EVE time as short as it can be told apart: the time today, the date as well on another day,
/// the year only when it is not this one.
pub(crate) fn eve_time(t: i64) -> String {
    let Some(d) = chrono::DateTime::from_timestamp(t, 0) else { return String::new() };
    let now = crate::clock::utc();
    if d.date_naive() == now.date_naive() {
        d.format("%H:%M").to_string()
    } else if d.format("%Y").to_string() == now.format("%Y").to_string() {
        d.format("%b %d %H:%M").to_string()
    } else {
        d.format("%Y-%m-%d %H:%M").to_string()
    }
}

/// An ISO time from an outside service (the fleet dashboard), told the same way.
pub(crate) fn iso_time(s: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(s).map(|d| eve_time(d.timestamp())).unwrap_or_else(|_| s.to_owned())
}

/// Leaves out what says nothing: nulls, empty text, empty lists and objects. Absent means none.
pub(crate) fn compact(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for x in o.values_mut() {
                compact(x);
            }
            o.retain(|_, x| !(x.is_null() || x.as_str() == Some("") || x.as_array().is_some_and(|a| a.is_empty()) || x.as_object().is_some_and(|m| m.is_empty())));
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                compact(x);
            }
        }
        _ => {}
    }
}

/// Fits a result into [`RESULT_CAP`] by dropping whole items from the end of its longest list,
/// never cutting one in half. Returns how many were dropped.
fn shrink(v: &mut Value) -> usize {
    enum Step {
        Key(String),
        Index(usize),
    }
    /// The path to the longest list with more than one item.
    fn find(v: &Value, path: &mut Vec<usize>, keys: &mut Vec<Step>, best: &mut Option<(usize, Vec<Step>)>) {
        match v {
            Value::Array(a) => {
                if a.len() > 1 && best.as_ref().is_none_or(|(n, _)| a.len() > *n) {
                    *best = Some((a.len(), keys.iter().map(|k| match k { Step::Key(s) => Step::Key(s.clone()), Step::Index(i) => Step::Index(*i) }).collect()));
                }
                for (i, x) in a.iter().enumerate() {
                    keys.push(Step::Index(i));
                    find(x, path, keys, best);
                    keys.pop();
                }
            }
            Value::Object(o) => {
                for (k, x) in o {
                    keys.push(Step::Key(k.clone()));
                    find(x, path, keys, best);
                    keys.pop();
                }
            }
            _ => {}
        }
    }
    fn longest(v: &mut Value) -> Option<&mut Vec<Value>> {
        let mut best = None;
        find(v, &mut Vec::new(), &mut Vec::new(), &mut best);
        let (_, steps) = best?;
        let mut at = v;
        for s in steps {
            at = match s {
                Step::Key(k) => at.get_mut(&k)?,
                Step::Index(i) => at.get_mut(i)?,
            };
        }
        at.as_array_mut()
    }
    let mut dropped = 0;
    while v.to_string().len() > RESULT_CAP {
        match longest(v) {
            Some(a) => {
                let cut = (a.len() / 4).max(1);
                a.truncate(a.len() - cut);
                dropped += cut;
            }
            None => break,
        }
    }
    dropped
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use std::collections::BTreeMap;

    pub fn facts(allow: &[&str]) -> AiFacts {
        let mut perms = BTreeMap::new();
        for k in allow {
            perms.insert((*k).to_owned(), true);
        }
        AiFacts {
            systems: Some(crate::uitest::fixtures::systems()),
            perms,
            unlocked: crate::ai::perms::Unlocked { fleet: true, rescue: true },
            ..Default::default()
        }
    }

    pub fn run(deps: &AiDeps, name: &str, input: Value) -> (Value, bool) {
        let facts = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = Ctx { deps, facts: &facts, store: None, now: crate::clock::utc().timestamp(), actions: &mut actions };
        let (text, err) = dispatch(&mut ctx, name, &input);
        let v = serde_json::from_str::<Value>(&text).map(|v| v["untrusted_data"].clone()).unwrap_or(Value::String(text));
        (v, err)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn denied_tools_are_neither_offered_nor_run() {
        let deps = AiDeps::for_tests(facts(&[]));
        let names: Vec<String> = tools_for(&deps.facts()).into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"route".to_owned()), "static data needs no permission");
        assert!(!names.contains(&"search_intel".to_owned()));
        let (out, err) = run(&deps, "search_intel", json!({}));
        assert!(err, "{out}");
        let deps = AiDeps::for_tests(facts(&["intel"]));
        assert!(tools_for(&deps.facts()).iter().any(|t| t.name == "search_intel"));
    }

    /// Two tools write to anyone, send_jabber and the fleet ping request, and both only propose: the
    /// user clicks or says yes. Reading the ping's progress writes nothing.
    #[test]
    fn only_send_jabber_sends_and_it_waits_for_a_click() {
        for t in registry() {
            let n = t.name.to_lowercase();
            if n == "send_jabber" {
                assert_eq!(t.kind, Kind::Action);
                assert!(matches!(t.need, Need::All(&["actions.jabber"])));
                continue;
            }
            if n == "fleet_request_ping" {
                assert_eq!(t.kind, Kind::Action);
                assert!(matches!(t.need, Need::All(&["actions.fleet"])));
                continue;
            }
            if n == "fleet_ping_status" {
                assert_eq!(t.kind, Kind::Read);
                continue;
            }
            for word in ["send", "post", "message", "ping_", "broadcast", "notify"] {
                assert!(!n.contains(word), "{} looks like it sends something", t.name);
            }
        }
    }

    #[test]
    fn reading_jabber_or_feeds_closes_the_web_for_the_conversation() {
        let deps = AiDeps::for_tests(facts(&["internet", "jabber"]));
        let (_, err) = run(&deps, "jabber_pings", json!({}));
        assert!(!err);
        assert!(deps.opsec.load(std::sync::atomic::Ordering::Relaxed));
        let (out, err) = run(&deps, "web_search", json!({"query": "anything"}));
        assert!(err && out.to_string().contains("must not leave"), "{out}");
        let (out, err) = run(&deps, "remember", json!({"kind": "other", "text": "the ops room said X"}));
        assert!(err && out.to_string().contains("not saved"), "{out}");
        for t in OPSEC {
            assert!(registry().iter().any(|r| r.name == *t) || ["jabber_rooms", "jabber_search"].contains(t), "{t} is a tool");
        }
    }

    #[test]
    fn empty_fields_go_and_long_lists_shrink_by_whole_items() {
        let mut v = json!({"a": null, "b": "", "c": [], "d": {}, "e": false, "f": 0, "g": [{"x": null, "y": 1}]});
        compact(&mut v);
        assert_eq!(v, json!({"e": false, "f": 0, "g": [{"y": 1}]}), "false and zero say something; null and empty do not");
        let mut big = json!({"items": (0..2000).map(|i| json!({"n": i, "text": "some words here"})).collect::<Vec<_>>(), "note": "kept"});
        let dropped = shrink(&mut big);
        assert!(dropped > 0 && big.to_string().len() <= RESULT_CAP);
        assert_eq!(big["note"], "kept");
        assert!(serde_json::from_str::<Value>(&big.to_string()).is_ok(), "still whole JSON");
    }

    #[test]
    fn long_results_are_cut_and_say_so() {
        let s = "x".repeat(RESULT_CAP * 2);
        let c = cap(&s);
        assert!(c.len() < RESULT_CAP + 200 && c.contains("truncated"));
    }

    #[test]
    fn every_tool_has_an_object_schema() {
        for t in registry() {
            assert_eq!((t.schema)()["type"], "object", "{}", t.name);
            assert!(!t.description.is_empty());
        }
    }
}
