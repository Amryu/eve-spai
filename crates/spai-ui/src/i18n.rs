//! The app's words in the user's language.
//!
//! The English text is the key: `tr!("Join comms")` shows the translation when there is one and
//! the English otherwise, so a missing entry is never a blank. Text with values in it goes through
//! `trf!("{n} bridges", n = count)`: the placeholders are named, so a translation can put them in
//! whatever order its language wants.
//!
//! Translations live in `i18n/<code>.json`, one object from English to the translation, built into
//! the binary. An empty translation counts as missing. Proper nouns (systems, alliances, pilots,
//! ships) are data, not marked text, and are never translated here.

use std::collections::HashMap;
use std::sync::RwLock;

/// The languages offered: code, and the name in that language.
pub const LANGUAGES: [(&str, &str); 6] = [("en", "English"), ("de", "Deutsch"), ("es", "Español"), ("fr", "Français"), ("ru", "Русский"), ("zh", "中文")];

fn file(code: &str) -> Option<&'static str> {
    Some(match code {
        "de" => include_str!("../i18n/de.json"),
        "es" => include_str!("../i18n/es.json"),
        "fr" => include_str!("../i18n/fr.json"),
        "ru" => include_str!("../i18n/ru.json"),
        "zh" => include_str!("../i18n/zh.json"),
        _ => return None,
    })
}

/// The table in force: English text to translation, leaked so lookups hand out `&'static str`.
/// A language change leaks one more table, which happens a handful of times in a run at most.
static TABLE: RwLock<Option<(&'static str, &'static HashMap<String, &'static str>)>> = RwLock::new(None);

/// The language in force, "en" when none was set.
pub fn language() -> &'static str {
    TABLE.read().unwrap_or_else(|e| e.into_inner()).map_or("en", |(c, _)| c)
}

/// Switches the language. "auto" (or empty) takes the system's; anything unknown falls back to
/// English.
pub fn set_language(code: &str) {
    let code = if code.is_empty() || code == "auto" { system_language() } else { code.to_owned() };
    let Some((code, _)) = LANGUAGES.iter().find(|(c, _)| *c == code) else {
        *TABLE.write().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    };
    if language() == *code {
        return;
    }
    let map: HashMap<String, String> = file(code).and_then(|j| parse(j)).unwrap_or_default();
    let leaked: HashMap<String, &'static str> = map.into_iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k, &*Box::leak(v.into_boxed_str()))).collect();
    *TABLE.write().unwrap_or_else(|e| e.into_inner()) = Some((code, Box::leak(Box::new(leaked))));
}

/// The two-letter language the system is set to, as far as it says.
pub fn system_language() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            let code: String = v.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_lowercase();
            if code.len() == 2 && code != "c" {
                return code;
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(c) = windows_language() {
            return c;
        }
    }
    "en".into()
}

#[cfg(target_os = "windows")]
fn windows_language() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    (n > 1).then(|| String::from_utf16_lossy(&buf[..2]).to_lowercase())
}

/// A flat JSON object of strings, read by hand: the files are simple and this crate also builds
/// for the browser, where a JSON dependency only for this would be weight.
pub fn parse(json: &str) -> Option<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut it = json.char_indices().peekable();
    let read_str = |it: &mut std::iter::Peekable<std::str::CharIndices>| -> Option<String> {
        let mut s = String::new();
        loop {
            let (_, c) = it.next()?;
            match c {
                '"' => return Some(s),
                '\\' => {
                    let (_, e) = it.next()?;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        'u' => {
                            let hex: String = (0..4).filter_map(|_| it.next().map(|(_, h)| h)).collect();
                            let mut cp = u32::from_str_radix(&hex, 16).ok()?;
                            // A surrogate pair: the second half follows as its own escape.
                            if (0xD800..0xDC00).contains(&cp) {
                                it.next();
                                it.next();
                                let lo: String = (0..4).filter_map(|_| it.next().map(|(_, h)| h)).collect();
                                let lo = u32::from_str_radix(&lo, 16).ok()?;
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                            }
                            s.push(char::from_u32(cp)?);
                        }
                        other => s.push(other),
                    }
                }
                c => s.push(c),
            }
        }
    };
    let mut key: Option<String> = None;
    while let Some((_, c)) = it.next() {
        if c == '"' {
            let s = read_str(&mut it)?;
            match key.take() {
                None => key = Some(s),
                Some(k) => {
                    out.insert(k, s);
                }
            }
        }
    }
    Some(out)
}

