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
    if count_of(s).is_some() {
        if let Some(x) = s.rfind(" x") {
            s = s[..x].trim_end();
        }
    }
    s.split_once(',').map_or(s, |(n, _)| n).trim()
}

/// A line's `xN` count; the forum writes big ones with thousands commas, `x150,000`.
fn count_of(line: &str) -> Option<u32> {
    let (_, n) = line.trim().rsplit_once(" x")?;
    let digits: String = n.chars().filter(|c| *c != ',').collect();
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| digits.parse().ok()).flatten()
}

fn counted(line: &str) -> bool {
    count_of(line).is_some()
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
        // A placeholder for the hull's own: "Racial Isotopes", "Racial Lance".
        None if l.starts_with("Racial ") && l.len() <= 40 => Kind::Item,
        None if (counted(l) || l.contains(" II") || l.ends_with(" I")) && l.len() <= 60 && !l.contains(':') => Kind::Unknown,
        None => Kind::Prose,
    }
}

/// A list of items written apart from any fit: a fit's ammo, implants and refits under its own
/// heading, or the refits every fit of a page carries. `heading` is the prose line above it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loose {
    pub heading: String,
    pub lines: Vec<String>,
}

/// The fits written in these lines, each with the prose since the one before. A spare module
/// after the drones and cargo begin is cargo, `xN` or not.
pub fn fits_in(dogma: &Dogma, post: &str, lines: &[String]) -> Vec<Found> {
    read(dogma, post, lines).0
}

/// The fits in a post with the lists written apart from them merged in: a list joins the fits
/// written before it in the post since the last list, or the next fit when none came before.
/// Lists that reach no fit, in a post without fits, come back on their own.
pub fn read(dogma: &Dogma, post: &str, lines: &[String]) -> (Vec<Found>, Vec<Loose>) {
    let mut out: Vec<Found> = Vec::new();
    let mut lists: Vec<(usize, Loose)> = Vec::new();
    let mut current: Option<Loose> = None;
    let mut context: Vec<String> = Vec::new();
    let flush = |current: &mut Option<Loose>, lists: &mut Vec<(usize, Loose)>, found: usize| {
        if let Some(l) = current.take() {
            lists.push((found, l));
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let Some(_) = header_ship(dogma, line) else {
            let kind = kind_of(dogma, line);
            match kind {
                // An item the data does not know stays in the list it is part of, or starts one
                // when it carries a count.
                Kind::Item | Kind::Unknown if kind == Kind::Item || current.is_some() || counted(line) => {
                    // The nearest short line above: a heading, not a sentence that names a hold.
                    let heading = context.iter().rev().find(|c| !c.eq_ignore_ascii_case("spoiler") && c.len() <= 60 && !c.ends_with('.')).cloned().unwrap_or_default();
                    current.get_or_insert_with(|| Loose { heading, lines: Vec::new() }).lines.push(line.clone());
                }
                Kind::Blank => {}
                _ => {
                    flush(&mut current, &mut lists, out.len());
                    if context.last().is_some_and(|c| c.eq_ignore_ascii_case("Fuel Requirements")) {
                        let fuel = fuel_in(line);
                        if !fuel.is_empty() {
                            lists.push((usize::MAX, Loose { heading: FUEL.into(), lines: fuel }));
                        }
                    }
                    context.push(line.clone());
                }
            }
            i += 1;
            continue;
        };
        flush(&mut current, &mut lists, out.len());
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
                // A heading such as "Implants" ends the fit and what follows is a list of its own;
                // a name the data lost in the middle of a rack does not.
                Kind::Prose => {
                    let next = lines[j + 1..].iter().find(|n| !n.trim().is_empty());
                    let rack_next = next.is_some_and(|n| !counted(n) && dogma.type_named(item_name(n)).is_some_and(|t| matches!(place_of(dogma, t), Place::Module(_))));
                    if rack_next && !in_bays && !l.ends_with(':') && l.len() <= 60 && header_ship(dogma, l).is_none() {
                        body.push(l.clone());
                    } else {
                        break;
                    }
                }
            }
            j += 1;
        }
        let text = std::iter::once(line.clone()).chain(body).collect::<Vec<_>>().join("\n");
        if let Some(mut imp) = eft::import(dogma, &text) {
            one_implant_per_slot(dogma, &mut imp.fit);
            out.push(Found { post: post.to_owned(), context: std::mem::take(&mut context), fit: imp.fit, unknown: imp.unknown });
        }
        i = j;
    }
    flush(&mut current, &mut lists, out.len());
    let mut orphans = Vec::new();
    let mut previous: Option<std::ops::Range<usize>> = None;
    // Fuel goes to every jump-capable fit of the page, so it comes back on its own.
    orphans.extend(lists.iter().filter(|(a, _)| *a == usize::MAX).map(|(_, l)| l.clone()));
    lists.retain(|(a, _)| *a != usize::MAX);
    for (n, (after, list)) in lists.iter().enumerate() {
        let start = if n == 0 { 0 } else { lists[n - 1].0 };
        let targets = if *after > start {
            start..*after
        } else if let Some(p) = previous.clone().filter(|_| n > 0 && *after > 0) {
            // Another list after the same fits: theirs too.
            p
        } else {
            // Written under a fit's heading before it: that fit's only.
            *after..(*after + 1).min(out.len())
        };
        previous = Some(targets.clone());
        if targets.is_empty() {
            orphans.push(list.clone());
            continue;
        }
        for f in &mut out[targets] {
            let unknown = add_items(dogma, &mut f.fit, list);
            f.unknown.extend(unknown);
        }
    }
    (out, orphans)
}

