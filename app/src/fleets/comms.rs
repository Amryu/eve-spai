//! Where a fleet talks, as links a client can be sent to.
//!
//! Two channels matter: the op channel the fleet itself is in, and the command channel its FC sits
//! in to hear the wider picture. Which command sector that is depends on what kind of fleet it is:
//! a strategic op goes to Alpha, a peacetime one to Bravo.

use super::model::TagItem;

const HOST: &str = "mumble.goonfleet.com";

/// Which command sector a fleet's commander belongs in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sector {
    Alpha,
    Bravo,
}

impl Sector {
    pub fn label(self) -> &'static str {
        match self {
            Sector::Alpha => "Alpha",
            Sector::Bravo => "Bravo",
        }
    }

    fn channel(self) -> &'static str {
        match self {
            Sector::Alpha => "Command Sector Alpha",
            Sector::Bravo => "Command Sector Bravo",
        }
    }
}

/// The sector a fleet's tags put it in. Strategic is the louder claim, so it wins when both are
/// somehow set; anything unlabelled is treated as peacetime rather than pulling an FC into Alpha.
pub fn sector(tags: &[TagItem]) -> Sector {
    let named = |name: &str| tags.iter().any(|t| t.name.trim().eq_ignore_ascii_case(name));
    if named("STRATEGIC") || tags.iter().any(|t| t.is_strategic) {
        Sector::Alpha
    } else {
        Sector::Bravo
    }
}

/// The command channel that goes with an op channel, by name.
///
/// The op channels are not numbered the way their ids are (id 8 is "Op 9"), so this reads the
/// name the API gave rather than counting. Anything that is not an op channel, like capital or
/// standing comms, has no command channel of its own.
pub fn command_channel(op_name: &str) -> Option<String> {
    let n = op_name.trim();
    if n.eq_ignore_ascii_case("o7") {
        return Some("Command 7".to_owned());
    }
    for named in ["HD", "Locust", "SV"] {
        if n.eq_ignore_ascii_case(named) {
            return Some(format!("{named} Command"));
        }
    }
    if n.eq_ignore_ascii_case("Scouts") {
        return Some("Scouts".to_owned());
    }
    let rest = n.get(..3).filter(|p| p.eq_ignore_ascii_case("op "))?;
    let _ = rest;
    let k: i32 = n[3..].trim().parse().ok()?;
    (1..=12).contains(&k).then(|| format!("Command {k}"))
}

/// The command channel for an op channel, in the right sector.
///
/// Alpha spells its last four "Command 9 - SC" and Bravo does not, which is the sort of thing only
/// a link that fails to open tells you about.
pub fn command_url(sector: Sector, op_name: &str) -> Option<String> {
    let chan = command_channel(op_name)?;
    let chan = match (sector, chan.strip_prefix("Command ").and_then(|k| k.parse::<i32>().ok())) {
        (Sector::Alpha, Some(k)) if k >= 9 => format!("Command {k} - SC"),
        _ => chan,
    };
    Some(link(&[sector.channel(), &chan]))
}

/// The gnf.lt short link for a comms channel, by the name the dashboard gives it.
///
/// Joining goes through these rather than a path built from the name: the real channels carry
/// vanity names ("OP 11 - bubbles on a blue fleet") that change whenever someone renames them, so
/// a built path lands in the parent channel. The short link is kept pointing at the right one.
///
/// Op channels come from what the app has learned out of fleet pings, then the built-in table.
/// The rest, like HD or capital comms, from the built-in table only.
pub fn short_link(
    channel_name: &str,
    learned_ops: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let name = channel_name.trim();
    if let Some(n) = op_number(name) {
        return learned_ops
            .get(&format!("op{n}"))
            .cloned()
            .or_else(|| builtin_op_link(n).map(str::to_owned));
    }
    builtin_named_link(name).map(str::to_owned)
}

