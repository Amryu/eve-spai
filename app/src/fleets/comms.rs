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

/// The fleet's own op channel, by the name the dashboard gave it.
///
/// Built from the channel name rather than the number, because the op channels are not named
/// uniformly ("o7" is channel 7) and the name is what the server has.
pub fn op_url(channel_name: &str) -> Option<String> {
    let name = channel_name.trim();
    if name.is_empty() {
        return None;
    }
    Some(link(&["Op Channels", name]))
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

    /// The op link uses the channel's own name, which is not always its number.
    #[test]
    fn an_op_link_uses_the_channel_name() {
        assert_eq!(
            op_url("Op 4").and_then(|u| crate::mumble::channel_path(&u)).as_deref(),
            Some("Ops/Op Channels/Op 4")
        );
        assert_eq!(
            op_url(" o7 ").and_then(|u| crate::mumble::channel_path(&u)).as_deref(),
            Some("Ops/Op Channels/o7")
        );
        assert_eq!(op_url("  "), None);
    }
}
