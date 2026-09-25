//! A battle report on br.evetools for a tracked fleet's fights, made from its recorded kills.

use crate::store::FleetKill;

/// The live site's own endpoint, under the API base its bundle falls back to (`/newapi`). The
/// `/api/v1/br/create` in its published source answers 405.
const CREATE_URL: &str = "https://br.evetools.org/newapi/old/br/create";
pub const VIEW_URL: &str = "https://br.evetools.org/br";
/// br.evetools takes at most this many systems in one report.
const MAX_SYSTEMS: usize = 10;
/// Nor a stretch longer than a day in any of them.
const MAX_WINDOW: i64 = 86_400;
/// Room either side of a system's first and last kill, for the mails zKill has not seen yet.
const PAD: i64 = 300;

/// One system's stretch of the fight, in unix seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub system_id: i64,
    pub start: i64,
    pub end: i64,
    pub kills: usize,
}

/// Each system the fleet fought in, from its first kill to its last, busiest first.
pub fn windows(kills: &[FleetKill]) -> Vec<Window> {
    let mut out: Vec<Window> = Vec::new();
    for k in kills {
        match out.iter_mut().find(|w| w.system_id == k.system_id) {
            Some(w) => {
                w.start = w.start.min(k.at - PAD);
                w.end = w.end.max(k.at + PAD);
                w.kills += 1;
            }
            None => out.push(Window { system_id: k.system_id, start: k.at - PAD, end: k.at + PAD, kills: 1 }),
        }
    }
    out.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.start.cmp(&b.start)));
    out.truncate(MAX_SYSTEMS);
    for w in &mut out {
        w.end = w.end.min(w.start + MAX_WINDOW);
    }
    out
}

/// Creates the report. Returns its link and the key br.evetools hands out to edit it.
pub fn create(windows: &[Window]) -> Result<(String, String), String> {
    #[derive(serde::Deserialize)]
    struct Reply {
        status: Option<String>,
        #[serde(rename = "brID")]
        br_id: Option<String>,
        /// The site reads the id from either field.
        #[serde(rename = "_id")]
        id: Option<String>,
        ukey: Option<String>,
    }
    let relateds: Vec<serde_json::Value> = windows
        .iter()
        .map(|w| serde_json::json!({ "systemID": w.system_id, "start": w.start, "end": w.end }))
        .collect();
    let client = crate::http::client(30).map_err(|e| e.to_string())?;
    let resp = client
        .post(CREATE_URL)
        .json(&serde_json::json!({ "relateds": relateds }))
        .send()
        .map_err(|e| format!("br.evetools: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("br.evetools answered {status}"));
    }
    let reply: Reply = resp.json().map_err(|_| "br.evetools sent something that is not a report".to_owned())?;
    // No status with an id is a success too; the site reads it the same way.
    match (reply.status.as_deref(), reply.br_id.or(reply.id)) {
        (None | Some("success"), Some(id)) if !id.is_empty() => Ok((format!("{VIEW_URL}/{id}"), reply.ukey.unwrap_or_default())),
        (status, _) => Err(format!("br.evetools did not create the report ({})", status.unwrap_or("no id"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(system_id: i64, at: i64) -> FleetKill {
        FleetKill { system_id, at, ..Default::default() }
    }

    #[test]
    fn each_system_gets_its_own_stretch_busiest_first() {
        let w = windows(&[k(1, 1000), k(2, 2000), k(2, 2600), k(1, 1100), k(2, 2300)]);
        assert_eq!(
            w,
            vec![
                Window { system_id: 2, start: 1700, end: 2900, kills: 3 },
                Window { system_id: 1, start: 700, end: 1400, kills: 2 },
            ]
        );
    }

    #[test]
    fn a_report_stays_within_the_sites_limits() {
        let many: Vec<FleetKill> = (0..15).map(|i| k(30_000_000 + i, 1000)).collect();
        assert_eq!(windows(&many).len(), MAX_SYSTEMS);
        let long = windows(&[k(1, 0), k(1, 3 * 86_400)]);
        assert_eq!(long[0].end - long[0].start, MAX_WINDOW);
    }
}