/// gnf.lt links for the channels no ping names, as the dashboard's own ping preview renders them.
/// Kept as the fallback behind the direct `mumble://` links below.
pub fn builtin_named_link(channel_name: &str) -> Option<&'static str> {
    Some(match channel_name.trim().to_lowercase().as_str() {
        "hd" => "https://gnf.lt/NFmGwzN.html",
        "capital comms" => "https://gnf.lt/rE6SwtF.html",
        "hellcamp comms" => "https://gnf.lt/RT4ePYG.html",
        "standing comms" => "https://gnf.lt/5X9X4XG.html",
        _ => return None,
    })
}

/// Direct `mumble://` links for those channels. Tried first: Mumble opens them with no page fetch
/// in between. A rename breaks one until it is updated here, which is what the gnf.lt link behind
/// it is for.
pub fn builtin_named_mumble(channel_name: &str) -> Option<&'static str> {
    Some(match channel_name.trim().to_lowercase().as_str() {
        "hd" => "mumble://mumble.goonfleet.com/Ops/Op%20Channels/Home%20Defense%20-%20How%20did%20you%20NOT%20know%20there%20was%20a%20strat%20op?title=Goonfleet&version=1.2.0",
        "capital comms" => "mumble://mumble.goonfleet.com/Ops/Op%20Channels/Capital%20Ops%20-%20just%20gate?title=Goonfleet&version=1.2.0",
        "standing comms" => "mumble://mumble.goonfleet.com/Ops/Op%20Channels/Standing%20Fleet?title=Goonfleet&version=1.2.0",
        _ => return None,
    })
}

/// Both ways into a channel, `mumble://` first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Links {
    pub mumble: Option<String>,
    pub short: Option<String>,
}

/// Every way this app knows into a channel.
///
/// `resolved` is what earlier short links turned out to point at, so a `mumble://` link is on
/// hand before the first fetch of this run.
pub fn links(
    channel_name: &str,
    learned_ops: &std::collections::HashMap<String, String>,
    resolved: &std::collections::HashMap<String, String>,
) -> Links {
    let short = short_link(channel_name, learned_ops);
    let mumble = builtin_named_mumble(channel_name)
        .map(str::to_owned)
        .or_else(|| short.as_ref().and_then(|s| resolved.get(s).cloned()));
    Links { mumble, short }
}

/// The op number a channel name stands for. "o7" is the seventh op channel, spelled that way.
pub fn op_number(name: &str) -> Option<u8> {
    let n = name.trim();
    if n.eq_ignore_ascii_case("o7") {
        return Some(7);
    }
    let rest = n.get(..2).filter(|p| p.eq_ignore_ascii_case("op"))?;
    let _ = rest;
    n[2..].trim().parse().ok().filter(|k| (1..=12).contains(k))
}

/// The short links known before any ping has been seen.
pub fn builtin_op_link(n: u8) -> Option<&'static str> {
    Some(match n {
        1 => "https://gnf.lt/dYehZh9.html",
        2 => "https://gnf.lt/vLwgoyY.html",
        3 => "https://gnf.lt/NOH1FNH.html",
        4 => "https://gnf.lt/2eMgwE2.html",
        5 => "https://gnf.lt/SwVWcXS.html",
        6 => "https://gnf.lt/bO9WiWH.html",
        7 => "https://gnf.lt/EGcAES9.html",
        8 => "https://gnf.lt/0Yi1Dua.html",
        9 => "https://gnf.lt/vEALwCF.html",
        10 => "https://gnf.lt/1oh4Y6V.html",
        11 => "https://gnf.lt/sBIoA65.html",
        12 => "https://gnf.lt/jzTuUij.html",
        _ => return None,
    })
}

