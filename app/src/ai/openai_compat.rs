//! Any server speaking OpenAI's chat completions: OpenAI itself, OpenRouter, Groq, Mistral, and
//! the local ones (Ollama, LM Studio, llama.cpp, vLLM).
//!
//! Tool calls arrive in pieces keyed by index and are put together when the turn ends. A model
//! without tool calling gets none offered and answers from the situation summary alone.

use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;

use super::provider::{parse_tool_input, Block, Caps, Delta, Msg, Provider, Request, Role, Stop, Usage};
use super::sse::SseReader;

pub struct OpenAiCompat {
    pub base: String,
    pub key: Option<String>,
    pub tools: bool,
}

pub fn body(req: &Request, tools: bool) -> Value {
    let mut sys = req.system_static.to_owned();
    if !req.system_dynamic.is_empty() {
        sys.push_str("\n\n");
        sys.push_str(req.system_dynamic);
    }
    let mut msgs = vec![json!({"role": "system", "content": sys})];
    for m in req.msgs {
        msgs.extend(messages(m));
    }
    let mut b = json!({
        "model": req.model,
        "stream": true,
        "stream_options": {"include_usage": true},
        "max_tokens": req.max_tokens,
        "messages": msgs,
    });
    if tools && !req.tools.is_empty() {
        b["tools"] = json!(req
            .tools
            .iter()
            .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.schema}}))
            .collect::<Vec<_>>());
    }
    b
}

/// One of our messages as OpenAI ones: tool results each become a `tool` message of their own.
fn messages(m: &Msg) -> Vec<Value> {
    match m.role {
        Role::User => {
            let mut out = Vec::new();
            let mut text = String::new();
            for b in &m.blocks {
                match b {
                    Block::Text(t) => text.push_str(t),
                    Block::ToolResult { id, content, is_error } => {
                        let c = if *is_error { format!("error: {content}") } else { content.clone() };
                        out.push(json!({"role": "tool", "tool_call_id": id, "content": c}));
                    }
                    _ => {}
                }
            }
            if !text.is_empty() {
                out.push(json!({"role": "user", "content": text}));
            }
            out
        }
        Role::Assistant => {
            let text = m.text();
            let calls: Vec<Value> = m
                .blocks
                .iter()
                .filter_map(|b| match b {
                    Block::ToolUse { id, name, input } => Some(json!({"id": id, "type": "function", "function": {"name": name, "arguments": input.to_string()}})),
                    _ => None,
                })
                .collect();
            let mut v = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) }});
            if !calls.is_empty() {
                v["tool_calls"] = json!(calls);
            }
            vec![v]
        }
    }
}

impl Provider for OpenAiCompat {
    fn stream(&mut self, req: &Request, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
        let client = crate::http::client(30)?;
        let mut rb = client
            .post(format!("{}/chat/completions", self.base))
            .timeout(std::time::Duration::from_secs(600))
            .header("content-type", "application/json");
        if let Some(k) = self.key.as_deref().filter(|k| !k.is_empty()) {
            rb = rb.header("authorization", format!("Bearer {k}"));
        }
        let resp = rb.body(body(req, self.tools).to_string()).send().map_err(|e| anyhow::anyhow!("could not reach {}: {e}", self.base))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            let secrets: Vec<String> = self.key.iter().cloned().collect();
            let msg = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                .unwrap_or_else(|| text.chars().take(300).collect());
            anyhow::bail!("{} answered {status}: {}", self.base, super::secrets::redact(&msg, &secrets));
        }
        parse_stream(resp, cancel, out)
    }

    fn caps(&self) -> Caps {
        Caps { tools: self.tools, hosts_own_tools: false }
    }
}

#[derive(Default)]
struct Call {
    id: String,
    name: String,
    args: String,
}

