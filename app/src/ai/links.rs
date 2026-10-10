//! Turns the assistant's text into spans the chat can make clickable.
//!
//! Two sources: links the model writes as `[text](spai:kind/arg)` for things it has an id for from a
//! tool (a killmail, a battle, a fleet, a conversation), and system and ship names found in plain
//! text, matched against the game's own spelling so an ordinary word is not taken for one.

#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    System(i64),
    Ship(i64),
    Pilot(String),
    Kill(i64),
    /// A battle, by any kill in it.
    Battle(i64),
    Fleet(String),
    /// A Jabber conversation, by its address.
    Chat(String),
    Pings,
    /// The wormholes of a system.
    Wormholes(i64),
    Url(String),
    /// A page of the app, by its name.
    Page(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub link: Option<Link>,
}

/// Names the text is matched against. Both answer only an exact, correctly spelled name.
pub trait Names {
    fn system(&self, name: &str) -> Option<i64>;
    fn ship(&self, name: &str) -> Option<i64>;
}

/// The pages a link can open.
pub const PAGES: [&str; 12] = ["overview", "map", "wormholes", "intel", "alerts", "battles", "lookup", "characters", "jabber", "fleet", "rescue", "settings"];

/// Longest system or ship name, in words.
const MAX_WORDS: usize = 4;

pub fn spans(line: &str, names: &dyn Names) -> Vec<Span> {
    let mut out = Vec::new();
    for (i, part) in line.split("**").enumerate() {
        let bold = i % 2 == 1;
        let part = part.replace('`', "");
        let mut rest = part.as_str();
        while let Some(open) = rest.find('[') {
            let Some((label, target, len)) = explicit(&rest[open..]) else {
                plain(&rest[..open + 1], bold, names, &mut out);
                rest = &rest[open + 1..];
                continue;
            };
            plain(&rest[..open], bold, names, &mut out);
            let link = target_link(target, names);
            out.push(Span { text: label.to_owned(), bold, link });
            rest = &rest[open + len..];
        }
        plain(rest, bold, names, &mut out);
    }
    // Neighbouring plain spans merge, so the chat lays out as few pieces as it can.
    let mut merged: Vec<Span> = Vec::new();
    for s in out.into_iter().filter(|s| !s.text.is_empty()) {
        match merged.last_mut() {
            Some(m) if m.link.is_none() && s.link.is_none() && m.bold == s.bold => m.text.push_str(&s.text),
            _ => merged.push(s),
        }
    }
    merged
}

/// `[label](target)` at the start of `s`: the label, the target and the length taken.
fn explicit(s: &str) -> Option<(&str, &str, usize)> {
    let close = s.find("](")?;
    let label = &s[1..close];
    if label.contains('[') || label.contains('\n') {
        return None;
    }
    let end = s[close + 2..].find(')')? + close + 2;
    Some((label, &s[close + 2..end], end + 1))
}

fn target_link(target: &str, names: &dyn Names) -> Option<Link> {
    if target.starts_with("https://") || target.starts_with("http://") {
        return Some(Link::Url(target.to_owned()));
    }
    let t = target.strip_prefix("spai:")?;
    let (kind, arg) = t.split_once('/').unwrap_or((t, ""));
    let arg = arg.trim();
    let num = || arg.parse::<i64>().ok();
    match kind {
        "system" => names.system(arg).or_else(|| num()).map(Link::System),
        "ship" => names.ship(arg).or_else(|| num()).map(Link::Ship),
        "wh" | "wormholes" => names.system(arg).or_else(|| num()).map(Link::Wormholes),
        "pilot" if !arg.is_empty() => Some(Link::Pilot(arg.to_owned())),
        "kill" => num().map(Link::Kill),
        "battle" => num().map(Link::Battle),
        "fleet" if !arg.is_empty() => Some(Link::Fleet(arg.to_owned())),
        "chat" if !arg.is_empty() => Some(Link::Chat(arg.to_owned())),
        "pings" => Some(Link::Pings),
        "page" if PAGES.contains(&arg.to_lowercase().as_str()) => Some(Link::Page(arg.to_lowercase())),
        _ => None,
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '\''
}

/// Plain text, with web addresses and any system or ship names in it made links.
fn plain(text: &str, bold: bool, names: &dyn Names, out: &mut Vec<Span>) {
    let mut rest = text;
    while let Some(at) = ["https://", "http://"].iter().filter_map(|p| rest.find(p)).min() {
        let tail = &rest[at..];
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        // Sentence punctuation after an address is not part of it.
        let url = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '\'', '"']);
        names_in(&rest[..at], bold, names, out);
        out.push(Span { text: url.to_owned(), bold, link: Some(Link::Url(url.to_owned())) });
        rest = &tail[url.len()..];
    }
    names_in(rest, bold, names, out);
}

