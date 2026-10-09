//! Claude and ChatGPT subscriptions, through the `claude` and `codex` programs the user is signed in
//! to. Each runs its own tool loop and reaches the app's tools over its MCP server; the session only
//! shows the calls it made. Their output formats change between versions, so anything unknown is
//! skipped rather than failing the turn.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::config::CliCfg;
use super::provider::{Block, Caps, Delta, Provider, Request, Role, Stop, Usage};

pub const TOKEN_ENV: &str = "EVE_SPAI_MCP_TOKEN";
const TOOL_PREFIX: &str = "mcp__eve-spai__";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Claude,
    Codex,
}

pub struct Cli {
    which: Which,
    program: String,
    model: String,
}

/// Conversations each CLI already holds: claude's by the id given to it, codex's by the one it named.
static CLAUDE_STARTED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static CODEX_THREADS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

impl Cli {
    pub fn new(which: Which, cfg: &CliCfg) -> Self {
        let program = if cfg.path.trim().is_empty() {
            match which {
                Which::Claude => "claude",
                Which::Codex => "codex",
            }
            .to_owned()
        } else {
            cfg.path.trim().to_owned()
        };
        Self { which, program, model: cfg.model.trim().to_owned() }
    }
}

/// The program's full path, by name on PATH or as given.
pub fn find(program: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(program);
    if p.components().count() > 1 {
        return p.is_file().then(|| p.to_path_buf());
    }
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat", ""] } else { &[""] };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|d| exts.iter().map(move |e| d.join(format!("{program}{e}"))))
        .find(|c| c.is_file())
}

/// A UUID made from the conversation's random id, which claude wants as its session id.
pub fn session_uuid(conv: &str) -> String {
    let mut h: Vec<char> = conv.chars().filter(char::is_ascii_hexdigit).chain(std::iter::repeat('0')).take(32).collect();
    h[12] = '4';
    h[16] = match h[16].to_digit(16).unwrap_or(0) & 0x3 {
        0 => '8',
        1 => '9',
        2 => 'a',
        _ => 'b',
    };
    let s: String = h.into_iter().collect();
    format!("{}-{}-{}-{}-{}", &s[0..8], &s[8..12], &s[12..16], &s[16..20], &s[20..32])
}

