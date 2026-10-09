//! Starting, stopping and listing watches. Asked for by the user in so many words, so they run
//! without a confirmation click; the chat lists them with a stop button.

use serde_json::{json, Value};

use super::{schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};
use crate::ai::watch::{Watch, MAX_WATCHES};

pub static TOOLS: &[&ToolSpec] = &[&START, &STOP, &RESUME, &LIST];

const NEED: Need = Need::Any(&["intel.reports", "kills.feed"]);

static START: ToolSpec = ToolSpec {
    name: "start_watch",
    description: "Starts watching new intel and kills for what the user wants to know about, when they ask you to keep an \
                  eye on, monitor or tell them about something. Matches are reported in the chat as they come in. Narrow it \
                  by where (a system and a radius) and by what (group, pilot, ship or keywords, any one of which must \
                  appear); give at least one. Set minutes only when the user said how long; without it the watch asks \
                  after a quiet half hour whether to carry on, and stops if nobody answers.",
    need: NEED,
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "goal": {"type": "string", "description": "What the user is after, in a short sentence"},
                "system": {"type": "string"},
                "within_jumps": {"type": "integer", "minimum": 0, "maximum": 15},
                "words": {"type": "array", "items": {"type": "string"}, "maxItems": 12, "description": "Group or alliance names and shorthands, pilots, ships, keywords"},
                "minutes": {"type": "integer", "minimum": 5, "maximum": 1440}
            }),
            &["goal"],
        )
    },
    run: start,
};

fn start(ctx: &mut Ctx, v: &Value) -> Result<Value, String> {
    let goal = str_arg(v, "goal").ok_or("what should be watched for?")?.trim().to_owned();
    let systems: std::collections::HashSet<i64> = match str_arg(v, "system") {
        Some(n) => {
            let id = ctx.system(n)?;
            ctx.geo()?.distances_from(id, u64_arg(v, "within_jumps", 0, 15) as u32).into_keys().collect()
        }
        None => Default::default(),
    };
    let mut words: Vec<String> = v.get("words").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    // A shorthand also matches the full alliance name it stands for.
    for w in words.clone() {
        if let Some((name, _)) = crate::alliances::lookup(&w) {
            words.push(name.to_owned());
        }
    }
    if systems.is_empty() && words.is_empty() {
        return Err("say where (a system) or what (a group, pilot, ship or keyword) to watch for".into());
    }
    let until = v.get("minutes").and_then(Value::as_u64).map(|m| ctx.now + m.clamp(5, 1440) as i64 * 60);
    let mut ws = ctx.deps.watches.lock().unwrap_or_else(|e| e.into_inner());
    if ws.iter().filter(|w| w.running()).count() >= MAX_WATCHES {
        return Err(format!("already {MAX_WATCHES} watches running; stop one first"));
    }
    // Stopped watches make room for new ones, oldest first.
    while ws.len() >= MAX_WATCHES {
        match ws.iter().position(|w| !w.running()) {
            Some(i) => {
                ws.remove(i);
            }
            None => break,
        }
    }
    let id = ws.iter().map(|w| w.id).max().unwrap_or(0) + 1;
    let n_systems = systems.len();
    ws.push(Watch::new(id, goal.clone(), systems, words, ctx.now, until));
    Ok(json!({"watch": id, "goal": goal, "systems_covered": n_systems, "runs_for_minutes": until.map(|u| (u - ctx.now) / 60), "status": "watching"}))
}

fn by_id<'a>(ws: &'a mut [Watch], v: &Value) -> Result<&'a mut Watch, String> {
    let id = v.get("watch").and_then(Value::as_u64).ok_or("which watch?")?;
    ws.iter_mut().find(|w| w.id == id).ok_or_else(|| format!("no watch {id}"))
}

static STOP: ToolSpec = ToolSpec {
    name: "stop_watch",
    description: "Stops a watch by its number, when the user says to stop watching.",
    need: NEED,
    kind: Kind::Read,
    schema: || schema(json!({"watch": {"type": "integer"}}), &["watch"]),
    run: |ctx, v| {
        let mut ws = ctx.deps.watches.lock().unwrap_or_else(|e| e.into_inner());
        let w = by_id(&mut ws, v)?;
        w.stop("stopped by you");
        Ok(json!({"stopped": w.id}))
    },
};

static RESUME: ToolSpec = ToolSpec {
    name: "resume_watch",
    description: "Carries on with a watch that asked whether to continue or that stopped, when the user says to keep watching. \
                  Minutes sets a new time span if the user gave one.",
    need: NEED,
    kind: Kind::Read,
    schema: || schema(json!({"watch": {"type": "integer"}, "minutes": {"type": "integer", "minimum": 5, "maximum": 1440}}), &["watch"]),
    run: |ctx, v| {
        let now = ctx.now;
        let mut ws = ctx.deps.watches.lock().unwrap_or_else(|e| e.into_inner());
        let w = by_id(&mut ws, v)?;
        w.resume(now);
        if let Some(m) = v.get("minutes").and_then(Value::as_u64) {
            w.until = Some(now + m.clamp(5, 1440) as i64 * 60);
        }
        Ok(json!({"watching": w.id, "goal": w.goal}))
    },
};

static LIST: ToolSpec = ToolSpec {
    name: "list_watches",
    description: "The watches there are, running or stopped, with their numbers.",
    need: NEED,
    kind: Kind::Read,
    schema: || schema(json!({}), &[]),
    run: |ctx, _| {
        let ws = ctx.deps.watches.lock().unwrap_or_else(|e| e.into_inner());
        Ok(json!(ws.iter().map(|w| json!({"watch": w.id, "goal": w.goal, "status": w.status(ctx.now), "found": w.hits})).collect::<Vec<_>>()))
    },
};

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    #[test]
    fn watches_start_with_a_where_or_a_what_and_stop_and_resume_by_number() {
        let deps = AiDeps::for_tests(facts(&["intel.reports"]));
        let (r, err) = run(&deps, "start_watch", json!({"goal": "the Frat gang", "system": "1DQ1-A", "within_jumps": 2, "words": ["frat"], "minutes": 60}));
        assert!(!err, "{r}");
        assert_eq!(r["runs_for_minutes"], 60);
        {
            let ws = deps.watches.lock().unwrap();
            assert!(ws[0].words.iter().any(|w| w.contains("fraternity")), "the shorthand brings the full name: {:?}", ws[0].words);
            assert!(ws[0].systems.len() > 1);
        }
        let (_, err) = run(&deps, "start_watch", json!({"goal": "anything"}));
        assert!(err, "a watch needs a where or a what");
        let (_, err) = run(&deps, "stop_watch", json!({"watch": 1}));
        assert!(!err);
        assert!(!deps.watches.lock().unwrap()[0].running());
        let (_, err) = run(&deps, "resume_watch", json!({"watch": 1}));
        assert!(!err);
        assert!(deps.watches.lock().unwrap()[0].running());
        let (none, err) = run(&AiDeps::for_tests(facts(&[])), "start_watch", json!({"goal": "x", "words": ["y"]}));
        assert!(err, "{none}");
    }
}
