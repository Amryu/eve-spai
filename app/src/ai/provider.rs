//! What every model backend speaks: a conversation of blocks in, a stream of deltas out.
//!
//! Each adapter turns these into its own wire format and back. A block only one provider
//! understands (a signed thinking block, a fallback marker) travels as [`Block::Raw`] and is sent
//! back unchanged, and only to the provider it came from.

use serde_json::Value;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(String),
    ToolUse { id: String, name: String, input: Value },
    ToolResult { id: String, content: String, is_error: bool },
    /// A provider's own block, replayed as it came. `provider` names who may read it.
    Raw { provider: &'static str, value: Value },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    pub role: Role,
    pub blocks: Vec<Block>,
}

impl Msg {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, blocks: vec![Block::Text(text.into())] }
    }

    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

pub struct Request<'a> {
    /// Stays the same between requests, so providers that cache a prefix can cache it.
    pub system_static: &'a str,
    /// Changes between requests (the situation summary); kept after the cached prefix.
    pub system_dynamic: &'a str,
    pub msgs: &'a [Msg],
    pub tools: &'a [ToolDef],
    pub model: &'a str,
    pub effort: &'a str,
    pub max_tokens: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cached: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The model finished its answer.
    End,
    /// It wants tool results before going on.
    ToolUse,
    /// Out of output room.
    MaxTokens,
    /// Declined by the provider's safety checks.
    Refusal,
    /// The provider paused a long server-side step; sending the conversation again continues it.
    Pause,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Delta {
    Text(String),
    /// A complete tool call, its input already parsed.
    ToolUse { id: String, name: String, input: Value },
    /// A tool call whose input did not parse: answered with an error result, never run.
    BadToolUse { id: String, name: String, error: String },
    /// A block to keep in the transcript and replay as is.
    Raw(Block),
    Usage(Usage),
    Done(Stop),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Caps {
    pub tools: bool,
    /// Runs the tool loop itself (the CLI backends): the session only shows what it did.
    pub hosts_own_tools: bool,
}

pub trait Provider: Send {
    /// Streams one model turn into `out`. Returns when the turn ends, fails, or `cancel` is set.
    fn stream(&mut self, req: &Request, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()>;
    fn caps(&self) -> Caps;
}

/// Parses a tool call's accumulated input. Empty input means no arguments.
pub fn parse_tool_input(raw: &str) -> Result<Value, String> {
    if raw.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    match serde_json::from_str::<Value>(raw) {
        Ok(v @ Value::Object(_)) => Ok(v),
        Ok(_) => Err("tool input must be a JSON object".into()),
        Err(e) => Err(format!("tool input is not valid JSON: {e}")),
    }
}

/// Replays a script of turns, for tests of the session loop.
#[cfg(test)]
pub struct FakeProvider {
    pub turns: std::collections::VecDeque<Vec<Delta>>,
    pub seen: std::sync::Arc<std::sync::Mutex<Vec<Vec<Msg>>>>,
}

#[cfg(test)]
impl FakeProvider {
    pub fn new(turns: Vec<Vec<Delta>>) -> Self {
        Self { turns: turns.into(), seen: Default::default() }
    }
}

#[cfg(test)]
impl Provider for FakeProvider {
    fn stream(&mut self, req: &Request, _cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
        self.seen.lock().unwrap().push(req.msgs.to_vec());
        for d in self.turns.pop_front().unwrap_or_else(|| vec![Delta::Done(Stop::End)]) {
            out(d);
        }
        Ok(())
    }

    fn caps(&self) -> Caps {
        Caps { tools: true, hosts_own_tools: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_input_parses_strictly() {
        assert_eq!(parse_tool_input("").unwrap(), serde_json::json!({}));
        assert_eq!(parse_tool_input(r#"{"system":"1DQ1-A"}"#).unwrap()["system"], "1DQ1-A");
        assert!(parse_tool_input(r#"{"system":"1DQ"#).is_err(), "cut short");
        assert!(parse_tool_input("[1]").is_err());
    }
}
