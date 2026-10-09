//! The assistant's own memory: keep what is worth knowing next time, correct it, drop it.
//! The user sees every memory in the Assistant tab and can change or delete it there.

use serde_json::{json, Value};

use super::{schema, str_arg, Kind, Need, ToolSpec};
use crate::ai::memory::MemKind;

pub static TOOLS: &[&ToolSpec] = &[&REMEMBER, &UPDATE, &FORGET];

const KINDS: [&str; 5] = ["about", "preference", "pilot", "place", "other"];

static REMEMBER: ToolSpec = ToolSpec {
    name: "remember",
    description: "Saves something worth knowing in later conversations: the user's home, corp, ships, what they care about, \
                  how they like answers, a pilot or group worth watching, a place. One short fact per memory; do not save \
                  what is already remembered, and never save passing intel.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"kind": {"type": "string", "enum": KINDS}, "text": {"type": "string"}}), &["kind", "text"]),
    run: |ctx, v| {
        let text = str_arg(v, "text").ok_or("nothing to remember")?;
        let kind = MemKind::from_code(str_arg(v, "kind").unwrap_or("other"));
        let mems = ctx.deps.memories.clone();
        let mut m = mems.lock().unwrap_or_else(|e| e.into_inner());
        let id = m.add(kind, text, false, ctx.now);
        m.save(ctx.store);
        Ok(json!({"saved": id}))
    },
};

static UPDATE: ToolSpec = ToolSpec {
    name: "update_memory",
    description: "Corrects a saved memory by its id, when it turned out wrong or changed.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"id": {"type": "integer"}, "text": {"type": "string"}, "kind": {"type": "string", "enum": KINDS}}), &["id", "text"]),
    run: |ctx, v| {
        let id = v.get("id").and_then(Value::as_u64).ok_or("which memory?")?;
        let text = str_arg(v, "text").ok_or("the new text is missing")?;
        let kind = str_arg(v, "kind").map(MemKind::from_code);
        let mems = ctx.deps.memories.clone();
        let mut m = mems.lock().unwrap_or_else(|e| e.into_inner());
        if !m.update(id, kind, text, false, ctx.now) {
            return Err(format!("no memory {id}"));
        }
        m.save(ctx.store);
        Ok(json!({"updated": id}))
    },
};

static FORGET: ToolSpec = ToolSpec {
    name: "forget",
    description: "Deletes a saved memory by its id, when it is wrong or the user asks to forget it.",
    need: Need::All(&["sde"]),
    kind: Kind::Read,
    schema: || schema(json!({"id": {"type": "integer"}}), &["id"]),
    run: |ctx, v| {
        let id = v.get("id").and_then(Value::as_u64).ok_or("which memory?")?;
        let mems = ctx.deps.memories.clone();
        let mut m = mems.lock().unwrap_or_else(|e| e.into_inner());
        if !m.remove(id) {
            return Err(format!("no memory {id}"));
        }
        m.save(ctx.store);
        Ok(json!({"forgotten": id}))
    },
};

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use crate::ai::deps::AiDeps;
    use serde_json::json;

    #[test]
    fn the_model_keeps_corrects_and_drops_memories() {
        let deps = AiDeps::for_tests(facts(&[]));
        let (r, err) = run(&deps, "remember", json!({"kind": "about", "text": "Flies Muninns out of 1DQ1-A"}));
        assert!(!err, "{r}");
        let id = r["saved"].as_u64().unwrap();
        let (_, err) = run(&deps, "update_memory", json!({"id": id, "text": "Flies Eagles out of 1DQ1-A"}));
        assert!(!err);
        assert_eq!(deps.memories.lock().unwrap().list[0].text, "Flies Eagles out of 1DQ1-A");
        let (_, err) = run(&deps, "forget", json!({"id": id}));
        assert!(!err);
        assert!(deps.memories.lock().unwrap().list.is_empty());
        let (_, err) = run(&deps, "forget", json!({"id": 999}));
        assert!(err);
    }
}