/// The translation of `english`, or `english` itself.
pub fn t(english: &'static str) -> &'static str {
    match *TABLE.read().unwrap_or_else(|e| e.into_inner()) {
        Some((_, map)) => map.get(english).copied().unwrap_or(english),
        None => english,
    }
}

/// The same for text that is not a literal, such as a label a type returns: translated when the
/// table has it, else as given.
pub fn t_dyn(english: &str) -> String {
    match *TABLE.read().unwrap_or_else(|e| e.into_inner()) {
        Some((_, map)) => map.get(english).map_or_else(|| english.to_owned(), |s| (*s).to_owned()),
        None => english.to_owned(),
    }
}

/// `template` with each `{name}` replaced by its value.
pub fn fill(template: &str, args: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open].replace("}}", "}"));
        let after = &rest[open + 1..];
        // "{{" is a literal brace, as in format!.
        if let Some(stripped) = after.strip_prefix('{') {
            out.push('{');
            rest = stripped;
            continue;
        }
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match args.iter().find(|(n, _)| *n == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(&rest.replace("}}", "}"));
    out
}

/// Shows a label that stays English elsewhere (the assistant, the web view) in the user's
/// language: `kind.label().tr()`. The sync test collects the literals of `label()` and `title()`.
pub trait Tr {
    type Out;
    fn tr(self) -> Self::Out;
}

impl Tr for &'static str {
    type Out = &'static str;
    fn tr(self) -> &'static str {
        t(self)
    }
}

impl Tr for String {
    type Out = String;
    fn tr(self) -> String {
        t_dyn(&self)
    }
}

/// Marked text: its translation, or the English.
#[macro_export]
macro_rules! tr {
    ($s:literal) => {
        $crate::i18n::t($s)
    };
}

/// Marked text with values: `trf!("{n} jumps to {sys}", n = jumps, sys = name)`.
#[macro_export]
macro_rules! trf {
    ($s:literal $(, $name:ident = $val:expr)* $(,)?) => {
        $crate::i18n::fill($crate::i18n::t($s), &[$((stringify!($name), ($val).to_string())),*])
    };
}

/// Marks text for translation where it has to stay a plain `&'static str` (a constant, a label a
/// type returns): show it through [`t_dyn`].
#[macro_export]
macro_rules! tr_noop {
    ($s:literal) => {
        $s
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_text_falls_back_and_values_fill_in_any_order() {
        assert_eq!(t("Never translated"), "Never translated");
        assert_eq!(fill("{n} bridges to {sys}", &[("sys", "Jita".into()), ("n", "3".into())]), "3 bridges to Jita");
        assert_eq!(fill("{sys}: {n}", &[("sys", "Jita".into()), ("n", "3".into())]), "Jita: 3", "a translation may reorder");
        assert_eq!(fill("{{literal}} {x}", &[("x", "1".into())]), "{literal} 1");
        assert_eq!(fill("{unknown} stays", &[]), "{unknown} stays");
    }

    #[test]
    fn the_language_files_parse() {
        for (code, _) in LANGUAGES.iter().skip(1) {
            assert!(parse(file(code).unwrap()).is_some(), "{code}");
        }
        let m = parse(r#"{"Join comms": "Comms beitreten", "Line\nTwo": "Zeileä", "Smile": "😀"}"#).unwrap();
        assert_eq!(m["Join comms"], "Comms beitreten");
        assert_eq!(m["Line\nTwo"], "Zeileä");
        assert_eq!(m["Smile"], "😀");
    }
}
