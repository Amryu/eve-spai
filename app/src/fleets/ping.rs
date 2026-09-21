//! Rendering a ping from the local template.
//!
//! The dashboard renders a ping of its own and `/ping-preview` is the better answer whenever it
//! can be had. This is what stands in when it cannot: a dry run, an expired session, a dashboard
//! that is down. An FC with no ping is an FC who cannot call a fleet, so the fallback is not
//! optional and it is not allowed to fail.

/// Everything a template can name. What is not known renders as `?` rather than leaving the
/// placeholder in, so a half-known situation still produces something sendable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vars {
    /// The system a capital is stuck in. Only a rescue has one.
    pub system: String,
    pub pilot: String,
    pub cyno: String,
    pub anomaly: String,
    pub op: String,
    pub doctrine: String,
    pub staging: String,
    pub fc: String,
    pub mumble: String,
}

const UNKNOWN: &str = "?";

pub fn render(template: &str, v: &Vars) -> String {
    let or = |s: &String| if s.trim().is_empty() { UNKNOWN } else { s.as_str() }.to_owned();
    template
        .replace("{system}", &or(&v.system))
        .replace("{pilot}", &or(&v.pilot))
        .replace("{cyno}", &or(&v.cyno))
        .replace("{anom}", &or(&v.anomaly))
        .replace("{op}", &or(&v.op))
        .replace("{doctrine}", &or(&v.doctrine))
        .replace("{staging}", &or(&v.staging))
        .replace("{fc}", &or(&v.fc))
        .replace("{mumble}", &or(&v.mumble))
}

/// The directorbot ping groups, in the order the rescue offers them.
pub const GROUPS: [&str; 3] = [COORD, "fc", "all"];

/// The coordinators. Starting a fleet asks them for a ping; nothing else on that form is a
/// decision about who hears it.
pub const COORD: &str = "coord";

/// What goes to `skirmish_commanders`: the bot reads the first line as the command.
pub fn bping(group: &str, body: &str) -> String {
    format!("!bping {group}\n\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_known_lands_and_what_is_not_says_so() {
        let v = Vars {
            fc: "Someone".into(),
            staging: "AAA-1".into(),
            op: "3".into(),
            doctrine: "Alpha Fleet".into(),
            mumble: "https://example.invalid/x.html".into(),
            ..Vars::default()
        };
        let out = render(
            "FC Name: {fc}\nFormup: {staging}\nComms: Op {op} {mumble}\nDoctrine: {doctrine}\n\
             Tackled: {pilot} in {system}",
            &v,
        );
        assert!(out.contains("FC Name: Someone"));
        assert!(out.contains("Comms: Op 3 https://example.invalid/x.html"));
        assert!(out.contains("Doctrine: Alpha Fleet"));
        // A fleet ping has no tackled capital, and a placeholder left in the text would be worse
        // than a question mark.
        assert!(out.contains("Tackled: ? in ?"));
        assert!(!out.contains('{'));
    }

    #[test]
    fn whitespace_only_counts_as_unknown() {
        let v = Vars { doctrine: "   ".into(), ..Vars::default() };
        assert_eq!(render("{doctrine}", &v), "?");
    }

    #[test]
    fn the_bot_reads_the_first_line() {
        assert_eq!(bping("coord", "body"), "!bping coord\n\nbody");
    }
}

/// How far either side of the fleet's start a ping is still plausibly that fleet's.
const BEFORE: i64 = 30 * 60;
const AFTER: i64 = 4 * 60 * 60;

/// When the fleet was actually pinged, out of the Jabber history.
///
/// Boost coverage is read from the boost channel between two times, and the fleet's `startedAt` is
/// when tracking began, not when anyone was called. Charges posted before the ping belong to
/// whatever ran before this fleet.
///
/// Two sources, whichever came first: the `!bping` posted to skirmish_commanders, and the ping
/// directorbot broadcast. A ping is this fleet's when it names the fleet or its comms channel and
/// lands in a window around the start.
///
/// `msgs` is `(who, body, outgoing, unix time)`, the shape `jabber_room_tail` returns.
pub fn ping_time(
    msgs: &[(String, String, bool, i64)],
    fleet_name: &str,
    channel: &str,
    started_at: i64,
) -> Option<i64> {
    let marks: Vec<String> = [fleet_name, channel]
        .iter()
        .map(|m| m.trim().to_lowercase())
        .filter(|m| m.len() >= 3)
        .collect();
    if marks.is_empty() {
        return None;
    }
    msgs.iter()
        .filter(|(_, _, _, at)| *at >= started_at - BEFORE && *at <= started_at + AFTER)
        .filter(|(_, body, _, _)| {
            let b = body.to_lowercase();
            marks.iter().any(|m| b.contains(m.as_str()))
        })
        .map(|(_, _, _, at)| *at)
        .min()
}

#[cfg(test)]
mod ping_time_tests {
    use super::*;

    fn msg(body: &str, at: i64) -> (String, String, bool, i64) {
        ("someone".to_owned(), body.to_owned(), false, at)
    }

    #[test]
    fn the_earliest_ping_naming_this_fleet_wins() {
        let start = 1_000_000;
        let msgs = vec![
            // Someone else's fleet, same window.
            msg("!bping coord\n\nFC Name: Other\nComms: Op 9", start - 60),
            msg("!bping coord\n\nHome Defence\nComms: Dankcomms", start + 30),
            msg("directorbot relayed: Home Defence is up", start + 45),
        ];
        assert_eq!(ping_time(&msgs, "Home Defence", "Dankcomms", start), Some(start + 30));
        // Matching on the channel alone is enough when the fleet has no name worth matching.
        assert_eq!(ping_time(&msgs, "", "Dankcomms", start), Some(start + 30));
    }

    #[test]
    fn a_ping_from_another_night_is_not_this_fleet() {
        let start = 1_000_000;
        let msgs = vec![msg("Home Defence", start - BEFORE - 1), msg("Home Defence", start + AFTER + 1)];
        assert_eq!(ping_time(&msgs, "Home Defence", "", start), None);
    }

    #[test]
    fn nothing_to_match_on_matches_nothing() {
        let start = 1_000_000;
        let msgs = vec![msg("anything at all", start)];
        // A two-letter fleet name would match half the channel; refuse rather than guess.
        assert_eq!(ping_time(&msgs, "OP", "", start), None);
        assert_eq!(ping_time(&msgs, "", "", start), None);
    }
}