/// The heading of a list of fuel read out of a page's fuel requirements.
pub const FUEL: &str = "Fuel";

/// Fuel named in a sentence, "carry 100,000 of their racial isotopes along with 1500 Strontium
/// Clathrates", as list lines.
pub fn fuel_in(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let number = |w: &str| -> Option<u64> {
        let w = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        let (digits, k) = match w.strip_suffix(['k', 'K']) {
            Some(d) => (d, 1000),
            None => (w, 1),
        };
        let digits: String = digits.chars().filter(|c| *c != ',').collect();
        (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| digits.parse::<u64>().ok().map(|n| n * k)).flatten()
    };
    let before = |i: usize| (i.saturating_sub(4)..i).rev().find_map(|j| number(words[j]));
    let mut out = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let w = w.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_lowercase();
        let name = match w.as_str() {
            "isotopes" | "isotope" => "Racial Isotopes",
            "strontium" => "Strontium Clathrates",
            "heavy" if words.get(i + 1).is_some_and(|n| n.to_ascii_lowercase().starts_with("water")) => "Heavy Water",
            _ => continue,
        };
        if let Some(n) = before(i) {
            if !out.iter().any(|l: &String| l.starts_with(name)) {
                out.push(format!("{name} x{n}"));
            }
        }
    }
    out
}

/// The first implant written for each slot: a list of swaps for a slot names several.
pub fn one_implant_per_slot(dogma: &Dogma, fit: &mut Fit) {
    let slot = |t: TypeId| dogma.type_(t).and_then(|d| d.attr(crate::dogma::attr::IMPLANTNESS)).map(|v| v as u32);
    let mut seen: Vec<u32> = Vec::new();
    fit.implants.retain(|t| match slot(*t) {
        Some(s) if seen.contains(&s) => false,
        Some(s) => {
            seen.push(s);
            true
        }
        None => true,
    });
}

/// The lance of the damage the fitted doomsday deals: `'Holy Destiny' Electromagnetic Lance`
/// beside a `'Judgment' Electromagnetic Doomsday`.
fn racial_lance<'a>(dogma: &'a Dogma, fit: &Fit) -> Option<&'a crate::dogma::TypeDef> {
    let doomsday = fit.modules.iter().filter_map(|m| dogma.type_(m.type_id)).find(|t| t.name.ends_with(" Doomsday"))?;
    let word = ["Electromagnetic", "Thermal", "Kinetic", "Explosive"].into_iter().find(|w| doomsday.name.contains(w))?;
    let suffix = format!(" {word} Lance");
    dogma.types.iter().find(|t| t.published && t.name.ends_with(&suffix) && t.name.starts_with('\''))
}

/// A capital's refit modules in its fleet hangar: its cargo hold is for the charges it fights
/// with. Hulls without a fleet hangar, and subcapitals, keep their cargo as written.
pub fn refits_to_fleet_hangar(dogma: &Dogma, fit: &mut Fit) {
    if crate::fleet::hull_class(dogma, fit.ship) != crate::fleet::HullClass::Capital {
        return;
    }
    let Some(hangar) = dogma.attr_named("fleetHangarCapacity").map(|a| a.id) else { return };
    if !dogma.type_(fit.ship).and_then(|t| t.attr(hangar)).is_some_and(|v| v > 0.0) {
        return;
    }
    for c in fit.cargo.iter_mut().filter(|c| c.hold.is_none()) {
        if dogma.type_(c.type_id).is_some_and(|t| matches!(place_of(dogma, t), Place::Module(_))) {
            c.hold = Some(hangar);
        }
    }
    // The same module written in both places is one stack.
    let mut merged: Vec<crate::fit::CargoItem> = Vec::new();
    for c in std::mem::take(&mut fit.cargo) {
        match merged.iter_mut().find(|m| m.type_id == c.type_id && m.hold == c.hold) {
            Some(m) => m.quantity += c.quantity,
            None => merged.push(c),
        }
    }
    fit.cargo = merged;
}

