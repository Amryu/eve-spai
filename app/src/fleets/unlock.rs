//! Whether this install may use fleet command: signed in to the dashboard, and the dashboard says
//! the account is a skirmish commander or above. It is a gate on the UI, not a security boundary:
//! the dashboard enforces its own permissions on every call.

use super::model::{Identity, Perm};

/// Command groups at skirmish commander or above, as the dashboard spells them.
pub const GROUPS: [&str; 6] = ["SC", "FC", "TC", "CC", "Coord", "GB/PP"];
/// How long an unlock holds without a fresh check, for a session that expired or a dashboard
/// that cannot be reached. A check that answers with a lower rank locks at once.
pub const GRACE_SECS: i64 = 30 * 86_400;
/// How often a stored session is checked again while the app runs.
pub const RECHECK_SECS: u64 = 6 * 3600;

#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    /// A commander: the group to show.
    Qualified(String),
    /// Signed in, but not a commander.
    Unqualified(String),
    /// No session stored, or the dashboard no longer accepts it.
    NoSession,
    /// The dashboard did not answer.
    Unreachable(String),
}

/// Both must hold: a command group at SC or above, and the right to start fleets.
pub fn qualifies(id: &Identity) -> Result<(), String> {
    let group = id.command_group.trim();
    if !GROUPS.iter().any(|g| g.eq_ignore_ascii_case(group)) {
        let shown = if group.is_empty() { "none" } else { group };
        return Err(format!("the dashboard lists your command group as {shown}; fleet command needs SC or above"));
    }
    if !id.can(Perm::StartFleet) {
        return Err("the dashboard does not let this account start fleets".to_owned());
    }
    Ok(())
}

/// Asks the dashboard who the stored session belongs to. Blocking.
pub fn check_stored() -> Check {
    let text = std::env::var(super::COOKIE_ENV).ok().filter(|t| !t.trim().is_empty()).or_else(super::creds::load);
    let Some(text) = text else { return Check::NoSession };
    let jar = super::http::Cookies::restore(&text).header();
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => return Check::Unreachable(e.to_string()),
    };
    let resp = match client
        .get(format!("{}/api/v1/authentication/is-authenticated", super::http::BASE))
        .header(reqwest::header::COOKIE, jar)
        .send()
    {
        Ok(r) => r,
        Err(e) => return Check::Unreachable(e.to_string()),
    };
    // An expired session is sent to the sign-in page rather than refused outright.
    if resp.status().is_redirection() || resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
        return Check::NoSession;
    }
    if !resp.status().is_success() {
        return Check::Unreachable(format!("the dashboard answered {}", resp.status()));
    }
    match resp.json::<Identity>() {
        Ok(id) => match qualifies(&id) {
            Ok(()) => Check::Qualified(id.command_group.trim().to_owned()),
            Err(why) => Check::Unqualified(why),
        },
        Err(e) => Check::Unreachable(format!("unexpected answer: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(group: &str, perms: &[&str]) -> Identity {
        Identity { command_group: group.into(), permissions: perms.iter().map(|p| (*p).to_owned()).collect(), ..Default::default() }
    }

    #[test]
    fn a_commander_group_and_the_right_to_start_fleets_are_both_needed() {
        assert!(qualifies(&id("SC", &["startFleet"])).is_ok());
        assert!(qualifies(&id("gb/pp ", &["startFleet"])).is_ok(), "case and stray spaces from the dashboard");
        assert!(qualifies(&id("SC", &["accessFleet"])).is_err(), "no startFleet");
        assert!(qualifies(&id("Line", &["startFleet"])).is_err(), "not a commander group");
        assert!(qualifies(&id("", &["startFleet"])).is_err());
    }
}
