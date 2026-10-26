//! Reading doctrines out of forum topics saved from the browser: the topic's address and title,
//! each post's author, time and last edit, and the EFT fits written into its text.
//!
//! The forum writes every line of a post as its own `<div>`, a blank line as one holding only a
//! non-breaking space, so blank lines survive and split a fit into its racks and bays the way EFT
//! does.

use crate::dogma::Dogma;
use crate::fit::Fit;
use crate::formats::{eft, place_of, Place};
use crate::ids::TypeId;
use crate::variations::{ModulePredicate, RefitOp, Variation, VariationKind};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub url: Option<String>,
    pub title: String,
    pub posts: Vec<Post>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Post {
    pub id: String,
    pub author: String,
    /// Unix seconds.
    pub posted: Option<i64>,
    pub edited: Option<i64>,
    /// The text line by line; an empty string is a blank line.
    pub lines: Vec<String>,
}

impl Post {
    /// When it last changed.
    pub fn changed(&self) -> Option<i64> {
        self.edited.or(self.posted)
    }
}

/// A fit as the topic writes it, with the prose above it back to the fit before.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub post: String,
    pub context: Vec<String>,
    pub fit: Fit,
    /// Names the data does not know: renamed or removed items.
    pub unknown: Vec<String>,
}

pub fn page(html: &str) -> Page {
    let html = &double_quoted(html);
    let url = attr_after(html, "name=\"identifier-url\"", "content").or_else(|| attr_after(html, "property=\"og:url\"", "content"));
    let title = between(html, "class=\"ipsType_pagetitle\">", "</h1>").map(|t| decode(&strip_tags(t)).trim().to_owned()).unwrap_or_default();
    let mut posts = Vec::new();
    let marker = "<div class=\"post_block";
    let starts: Vec<usize> = html.match_indices(marker).map(|(i, _)| i).collect();
    for (n, &at) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(html.len());
        let block = &html[at..end];
        let id = attr_after(block, "post_block", "id").map(|v| v.trim_start_matches("post_id_").to_owned()).unwrap_or_default();
        let author = between(block, "itemprop=\"name\">", "<").map(|a| decode(a).trim().to_owned()).unwrap_or_default();
        let posted = attr_after(block, "itemprop=\"commentTime\"", "title").and_then(|t| iso_time(&t));
        let edited = between(block, "<strong>Edited by ", ".</strong>").and_then(|e| e.split_once(", ").and_then(|(_, when)| forum_time(when)));
        let text = block.find("itemprop=\"commentText\"").map(|i| {
            let body = &block[i..];
            let body = &body[body.find('>').map_or(0, |g| g + 1)..];
            let stop = ["<p class=\"edit\">", "<div class=\"rep_bar", "<ul class=\"post_controls"].iter().filter_map(|s| body.find(s)).min().unwrap_or(body.len());
            &body[..stop]
        });
        let lines = text.map(lines_of).unwrap_or_default();
        posts.push(Post { id, author, posted, edited, lines });
    }
    Page { url, title, posts }
}

/// The same page with every attribute in double quotes: browsers save a page either way.
fn double_quoted(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    let mut in_tag = false;
    while let Some(c) = chars.next() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            '=' if in_tag && chars.peek() == Some(&'\'') => {
                chars.next();
                out.push_str("=\"");
                for v in chars.by_ref() {
                    match v {
                        '\'' => break,
                        '"' => out.push_str("&quot;"),
                        v => out.push(v),
                    }
                }
                out.push('"');
                continue;
            }
            '=' if in_tag && chars.peek() == Some(&'"') => {
                // Copied whole, so a quote or bracket inside the value does not confuse the rest.
                chars.next();
                out.push_str("=\"");
                for v in chars.by_ref() {
                    out.push(v);
                    if v == '"' {
                        break;
                    }
                }
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}

/// The text of a post, a line per element; a line of only a non-breaking space is blank.
fn lines_of(html: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut rest = html;
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        let raw = decode(cur);
        let text = raw.trim_matches(|c: char| c.is_whitespace());
        if !text.is_empty() {
            out.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
        } else if raw.contains('\u{a0}') && out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
        cur.clear();
    };
    while let Some(open) = rest.find('<') {
        cur.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else { break };
        let tag = &rest[open + 1..open + close];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
        if matches!(name.as_str(), "div" | "p" | "br" | "li" | "h1" | "h2" | "h3" | "h4" | "tr" | "ul" | "ol") {
            flush(&mut cur, &mut out);
        } else if name == "script" || name == "style" {
            let end = format!("</{name}");
            if let Some(e) = rest[open..].to_ascii_lowercase().find(&end) {
                rest = &rest[open + e..];
                continue;
            }
        }
        rest = &rest[open + close + 1..];
    }
    cur.push_str(rest);
    flush(&mut cur, &mut out);
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)?;
    Some(&s[i..i + j])
}