/// Fuel in the fuel bay as far as it holds, Strontium and Heavy Water first; what is left over
/// goes to the fleet hangar where the hull has one.
pub fn fuel_to_bay(dogma: &Dogma, fit: &mut Fit) {
    use crate::dogma::attr;
    let Some(bay) = dogma.attr_named("specialFuelBayCapacity").map(|a| a.id) else { return };
    let ship = dogma.type_(fit.ship);
    let Some(mut room) = ship.and_then(|t| t.attr(bay)).filter(|v| *v > 0.0).map(f64::from) else { return };
    let hangar = dogma.attr_named("fleetHangarCapacity").map(|a| a.id).filter(|h| ship.and_then(|t| t.attr(*h)).is_some_and(|v| v > 0.0));
    let is_fuel = |t: TypeId| dogma.type_(t).is_some_and(|d| d.group == crate::ids::group::ICE_PRODUCT);
    let isotopes = |t: TypeId| dogma.type_(t).is_some_and(|d| d.name.ends_with("Isotopes"));
    let mut fuel: Vec<crate::fit::CargoItem> = Vec::new();
    fit.cargo.retain(|c| {
        let take = c.hold.is_none() && is_fuel(c.type_id);
        if take {
            fuel.push(c.clone());
        }
        !take
    });
    fuel.sort_by_key(|c| isotopes(c.type_id));
    for c in fuel {
        let each = dogma.type_(c.type_id).and_then(|t| t.attr(attr::VOLUME)).map_or(0.0, f64::from);
        let fits = if each > 0.0 { ((room / each).floor() as u32).min(c.quantity) } else { c.quantity };
        room -= f64::from(fits) * each;
        if fits > 0 {
            fit.cargo.push(crate::fit::CargoItem { quantity: fits, hold: Some(bay), ..c.clone() });
        }
        if c.quantity > fits {
            fit.cargo.push(crate::fit::CargoItem { quantity: c.quantity - fits, hold: hangar, ..c });
        }
    }
}

/// A list's items into the fit: modules, charges and anything counted go into the cargo (the
/// fleet hangar when the list's heading names it and the hull has one), drones and fighters into
/// their bays, an implant into a slot still free. Returns the names the data does not know.
pub fn add_items(dogma: &Dogma, fit: &mut Fit, list: &Loose) -> Vec<String> {
    let mut unknown = Vec::new();
    let jump_fuel = dogma.attr_named("jumpDriveConsumptionType").and_then(|a| dogma.type_(fit.ship).and_then(|s| s.attr(a.id)));
    if list.heading == FUEL && jump_fuel.is_none() {
        return unknown;
    }
    let fleet_hangar = list.heading.to_lowercase().contains("fleet hangar");
    let hold = fleet_hangar.then(|| dogma.attr_named("fleetHangarCapacity").map(|a| a.id)).flatten().filter(|a| dogma.type_(fit.ship).and_then(|t| t.attr(*a)).is_some_and(|v| v > 0.0));
    for line in &list.lines {
        let name = item_name(line);
        let quantity = count_of(line).unwrap_or(1).max(1);
        // "Racial Isotopes" stands for whichever isotope the hull's jump drive burns.
        let fuel = name.eq_ignore_ascii_case("Racial Isotopes").then(|| dogma.attr_named("jumpDriveConsumptionType").and_then(|a| dogma.type_(fit.ship).and_then(|s| s.attr(a.id)))).flatten().and_then(|id| dogma.type_(id as TypeId));
        // "Racial Lance" is the lance of the titan's doomsday damage, unless its own pack has one.
        let lance = if name.eq_ignore_ascii_case("Racial Lance") {
            if fit.cargo.iter().any(|c| dogma.type_(c.type_id).is_some_and(|d| d.name.ends_with(" Lance"))) {
                continue;
            }
            racial_lance(dogma, fit)
        } else {
            None
        };
        // Written without its tier, "Cap Recharger": the Tech II one, else the Tech I.
        let tiered = || dogma.type_named(&format!("{name} II")).or_else(|| dogma.type_named(&format!("{name} I")));
        let Some(t) = fuel.or(lance).or_else(|| dogma.type_named(name)).or_else(tiered) else {
            unknown.push(name.to_owned());
            continue;
        };
        match place_of(dogma, t) {
            Place::Drone | Place::Fighter => {
                crate::formats::add_item(dogma, fit, t.id, quantity);
            }
            Place::Implant => {
                fit.implants.push(t.id);
                one_implant_per_slot(dogma, fit);
            }
            Place::Booster if !counted(line) && !fit.boosters.iter().any(|b| b.type_id == t.id) => {
                crate::formats::add_item(dogma, fit, t.id, 1);
            }
            Place::Ship => {}
            // A page names its fuel in a sentence and again in a list: once is what it means.
            _ if t.group == crate::ids::group::ICE_PRODUCT && fit.cargo.iter().any(|c| c.type_id == t.id) => {}
            _ => match fit.cargo.iter_mut().find(|c| c.type_id == t.id && c.hold == hold) {
                Some(c) => c.quantity += quantity,
                None => fit.cargo.push(crate::fit::CargoItem { type_id: t.id, quantity, hold }),
            },
        }
    }
    unknown
}

