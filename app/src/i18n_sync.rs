//! Keeps the translation files in step with the marked text in the code.
//!
//! `cargo test --bin eve-spai i18n_files_are_in_sync` fails when a marked text has no entry in a
//! language file; `cargo test --bin eve-spai i18n_update -- --ignored` writes the missing ones in
//! (empty, for a translator to fill) and drops entries whose English is gone. Translations already
//! there are kept.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("app/src"), &mut out);
    walk(&root().join("crates/spai-ui/src"), &mut out);
    walk(&root().join("crates/spai-core/src"), &mut out);
    out
}

/// The string literals of `src`, unescaped, in order.
fn literals(src: &str) -> Vec<String> {
    let b = src.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        match b[i] {
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                out.push(unescape(&src[i + 1..j.min(b.len())]));
                i = j + 1;
            }
            // A char literal holding a quote would open a string that is not there.
            b'\'' if b.get(i + 1) == Some(&b'"') => i += 3,
            _ => i += 1,
        }
    }
    out
}

/// What `label()` and `title()` return: the UI shows them through `.tr()`.
fn label_fn_texts(src: &str, out: &mut BTreeSet<String>) {
    for head in ["fn label(self) -> &'static str {", "fn title(self) -> &'static str {"] {
        let mut rest = src;
        while let Some(i) = rest.find(head) {
            let body = &rest[i + head.len()..];
            let mut depth = 1;
            let mut end = body.len();
            for (k, c) in body.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = k;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            out.extend(literals(&body[..end]).into_iter().filter(|l| l.chars().any(char::is_alphabetic)));
            rest = &body[end..];
        }
    }
}

/// A Rust string literal's text, escapes resolved.
fn unescape(body: &str) -> String {
    let mut out = String::new();
    let mut it = body.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('u') => {
                it.next();
                let hex: String = it.by_ref().take_while(|c| *c != '}').collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            // A line continuation: the newline and the next line's indent go.
            Some('\n') => {
                while it.peek().is_some_and(|c| c.is_whitespace()) {
                    it.next();
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Every marked text in the code.
pub fn marked() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for f in sources() {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        if f.ends_with("i18n_sync.rs") || f.ends_with("i18n.rs") {
            continue;
        }
        label_fn_texts(&src, &mut out);
        for mac in ["tr!(", "trf!(", "tr_noop!("] {
            let mut rest = src.as_str();
            while let Some(i) = rest.find(mac) {
                // A whole macro name, not the end of another (include_str!).
                let whole = rest[..i].chars().last().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                rest = &rest[i + mac.len()..];
                if !whole {
                    continue;
                }
                let t = rest.trim_start();
                if !t.starts_with('"') {
                    continue;
                }
                let bytes = t.as_bytes();
                let mut j = 1;
                while j < bytes.len() {
                    match bytes[j] {
                        b'\\' => j += 2,
                        b'"' => break,
                        _ => j += 1,
                    }
                }
                out.insert(unescape(&t[1..j.min(t.len())]));
            }
        }
    }
    out
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn file_of(code: &str) -> PathBuf {
    root().join("crates/spai-ui/i18n").join(format!("{code}.json"))
}

fn write(code: &str, map: &BTreeMap<String, String>) {
    let mut s = String::from("{\n");
    let n = map.len();
    for (i, (k, v)) in map.iter().enumerate() {
        s.push_str(&format!("  {}: {}{}\n", json_escape(k), json_escape(v), if i + 1 < n { "," } else { "" }));
    }
    s.push_str("}\n");
    std::fs::write(file_of(code), s).expect("write the language file");
}

#[test]
fn i18n_files_are_in_sync() {
    let keys = marked();
    assert!(keys.len() > 1000, "the marked text was not found: {}", keys.len());
    for (code, _) in spai_ui::i18n::LANGUAGES.iter().skip(1) {
        let have = spai_ui::i18n::parse(&std::fs::read_to_string(file_of(code)).unwrap()).unwrap();
        let missing: Vec<&String> = keys.iter().filter(|k| !have.contains_key(*k)).take(5).collect();
        assert!(missing.is_empty(), "{code}.json lacks {missing:?}...: run `cargo test --bin eve-spai i18n_update -- --ignored`");
    }
}

#[test]
#[ignore = "rewrites the language files"]
fn i18n_update() {
    let keys = marked();
    for (code, _) in spai_ui::i18n::LANGUAGES.iter().skip(1) {
        let have = spai_ui::i18n::parse(&std::fs::read_to_string(file_of(code)).unwrap_or_default()).unwrap_or_default();
        let map: BTreeMap<String, String> = keys.iter().map(|k| (k.clone(), have.get(k).cloned().unwrap_or_default())).collect();
        write(code, &map);
        let done = map.values().filter(|v| !v.is_empty()).count();
        println!("{code}: {} texts, {done} translated", map.len());
    }
}
