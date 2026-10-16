//! Starting, stopping and listing watches. Asked for by the user in so many words, so they run
//! without a confirmation click; the chat lists them with a stop button.

use serde_json::{json, Value};

use super::{schema, str_arg, u64_arg, Ctx, Kind, Need, ToolSpec};
use crate::ai::watch::{Watch, MAX_WATCHES};

pub static TOOLS: &[&ToolSpec] = &[&START, &STOP, &RESUME, &LIST, &FEED_ITEMS, &INTEL_FEED_ITEMS];

const NEED: Need = Need::Any(&["intel.reports", "kills.feed"]);

static START: ToolSpec = ToolSpec {
    name: "start_watch",
    description: "Starts watching new intel and kills for what the user wants to know about, when they ask you to keep an \
                  eye on, monitor or tell them about something. Matches are reported in the chat as they come in. Narrow it \
                  by where (a system, constellation or region, and a radius) and by what (group, pilot, ship or keywords, any one of which must \
                  appear); give at least one. Set minutes only when the user said how long; without it the watch asks \
                  after a quiet half hour whether to carry on, and stops if nobody answers.",
    need: NEED,
    kind: Kind::Read,
    schema: || {
        schema(
            json!({
                "goal": {"type": "string", "description": "What the user is after, in a short sentence"},
                "system": {"type": "string", "description": "A system, or a constellation or region: those cover every system in them"},
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
    let within = u64_arg(v, "within_jumps", 0, 15) as u32;
    let mut centres: Vec<i64> = Vec::new();
    if let Some(n) = str_arg(v, "system") {
        match ctx.geo()?.lookup(n.trim()) {
            Some(i) => centres.push(i.id),
            None => match area(ctx.geo()?, n) {
                Some(ids) => centres.extend(ids),
                None => centres.push(ctx.system(n).map_err(|e| format!("{e}, constellation or region"))?),
            },
        }
    }
    let mut words: Vec<String> = Vec::new();
    for w in v.get("words").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        // Reports name systems, not the region they are in, so an area given as a word would only
        // match the rare report that spells it out.
        match area(ctx.geo()?, w) {
            Some(ids) => centres.extend(ids),
            None => words.push(w.to_owned()),
        }
    }
    let systems: std::collections::HashSet<i64> = if within == 0 {
        centres.into_iter().collect()
    } else {
        let geo = ctx.geo()?;
        centres.iter().flat_map(|&c| geo.distances_from(c, within).into_keys()).collect()
    };
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

/// The systems of the constellation or region called `name`.
fn area(geo: &crate::geo::Systems, name: &str) -> Option<Vec<i64>> {
    let n = name.trim();
    if n.is_empty() {
        return None;
    }
    let ids: Vec<i64> = geo
        .all_ids()
        .filter(|&id| geo.info_of(id).is_some_and(|i| i.region.eq_ignore_ascii_case(n) || i.constellation.eq_ignore_ascii_case(n)))
        .collect();
    (!ids.is_empty()).then_some(ids)
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
        w.stop(spai_ui::tr_noop!("stopped by you"));
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

static FEED_ITEMS: ToolSpec = ToolSpec {
    name: "feed_items",
    description: "Items from the outside feeds the user added (news, timers, alliance broadcasts, anything with a feed), \
                  newest first. Narrow by feed name, words, and age.",
    need: Need::AnyUnder("feeds"),
    kind: Kind::Read,
    schema: || schema(json!({"feed": {"type": "string"}, "query": {"type": "string"}, "since_minutes": {"type": "integer", "minimum": 1, "maximum": 100_000}, "limit": {"type": "integer", "minimum": 1, "maximum": 40}}), &[]),
    run: |ctx, v| {
        let since = ctx.now - 60 * u64_arg(v, "since_minutes", 1440, 100_000) as i64;
        let want = str_arg(v, "feed").map(str::to_lowercase);
        let words: Vec<String> = str_arg(v, "query").unwrap_or_default().to_lowercase().split_whitespace().map(str::to_owned).collect();
        let defs: Vec<&crate::ai::feeds::FeedDef> = ctx
            .facts
            .ai
            .feeds
            .iter()
            .filter(|d| ctx.facts.allowed(&d.perm_key()))
            .filter(|d| want.as_ref().is_none_or(|w| d.name.to_lowercase().contains(w.as_str())))
            .collect();
        if defs.is_empty() {
            return Err("no feed allowed by that name".into());
        }
        let st = ctx.deps.feeds.lock().unwrap_or_else(|e| e.into_inner());
        let out: Vec<Value> = st
            .items
            .iter()
            .rev()
            .filter(|i| i.time >= since)
            .filter_map(|i| defs.iter().find(|d| d.id == i.feed).map(|d| (d, i)))
            .filter(|(_, i)| {
                let hay = format!("{} {}", i.title, i.text).to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .take(u64_arg(v, "limit", 15, 40) as usize)
            .map(|(d, i)| json!({"feed": d.name, "when": super::eve_time(i.time), "age": super::fmt_age(ctx.now, i.time), "title": i.title, "text": i.text, "link": i.link}))
            .collect();
        Ok(json!({"items": out}))
    },
};

static INTEL_FEED_ITEMS: ToolSpec = ToolSpec {
    name: "intel_feed_items",
    description: "Items from the user's sealed intel feed exactly as received, newest first, with the systems each was \
                  mapped to (empty: it named no known system). Items that named a system are also in the intel reports. \
                  Narrow by words and age.",
    need: Need::All(&[crate::sealed_feed::AI_PERM]),
    kind: Kind::Read,
    schema: || schema(json!({"query": {"type": "string"}, "since_minutes": {"type": "integer", "minimum": 1, "maximum": 100_000}, "limit": {"type": "integer", "minimum": 1, "maximum": 40}}), &[]),
    run: |ctx, v| {
        let since = ctx.now - 60 * u64_arg(v, "since_minutes", 1440, 100_000) as i64;
        let words: Vec<String> = str_arg(v, "query").unwrap_or_default().to_lowercase().split_whitespace().map(str::to_owned).collect();
        let items = ctx.deps.sealed_items.lock().unwrap_or_else(|e| e.into_inner());
        let out: Vec<Value> = items
            .iter()
            .filter(|i| i.event.timestamp.unwrap_or(i.seen) >= since)
            .filter(|i| {
                let hay = i.event.raw.to_string().to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .take(u64_arg(v, "limit", 15, 40) as usize)
            .map(|i| {
                let t = i.event.timestamp.unwrap_or(i.seen);
                json!({"when": super::eve_time(t), "age": super::fmt_age(ctx.now, t), "systems": i.systems, "item": i.event.raw})
            })
            .collect();
        Ok(json!({"items": out}))
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

    /// A region or constellation covers its systems, whether it comes as the where or as a word.
    #[test]
    fn a_region_or_constellation_watch_covers_its_systems() {
        let deps = AiDeps::for_tests(facts(&["intel.reports"]));
        let delve: std::collections::HashSet<i64> = [30_004_759, 30_004_608].into();
        let (r, err) = run(&deps, "start_watch", json!({"goal": "Delve", "system": "delve"}));
        assert!(!err, "{r}");
        let (r, err) = run(&deps, "start_watch", json!({"goal": "Delve hostiles", "words": ["Delve", "frat"]}));
        assert!(!err, "{r}");
        let (r, err) = run(&deps, "start_watch", json!({"goal": "the constellation", "system": "O-EImg"}));
        assert!(!err, "{r}");
        let ws = deps.watches.lock().unwrap();
        assert_eq!(ws[0].systems, delve);
        assert!(ws[0].candidate(&[30_004_608], "hostile in 319-3d"), "a report in a Delve system that does not say Delve");
        assert!(!ws[0].candidate(&[30_003_704], "hostile in 7-k5el"));
        assert_eq!(ws[1].systems, delve);
        assert!(!ws[1].words.iter().any(|w| w.eq_ignore_ascii_case("delve")), "the region is a where, not a word: {:?}", ws[1].words);
        assert!(ws[1].candidate(&[30_004_759], "frat gang in 1dq1-a"));
        assert!(ws[2].systems.contains(&30_003_704) && ws[2].systems.contains(&30_000_142));
        drop(ws);
        let (_, err) = run(&deps, "start_watch", json!({"goal": "x", "system": "Nowhere Land"}));
        assert!(err);
    }

    #[test]
    fn feed_items_follow_each_feeds_own_permission() {
        use crate::ai::feeds::{FeedDef, FeedItem};
        let mut f = facts(&["feeds.1"]);
        f.ai.feeds = vec![FeedDef { id: 1, name: "Timers".into(), ..Default::default() }, FeedDef { id: 2, name: "Secret".into(), ..Default::default() }];
        let deps = AiDeps::for_tests(f);
        let now = crate::clock::utc().timestamp();
        let it = |feed: u64, text: &str| FeedItem { feed, key: text.into(), time: now - 60, seen: now, title: String::new(), text: text.into(), link: String::new() };
        deps.feeds.lock().unwrap().add(vec![it(1, "Keepstar armor timer in 1DQ1-A"), it(2, "hidden")]);
        let (out, err) = run(&deps, "feed_items", json!({"query": "timer"}));
        assert!(!err, "{out}");
        assert_eq!(out["items"].as_array().unwrap().len(), 1);
        assert!(!out.to_string().contains("hidden"));
        let (_, err) = run(&deps, "feed_items", json!({"feed": "secret"}));
        assert!(err);
    }

    /// The sealed feed's raw items, under its own permission; reading them closes the web.
    #[test]
    fn intel_feed_items_are_read_as_received_under_their_permission() {
        let ev = |j: &str| crate::sealed_feed::wire::read_event(&serde_json::from_str(j).unwrap());
        let now = crate::clock::utc().timestamp();
        let fill = |deps: &AiDeps| {
            deps.sealed_items.lock().unwrap().extend([
                crate::sealed_feed::Item { event: ev(r#"{"id":"a","msg":"cyno lit in 1DQ1-A"}"#), systems: vec!["1DQ1-A".into()], seen: now },
                crate::sealed_feed::Item { event: ev(r#"{"id":"b","msg":"quiet"}"#), systems: vec![], seen: now },
            ])
        };
        let deps = AiDeps::for_tests(facts(&["intel"]));
        fill(&deps);
        let (out, err) = run(&deps, "intel_feed_items", json!({"query": "cyno"}));
        assert!(!err, "{out}");
        let items = out["items"].as_array().unwrap();
        assert_eq!((items.len(), items[0]["item"]["msg"].as_str(), items[0]["systems"][0].as_str()), (1, Some("cyno lit in 1DQ1-A"), Some("1DQ1-A")));
        assert!(deps.opsec.load(std::sync::atomic::Ordering::Relaxed));
        let off = AiDeps::for_tests(facts(&["intel.reports"]));
        fill(&off);
        let (_, err) = run(&off, "intel_feed_items", json!({}));
        assert!(err, "not allowed without the feed's permission");
    }
}