pub fn parse_stream<R: std::io::Read>(r: R, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
    let mut sse = SseReader::new(r);
    let mut calls: Vec<Call> = Vec::new();
    let mut usage = Usage::default();
    let mut finish: Option<String> = None;
    while let Some(ev) = sse.next(cancel) {
        if ev.data.trim() == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else { continue };
        if let Some(e) = v["error"]["message"].as_str() {
            anyhow::bail!("{e}");
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            usage.input = u["prompt_tokens"].as_u64().unwrap_or(0);
            usage.output = u["completion_tokens"].as_u64().unwrap_or(0);
            usage.cached = u["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0);
        }
        let Some(choice) = v["choices"].get(0) else { continue };
        let d = &choice["delta"];
        if let Some(t) = d["content"].as_str().filter(|t| !t.is_empty()) {
            out(Delta::Text(t.to_owned()));
        }
        for tc in d["tool_calls"].as_array().into_iter().flatten() {
            let i = tc["index"].as_u64().unwrap_or(calls.len() as u64) as usize;
            if calls.len() <= i {
                calls.resize_with(i + 1, Call::default);
            }
            let c = &mut calls[i];
            if let Some(id) = tc["id"].as_str() {
                c.id = id.to_owned();
            }
            if let Some(n) = tc["function"]["name"].as_str() {
                c.name.push_str(n);
            }
            match &tc["function"]["arguments"] {
                Value::String(a) => c.args.push_str(a),
                // Some local servers send the arguments as an object, whole.
                Value::Object(_) => c.args = tc["function"]["arguments"].to_string(),
                _ => {}
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            finish = Some(f.to_owned());
        }
    }
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        out(Delta::Done(Stop::Cancelled));
        return Ok(());
    }
    for (i, c) in calls.into_iter().enumerate().filter(|(_, c)| !c.name.is_empty()) {
        let id = if c.id.is_empty() { format!("call_{i}") } else { c.id };
        match parse_tool_input(&c.args) {
            Ok(input) => out(Delta::ToolUse { id, name: c.name, input }),
            Err(error) => out(Delta::BadToolUse { id, name: c.name, error }),
        }
    }
    out(Delta::Usage(usage));
    out(Delta::Done(match finish.as_deref() {
        Some("tool_calls") => Stop::ToolUse,
        Some("length") => Stop::MaxTokens,
        Some("content_filter") => Stop::Refusal,
        _ => Stop::End,
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::ToolDef;

    #[test]
    fn tool_calls_in_pieces_are_put_together() {
        let mut d = Vec::new();
        parse_stream(include_str!("fixtures/openai_tool.sse").as_bytes(), &AtomicBool::new(false), &mut |x| d.push(x)).unwrap();
        let text: String = d.iter().filter_map(|x| if let Delta::Text(t) = x { Some(t.as_str()) } else { None }).collect();
        assert_eq!(text, "Let me look.");
        let calls: Vec<&Delta> = d.iter().filter(|x| matches!(x, Delta::ToolUse { .. })).collect();
        assert_eq!(calls.len(), 2);
        assert!(matches!(calls[0], Delta::ToolUse { name, input, .. } if name == "route" && input["to"] == "Jita"));
        assert!(matches!(calls[1], Delta::ToolUse { name, input, id } if name == "system_info" && input["system"] == "Amarr" && id == "call_b"));
        assert!(d.iter().any(|x| matches!(x, Delta::Usage(u) if u.input == 50 && u.output == 20)));
        assert_eq!(d.last(), Some(&Delta::Done(Stop::ToolUse)));
    }

    #[test]
    fn tool_results_become_tool_messages() {
        let msgs = vec![
            Msg::user("route?"),
            Msg { role: Role::Assistant, blocks: vec![Block::ToolUse { id: "c1".into(), name: "route".into(), input: json!({"from": "A"}) }] },
            Msg { role: Role::User, blocks: vec![Block::ToolResult { id: "c1".into(), content: "3 jumps".into(), is_error: false }] },
        ];
        let tools = vec![ToolDef { name: "route".into(), description: "d".into(), schema: json!({"type": "object"}) }];
        let req = Request { system_static: "s", system_dynamic: "d", msgs: &msgs, tools: &tools, model: "m", effort: "", max_tokens: 100 };
        let b = body(&req, true);
        let m = b["messages"].as_array().unwrap();
        assert_eq!(m[0]["content"], "s\n\nd");
        assert_eq!(m[2]["tool_calls"][0]["function"]["arguments"], "{\"from\":\"A\"}");
        assert_eq!(m[3]["role"], "tool");
        assert_eq!(b["tools"][0]["function"]["name"], "route");
        assert!(body(&req, false).get("tools").is_none(), "no tools for a model without them");
    }
}