/// The value of `attr` in the tag holding `anchor`.
fn attr_after(s: &str, anchor: &str, attr: &str) -> Option<String> {
    let at = s.find(anchor)?;
    let open = s[..at].rfind('<')?;
    let close = at + s[at..].find('>')?;
    let tag = &s[open..close];
    let key = format!("{attr}=");
    let i = tag.find(&key)? + key.len();
    let quote = tag[i..].chars().next()?;
    let v = &tag[i + 1..];
    Some(decode(&v[..v.find(quote)?]))
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(semi) = tail[..tail.len().min(10)].find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..semi];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2025-06-22T02:42:40+00:00`.
fn iso_time(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.get(..8)?.split(':').map(|p| p.parse::<i64>().ok());
    let (h, min, sec) = (t.next()??, t.next()??, t.next()??);
    let offset = match time.get(8..) {
        Some(z) if z.len() >= 6 && (z.starts_with('+') || z.starts_with('-')) => {
            let sign = if z.starts_with('-') { -1 } else { 1 };
            sign * (z[1..3].parse::<i64>().ok()? * 3600 + z[4..6].parse::<i64>().ok()? * 60)
        }
        _ => 0,
    };
    Some(days_from_civil(y, m, day) * 86_400 + h * 3600 + min * 60 + sec - offset)
}

const MONTHS: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];

/// `04 October 2025 - 05:51 PM`, as the forum shows an edit: taken as UTC, close enough for the
/// day it names.
fn forum_time(s: &str) -> Option<i64> {
    let (date, time) = s.split_once(" - ")?;
    let mut parts = date.split_whitespace();
    let day: i64 = parts.next()?.parse().ok()?;
    let month_name = parts.next()?;
    let month = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(month_name))? as i64 + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    let (hm, ampm) = time.trim().split_once(' ').unwrap_or((time.trim(), ""));
    let (h, m) = hm.split_once(':')?;
    let (mut h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    match ampm.to_ascii_uppercase().as_str() {
        "PM" if h < 12 => h += 12,
        "AM" if h == 12 => h = 0,
        _ => {}
    }
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + m * 60)
}

/// The hull a `[Hull, name]` line opens a fit of.
fn header_ship(dogma: &Dogma, line: &str) -> Option<TypeId> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let ship = inner.split_once(',').map_or(inner, |(s, _)| s).trim();
    dogma.type_named(ship).filter(|t| place_of(dogma, t) == Place::Ship).map(|t| t.id)
}

/// The item a fit line names, without its charge, count or state.
fn item_name(line: &str) -> &str {
    let mut s = line.trim();
    if let Some(cut) = s.to_ascii_lowercase().rfind("/offline") {
        s = s[..cut].trim_end();
    }
    if let Some(x) = s.rfind(" x") {
        if s[x + 2..].chars().all(|c| c.is_ascii_digit()) && x + 2 < s.len() {
            s = s[..x].trim_end();
        }
    }
    s.split_once(',').map_or(s, |(n, _)| n).trim()
}

fn counted(line: &str) -> bool {
    line.trim().rsplit_once(" x").is_some_and(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// What a line inside a fit is: part of it, or the prose after it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Blank,
    Empty,
    Item,
    /// Looks like an item the data does not know.
    Unknown,
    Prose,
}

fn kind_of(dogma: &Dogma, line: &str) -> Kind {
    let l = line.trim();
    if l.is_empty() {
        return Kind::Blank;
    }
    if l.starts_with("[Empty ") && l.ends_with(']') {
        return Kind::Empty;
    }
    if l.starts_with('[') {
        return Kind::Prose;
    }
    match dogma.type_named(item_name(l)) {
        // A hull named on a line of its own heads the next part of the topic.
        Some(t) if place_of(dogma, t) == Place::Ship && !counted(l) => Kind::Prose,
        Some(_) => Kind::Item,
        None if counted(l) || l.contains(" II") || l.ends_with(" I") => Kind::Unknown,
        None => Kind::Prose,
    }
}

/// The fits written in these lines, each with the prose since the one before. A spare module
/// after the drones and cargo begin is cargo, `xN` or not.
pub fn fits_in(dogma: &Dogma, post: &str, lines: &[String]) -> Vec<Found> {
    let mut out = Vec::new();
    let mut context: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let Some(_) = header_ship(dogma, line) else {
            if !line.is_empty() {
                context.push(line.clone());
            }
            i += 1;
            continue;
        };
        let mut body: Vec<String> = Vec::new();
        let mut in_bays = false;
        let mut j = i + 1;
        while j < lines.len() {
            let l = &lines[j];
            match kind_of(dogma, l) {
                Kind::Blank => body.push(String::new()),
                Kind::Empty => body.push(l.clone()),
                Kind::Item | Kind::Unknown => {
                    let is_module = dogma.type_named(item_name(l)).is_some_and(|t| matches!(place_of(dogma, t), Place::Module(_)));
                    if counted(l) {
                        in_bays = true;
                        body.push(l.clone());
                    } else if in_bays && is_module {
                        body.push(format!("{} x1", l.trim()));
                    } else {
                        if !is_module && dogma.type_named(item_name(l)).is_some() {
                            in_bays = true;
                        }
                        body.push(l.clone());
                    }
                }
                Kind::Prose => {
                    // An unknown line followed by items is an item the data lost, not prose.
                    let next_items = lines[j + 1..].iter().take(3).any(|n| matches!(kind_of(dogma, n), Kind::Item));
                    if next_items && !l.contains(" - ") && l.len() < 60 && header_ship(dogma, l).is_none() && !l.ends_with(':') {
                        body.push(l.clone());
                    } else {
                        break;
                    }
                }
            }
            j += 1;
        }
        let text = std::iter::once(line.clone()).chain(body).collect::<Vec<_>>().join("\n");
        if let Some(imp) = eft::import(dogma, &text) {
            out.push(Found { post: post.to_owned(), context: std::mem::take(&mut context), fit: imp.fit, unknown: imp.unknown });
        }
        i = j;
    }
    out
}

/// Drones as a pilot launches them: five at most, within the hull's bandwidth, the first stacks
/// first. A topic lists the whole bay, and counting all of it as out would fly hundreds.
pub fn launch_drones(dogma: &Dogma, fit: &mut Fit) {
    use crate::dogma::attr;
    let mut bandwidth = dogma.type_(fit.ship).and_then(|t| t.attr(attr::DRONE_BANDWIDTH)).map_or(0.0, f64::from);
    let mut left = 5u32;
    for d in &mut fit.drones {
        let each = dogma.type_(d.type_id).and_then(|t| t.attr(attr::DRONE_BANDWIDTH_USED)).map_or(0.0, f64::from);
        let fits = if each > 0.0 { (bandwidth / each).floor() as u32 } else { left };
        let out = d.count.min(left).min(fits);
        left -= out;
        bandwidth -= f64::from(out) * each;
        d.active = out > 0;
        d.launched = (out > 0 && out < d.count).then_some(out);
    }
}

const PROPULSION_GROUP: crate::ids::GroupId = 46;

fn is_effect(dogma: &Dogma, t: TypeId, name: &str) -> bool {
    dogma.type_(t).is_some_and(|d| d.effects.iter().filter_map(|(id, _)| dogma.effect(*id)).any(|e| e.name == name))
}

/// The propulsion refit its cargo carries: an afterburner beside a fitted microwarpdrive or the
/// other way round, as a variation keyed so a whole fleet swaps together. Returns the cargo
/// modules left over, which stay spares.
pub fn prop_refit(dogma: &Dogma, fit: &mut Fit) -> Vec<TypeId> {
    let fitted: Vec<TypeId> = fit.modules.iter().filter(|m| dogma.type_(m.type_id).is_some_and(|t| t.group == PROPULSION_GROUP)).map(|m| m.type_id).collect();
    let mut left = Vec::new();
    let mut made = dual_prop(dogma, fit);
    for c in fit.cargo.clone() {
        let Some(t) = dogma.type_(c.type_id) else { continue };
        if !matches!(place_of(dogma, t), Place::Module(_)) {
            continue;
        }
        let swap = !made && t.group == PROPULSION_GROUP && !fitted.is_empty() && !fitted.contains(&c.type_id);
        let ab = is_effect(dogma, c.type_id, "moduleBonusAfterburner");
        let mwd = is_effect(dogma, c.type_id, "moduleBonusMicrowarpdrive");
        let differs = fitted.iter().any(|f| is_effect(dogma, *f, "moduleBonusAfterburner") != ab || is_effect(dogma, *f, "moduleBonusMicrowarpdrive") != mwd);
        if swap && differs && (ab || mwd) {
            let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
            let (name, key) = if ab { ("Afterburner", "prop:ab") } else { ("Microwarpdrive", "prop:mwd") };
            fit.variations.push(Variation {
                id,
                name: name.into(),
                kind: VariationKind::Refit,
                refit_key: Some(key.into()),
                ops: vec![RefitOp::ReplaceWhere { matches: ModulePredicate { group: Some(PROPULSION_GROUP), ..Default::default() }, with: c.type_id }],
            });
            made = true;
        } else {
            left.push(c.type_id);
        }
    }
    left
}

/// Both an afterburner and a microwarpdrive fitted: one runs at a time, so the base runs the
/// microwarpdrive and a keyed variation the afterburner.
fn dual_prop(dogma: &Dogma, fit: &mut Fit) -> bool {
    use crate::fit::State;
    let pos = |name: &str| fit.modules.iter().find(|m| is_effect(dogma, m.type_id, name)).map(|m| (m.slot, m.index));
    let (Some(mwd), Some(ab)) = (pos("moduleBonusMicrowarpdrive"), pos("moduleBonusAfterburner")) else { return false };
    if let Some(m) = fit.module_mut(ab.0, ab.1) {
        m.state = State::Online;
    }
    let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
    fit.variations.push(Variation {
        id,
        name: "Afterburner".into(),
        kind: VariationKind::Refit,
        refit_key: Some("prop:ab".into()),
        ops: vec![RefitOp::SetState { slot: mwd.0, index: mwd.1, state: State::Online }, RefitOp::SetState { slot: ab.0, index: ab.1, state: State::Active }],
    });
    true
}

/// Fighters as a carrier launches them: no more squadrons than tubes, nor of a class than its
/// slots, the first ones written first; the rest wait in the hangar.
pub fn launch_fighters(dogma: &Dogma, fit: &mut Fit) {
    let named = |t: TypeId, n: &str| dogma.attr_named(n).and_then(|a| dogma.type_(t).and_then(|d| d.attr(a.id))).filter(|v| *v > 0.0).map(|v| v as u32);
    // A stack written bigger than a squadron holds is that many full squadrons and the rest.
    let mut squads = Vec::new();
    for f in std::mem::take(&mut fit.fighters) {
        let max = named(f.type_id, "fighterSquadronMaxSize").unwrap_or(f.count.max(1));
        let mut left = f.count;
        while left > 0 {
            let n = left.min(max);
            squads.push(crate::fit::FighterSquad { count: n, ..f.clone() });
            left -= n;
        }
    }
    fit.fighters = squads;
    let mut tubes = named(fit.ship, "fighterTubes").unwrap_or(0);
    let mut slots = [("fighterSquadronIsLight", named(fit.ship, "fighterLightSlots").unwrap_or(0)), ("fighterSquadronIsSupport", named(fit.ship, "fighterSupportSlots").unwrap_or(0)), ("fighterSquadronIsHeavy", named(fit.ship, "fighterHeavySlots").unwrap_or(0))];
    for f in &mut fit.fighters {
        let class = slots.iter_mut().find(|(is, _)| named(f.type_id, is).is_some());
        f.active = match class {
            Some((_, left)) if *left > 0 && tubes > 0 => {
                *left -= 1;
                tubes -= 1;
                true
            }
            _ => false,
        };
    }
}

const WARP_SCRAMBLER_GROUP: crate::ids::GroupId = 52;

/// A spare warp scrambler beside a fitted disruptor, or the other way round: a keyed variation
/// swapping one for the other. Returns the spares it did not use.
pub fn point_refit(dogma: &Dogma, fit: &mut Fit, spares: Vec<TypeId>) -> Vec<TypeId> {
    let scram = |t: TypeId| dogma.type_(t).is_some_and(|d| d.name.contains("Scrambler"));
    let in_group = |t: TypeId| dogma.type_(t).is_some_and(|d| d.group == WARP_SCRAMBLER_GROUP);
    let mut left = Vec::new();
    let mut made = false;
    for t in spares {
        let fitted = fit.modules.iter().find(|m| in_group(m.type_id) && scram(m.type_id) != scram(t)).map(|m| (m.slot, m.index));
        match fitted {
            Some((slot, index)) if !made && in_group(t) => {
                let (name, key) = if scram(t) { ("Warp scrambler", "point:scram") } else { ("Warp disruptor", "point:disruptor") };
                let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
                fit.variations.push(Variation { id, name: name.into(), kind: VariationKind::Refit, refit_key: Some(key.into()), ops: vec![RefitOp::Replace { slot, index, with: Some(t) }] });
                made = true;
            }
            _ => left.push(t),
        }
    }
    left
}

/// A refit of one fitted module for a cargo spare, named and keyed by hand.
pub fn module_refit(fit: &mut Fit, name: &str, key: Option<&str>, replace: TypeId, with: TypeId, all: bool) -> bool {
    let positions: Vec<(crate::fit::Slot, u8)> = fit.modules.iter().filter(|m| m.type_id == replace).map(|m| (m.slot, m.index)).collect();
    if positions.is_empty() {
        return false;
    }
    let ops: Vec<RefitOp> = if all {
        vec![RefitOp::ReplaceWhere { matches: ModulePredicate { type_id: Some(replace), ..Default::default() }, with }]
    } else {
        let (slot, index) = positions[0];
        vec![RefitOp::Replace { slot, index, with: Some(with) }]
    };
    let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
    fit.variations.push(Variation { id, name: name.into(), kind: VariationKind::Refit, refit_key: key.map(str::to_owned), ops });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_as_the_forum_writes_them() {
        assert_eq!(iso_time("1970-01-02T00:00:00+00:00"), Some(86_400));
        assert_eq!(iso_time("2025-06-22T02:42:40+00:00"), Some(1_750_560_160));
        assert_eq!(iso_time("2025-06-22T03:42:40+01:00"), Some(1_750_560_160));
        assert_eq!(forum_time("22 June 2025 - 02:42 AM"), Some(1_750_560_120));
        assert_eq!(forum_time("04 October 2025 - 05:51 PM"), Some(days_from_civil(2025, 10, 4) * 86_400 + 17 * 3600 + 51 * 60));
        assert_eq!(forum_time("01 May 2024 - 12:05 AM"), Some(days_from_civil(2024, 5, 1) * 86_400 + 5 * 60));
        assert_eq!(forum_time("not a date"), None);
    }

    const PAGE: &str = r#"<html><head><meta name="identifier-url" content="https://forum.example/topic/1-active-strat-rifters/"></head><body>
<h1 class="ipsType_pagetitle">ACTIVE - STRAT - Rifters &amp; Friends</h1>
<div class="post_block hentry clear clearfix  " id="post_id_11">
<span itemprop="name">Some FC</span><abbr class="published" itemprop="commentTime" title="2024-06-22T00:53:51+00:00">22 June</abbr>
<div itemprop="commentText" class="post entry-content "><p>Ship of the line</p><p>Rifter - we want 100 of these</p>
<div class="bbc_spoiler"><div class="bbc_spoiler_content"><div>
<div>[Rifter, Fleet Rifter]</div><div>Damage Control II</div><div>&nbsp;</div><div>Gyrostabilizer II</div><div>&nbsp;</div>
<div>Warrior II x5</div><div>&nbsp;</div><div>EMP S x1000</div><div>Medium Shield Thingy II</div><div>&nbsp;</div></div></div></div>
<p>Logi</p></div>
<p class="edit"><strong>Edited by Some FC, 04 October 2025 - 05:51 PM.</strong></p>
<div class="rep_bar clearfix right" id="rep_post_11"></div></div>
<div class="post_block hentry clear clearfix  " id="post_id_12"><span itemprop="name">Someone Else</span>
<abbr itemprop="commentTime" title="2025-01-01T00:00:00+00:00">x</abbr><div itemprop="commentText" class="post entry-content "><p>nice &lt;3</p></div></div>
</body></html>"#;

    #[test]
    fn a_page_saved_with_single_quotes_reads_the_same() {
        let single = PAGE.replace("\"post_block hentry clear clearfix  \"", "'post_block hentry clear clearfix  '").replace("id=\"post_id_11\"", "id='post_id_11'").replace("itemprop=\"commentText\"", "itemprop='commentText'");
        assert_ne!(single, PAGE);
        assert_eq!(page(&single), page(PAGE));
        assert_eq!(double_quoted("<a title='say \"hi\"' x=\"it's\">it's</a>"), "<a title=\"say &quot;hi&quot;\" x=\"it's\">it's</a>");
    }

    #[test]
    fn a_saved_topic_reads_into_posts_lines_and_times() {
        let p = page(PAGE);
        assert_eq!(p.url.as_deref(), Some("https://forum.example/topic/1-active-strat-rifters/"));
        assert_eq!(p.title, "ACTIVE - STRAT - Rifters & Friends");
        assert_eq!(p.posts.len(), 2);
        let op = &p.posts[0];
        assert_eq!((op.id.as_str(), op.author.as_str()), ("11", "Some FC"));
        assert_eq!(op.posted, iso_time("2024-06-22T00:53:51+00:00"));
        assert_eq!(op.changed(), forum_time("04 October 2025 - 05:51 PM"));
        let i = op.lines.iter().position(|l| l == "[Rifter, Fleet Rifter]").unwrap();
        assert_eq!(&op.lines[i..i + 4], &["[Rifter, Fleet Rifter]", "Damage Control II", "", "Gyrostabilizer II"]);
        assert_eq!(op.lines.last().map(String::as_str), Some("Logi"));
        assert_eq!(p.posts[1].lines, vec!["nice <3".to_owned()]);
        assert_eq!(p.posts[1].changed(), iso_time("2025-01-01T00:00:00+00:00"));
    }

    #[test]
    fn fits_come_out_with_the_prose_above_and_unknown_items_named() {
        let d = crate::test_support::dataset();
        let p = page(PAGE);
        let found = fits_in(&d, "11", &p.posts[0].lines);
        assert_eq!(found.len(), 1);
        let f = &found[0];
        assert_eq!(f.fit.ship, crate::test_support::ids::SHIP);
        assert_eq!(f.fit.name, "Fleet Rifter");
        assert!(f.context.iter().any(|c| c == "Ship of the line"), "{:?}", f.context);
        assert_eq!(f.unknown, vec!["Medium Shield Thingy II".to_owned()]);
        assert_eq!(f.fit.drones.len(), 1);
        assert!(f.fit.modules.iter().any(|m| m.type_id == crate::test_support::ids::DC));
        let mut fit = f.fit.clone();
        launch_drones(&d, &mut fit);
        assert_eq!(fit.drones[0].in_space(), 3, "fifteen of bandwidth holds three five-a-piece drones of the five");
    }
}