fn text_of(blocks: &[Block]) -> String {
    blocks.iter().filter_map(|b| if let Block::Text(t) = b { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
}

/// What the CLI is asked: the latest question, with the conversation so far when the CLI has not
/// seen it (a chat begun on another backend), and for codex, which has no system prompt flag, the
/// instructions too.
pub fn prompt(req: &Request, resumed: bool, with_system: bool) -> String {
    let mut p = String::new();
    if with_system && !resumed {
        p.push_str(req.system_static);
        p.push_str("\n\n");
    }
    if !req.system_dynamic.is_empty() {
        p.push_str("Current situation:\n");
        p.push_str(req.system_dynamic);
        p.push_str("\n\n");
    }
    let (last, earlier) = match req.msgs.split_last() {
        Some((l, e)) => (text_of(&l.blocks), e),
        None => (String::new(), &[][..]),
    };
    if !resumed && !earlier.is_empty() {
        p.push_str("The conversation so far:\n");
        for m in earlier {
            let t = text_of(&m.blocks);
            if !t.is_empty() {
                p.push_str(if m.role == Role::User { "User: " } else { "You: " });
                p.push_str(&t);
                p.push('\n');
            }
        }
        p.push('\n');
    }
    p.push_str(&last);
    p
}

pub fn claude_args(model: &str, uuid: &str, resumed: bool, mcp_file: &str, prompt_file: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "project",
        "--permission-mode",
        "default",
        "--allowedTools",
        "mcp__eve-spai",
        "--disallowedTools",
        "Bash Edit Write Read NotebookEdit Glob Grep Task WebFetch WebSearch",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.extend(["--mcp-config".into(), mcp_file.into(), "--system-prompt-file".into(), prompt_file.into()]);
    if resumed {
        a.extend(["--resume".into(), uuid.into()]);
    } else {
        a.extend(["--session-id".into(), uuid.into()]);
    }
    if !model.is_empty() {
        a.extend(["--model".into(), model.into()]);
    }
    a
}

pub fn codex_args(model: &str, port: u16, thread: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec!["exec".into()];
    if let Some(t) = thread {
        a.extend(["resume".into(), t.into()]);
    }
    a.extend(
        [
            "--json",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "-c",
            "approval_policy=\"never\"",
            "-c",
            "tools.web_search=false",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    a.extend([
        "-c".into(),
        format!("mcp_servers.eve-spai.url=\"http://127.0.0.1:{port}/mcp\""),
        "-c".into(),
        format!("mcp_servers.eve-spai.bearer_token_env_var=\"{TOKEN_ENV}\""),
    ]);
    if !model.is_empty() {
        a.extend(["-m".into(), model.into()]);
    }
    a.push("-".into());
    a
}

/// Reads claude's stream-json lines. Returns the session's error, if it ended in one.
pub fn parse_claude<R: BufRead>(r: R, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
    let mut usage = Usage::default();
    let mut stop = Stop::End;
    for line in r.lines() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line?) else { continue };
        match v["type"].as_str() {
            Some("stream_event") => {
                let e = &v["event"];
                // Text written inside a subagent is not the answer.
                if !v["parent_tool_use_id"].is_null() {
                    continue;
                }
                if e["type"] == "content_block_delta" && e["delta"]["type"] == "text_delta" {
                    if let Some(t) = e["delta"]["text"].as_str() {
                        out(Delta::Text(t.to_owned()));
                    }
                }
            }
            Some("assistant") => {
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    if b["type"] == "tool_use" {
                        let name = b["name"].as_str().unwrap_or_default();
                        out(Delta::ToolUse {
                            id: b["id"].as_str().unwrap_or_default().to_owned(),
                            name: name.strip_prefix(TOOL_PREFIX).unwrap_or(name).to_owned(),
                            input: b["input"].clone(),
                        });
                    }
                }
            }
            Some("result") => {
                let u = &v["usage"];
                usage.input = u["input_tokens"].as_u64().unwrap_or(0) + u["cache_creation_input_tokens"].as_u64().unwrap_or(0) + u["cache_read_input_tokens"].as_u64().unwrap_or(0);
                usage.cached = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
                usage.output = u["output_tokens"].as_u64().unwrap_or(0);
                if v["is_error"].as_bool() == Some(true) {
                    let msg = v["result"].as_str().filter(|s| !s.is_empty()).or(v["subtype"].as_str()).unwrap_or("failed");
                    anyhow::bail!("claude: {msg}");
                }
                if v["stop_reason"] == "max_tokens" {
                    stop = Stop::MaxTokens;
                } else if v["stop_reason"] == "refusal" {
                    stop = Stop::Refusal;
                }
            }
            _ => {}
        }
    }
    if cancel.load(Ordering::Relaxed) {
        out(Delta::Done(Stop::Cancelled));
        return Ok(());
    }
    out(Delta::Usage(usage));
    out(Delta::Done(stop));
    Ok(())
}

/// Reads codex's `exec --json` events. Returns the thread id it reported, for the next turn.
pub fn parse_codex<R: BufRead>(r: R, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<Option<String>> {
    let mut usage = Usage::default();
    let mut thread = None;
    let mut err: Option<String> = None;
    for line in r.lines() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line?) else { continue };
        match v["type"].as_str() {
            Some("thread.started") => thread = v["thread_id"].as_str().map(str::to_owned),
            Some("item.completed") => {
                let it = &v["item"];
                match it["type"].as_str() {
                    Some("agent_message") => {
                        if let Some(t) = it["text"].as_str() {
                            out(Delta::Text(t.to_owned()));
                        }
                    }
                    Some("mcp_tool_call") => {
                        let args = match &it["arguments"] {
                            Value::String(s) => serde_json::from_str(s).unwrap_or(json!({})),
                            other => other.clone(),
                        };
                        out(Delta::ToolUse {
                            id: it["id"].as_str().unwrap_or_default().to_owned(),
                            name: it["tool"].as_str().unwrap_or_default().to_owned(),
                            input: args,
                        });
                    }
                    _ => {}
                }
            }
            Some("turn.completed") => {
                let u = &v["usage"];
                usage.input = u["input_tokens"].as_u64().unwrap_or(0);
                usage.cached = u["cached_input_tokens"].as_u64().unwrap_or(0);
                usage.output = u["output_tokens"].as_u64().unwrap_or(0);
            }
            Some("turn.failed") => err = v["error"]["message"].as_str().map(str::to_owned).or(Some("failed".into())),
            Some("error") => err = v["message"].as_str().map(str::to_owned).or(err),
            _ => {}
        }
    }
    if cancel.load(Ordering::Relaxed) {
        out(Delta::Done(Stop::Cancelled));
        return Ok(thread);
    }
    if let Some(e) = err {
        anyhow::bail!("codex: {e}");
    }
    out(Delta::Usage(usage));
    out(Delta::Done(Stop::End));
    Ok(thread)
}

