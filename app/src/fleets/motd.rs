//! The channel names an FC writes into the fleet MOTD, read back out of it.
//!
//! The MOTD is EVE's rich text: `<font ...>`, `<color=...>` and `<a href=...>` around lines like
//! `Boost Channel: Boosts 3`. Three lines are worth reading: comms, logi and boosts.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channels {
    /// The op channel's name, "Op 11", without the link that follows it.
    pub comms: Option<String>,
    /// The comms link the line carries, when it does.
    pub comms_link: Option<String>,
    pub logi: Option<String>,
    pub boost: Option<String>,
    /// What follows `Doctrine:`, as written: the setup name and the hull priority after it.
    pub doctrine: Option<String>,
}

impl Channels {
    pub fn is_empty(&self) -> bool {
        self.comms.is_none() && self.logi.is_none() && self.boost.is_none() && self.doctrine.is_none()
    }

    /// The op number the comms line names, for the join button.
    pub fn op(&self) -> Option<u8> {
        super::comms::op_number(self.comms.as_deref()?)
    }
}

/// Drops every `<...>` tag, keeping what it wraps.
pub fn strip_tags(s: &str) -> String {
    // A line break is a tag too, and the only one that means anything here.
    let s = s.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
    // Pings and MOTDs carry runs of invisible characters, which would keep "Doctrine:" from
    // matching its setup.
    let s: String = s.chars().filter(|c| !matches!(c, '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}')).collect();
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The channel lines, whatever else the MOTD says. A label is matched on its first word, so
/// "Boost Channel:", "Boosts:" and "Boost channel -" all read the same.
pub fn parse(motd: &str) -> Channels {
    let mut ch = Channels::default();
    for line in strip_tags(motd).lines() {
        let line = line.trim();
        // A colon only: an op channel or a doctrine name can carry a dash.
        let Some((label, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let head = label.trim().to_ascii_lowercase();
        if head.starts_with("comms") || head.starts_with("mumble") || head.starts_with("voice") {
            if ch.comms.is_none() {
                let (name, link) = match value.find("http") {
                    Some(i) => (value[..i].trim(), Some(value[i..].split_whitespace().next().unwrap_or("").to_owned())),
                    None => (value, None),
                };
                if !name.is_empty() {
                    ch.comms = Some(name.to_owned());
                }
                ch.comms_link = link.filter(|l| !l.is_empty());
            }
            continue;
        }
        let slot = if head.starts_with("doctrine") {
            &mut ch.doctrine
        } else if head.starts_with("boost") {
            &mut ch.boost
        } else if head.starts_with("logi") {
            &mut ch.logi
        } else {
            continue;
        };
        if slot.is_none() {
            *slot = Some(value.to_owned());
        }
    }
    ch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_channels_come_out_of_the_rich_text() {
        let motd = "<font size=\"13\" color=\"#bfffffff\">Welcome!</font><br><font size=\"12\" color=\"#ff00ff00\">Comms: Op 3</font>\n<font size=\"12\">Logi Channel: Logi 2</font>\nBoost Channel: <a href=\"joinChannel:x\">Boosts 1</a>\nDoctrine: Ferox";
        let ch = parse(motd);
        assert_eq!(ch.comms.as_deref(), Some("Op 3"));
        assert_eq!(ch.logi.as_deref(), Some("Logi 2"));
        assert_eq!(ch.boost.as_deref(), Some("Boosts 1"));
    }

    /// The MOTD a fleet actually carries, as the in-game window copies it.
    #[test]
    fn a_real_motd_reads_completely() {
        let motd = "<color=\"#ffffffff\">\nComms: Op 11 https://gnf.lt/sBIoA65.html\nLogi Channel: <a href=\"joinChannel:-76954974//None//None\">Dankaugswarm</a>\nBoost Channel: <a href=\"joinChannel:-85362243//None//None\">Awesomeboosts</a>\nDoctrine: Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)\nhttps://goonfleet.com/index.php/topic/357801-active-strat-hammerfleet-fni/";
        let ch = parse(motd);
        assert_eq!(ch.comms.as_deref(), Some("Op 11"));
        assert_eq!(ch.op(), Some(11));
        assert_eq!(ch.comms_link.as_deref(), Some("https://gnf.lt/sBIoA65.html"));
        assert_eq!(ch.logi.as_deref(), Some("Dankaugswarm"));
        assert_eq!(ch.boost.as_deref(), Some("Awesomeboosts"));
        assert_eq!(ch.doctrine.as_deref(), Some("Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)"));
    }

    #[test]
    fn invisible_characters_do_not_hide_a_line() {
        let ch = parse("Doctrine:\u{200D}\u{200D}\u{FEFF}\u{200D} Typhoon (Boosters > Guardian > Typhoon)");
        assert_eq!(ch.doctrine.as_deref(), Some("Typhoon (Boosters > Guardian > Typhoon)"));
    }

    #[test]
    fn a_motd_without_them_names_nothing() {
        assert!(parse("Welcome to the fleet\nUndock and align").is_empty());
        assert!(parse("Boost Channel:   ").is_empty());
        assert_eq!(strip_tags("<color=\"#ffffffff\">a<b>c</b>"), "ac");
    }
}
