//! Claude through the Anthropic Messages API, streamed.
//!
//! The tool definitions and the fixed instructions are marked for prompt caching; the situation
//! summary follows them, so it can change every turn without spoiling the cache. Thinking blocks
//! and other Claude-only blocks come back as `Block::Raw` and are replayed unchanged. A refusal
//! falls back to the model Anthropic picks for the case (`fallbacks: "default"`).

use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;

use super::provider::{parse_tool_input, Block, Caps, Delta, Msg, Provider, Request, Role, Stop, Usage};
use super::sse::SseReader;

pub const NAME: &str = "anthropic";
const URL: &str = "https://api.anthropic.com/v1/messages";

pub struct Anthropic {
    pub key: String,
    /// Swap the client's own web search for Claude's server-side one.
    pub server_web_search: bool,
}

/// Models that take the server-side refusal fallback.
fn takes_fallbacks(model: &str) -> bool {
    ["claude-opus-5", "claude-sonnet-5-5", "claude-fable-5"].iter().any(|p| model.starts_with(p))
}

pub fn body(req: &Request, server_web_search: bool) -> Value {
    let mut tools: Vec<Value> = Vec::new();
    if server_web_search {
        tools.push(json!({"type": "web_search_20260209", "name": "web_search", "max_uses": 5}));
    }
    for t in req.tools.iter().filter(|t| !(server_web_search && t.name == "web_search")) {
        tools.push(json!({"name": t.name, "description": t.description, "input_schema": t.schema, "eager_input_streaming": true}));
    }
    if let Some(last) = tools.last_mut() {
        last["cache_control"] = json!({"type": "ephemeral"});
    }
    let mut system = vec![json!({"type": "text", "text": req.system_static, "cache_control": {"type": "ephemeral"}})];
    if !req.system_dynamic.is_empty() {
        system.push(json!({"type": "text", "text": req.system_dynamic}));
    }
    let mut b = json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "stream": true,
        "system": system,
        "messages": req.msgs.iter().filter_map(message).collect::<Vec<_>>(),
    });
    if !tools.is_empty() {
        b["tools"] = json!(tools);
    }
    if !req.effort.is_empty() {
        b["output_config"] = json!({"effort": req.effort});
    }
    if takes_fallbacks(req.model) {
        b["fallbacks"] = json!("default");
    }
    b
}

fn message(m: &Msg) -> Option<Value> {
    let content: Vec<Value> = m
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Text(t) if !t.is_empty() => Some(json!({"type": "text", "text": t})),
            Block::Text(_) => None,
            Block::ToolUse { id, name, input } => Some(json!({"type": "tool_use", "id": id, "name": name, "input": input})),
            Block::ToolResult { id, content, is_error } => {
                Some(json!({"type": "tool_result", "tool_use_id": id, "content": content, "is_error": is_error}))
            }
            Block::Raw { provider, value } if *provider == NAME => Some(value.clone()),
            Block::Raw { .. } => None,
        })
        .collect();
    if content.is_empty() {
        return None;
    }
    Some(json!({"role": if m.role == Role::User { "user" } else { "assistant" }, "content": content}))
}

impl Provider for Anthropic {
    fn stream(&mut self, req: &Request, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
        let mut betas = Vec::new();
        if takes_fallbacks(req.model) {
            betas.push("server-side-fallback-2026-07-01");
        }
        let client = crate::http::client(30)?;
        let mut rb = client
            .post(URL)
            .timeout(std::time::Duration::from_secs(600))
            .header("x-api-key", &self.key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
        if !betas.is_empty() {
            rb = rb.header("anthropic-beta", betas.join(","));
        }
        let resp = rb.body(body(req, self.server_web_search).to_string()).send()?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("{}", error_text(status.as_u16(), &super::secrets::redact(&text, &[self.key.clone()])));
        }
        parse_stream(resp, cancel, out)
    }

    fn caps(&self) -> Caps {
        Caps { tools: true, hosts_own_tools: false }
    }
}

/// A readable error from the API's error body.
pub fn error_text(status: u16, body: &str) -> String {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status {
        401 => format!("The API key was not accepted ({msg})"),
        429 => format!("Rate limited by Anthropic, try again shortly ({msg})"),
        529 => "Anthropic is overloaded, try again shortly".to_owned(),
        _ => format!("Anthropic answered {status}: {msg}"),
    }
}

enum Open {
    Text,
    Tool { id: String, name: String, json: String },
    /// A block kept whole: thinking, server tool calls and their results, fallback markers.
    Raw { value: Value, json: String },
}