/// Weapons the topic lists bare loaded with the charge the fit carries most of that they take:
/// the EFT block leaves the ammo to the cargo list, and an empty gun does no damage. Scripts are
/// a choice and stay out; bursts get their charges with the boosts.
pub fn load_charges(dogma: &Dogma, fit: &mut Fit) {
    let is_script = |c: TypeId| dogma.type_(c).and_then(|t| dogma.group(t.group)).is_some_and(|g| g.name.contains("Script"));
    let is_burst = |t: TypeId| is_effect(dogma, t, "moduleBonusWarfareLinkShield") || is_effect(dogma, t, "moduleBonusWarfareLinkArmor") || is_effect(dogma, t, "moduleBonusWarfareLinkSkirmish") || is_effect(dogma, t, "moduleBonusWarfareLinkInfo") || is_effect(dogma, t, "moduleBonusWarfareLinkMining");
    let cargo = fit.cargo.clone();
    for m in fit.modules.iter_mut().filter(|m| m.charge.is_none() && !is_burst(m.type_id)) {
        let best = cargo.iter().filter(|c| !is_script(c.type_id) && dogma.takes_charge(m.type_id, c.type_id)).max_by_key(|c| c.quantity);
        if let Some(c) = best {
            m.charge = Some(c.type_id);
        }
    }
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

fn prop_kind(dogma: &Dogma, t: TypeId) -> Option<(&'static str, &'static str)> {
    if is_effect(dogma, t, "moduleBonusMicrowarpdrive") {
        Some(("Microwarpdrive", "prop:mwd"))
    } else if is_effect(dogma, t, "moduleBonusAfterburner") {
        Some(("Afterburner", "prop:ab"))
    } else {
        None
    }
}

/// A module whose job is to make room on the fit: more powergrid or CPU.
fn is_fitting_module(dogma: &Dogma, t: TypeId) -> bool {
    dogma.type_(t).is_some_and(|d| ["Reactor Control Unit", "Co-Processor", "Micro Auxiliary Power Core", "Power Diagnostic System"].iter().any(|n| d.name.contains(n)))
}

/// What the refits from a fit's cargo came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refits {
    /// Cargo modules no refit used: spares.
    pub left: Vec<TypeId>,
    /// Refits that would not fit, by name.
    pub unfit: Vec<String>,
}

