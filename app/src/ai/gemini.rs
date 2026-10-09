//! Gemini through Google's Generative Language API, streamed as server-sent events.
//!
//! Gemini names no tool calls, so each gets an id made up here, and the result goes back as a
//! `functionResponse` under the tool's name. Its schema dialect rejects a few JSON Schema keys, which
//! are stripped before sending.

use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;

use super::provider::{Block, Caps, Delta, Provider, Request, Role, Stop, Usage};
use super::sse::SseReader;

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct Gemini {
    pub key: String,
}

/// A JSON Schema as Gemini takes it: without keys its parser refuses.
fn clean_schema(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(k, _)| !matches!(k.as_str(), "additionalProperties" | "$schema" | "maxItems" | "minItems" | "minimum" | "maximum"))
                .map(|(k, v)| (k.clone(), clean_schema(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(clean_schema).collect()),
        other => other.clone(),
    }
}

pub fn body(req: &Request) -> Value {
    // A tool result is answered under the tool's name, which Gemini matches on.
    let mut names = std::collections::HashMap::new();
    for m in req.msgs {
        for b in &m.blocks {
            if let Block::ToolUse { id, name, .. } = b {
                names.insert(id.clone(), name.clone());
            }
        }
    }
    let contents: Vec<Value> = req
        .msgs
        .iter()
        .filter_map(|m| {
            let parts: Vec<Value> = m
                .blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Text(t) if !t.is_empty() => Some(json!({"text": t})),
                    Block::ToolUse { name, input, .. } => Some(json!({"functionCall": {"name": name, "args": input}})),
                    Block::ToolResult { id, content, is_error } => Some(json!({"functionResponse": {
                        "name": names.get(id).cloned().unwrap_or_default(),
                        "response": if *is_error { json!({"error": content}) } else { json!({"result": content}) },
                    }})),
                    _ => None,
                })
                .collect();
            (!parts.is_empty()).then(|| json!({"role": if m.role == Role::User { "user" } else { "model" }, "parts": parts}))
        })
        .collect();
    let mut sys = req.system_static.to_owned();
    if !req.system_dynamic.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(req.system_dynamic);
    }
    let mut b = json!({
        "systemInstruction": {"parts": [{"text": sys}]},
        "contents": contents,
        "generationConfig": {"maxOutputTokens": req.max_tokens},
    });
    if !req.tools.is_empty() {
        b["tools"] = json!([{"functionDeclarations": req.tools.iter().map(|t| json!({
            "name": t.name, "description": t.description, "parameters": clean_schema(&t.schema),
        })).collect::<Vec<_>>()}]);
    }
    if !req.effort.is_empty() {
        let budget = match req.effort {
            "low" => 0,
            "medium" => 2048,
            "high" => 8192,
            _ => -1,
        };
        b["generationConfig"]["thinkingConfig"] = json!({"thinkingBudget": budget});
    }
    b
}

impl Provider for Gemini {
    fn stream(&mut self, req: &Request, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
        let client = crate::http::client(30)?;
        let url = format!("{BASE}/{}:streamGenerateContent?alt=sse", req.model);
        let resp = client
            .post(url)
            .timeout(std::time::Duration::from_secs(600))
            .header("x-goog-api-key", &self.key)
            .header("content-type", "application/json")
            .body(body(req).to_string())
            .send()?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| text.chars().take(300).collect());
            anyhow::bail!("Gemini answered {status}: {}", super::secrets::redact(&msg, &[self.key.clone()]));
        }
        parse_stream(resp, cancel, out)
    }

    fn caps(&self) -> Caps {
        Caps { tools: true, hosts_own_tools: false }
    }
}

pub fn parse_stream<R: std::io::Read>(r: R, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
    let mut sse = SseReader::new(r);
    let mut usage = Usage::default();
    let mut stop = Stop::End;
    let mut calls = 0usize;
    while let Some(ev) = sse.next(cancel) {
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else { continue };
        if let Some(e) = v["error"]["message"].as_str() {
            anyhow::bail!("{e}");
        }
        if let Some(u) = v.get("usageMetadata") {
            usage.input = u["promptTokenCount"].as_u64().unwrap_or(usage.input);
            usage.output = u["candidatesTokenCount"].as_u64().unwrap_or(usage.output) + u["thoughtsTokenCount"].as_u64().unwrap_or(0);
            usage.cached = u["cachedContentTokenCount"].as_u64().unwrap_or(usage.cached);
        }
        let Some(c) = v["candidates"].get(0) else { continue };
        for p in c["content"]["parts"].as_array().into_iter().flatten() {
            if p["thought"].as_bool() == Some(true) {
                continue;
            }
            if let Some(t) = p["text"].as_str().filter(|t| !t.is_empty()) {
                out(Delta::Text(t.to_owned()));
            }
            if let Some(fc) = p.get("functionCall") {
                calls += 1;
                let input = if fc["args"].is_object() { fc["args"].clone() } else { json!({}) };
                out(Delta::ToolUse { id: format!("gemini_{calls}"), name: fc["name"].as_str().unwrap_or_default().to_owned(), input });
            }
        }
        match c["finishReason"].as_str() {
            Some("MAX_TOKENS") => stop = Stop::MaxTokens,
            Some("SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "RECITATION") => stop = Stop::Refusal,
            _ => {}
        }
    }
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        out(Delta::Done(Stop::Cancelled));
        return Ok(());
    }
    if calls > 0 && stop == Stop::End {
        stop = Stop::ToolUse;
    }
    out(Delta::Usage(usage));
    out(Delta::Done(stop));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::{Msg, ToolDef};

    #[test]
    fn calls_get_ids_and_results_go_back_by_name() {
        let sse = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Looking\"}]}}]}\n\n\
                   data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"thought\":true,\"text\":\"hmm\"},{\"functionCall\":{\"name\":\"route\",\"args\":{\"from\":\"1DQ1-A\",\"to\":\"Jita\"}}}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":30,\"candidatesTokenCount\":9}}\n\n";
        let mut d = Vec::new();
        parse_stream(sse.as_bytes(), &AtomicBool::new(false), &mut |x| d.push(x)).unwrap();
        assert_eq!(d[0], Delta::Text("Looking".into()));
        assert!(matches!(&d[1], Delta::ToolUse { id, name, input } if id == "gemini_1" && name == "route" && input["to"] == "Jita"));
        assert_eq!(d.last(), Some(&Delta::Done(Stop::ToolUse)), "a call means the turn waits for results");
        assert!(!d.iter().any(|x| x == &Delta::Text("hmm".into())), "thoughts are not shown");

        let msgs = vec![
            Msg::user("route?"),
            Msg { role: Role::Assistant, blocks: vec![Block::ToolUse { id: "gemini_1".into(), name: "route".into(), input: json!({"from": "A"}) }] },
            Msg { role: Role::User, blocks: vec![Block::ToolResult { id: "gemini_1".into(), content: "2 jumps".into(), is_error: false }] },
        ];
        let tools = vec![ToolDef { name: "route".into(), description: "d".into(), schema: json!({"type": "object", "additionalProperties": false, "properties": {"n": {"type": "integer", "minimum": 1}}}) }];
        let req = Request { system_static: "s", system_dynamic: "", msgs: &msgs, tools: &tools, model: "gemini-2.5-flash", effort: "", max_tokens: 100, conv: "c", mcp: None };
        let b = body(&req);
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["contents"][2]["parts"][0]["functionResponse"]["name"], "route");
        let params = &b["tools"][0]["functionDeclarations"][0]["parameters"];
        assert!(params.get("additionalProperties").is_none() && params["properties"]["n"].get("minimum").is_none());
    }
}