pub fn parse_stream<R: std::io::Read>(r: R, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
    let mut sse = SseReader::new(r);
    let mut open: Vec<Option<Open>> = Vec::new();
    let mut usage = Usage::default();
    let mut stop = Stop::End;
    while let Some(ev) = sse.next(cancel) {
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else { continue };
        match v["type"].as_str().unwrap_or(&ev.event) {
            "message_start" => {
                let u = &v["message"]["usage"];
                usage.input = u["input_tokens"].as_u64().unwrap_or(0) + u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                usage.cached = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
            }
            "content_block_start" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                if open.len() <= i {
                    open.resize_with(i + 1, || None);
                }
                let cb = &v["content_block"];
                open[i] = Some(match cb["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = cb["text"].as_str().filter(|t| !t.is_empty()) {
                            out(Delta::Text(t.to_owned()));
                        }
                        Open::Text
                    }
                    Some("tool_use") => Open::Tool {
                        id: cb["id"].as_str().unwrap_or_default().to_owned(),
                        name: cb["name"].as_str().unwrap_or_default().to_owned(),
                        json: String::new(),
                    },
                    _ => Open::Raw { value: cb.clone(), json: String::new() },
                });
            }
            "content_block_delta" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                let d = &v["delta"];
                match (open.get_mut(i).and_then(Option::as_mut), d["type"].as_str()) {
                    (Some(Open::Text), Some("text_delta")) => {
                        if let Some(t) = d["text"].as_str() {
                            out(Delta::Text(t.to_owned()));
                        }
                    }
                    (Some(Open::Tool { json, .. } | Open::Raw { json, .. }), Some("input_json_delta")) => {
                        json.push_str(d["partial_json"].as_str().unwrap_or_default());
                    }
                    (Some(Open::Raw { value, .. }), Some("thinking_delta")) => {
                        let t = value["thinking"].as_str().unwrap_or_default().to_owned() + d["thinking"].as_str().unwrap_or_default();
                        value["thinking"] = json!(t);
                    }
                    (Some(Open::Raw { value, .. }), Some("signature_delta")) => {
                        let s = value["signature"].as_str().unwrap_or_default().to_owned() + d["signature"].as_str().unwrap_or_default();
                        value["signature"] = json!(s);
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                match open.get_mut(i).and_then(Option::take) {
                    Some(Open::Tool { id, name, json }) => match parse_tool_input(&json) {
                        Ok(input) => out(Delta::ToolUse { id, name, input }),
                        Err(error) => out(Delta::BadToolUse { id, name, error }),
                    },
                    Some(Open::Raw { mut value, json }) => {
                        if !json.is_empty() {
                            value["input"] = serde_json::from_str(&json).unwrap_or(json!({}));
                        }
                        out(Delta::Raw(Block::Raw { provider: NAME, value }));
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                stop = match v["delta"]["stop_reason"].as_str() {
                    Some("tool_use") => Stop::ToolUse,
                    Some("max_tokens") => Stop::MaxTokens,
                    Some("refusal") => Stop::Refusal,
                    Some("pause_turn") => Stop::Pause,
                    _ => Stop::End,
                };
                usage.output = v["usage"]["output_tokens"].as_u64().unwrap_or(usage.output);
            }
            "message_stop" => break,
            "error" => anyhow::bail!("{}", v["error"]["message"].as_str().unwrap_or("the stream failed")),
            _ => {}
        }
    }
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        out(Delta::Done(Stop::Cancelled));
        return Ok(());
    }
    out(Delta::Usage(usage));
    out(Delta::Done(stop));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::ToolDef;

    fn collect(sse: &str) -> Vec<Delta> {
        let mut v = Vec::new();
        parse_stream(sse.as_bytes(), &AtomicBool::new(false), &mut |d| v.push(d)).unwrap();
        v
    }

    #[test]
    fn text_thinking_and_a_split_tool_call_come_back_in_order() {
        let sse = include_str!("fixtures/anthropic_tool.sse");
        let d = collect(sse);
        let text: String = d.iter().filter_map(|x| if let Delta::Text(t) = x { Some(t.as_str()) } else { None }).collect();
        assert_eq!(text, "Checking the kills.");
        assert!(matches!(&d[0], Delta::Raw(Block::Raw { value, .. }) if value["type"] == "thinking" && value["signature"] == "sig123"), "{:?}", d[0]);
        assert!(d.iter().any(|x| matches!(x, Delta::ToolUse { name, input, .. } if name == "recent_kills" && input["system"] == "1DQ1-A")));
        assert_eq!(d.last(), Some(&Delta::Done(Stop::ToolUse)));
        assert!(d.iter().any(|x| matches!(x, Delta::Usage(u) if u.cached == 900 && u.output == 42)));
    }

    #[test]
    fn a_broken_tool_input_is_reported_not_run() {
        let sse = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"route\",\"input\":{}}}\n\n\
                   event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"from\\\":\"}}\n\n\
                   event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                   event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"},\"usage\":{\"output_tokens\":3}}\n\n";
        let d = collect(sse);
        assert!(d.iter().any(|x| matches!(x, Delta::BadToolUse { name, .. } if name == "route")));
        assert_eq!(d.last(), Some(&Delta::Done(Stop::MaxTokens)));
    }

    #[test]
    fn the_request_caches_its_fixed_prefix() {
        let tools = vec![
            ToolDef { name: "route".into(), description: "d".into(), schema: json!({"type": "object"}) },
            ToolDef { name: "web_search".into(), description: "d".into(), schema: json!({"type": "object"}) },
        ];
        let msgs = vec![
            Msg::user("where"),
            Msg { role: Role::Assistant, blocks: vec![Block::Raw { provider: "openai", value: json!({"x": 1}) }, Block::Text("ok".into())] },
        ];
        let req = Request { system_static: "rules", system_dynamic: "now", msgs: &msgs, tools: &tools, model: "claude-opus-5-5", effort: "low", max_tokens: 1000 };
        let b = body(&req, true);
        assert_eq!(b["tools"][0]["type"], "web_search_20260209", "the server search replaces ours");
        assert_eq!(b["tools"].as_array().unwrap().len(), 2);
        assert_eq!(b["tools"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(b["tools"][1]["eager_input_streaming"], true);
        assert_eq!(b["system"][0]["cache_control"]["type"], "ephemeral");
        assert!(b["system"][1].get("cache_control").is_none());
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["output_config"]["effort"], "low");
        assert_eq!(b["messages"][1]["content"].as_array().unwrap().len(), 1, "another provider's block is not sent");
    }
}