/// Plain text, with any system or ship names in it made links.
fn names_in(text: &str, bold: bool, names: &dyn Names, out: &mut Vec<Span>) {
    // Word starts and ends, as byte offsets.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (is_word_char(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push((s, text.len()));
    }
    let mut at = 0;
    let mut w = 0;
    while w < words.len() {
        let mut hit = None;
        for n in (1..=MAX_WORDS.min(words.len() - w)).rev() {
            let (s, e) = (words[w].0, words[w + n - 1].1);
            // Only words joined by single spaces make one name.
            if (w..w + n - 1).any(|k| &text[words[k].1..words[k + 1].0] != " ") {
                continue;
            }
            // A trailing hyphen or apostrophe is punctuation, not part of the name.
            let cand = text[s..e].trim_end_matches(['-', '\'']);
            if cand.chars().count() < 3 {
                continue;
            }
            let link = names.system(cand).map(Link::System).or_else(|| names.ship(cand).map(Link::Ship));
            if let Some(l) = link {
                hit = Some((s, s + cand.len(), n, l));
                break;
            }
        }
        match hit {
            Some((s, e, n, l)) => {
                out.push(Span { text: text[at..s].to_owned(), bold, link: None });
                out.push(Span { text: text[s..e].to_owned(), bold, link: Some(l) });
                at = e;
                w += n;
            }
            None => w += 1,
        }
    }
    out.push(Span { text: text[at..].to_owned(), bold, link: None });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl Names for Fake {
        fn system(&self, n: &str) -> Option<i64> {
            match n {
                "1DQ1-A" => Some(1),
                "QX-LIJ" => Some(2),
                "Old Man Star" => Some(3),
                "Jita" => Some(4),
                _ => None,
            }
        }
        fn ship(&self, n: &str) -> Option<i64> {
            match n {
                "Muninn" => Some(12015),
                "Sabre" => Some(22456),
                "Federation Navy Comet" => Some(17841),
                _ => None,
            }
        }
    }

    fn links(line: &str) -> Vec<(String, Option<Link>)> {
        spans(line, &Fake).into_iter().map(|s| (s.text, s.link)).filter(|(_, l)| l.is_some()).collect()
    }

    #[test]
    fn names_in_plain_text_become_links_in_the_games_spelling_only() {
        let l = links("Muninns and a Sabre went from 1DQ1-A to QX-LIJ, then Old Man Star.");
        assert_eq!(
            l,
            vec![
                ("Sabre".into(), Some(Link::Ship(22456))),
                ("1DQ1-A".into(), Some(Link::System(1))),
                ("QX-LIJ".into(), Some(Link::System(2))),
                ("Old Man Star".into(), Some(Link::System(3))),
            ],
            "a plural is not the ship's name"
        );
        assert!(links("a sabre, jita and old man star").is_empty(), "the game's own capitals only");
        assert_eq!(links("a Federation Navy Comet").len(), 1, "the longest name wins");
        assert!(links("1DQ1-ABC").is_empty(), "not inside a longer word");
    }

    #[test]
    fn written_links_bold_and_markdown_survive() {
        let s = spans("**Killed** in [the fight](spai:battle/123), see [this kill](spai:kill/99) and [dotlan](https://evemaps.dotlan.net/system/Jita) or [bad](spai:nope/1) [x]", &Fake);
        assert!(s[0].bold && s[0].text == "Killed" && s[0].link.is_none());
        let l: Vec<_> = s.iter().filter_map(|x| x.link.clone()).collect();
        assert_eq!(l, vec![Link::Battle(123), Link::Kill(99), Link::Url("https://evemaps.dotlan.net/system/Jita".into())]);
        let all: String = s.iter().map(|x| x.text.as_str()).collect();
        assert!(all.ends_with("or bad [x]"), "an unknown target leaves its label, a lone bracket stays: {all}");
        assert_eq!(spans("[pings](spai:pings) in [Jita](spai:wh/Jita)", &Fake).iter().filter_map(|x| x.link.clone()).collect::<Vec<_>>(), vec![Link::Pings, Link::Wormholes(4)]);
        assert_eq!(spans("[the map](spai:page/Map) [x](spai:page/nowhere)", &Fake).iter().filter_map(|x| x.link.clone()).collect::<Vec<_>>(), vec![Link::Page("map".into())]);
        let bare = spans("See https://zkillboard.com/kill/1/. Then Jita.", &Fake);
        let l: Vec<_> = bare.iter().filter_map(|x| x.link.clone()).collect();
        assert_eq!(l, vec![Link::Url("https://zkillboard.com/kill/1/".into()), Link::System(4)], "a bare address is a link, its full stop is not");
    }
}
