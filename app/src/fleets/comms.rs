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

/// The command channel for an op number, in the right sector.
///
/// Ops 9 and up carry a "- SC" suffix, which is how the server spells the ones an SC runs.
pub fn command_url(sector: Sector, op: i32) -> String {
    let op = op.clamp(1, 12);
    let chan = if op >= 9 { format!("Command {op} - SC") } else { format!("Command {op}") };
    link(&[sector.channel(), &chan])
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

    /// The command link names its sector and its channel, with the SC suffix where the server has
    /// one.
    #[test]
    fn a_command_link_points_at_its_sector() {
        let a = command_url(Sector::Alpha, 3);
        assert_eq!(
            crate::mumble::channel_path(&a).as_deref(),
            Some("Ops/Command Sector Alpha/Command 3")
        );
        let b = command_url(Sector::Bravo, 11);
        assert_eq!(
            crate::mumble::channel_path(&b).as_deref(),
            Some("Ops/Command Sector Bravo/Command 11 - SC")
        );
        // Out of range is clamped rather than producing a channel nobody has.
        assert_eq!(
            crate::mumble::channel_path(&command_url(Sector::Alpha, 99)).as_deref(),
            Some("Ops/Command Sector Alpha/Command 12 - SC")
        );
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