/// The fit's refits out of its cargo, as variations. A propulsion module of the other kind (an
/// afterburner beside a fitted microwarpdrive, or both fitted, one run at a time) makes two
/// categories: the base, named for its own propulsion, and the other. Each ewar or tackle spare
/// then makes a variation under each category: it takes the place of a fitted module of another
/// kind in its rack (a point or scram only the fitted point or scram, racial jammers every
/// jammer of another race), loading a carried script where it takes one. A variation is kept
/// only where it fits at all skills V, tried against each module it could replace.
pub fn refits(dogma: &Dogma, fit: &mut Fit) -> Refits {
    use crate::fit::{Slot, State};
    let mut out = Refits::default();
    let spares: Vec<TypeId> = fit.cargo.iter().map(|c| c.type_id).filter(|t| dogma.type_(*t).is_some_and(|d| matches!(place_of(dogma, d), Place::Module(_)))).collect();
    let fitted_props: Vec<(Slot, u8, TypeId)> = fit.modules.iter().filter(|m| dogma.type_(m.type_id).is_some_and(|t| t.group == PROPULSION_GROUP)).map(|m| (m.slot, m.index, m.type_id)).collect();
    // The other propulsion: the ops that switch to it, its name and key.
    let mut alt: Option<(Vec<RefitOp>, &'static str, &'static str)> = None;
    let mut base_prop: Option<&'static str> = None;
    let mwd = fitted_props.iter().find(|p| prop_kind(dogma, p.2).is_some_and(|k| k.1 == "prop:mwd"));
    let ab = fitted_props.iter().find(|p| prop_kind(dogma, p.2).is_some_and(|k| k.1 == "prop:ab"));
    let mut used: Vec<TypeId> = Vec::new();
    if let (Some(m), Some(a)) = (mwd, ab) {
        if let Some(x) = fit.module_mut(a.0, a.1) {
            x.state = State::Online;
        }
        alt = Some((vec![RefitOp::SetState { slot: m.0, index: m.1, state: State::Online }, RefitOp::SetState { slot: a.0, index: a.1, state: State::Active }], "Afterburner", "prop:ab"));
        base_prop = Some("Microwarpdrive");
    } else if let Some(&(_, _, own)) = fitted_props.first() {
        let own_kind = prop_kind(dogma, own);
        if let Some(spare) = spares.iter().copied().find(|t| dogma.type_(*t).is_some_and(|d| d.group == PROPULSION_GROUP) && prop_kind(dogma, *t).is_some() && prop_kind(dogma, *t) != own_kind) {
            let (name, key) = prop_kind(dogma, spare).expect("checked");
            alt = Some((vec![RefitOp::ReplaceWhere { matches: ModulePredicate { group: Some(PROPULSION_GROUP), ..Default::default() }, with: spare }], name, key));
            base_prop = own_kind.map(|k| k.0);
            used.push(spare);
        }
    }
    // Fitting modules carried to make room: each may take the place of a fitted power
    // diagnostic system, one more at a time, never a tank or damage module.
    let support: Vec<(TypeId, u32)> = fit.cargo.iter().filter(|c| is_fitting_module(dogma, c.type_id)).map(|c| (c.type_id, c.quantity)).collect();
    let push = |fit: &mut Fit, name: &str, key: String, group: Option<&str>, ops: Vec<RefitOp>| -> bool {
        let id = fit.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
        let touched: Vec<(Slot, u8)> = ops.iter().filter_map(|o| match o {
            RefitOp::Replace { slot, index, .. } | RefitOp::SetState { slot, index, .. } | RefitOp::SetCharge { slot, index, .. } => Some((*slot, *index)),
            _ => None,
        }).collect();
        let diagnostics: Vec<(Slot, u8)> = fit.modules.iter().filter(|m| !touched.contains(&(m.slot, m.index)) && dogma.type_(m.type_id).is_some_and(|t| t.name.starts_with("Power Diagnostic System"))).map(|m| (m.slot, m.index)).collect();
        // Smartbombs a pilot can offline for the room: utility, not what the fit flies for.
        let smartbombs: Vec<(Slot, u8)> = fit.modules.iter().filter(|m| !touched.contains(&(m.slot, m.index)) && m.state != State::Offline && dogma.type_(m.type_id).is_some_and(|t| t.name.contains("Smartbomb"))).map(|m| (m.slot, m.index)).collect();
        let offline = |k: usize| smartbombs[..k].iter().map(|(s, i)| RefitOp::SetState { slot: *s, index: *i, state: State::Offline }).collect::<Vec<_>>();
        let mut fitting: Vec<Vec<RefitOp>> = vec![Vec::new()];
        for (t, n) in &support {
            if dogma.type_(*t).is_some_and(|d| d.name.starts_with("Power Diagnostic System")) {
                continue;
            }
            for k in 1..=(*n as usize).min(diagnostics.len()) {
                fitting.push(diagnostics[..k].iter().map(|(s, i)| RefitOp::Replace { slot: *s, index: *i, with: Some(*t) }).collect());
            }
        }
        // Fewest changes first: the carried fitting modules, then smartbombs offline with them.
        let mut attempts: Vec<Vec<RefitOp>> = Vec::new();
        for k in 0..=smartbombs.len() {
            for f in &fitting {
                attempts.push(ops.iter().cloned().chain(f.iter().cloned()).chain(offline(k)).collect());
            }
        }
        for attempt in attempts {
            fit.variations.push(Variation { id, name: name.to_owned(), kind: VariationKind::Refit, refit_key: Some(key.clone()), ops: attempt, group: group.map(str::to_owned) });
            if variation_fits(dogma, fit, id) {
                return true;
            }
            fit.variations.pop();
        }
        false
    };
    let alt_ok = match &alt {
        Some((ops, name, key)) => {
            let ok = push(fit, name, (*key).to_owned(), Some(name), ops.clone());
            if !ok {
                out.unfit.push((*name).to_owned());
            }
            ok
        }
        None => false,
    };
    if alt_ok && fit.base_name.is_none() {
        fit.base_name = base_prop.map(str::to_owned);
    }
    // The ewar kinds the cargo offers, each with the fitted modules it could replace.
    let fitted: Vec<(Slot, u8, &'static str)> = fit.modules.iter().filter_map(|m| ewar_class(dogma, m.type_id).map(|c| (m.slot, m.index, c))).collect();
    let mut kinds: Vec<(TypeId, &'static str, Vec<Vec<RefitOp>>)> = Vec::new();
    for &t in &spares {
        let Some(class) = ewar_class(dogma, t) else { continue };
        if kinds.iter().any(|(_, c, _)| *c == class) || fitted.iter().any(|(_, _, c)| *c == class) {
            continue;
        }
        let slot = dogma.slot_of(t);
        let ecm = class.starts_with("ecm:");
        let places: Vec<(Slot, u8)> = fitted.iter().filter(|(s, _, c)| Some(*s) == slot && *c != class && is_tackle(c) == is_tackle(class) && (!ecm || c.starts_with("ecm:"))).map(|(s, i, _)| (*s, *i)).collect();
        if places.is_empty() {
            continue;
        }
        let script = carried_script(dogma, fit, t);
        let op_for = |(s, i): (Slot, u8)| {
            let mut ops = vec![RefitOp::Replace { slot: s, index: i, with: Some(t) }];
            if script.is_some() {
                ops.push(RefitOp::SetCharge { slot: s, index: i, charge: script });
            }
            ops
        };
        // Racial jammers swap the whole set; anything else one module, the first that fits.
        let tries = if ecm { vec![places.iter().flat_map(|p| op_for(*p)).collect()] } else { places.iter().map(|p| op_for(*p)).collect() };
        kinds.push((t, class, tries));
    }
    let base_group = if alt_ok { base_prop } else { None };
    for (t, class, tries) in &kinds {
        let name = ewar_name(class);
        let mut any = false;
        let label = |group: Option<&str>| group.map_or_else(|| name.to_owned(), |g| format!("{g} · {name}"));
        if tries.iter().any(|ops| push(fit, name, (*class).to_owned(), base_group, ops.clone())) {
            any = true;
        } else {
            out.unfit.push(label(base_group));
        }
        if let (true, Some((prop_ops, prop_name, prop_key))) = (alt_ok, &alt) {
            let made = tries.iter().any(|ops| push(fit, name, format!("{prop_key}|{class}"), Some(prop_name), prop_ops.iter().cloned().chain(ops.iter().cloned()).collect()));
            if made {
                any = true;
            } else {
                out.unfit.push(label(Some(prop_name)));
            }
        }
        if any {
            used.push(*t);
        }
    }
    out.left = spares.into_iter().filter(|t| !used.contains(t)).collect();
    out
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

/// What an ewar or tackle module does, by its name: a refit swaps one kind for another.
pub fn ewar_class(dogma: &Dogma, t: TypeId) -> Option<&'static str> {
    let name = dogma.type_(t)?.name.as_str();
    let has = |w: &str| name.contains(w);
    Some(match () {
        _ if has("Warp Scrambler") => "point:scram",
        _ if has("Warp Disruptor") && !has("Field") && !has("Mobile") => "point:disruptor",
        _ if has("Webifier") || has("Stasis Grappler") => "ewar:web",
        _ if has("Sensor Dampener") => "ewar:damp",
        _ if has("Target Painter") => "ewar:paint",
        _ if has("Tracking Disruptor") => "ewar:td",
        _ if has("Guidance Disruptor") => "ewar:gd",
        _ if has("ECM") && !has("Burst") => {
            if has("Ladar") {
                "ecm:ladar"
            } else if has("Magnetometric") {
                "ecm:magnetometric"
            } else if has("Radar") {
                "ecm:radar"
            } else if has("Gravimetric") {
                "ecm:gravimetric"
            } else {
                "ecm:multispectral"
            }
        }
        _ => return None,
    })
}

fn is_tackle(class: &str) -> bool {
    class.starts_with("point:")
}

/// A fit is no worse with the variation on: no more CPU, powergrid, calibration or slots over
/// than the base, at all skills V.
pub fn variation_fits(dogma: &Dogma, fit: &Fit, id: u32) -> bool {
    use crate::warnings::WarningKind as W;
    let over = |f: &Fit| {
        let computed = crate::stats::compute(dogma, f, &crate::skills::SkillSet::all_v(), &Default::default());
        crate::warnings::check(dogma, f, &computed, &[]).into_iter().filter(|w| matches!(w.kind, W::CpuOver { .. } | W::PowerOver { .. } | W::CalibrationOver { .. } | W::SlotsOver { .. } | W::TurretsOver { .. } | W::LaunchersOver { .. })).count()
    };
    let mut base = fit.clone();
    base.active_variations.clear();
    let mut on = base.clone();
    crate::variations::activate(&mut on, id);
    over(&crate::variations::resolve(dogma, &on)) <= over(&base)
}

/// The script a swapped-in module loads: the first carried one it takes.
fn carried_script(dogma: &Dogma, fit: &Fit, module: TypeId) -> Option<TypeId> {
    let scripts = crate::fleet::scripts_of(dogma, module);
    fit.cargo.iter().map(|c| c.type_id).find(|c| scripts.contains(c))
}

/// An ewar or tackle key's name, if it is one.
pub fn ewar_name_of(key: &str) -> Option<&'static str> {
    (key.starts_with("ewar:") || key.starts_with("ecm:") || key.starts_with("point:")).then(|| ewar_name(key))
}

/// A variation's name for an ewar or tackle kind.
pub fn ewar_name(class: &str) -> &'static str {
    match class {
        "point:scram" => "Warp scrambler",
        "point:disruptor" => "Warp disruptor",
        "ewar:web" => "Webifier",
        "ewar:damp" => "Sensor dampener",
        "ewar:paint" => "Target painter",
        "ewar:td" => "Tracking disruptor",
        "ewar:gd" => "Guidance disruptor",
        "ecm:ladar" => "Ladar ECM",
        "ecm:magnetometric" => "Magnetometric ECM",
        "ecm:radar" => "Radar ECM",
        "ecm:gravimetric" => "Gravimetric ECM",
        _ => "Multispectral ECM",
    }
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
    fit.variations.push(Variation { id, name: name.into(), kind: VariationKind::Refit, refit_key: key.map(str::to_owned), ops, group: None });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_written_apart_join_the_fits_they_belong_to() {
        let d = crate::test_support::dataset();
        let lines: Vec<String> = [
            "Implants for the first one",
            "Zor's Custom Navigation Hyper-Link",
            "[Rifter, First]",
            "Damage Control II",
            "Some words between",
            "[Rifter, Second]",
            "Gyrostabilizer II",
            "Ammo and refits for both",
            "Spoiler",
            "EMP S x500",
            "Stasis Webifier II x1",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (fits, orphans) = read(&d, "1", &lines);
        assert!(orphans.is_empty());
        assert_eq!(fits[0].fit.implants, vec![crate::test_support::ids::IMPLANT], "a list above a fit is that fit's");
        assert!(fits[1].fit.implants.is_empty());
        for f in &fits {
            assert!(f.fit.cargo.iter().any(|c| c.type_id == crate::test_support::ids::AMMO && c.quantity == 500));
            assert!(f.fit.cargo.iter().any(|c| c.type_id == crate::test_support::ids::WEB), "a list after fits is all of theirs");
        }
        let (none, orphans) = read(&d, "0", &["Refit modules".to_string(), "Spoiler".to_string(), "Warp Scrambler II x2".to_string()]);
        assert!(none.is_empty());
        assert_eq!(orphans[0].heading, "Refit modules", "the heading skips the forum's spoiler line");
        let hangar: Vec<String> = ["Fleet Hangar", "A long sentence about what the fleet hangar holds, which is not a heading.", "Spoiler", "Racial Lance", "Mobile Thing II x3", "How can I upgrade my damage: Pyro II adds 2% damage", "EMP S x10"].iter().map(|s| s.to_string()).collect();
        let (_, lists) = read(&d, "0", &hangar);
        assert_eq!(lists.len(), 2, "{lists:?}");
        assert_eq!((lists[0].heading.as_str(), lists[0].lines.len()), ("Fleet Hangar", 2), "a placeholder and an unknown item stay in the list");
    }

    #[test]
    fn racial_isotopes_are_the_hulls_own_and_fuel_stays_in_the_cargo() {
        let d = crate::test_support::dataset();
        let ids = crate::test_support::ids::ISOTOPES;
        let mut fit = Fit::new(crate::test_support::ids::SHIP, "Jumper");
        let list = Loose { heading: "Fuel".into(), lines: vec!["Racial Isotopes x150,000".into()] };
        assert!(add_items(&d, &mut fit, &list).is_empty());
        assert_eq!((fit.cargo[0].type_id, fit.cargo[0].quantity), (ids, 150_000));
        let eft = crate::formats::eft::import(&d, "[Rifter, Fuel]\nHelium Isotopes x500\n").unwrap();
        assert!(eft.unknown.is_empty());
        assert_eq!((eft.fit.cargo[0].type_id, eft.fit.cargo[0].quantity), (ids, 500));
    }

    #[test]
    fn a_racial_lance_matches_the_doomsday_unless_the_titans_own_pack_has_one_and_fuel_goes_in_the_bay() {
        use crate::test_support::ids;
        let d = crate::test_support::dataset();
        let mut fit = Fit::new(ids::SHIP, "Titan");
        fit.add_module(ids::DOOMSDAY, crate::fit::Slot::High);
        let common = Loose { heading: "Refit modules".into(), lines: vec!["Racial Lance".into(), "Racial Isotopes x100".into()] };
        assert!(add_items(&d, &mut fit, &common).is_empty());
        assert!(fit.cargo.iter().any(|c| c.type_id == ids::LANCE));
        assert!(add_items(&d, &mut fit, &Loose { heading: String::new(), lines: vec!["Gyrostabilizer x2".into()] }).is_empty(), "a name without its tier reads as Tech II");
        assert!(fit.cargo.iter().any(|c| c.type_id == ids::GYRO && c.quantity == 2));
        fuel_to_bay(&d, &mut fit);
        let fuel = fit.cargo.iter().find(|c| c.type_id == ids::ISOTOPES).unwrap();
        assert_eq!(fuel.hold, Some(ids::FUEL_BAY));
        // A titan's 60,000 m3 bay holds two million isotopes at 0.03; the rest goes to the hangar.
        let mut titan = Fit::new(ids::TITAN, "Titan");
        titan.cargo.push(crate::fit::CargoItem { type_id: ids::ISOTOPES, quantity: 2_500_000, hold: None });
        fuel_to_bay(&d, &mut titan);
        let held: Vec<(u32, Option<crate::ids::AttrId>)> = titan.cargo.iter().map(|c| (c.quantity, c.hold)).collect();
        assert_eq!(held, vec![(2_000_000, Some(ids::FUEL_BAY)), (500_000, Some(ids::FLEET_HANGAR))]);
        assert!(add_items(&d, &mut fit, &Loose { heading: "Fleet Hangar".into(), lines: vec!["Helium Isotopes x50".into()] }).is_empty());
        assert_eq!(fit.cargo.iter().filter(|c| c.type_id == ids::ISOTOPES).map(|c| c.quantity).collect::<Vec<_>>(), vec![100], "fuel named twice counts once");
        let before = fit.cargo.len();
        assert!(add_items(&d, &mut fit, &Loose { heading: String::new(), lines: vec!["Racial Lance".into()] }).is_empty());
        assert_eq!(fit.cargo.len(), before, "a lance already carried is the one meant");
        assert_eq!(fit.cargo.iter().find(|c| c.type_id == ids::LANCE).unwrap().quantity, 1);
    }

    #[test]
    fn a_name_the_data_lost_inside_a_rack_keeps_the_fit_going() {
        let d = crate::test_support::dataset();
        let lines: Vec<String> = ["[Rifter, Odd]", "Damage Control II", "Large 'Vanished' Thing", "Gyrostabilizer II", "Implants", "Zor's Custom Navigation Hyper-Link"].iter().map(|s| s.to_string()).collect();
        let (fits, _) = read(&d, "1", &lines);
        assert_eq!(fits[0].fit.modules.len(), 2, "the rack goes on past the lost name");
        assert_eq!(fits[0].unknown, vec!["Large 'Vanished' Thing".to_owned()]);
        assert_eq!(fits[0].fit.implants.len(), 1, "the implants heading ends the fit and its list joins it");
    }

    #[test]
    fn fuel_reads_out_of_the_requirements_sentence() {
        assert_eq!(fuel_in("All dreadnoughts should carry 100,000 of their racial isotopes along with 1500 Strontium Clathrates. Any fuel used"), vec!["Racial Isotopes x100000", "Strontium Clathrates x1500"]);
        assert_eq!(fuel_in("Your Supercarrier should carry 150,000 Racial Isotopes along with 1,250 Heavy Water."), vec!["Racial Isotopes x150000", "Heavy Water x1250"]);
        assert_eq!(fuel_in("All Carriers should carry 120K Isotopes."), vec!["Racial Isotopes x120000"]);
        assert!(fuel_in("Any fuel used on capital ops is reimbursable").is_empty());
        let d = crate::test_support::dataset();
        let lines: Vec<String> = ["Fuel Requirements", "Carry 1,000 Racial Isotopes please.", "[Rifter, A]", "Damage Control II"].iter().map(|s| s.to_string()).collect();
        let (fits, orphans) = read(&d, "1", &lines);
        assert!(fits[0].fit.cargo.is_empty(), "fuel is the page's, not the next fit's");
        assert_eq!(orphans, vec![Loose { heading: FUEL.into(), lines: vec!["Racial Isotopes x1000".into()] }]);
    }

    #[test]
    fn a_capitals_refits_go_in_its_fleet_hangar_and_its_charges_stay_in_the_cargo() {
        use crate::fit::CargoItem;
        use crate::test_support::ids;
        let d = crate::test_support::dataset();
        let cargo = vec![CargoItem { type_id: ids::WEB, quantity: 2, hold: None }, CargoItem { type_id: ids::AMMO, quantity: 500, hold: None }, CargoItem { type_id: ids::WEB, quantity: 1, hold: Some(ids::FLEET_HANGAR) }];
        let mut titan = Fit::new(ids::TITAN, "Titan");
        titan.cargo = cargo.clone();
        refits_to_fleet_hangar(&d, &mut titan);
        assert_eq!(titan.cargo, vec![CargoItem { type_id: ids::WEB, quantity: 3, hold: Some(ids::FLEET_HANGAR) }, CargoItem { type_id: ids::AMMO, quantity: 500, hold: None }]);
        let mut rifter = Fit::new(ids::SHIP, "Rifter");
        rifter.cargo = cargo.clone();
        refits_to_fleet_hangar(&d, &mut rifter);
        assert_eq!(rifter.cargo, cargo, "a subcapital keeps its cargo as written");
    }

    /// The guns a topic lists bare load the ammo the fit carries most of; a module with nothing
    /// it takes in the cargo stays empty.
    #[test]
    fn bare_weapons_load_the_charge_carried_most() {
        use crate::fit::{CargoItem, Module, Slot};
        use crate::test_support::ids;
        let d = crate::test_support::dataset();
        let mut f = Fit::new(ids::SHIP, "Rifter");
        f.modules.push(Module::new(ids::GUN, Slot::High, 0));
        f.modules.push(Module::new(ids::GUN, Slot::High, 1).with_charge(ids::AMMO));
        f.modules.push(Module::new(ids::LAUNCHER, Slot::High, 2));
        f.cargo = vec![CargoItem { type_id: ids::AMMO, quantity: 100, hold: None }, CargoItem { type_id: ids::AMMO_LONG, quantity: 500, hold: None }];
        load_charges(&d, &mut f);
        assert_eq!(f.modules.iter().map(|m| m.charge).collect::<Vec<_>>(), [Some(ids::AMMO_LONG), Some(ids::AMMO), None]);
    }

    #[test]
    fn counts_read_with_thousands_commas() {
        assert_eq!(count_of("Racial Isotopes x150,000"), Some(150_000));
        assert_eq!(count_of("Warrior II x5"), Some(5));
        assert_eq!(count_of("Warrior II"), None);
        assert_eq!(item_name("Heavy Water x1,250"), "Heavy Water");
        assert_eq!(item_name("Dual Light Beam Laser II, Aurora S"), "Dual Light Beam Laser II");
    }

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
