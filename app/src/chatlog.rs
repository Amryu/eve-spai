use std::path::Path;

#[derive(Clone, Debug)]
pub struct ChatMeta {
    pub channel: String,
    #[allow(dead_code)]
    pub listener: String,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub timestamp: String,
    pub author: String,
    pub text: String,
}

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