/// The `mumble://` link a short-link page redirects to.
///
/// Mumble's own `openUrl` only takes `mumble://`, and a short link opened any other way goes
/// through the browser. The page carries the target in its markup, HTML-escaped.
pub fn mumble_url_in(page: &str) -> Option<String> {
    let start = page.find("mumble://")?;
    let rest = &page[start..];
    let end = rest.find(|c: char| matches!(c, '"' | '\'' | '<' | '>') || c.is_whitespace())?;
    Some(rest[..end].replace("&amp;", "&"))
}

fn link(path: &[&str]) -> String {
    let path: Vec<String> = path.iter().map(|s| encode(s)).collect();
    format!("mumble://{HOST}/Ops/{}?title=Goonfleet&version=1.2.0", path.join("/"))
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleets::model::TagId;

    fn tag(name: &str, strategic: bool) -> TagItem {
        TagItem {
            id: TagId(1),
            name: name.to_owned(),
            colour_class: String::new(),
            is_primary: true,
            is_strategic: strategic,
        }
    }

    /// Strategic goes to Alpha, peacetime to Bravo, and anything unlabelled to Bravo rather than
    /// putting an FC into the strategic sector by accident.
    #[test]
    fn the_tags_decide_the_command_sector() {
        assert_eq!(sector(&[tag("STRATEGIC", true)]), Sector::Alpha);
        assert_eq!(sector(&[tag("PEACETIME", false)]), Sector::Bravo);
        assert_eq!(sector(&[]), Sector::Bravo);
        assert_eq!(sector(&[tag("Home Defence", false)]), Sector::Bravo);
        // The flag counts even when the name is spelled some other way.
        assert_eq!(sector(&[tag("Strat op", true)]), Sector::Alpha);
        // Both set is a strategic fleet.
        assert_eq!(sector(&[tag("PEACETIME", false), tag("STRATEGIC", true)]), Sector::Alpha);
    }

    /// The op channels are not numbered the way their ids are, so the name is what is read.
    #[test]
    fn the_command_channel_follows_the_op_channels_name() {
        assert_eq!(command_channel("Op 3").as_deref(), Some("Command 3"));
        assert_eq!(command_channel(" op 12 ").as_deref(), Some("Command 12"));
        // Channel id 7 is spelled "o7" and its command channel is the seventh.
        assert_eq!(command_channel("o7").as_deref(), Some("Command 7"));
        assert_eq!(command_channel("HD").as_deref(), Some("HD Command"));
        assert_eq!(command_channel("Locust").as_deref(), Some("Locust Command"));
        assert_eq!(command_channel("SV").as_deref(), Some("SV Command"));
        assert_eq!(command_channel("Scouts").as_deref(), Some("Scouts"));
        // Nothing an FC sits above.
        assert_eq!(command_channel("Capital Comms"), None);
        assert_eq!(command_channel("Standing Comms"), None);
        assert_eq!(command_channel("Op 99"), None);
        assert_eq!(command_channel(""), None);
    }

    /// Alpha suffixes its last four and Bravo does not.
    #[test]
    fn a_command_link_points_at_its_sector() {
        let path = |s: Sector, n: &str| {
            command_url(s, n).and_then(|u| crate::mumble::channel_path(&u))
        };
        assert_eq!(
            path(Sector::Alpha, "Op 3").as_deref(),
            Some("Ops/Command Sector Alpha/Command 3")
        );
        assert_eq!(
            path(Sector::Alpha, "Op 11").as_deref(),
            Some("Ops/Command Sector Alpha/Command 11 - SC")
        );
        assert_eq!(
            path(Sector::Bravo, "Op 11").as_deref(),
            Some("Ops/Command Sector Bravo/Command 11")
        );
        assert_eq!(
            path(Sector::Bravo, "HD").as_deref(),
            Some("Ops/Command Sector Bravo/HD Command")
        );
        assert_eq!(path(Sector::Alpha, "Capital Comms"), None);
    }

    /// The pages the live short links serve, trimmed. The op 8 one escapes its ampersand, which a
    /// naive cut would hand to Mumble as part of the channel name.
    #[test]
    fn a_short_link_page_yields_its_mumble_url() {
        let page = r#"<html><head><meta http-equiv="refresh" content="0; url=mumble://mumble.goonfleet.com/Ops/Op%20Channels/OP%2011%20-%20bubbles%20on%20a%20blue%20fleet?title=Goonfleet&version=1.2.0"></head></html>"#;
        assert_eq!(
            mumble_url_in(page).and_then(|u| crate::mumble::channel_path(&u)).as_deref(),
            Some("Ops/Op Channels/OP 11 - bubbles on a blue fleet")
        );
        let escaped = r#"<a href='mumble://mumble.goonfleet.com/Ops/Op%20Channels/OP%208?title=Goonfleet&amp;version=1.2.0'>"#;
        assert_eq!(
            mumble_url_in(escaped).as_deref(),
            Some("mumble://mumble.goonfleet.com/Ops/Op%20Channels/OP%208?title=Goonfleet&version=1.2.0")
        );
        assert_eq!(mumble_url_in("<html>nothing here</html>"), None);
        // What the live pages actually are: a script assignment in single quotes.
        let script = "<script>window.location = 'mumble://mumble.goonfleet.com/Ops/Op%20Channels/OP%208%20-%20%20Asher%20Electropunched%20the%20server?title=Goonfleet&amp;version=1.2.0';</script></html>";
        assert_eq!(
            mumble_url_in(script).and_then(|u| crate::mumble::channel_path(&u)).as_deref(),
            Some("Ops/Op Channels/OP 8 -  Asher Electropunched the server")
        );
        // The one gnf.lt sometimes serves instead, an empty 400, yields nothing to cache.
        assert_eq!(mumble_url_in(""), None);
    }

    /// A channel's link is found by its name: op channels from what pings taught the app, then
    /// the built-in table, and everything else from the built-in table only.
    #[test]
    fn a_short_link_is_found_by_channel_name() {
        let mut learned = std::collections::HashMap::new();
        learned.insert("op11".to_owned(), "https://gnf.lt/learned.html".to_owned());

        // Learned beats built in, because the ping is newer than this source file.
        assert_eq!(short_link("Op 11", &learned).as_deref(), Some("https://gnf.lt/learned.html"));
        assert_eq!(short_link("Op 3", &learned).as_deref(), builtin_op_link(3));
        assert_eq!(short_link("o7", &learned).as_deref(), builtin_op_link(7));
        // The dashboard's own links for the channels no ping names.
        assert_eq!(short_link(" Capital Comms ", &learned).as_deref(), builtin_named_link("capital comms"));
        assert_eq!(short_link("HD", &learned).as_deref(), builtin_named_link("HD"));
        // Unknown stays unknown rather than falling back to a guessed path.
        assert_eq!(short_link("Somewhere New", &learned), None);
        assert_eq!(short_link("Op 99", &learned), None);
    }

    /// `mumble://` first, from wherever one is known; the short link behind it either way.
    #[test]
    fn links_put_the_mumble_link_first() {
        let none = std::collections::HashMap::new();
        // Built in directly: no fetch needed at all.
        let hd = links("HD", &none, &none);
        assert!(hd.mumble.as_deref().is_some_and(|m| m.starts_with("mumble://")));
        assert_eq!(hd.short.as_deref(), builtin_named_link("HD"));

        // An op channel before anything has been resolved: short link only.
        let op = links("Op 3", &none, &none);
        assert_eq!(op.mumble, None);
        assert_eq!(op.short.as_deref(), builtin_op_link(3));

        // Once resolved and remembered, the mumble link is on hand before the next fetch.
        let mut resolved = std::collections::HashMap::new();
        resolved.insert(builtin_op_link(3).unwrap().to_owned(), "mumble://x/Ops/OP 3".to_owned());
        assert_eq!(links("Op 3", &none, &resolved).mumble.as_deref(), Some("mumble://x/Ops/OP 3"));
    }
}
