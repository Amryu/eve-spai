//! Parsing EVE Online chat-log files.
//!
//! EVE writes chat logs as UTF-16LE text. Each file starts with a header block of
//! `Key: value` lines (Channel Name, Listener, Session started, …) and message
//! lines of the form `[ YYYY.MM.DD HH:MM:SS ] Author > message`. Every line is
//! prefixed with a U+FEFF BOM, which we strip. These are EVE's static formats.

use std::path::Path;

#[derive(Clone, Debug)]
pub struct ChatMeta {
    pub channel: String,
    /// The character whose client wrote the log (used for local-system tracking later).
    #[allow(dead_code)]
    pub listener: String,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    /// Raw EVE timestamp, e.g. "2026.06.22 18:30:45" (EVE/UTC).
    pub timestamp: String,
    pub author: String,
    pub text: String,
}

/// Read and parse a chat-log file. Returns metadata + all message lines.
pub fn read(path: &Path) -> Option<(ChatMeta, Vec<ChatMessage>)> {
    let bytes = std::fs::read(path).ok()?;
    parse(&decode_utf16le(&bytes))
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

fn parse(text: &str) -> Option<(ChatMeta, Vec<ChatMessage>)> {
    let mut channel: Option<String> = None;
    let mut listener: Option<String> = None;
    let mut messages = Vec::new();

    for raw in text.lines() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        if let Some(rest) = line.strip_prefix("Channel Name:") {
            channel = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("Listener:") {
            listener = Some(rest.trim().to_owned());
        } else if let Some(m) = parse_message(line) {
            messages.push(m);
        }
    }

    Some((
        ChatMeta {
            channel: channel?,
            listener: listener.unwrap_or_default(),
        },
        messages,
    ))
}

/// Parse one message line: `[ 2026.06.22 18:30:45 ] Author > message`.
fn parse_message(line: &str) -> Option<ChatMessage> {
    let line = line.strip_prefix("[ ")?;
    let (timestamp, rest) = line.split_once(" ] ")?;
    let (author, text) = rest.split_once(" > ")?;
    Some(ChatMessage {
        timestamp: timestamp.trim().to_owned(),
        author: author.trim().to_owned(),
        text: text.to_owned(),
    })
}
