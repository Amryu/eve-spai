use std::path::Path;

#[derive(Clone, Debug)]
pub struct ChatMeta {
    pub channel: String,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub timestamp: String,
    pub author: String,
    pub text: String,
}

/// The whole log: channel header and every message.
pub fn read(path: &Path) -> Option<(ChatMeta, Vec<ChatMessage>)> {
    let (meta, messages, _) = read_tail(path, 0)?;
    Some((meta?, messages))
}

/// What the log gained since `offset` bytes, and where the next read starts. A log is re-read
/// every poll while EVE is writing to it, and a day-old channel log is megabytes, so only the new
/// bytes are parsed. The header, and with it the channel name, only comes back for `offset == 0`.
///
/// The returned offset stops at the last complete line: a line half-written when we read it is
/// parsed on the next poll instead of being lost.
pub fn read_tail(path: &Path, offset: u64) -> Option<(Option<ChatMeta>, Vec<ChatMessage>, u64)> {
    use std::io::{Read, Seek};
    let mut f = std::fs::File::open(path).ok()?;
    if offset > 0 {
        f.seek(std::io::SeekFrom::Start(offset)).ok()?;
    }
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).ok()?;
    // UTF-16: two bytes a unit, and a read can land between them.
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let complete = units.iter().rposition(|u| *u == b'\n' as u16).map_or(0, |i| i + 1);
    let text = String::from_utf16_lossy(&units[..complete]);
    let next = offset + complete as u64 * 2;
    if offset > 0 {
        return Some((None, text.lines().filter_map(|l| parse_message(clean(l))).collect(), next));
    }
    let (meta, messages) = match parse(&text) {
        Some(p) => p,
        // No readable header: the channel is the first part of the file name instead.
        None => {
            let channel = channel_of_file(path)?;
            (ChatMeta { channel }, text.lines().filter_map(|l| parse_message(clean(l))).collect())
        }
    };
    Some((Some(meta), messages, next))
}

/// EVE names a log `<channel>_<YYYYMMDD>_<HHMMSS>_<character id>.txt`; the channel may hold `_`.
pub fn channel_of_file(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let mut parts = stem.rsplitn(4, '_');
    let (_id, _time, _date, channel) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    (!channel.is_empty()).then(|| channel.to_owned())
}

/// Whether an author is EVE itself (MOTDs, channel changes), in any client language.
pub fn is_system_author(author: &str) -> bool {
    let a = author.trim().to_lowercase();
    a.starts_with("eve") && ["system", "系统", "систем", "système", "sistema"].iter().any(|w| a.contains(w))
}

/// The header is written in the client's language ("Channel Name:" in English, other words in a
/// Chinese or Russian client), but always as the same lines in the same order: channel id, channel
/// name, listener, session start. So the English label is taken when it is there, and otherwise the
/// second labelled line of the header.
fn parse(text: &str) -> Option<(ChatMeta, Vec<ChatMessage>)> {
    let mut channel: Option<String> = None;
    let mut labelled: Vec<String> = Vec::new();
    let mut messages = Vec::new();

    for raw in text.lines() {
        let line = clean(raw);
        if let Some(rest) = line.strip_prefix("Channel Name:") {
            channel = Some(rest.trim().to_owned());
        } else if let Some(m) = parse_message(line) {
            messages.push(m);
        } else if messages.is_empty() && !line.starts_with('-') {
            // A header line: a label, a colon (full width in a Chinese client), then the value.
            if let Some(i) = line.find([':', '\u{ff1a}']) {
                let value = line[i..].trim_start_matches([':', '\u{ff1a}']).trim();
                labelled.push(value.to_owned());
            }
        }
    }
    let channel = channel.or_else(|| (labelled.len() >= 4).then(|| labelled[1].clone())).filter(|c| !c.is_empty());

    Some((
        ChatMeta {
            channel: channel?,
        },
        messages,
    ))
}

/// EVE writes a byte-order mark before appended lines, not only at the head of the file, and a BOM
/// is not whitespace: without this every line after the first read fails to parse.
fn clean(line: &str) -> &str {
    line.trim_start_matches('\u{feff}').trim()
}

fn parse_message(line: &str) -> Option<ChatMessage> {
    let line = line.strip_prefix("[ ")?;
    let (timestamp, rest) = line.split_once(" ] ")?;
    let (author, text) = rest.split_once(" > ")?;
    crate::theme::note_text(rest);
    Some(ChatMessage {
        timestamp: timestamp.trim().to_owned(),
        author: author.trim().to_owned(),
        text: text.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn a_tail_read_returns_only_what_was_appended() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/chatlog-test");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("tail.txt");
        let header = "\u{feff}---------------------------------\r\n  Channel Name:    Delve Intel\r\n  Listener:        Fake Pilot\r\n";
        let first = "[ 2026.09.22 18:00:01 ] Fake Pilot > 1DQ1-A clr\r\n";
        std::fs::write(&path, utf16(&format!("{header}{first}"))).expect("write");

        let (meta, msgs, offset) = read_tail(&path, 0).expect("first read");
        assert_eq!(meta.expect("header").channel, "Delve Intel");
        assert_eq!(msgs.len(), 1);
        assert_eq!(offset, utf16(&format!("{header}{first}")).len() as u64);

        // Appended, with a fresh BOM as EVE writes one, and the last line still being written.
        let more = "\u{feff}[ 2026.09.22 18:00:05 ] Fake Pilot > 319-3D hostile\r\n[ 2026.09.22 18:00:0";
        std::fs::write(&path, utf16(&format!("{header}{first}{more}"))).expect("append");
        let (meta, msgs, next) = read_tail(&path, offset).expect("tail read");
        assert!(meta.is_none(), "the header is only in the first read");
        assert_eq!(msgs.len(), 1, "only the finished line");
        assert_eq!(msgs[0].text, "319-3D hostile");
        assert_eq!(
            next,
            offset + utf16("\u{feff}[ 2026.09.22 18:00:05 ] Fake Pilot > 319-3D hostile\r\n").len() as u64
        );

        // Nothing new: no messages, same offset.
        assert_eq!(read_tail(&path, next).expect("empty read").1.len(), 0);
    }

    /// A Chinese client writes the header in Chinese, with full-width colons; the channel is still
    /// found, and its lines read.
    #[test]
    fn a_header_in_another_language_still_names_the_channel() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/chatlog-test");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("delve.imperium_20261010_080000_2112000900.txt");
        let header = "\u{feff}\r\n\r\n        ---------------------------------------------------------------\r\n\r\n          \
                      频道 ID：      -69123456\r\n          频道名称：    delve.imperium\r\n          收听者：      测试飞行员\r\n          \
                      会话开始时间：2026.10.10 08:00:00\r\n\r\n        ---------------------------------------------------------------\r\n\r\n";
        let line = "[ 2026.10.10 08:01:00 ] 测试飞行员 > 1DQ1-A 红名 3\r\n";
        std::fs::write(&path, utf16(&format!("{header}{line}"))).expect("write");
        let (meta, msgs, _) = read_tail(&path, 0).expect("read");
        assert_eq!(meta.expect("header").channel, "delve.imperium");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].author, "测试飞行员");

        // No header at all: the file name still says which channel it is.
        let bare = dir.join("Local_20261010_080000_2112000900.txt");
        std::fs::write(&bare, utf16(line)).expect("write");
        assert_eq!(read_tail(&bare, 0).expect("read").0.expect("meta").channel, "Local");
        assert!(is_system_author("EVE System") && is_system_author("EVE系统") && !is_system_author("Steve"));
    }
}