/// Where the CLI runs: an empty private folder, so no project files or settings are picked up.
fn work_dir() -> anyhow::Result<std::path::PathBuf> {
    let d = crate::store::data_dir()?.join("ai").join("cli");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// Writes a file only the user can read; it holds the tool server's token.
fn write_private(path: &std::path::Path, body: &str) -> anyhow::Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)?.write_all(body.as_bytes())?;
    Ok(())
}

impl Provider for Cli {
    fn stream(&mut self, req: &Request, cancel: &AtomicBool, out: &mut dyn FnMut(Delta)) -> anyhow::Result<()> {
        let (port, token) = req.mcp.ok_or_else(|| anyhow::anyhow!("the tool server is not running"))?;
        let dir = work_dir()?;
        // Resolved here so a Windows `.cmd` shim is found too.
        let mut cmd = Command::new(find(&self.program).unwrap_or_else(|| self.program.clone().into()));
        cmd.current_dir(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).env(TOKEN_ENV, token);
        let mut cleanup = Vec::new();
        let input = match self.which {
            Which::Claude => {
                let uuid = session_uuid(req.conv);
                let resumed = CLAUDE_STARTED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).contains(req.conv);
                let mcp_file = dir.join(format!("mcp-{}.json", &req.conv[..8.min(req.conv.len())]));
                let sys_file = dir.join(format!("system-{}.txt", &req.conv[..8.min(req.conv.len())]));
                let cfg = json!({"mcpServers": {"eve-spai": {"type": "http", "url": format!("http://127.0.0.1:{port}/mcp"), "headers": {"Authorization": format!("Bearer {token}")}}}});
                write_private(&mcp_file, &cfg.to_string())?;
                write_private(&sys_file, req.system_static)?;
                cmd.args(claude_args(&self.model, &uuid, resumed, &mcp_file.to_string_lossy(), &sys_file.to_string_lossy()));
                cleanup.extend([mcp_file, sys_file]);
                prompt(req, resumed, false)
            }
            Which::Codex => {
                let thread = CODEX_THREADS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).get(req.conv).cloned();
                cmd.args(codex_args(&self.model, port, thread.as_deref()));
                prompt(req, thread.is_some(), true)
            }
        };
        let mut child = cmd.spawn().map_err(|e| {
            for f in &cleanup {
                let _ = std::fs::remove_file(f);
            }
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("{} was not found: install it and sign in, or set its path in Settings, Assistant", self.program)
            } else {
                anyhow::anyhow!("could not start {}: {e}", self.program)
            }
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes());
        }
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no output"))?;
        let stderr = child.stderr.take();
        let err_text = Arc::new(Mutex::new(String::new()));
        let et = err_text.clone();
        let err_thread = std::thread::spawn(move || {
            if let Some(s) = stderr {
                let mut buf = String::new();
                let _ = std::io::Read::read_to_string(&mut std::io::Read::take(BufReader::new(s), 16_384), &mut buf);
                *et.lock().unwrap_or_else(|e| e.into_inner()) = buf;
            }
        });
        // The reader blocks on output, so a stop has to end the process for it to return.
        let child = Mutex::new(child);
        let finished = AtomicBool::new(false);
        let res = std::thread::scope(|sc| {
            sc.spawn(|| {
                while !finished.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            });
            let reader = BufReader::new(stdout);
            let res = match self.which {
                Which::Claude => parse_claude(reader, cancel, out),
                Which::Codex => parse_codex(reader, cancel, out).map(|t| {
                    if let Some(t) = t {
                        CODEX_THREADS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(req.conv.to_owned(), t);
                    }
                }),
            };
            finished.store(true, Ordering::Relaxed);
            res
        });
        let status = child.into_inner().unwrap_or_else(|e| e.into_inner()).wait();
        let _ = err_thread.join();
        for f in &cleanup {
            let _ = std::fs::remove_file(f);
        }
        let err = err_text.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let err = super::secrets::redact(err.trim(), &[token.to_owned()]);
        res.map_err(|e| anyhow::anyhow!(super::secrets::redact(&e.to_string(), &[token.to_owned()])))?;
        if self.which == Which::Claude && status.as_ref().is_ok_and(|s| s.success()) {
            CLAUDE_STARTED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(req.conv.to_owned());
        }
        match status {
            Ok(s) if !s.success() && !cancel.load(Ordering::Relaxed) => {
                let tail: String = err.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" ");
                anyhow::bail!("{} stopped ({s}){}", self.program, if tail.is_empty() { String::new() } else { format!(": {tail}") })
            }
            _ => Ok(()),
        }
    }

    fn caps(&self) -> Caps {
        Caps { tools: true, hosts_own_tools: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::Msg;

    #[test]
    fn claude_events_give_text_calls_and_usage() {
        let lines = include_str!("fixtures/claude_cli.jsonl");
        let mut d = Vec::new();
        parse_claude(lines.as_bytes(), &AtomicBool::new(false), &mut |x| d.push(x)).unwrap();
        let text: String = d.iter().filter_map(|x| if let Delta::Text(t) = x { Some(t.as_str()) } else { None }).collect();
        assert_eq!(text, "Two jumps.");
        assert!(d.iter().any(|x| matches!(x, Delta::ToolUse { name, .. } if name == "route")), "{d:?}");
        assert!(matches!(d[d.len() - 2], Delta::Usage(Usage { output: 40, .. })));
        assert_eq!(d.last(), Some(&Delta::Done(Stop::End)));
        let failed = "{\"type\":\"result\",\"subtype\":\"error_during_execution\",\"is_error\":true,\"result\":\"Not logged in\",\"usage\":{}}\n";
        let e = parse_claude(failed.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains("Not logged in"));
    }

    #[test]
    fn codex_events_give_text_calls_and_the_thread() {
        let lines = "{\"type\":\"thread.started\",\"thread_id\":\"th_1\"}\n\
                     {\"type\":\"item.completed\",\"item\":{\"id\":\"i1\",\"type\":\"mcp_tool_call\",\"server\":\"eve-spai\",\"tool\":\"route\",\"arguments\":{\"from\":\"1DQ1-A\"}}}\n\
                     {\"type\":\"item.completed\",\"item\":{\"id\":\"i2\",\"type\":\"agent_message\",\"text\":\"Two jumps.\"}}\n\
                     {\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":100,\"cached_input_tokens\":80,\"output_tokens\":7}}\n";
        let mut d = Vec::new();
        let t = parse_codex(lines.as_bytes(), &AtomicBool::new(false), &mut |x| d.push(x)).unwrap();
        assert_eq!(t.as_deref(), Some("th_1"));
        assert!(matches!(&d[0], Delta::ToolUse { name, input, .. } if name == "route" && input["from"] == "1DQ1-A"));
        assert_eq!(d[1], Delta::Text("Two jumps.".into()));
        let failed = "{\"type\":\"turn.failed\",\"error\":{\"message\":\"usage limit reached\"}}\n";
        assert!(parse_codex(failed.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap_err().to_string().contains("usage limit"));
    }

    #[test]
    fn the_cli_gets_only_the_apps_tools() {
        let a = claude_args("", &session_uuid("ab"), false, "/m.json", "/s.txt");
        let at = |k: &str| a.iter().position(|x| x == k).map(|i| a[i + 1].as_str());
        assert_eq!(at("--tools"), Some(""), "no built-in tools");
        assert_eq!(at("--allowedTools"), Some("mcp__eve-spai"));
        assert!(at("--disallowedTools").unwrap().contains("Bash"));
        assert!(a.contains(&"--strict-mcp-config".to_owned()));
        assert!(at("--session-id").is_some() && at("--resume").is_none());
        assert!(claude_args("", "u", true, "m", "s").contains(&"--resume".to_owned()));
        let c = codex_args("gpt-5", 4321, Some("th_1"));
        assert_eq!(&c[..3], ["exec", "resume", "th_1"]);
        assert!(c.iter().any(|x| x.contains("read-only")));
        assert!(c.iter().any(|x| x.contains("127.0.0.1:4321")));
        assert!(!c.iter().any(|x| x.contains("Bearer")), "the token goes by environment, not argument");
    }

    /// Runs the real `claude` against the app's tool server: `cargo test --bin eve-spai ai_cli_live -- --ignored --nocapture`.
    #[test]
    #[ignore = "runs the claude CLI on the user's subscription"]
    fn ai_cli_live() {
        std::env::set_var("EVE_SPAI_DATA_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/../target/uitest-profile"));
        let deps = crate::ai::deps::AiDeps::for_tests(crate::ai::tools::testkit::facts(&["actions.map"]));
        let server = crate::ai::mcp::start(deps).unwrap();
        let mut cli = Cli::new(Which::Claude, &CliCfg { path: String::new(), model: "claude-haiku-4-5".into() });
        let conv = crate::ai::mcp::new_token();
        for q in ["How many jumps from 1DQ1-A to 7-K5EL? Use the route tool, then answer with just the number.", "And back again? Just the number."] {
            let msgs = vec![Msg::user(q)];
            let req = Request { system_static: "You answer EVE questions with the tools given.", system_dynamic: "", msgs: &msgs, tools: &[], model: "", effort: "", max_tokens: 1, conv: &conv, mcp: Some((server.port, &server.token)) };
            let mut d = Vec::new();
            cli.stream(&req, &AtomicBool::new(false), &mut |x| d.push(x)).unwrap();
            println!("{d:?}");
            assert!(d.iter().any(|x| matches!(x, Delta::Text(t) if !t.trim().is_empty())));
        }
    }

    #[test]
    fn session_ids_are_uuids_and_prompts_carry_what_the_cli_has_not_seen() {
        let u = session_uuid(&crate::ai::mcp::new_token());
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        let msgs = vec![Msg::user("where is the gang?"), Msg { role: Role::Assistant, blocks: vec![Block::Text("QX-LIJ".into())] }, Msg::user("and now?")];
        let req = Request { system_static: "RULES", system_dynamic: "You are in 1DQ1-A", msgs: &msgs, tools: &[], model: "", effort: "", max_tokens: 1, conv: "c", mcp: None };
        let fresh = prompt(&req, false, true);
        assert!(fresh.starts_with("RULES") && fresh.contains("You: QX-LIJ") && fresh.ends_with("and now?"));
        let resumed = prompt(&req, true, true);
        assert!(!resumed.contains("RULES") && !resumed.contains("QX-LIJ") && resumed.contains("1DQ1-A"));
    }
}
