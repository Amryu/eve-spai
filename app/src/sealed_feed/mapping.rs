//! A feed event as one of Spai's own intel reports. Structured already, so the chat parser is
//! not involved: systems, pilots and hulls arrive with their ids.

use super::wire::{Event, Location};
use crate::intel::{DetectedShip, DetectedSystem, IntelReport};

fn system(loc: &Location, systems: &crate::geo::Systems) -> Option<DetectedSystem> {
    let info = loc.system_id.and_then(|id| systems.info_of(id)).or_else(|| systems.lookup(loc.system.trim()))?;
    Some(DetectedSystem { id: info.id, name: info.name.clone(), security: info.security })
}

/// The report, and the pilot ids the event named. `None` when no system it names is known.
/// `origin` is the feed's name, shown as the report's channel and reporter.
pub fn to_report(ev: &Event, fields: &super::proto::Fields, origin: &str, systems: &crate::geo::Systems, now: i64) -> Option<(IntelReport, Vec<(String, i64)>)> {
    let mut found: Vec<DetectedSystem> = Vec::new();
    for loc in &ev.locations {
        if let Some(s) = system(loc, systems) {
            if !found.iter().any(|x| x.id == s.id) {
                found.push(s);
            }
        }
    }
    if found.is_empty() {
        return None;
    }
    let pilots: Vec<String> = ev.hostiles.iter().map(|(n, _)| n.trim().to_owned()).filter(|n| !n.is_empty()).collect();
    let ids: Vec<(String, i64)> = ev.hostiles.iter().filter_map(|(n, id)| Some((n.trim().to_lowercase(), (*id)?))).filter(|(n, _)| !n.is_empty()).collect();
    let ships: Vec<DetectedShip> = ev.ships.iter().filter(|(n, _)| !n.trim().is_empty()).map(|(n, id)| DetectedShip { id: id.unwrap_or(0), name: n.trim().to_owned() }).collect();
    let kw: Vec<String> = ev.keywords.iter().map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect();
    let has = |w: &[&str]| kw.iter().any(|k| w.contains(&k.as_str()));
    let is_kind = |k: &str| !k.is_empty() && ev.kind.eq_ignore_ascii_case(k);
    let clear = is_kind(&fields.kind_clear);
    let count = ev.count.filter(|&c| c > 0).or_else(|| (!pilots.is_empty()).then_some(pilots.len() as u32));
    let gate = ev.gate.as_deref().map(str::trim).filter(|g| !g.is_empty());

    let mut text: Vec<String> = vec![found[0].name.clone()];
    if clear {
        text.push("clr".into());
    }
    if let Some(c) = count {
        text.push(format!("+{c}"));
    }
    text.extend(pilots.iter().cloned());
    text.extend(ships.iter().map(|s| s.name.clone()));
    if let Some(g) = gate {
        text.push(format!("{g} gate"));
    }
    if is_kind(&fields.kind_kill) {
        text.push("kill".into());
    }
    text.extend(kw.iter().cloned());

    let report = IntelReport {
        received: ev.timestamp.unwrap_or(now).min(now),
        channel: origin.to_owned(),
        reporter: origin.to_owned(),
        text: text.join(" "),
        systems: found,
        ships,
        pilots,
        count,
        count_extra: ev.count,
        clear,
        gates: gate.map(str::to_owned).into_iter().collect(),
        camp: has(&["camp", "gatecamp"]),
        bubble: has(&["bubble", "bubbled"]),
        cyno: has(&["cyno"]),
        spike: has(&["spike"]),
        no_visual: has(&["nv", "no visual"]),
        tackled: has(&["tackled", "tackle"]),
        dropper: has(&["dropper", "hotdrop"]),
        help: has(&["help"]),
        status: has(&["status"]),
        origin: Some(origin.to_owned()),
        ..Default::default()
    };
    Some((report, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sealed_feed::proto::Fields;

    fn geo() -> crate::geo::Systems {
        let sys = |id: i64, name: &str| (name.to_lowercase(), crate::geo::SystemInfo { id, name: name.into(), security: -0.4, constellation: String::new(), region: "Delve".into(), faction: String::new() });
        crate::geo::Systems::new([sys(30004759, "1DQ1-A"), sys(30004758, "8QT-H4")].into_iter().collect(), Default::default())
    }

    fn event(json: &str) -> Event {
        crate::sealed_feed::wire::read_event(&serde_json::from_str(json).unwrap(), &Fields::default())
    }

    #[test]
    fn a_report_maps_with_its_ids_and_is_marked_with_its_feed() {
        let ev = event(r#"{"id":"e1","timestamp":"2026-10-12T19:00:00Z","kind":"report","location":{"system":"1DQ1-A","system_id":30004759},
            "additional_systems":[{"system":"8QT-H4"}],"hostiles":[{"name":"Some Pilot","id":91000001}],
            "count":5,"ships":[{"name":"Sabre","type_id":22456}],"keywords":["Bubble","camp"],"gate":"8QT-H4"}"#);
        let (r, ids) = to_report(&ev, &Fields::default(), "Test feed", &geo(), 1_791_900_000).unwrap();
        assert_eq!(r.received, 1_791_831_600);
        assert_eq!(r.systems.iter().map(|s| s.id).collect::<Vec<_>>(), vec![30004759, 30004758]);
        assert_eq!((r.pilots.clone(), r.count, r.ships[0].id), (vec!["Some Pilot".to_owned()], Some(5), 22456));
        assert!(r.bubble && r.camp && !r.clear);
        assert_eq!(r.gates, vec!["8QT-H4".to_owned()]);
        assert_eq!((r.origin.as_deref(), r.channel.as_str()), (Some("Test feed"), "Test feed"));
        assert_eq!(ids, vec![("some pilot".to_owned(), 91000001)]);
    }

    #[test]
    fn a_clear_maps_to_a_clear_and_an_unknown_system_to_nothing() {
        let (r, _) = to_report(&event(r#"{"id":"e2","timestamp":1791831600,"kind":"clear","location":{"system":"x","system_id":30004758}}"#), &Fields::default(), "f", &geo(), 1_791_900_000).unwrap();
        assert!(r.clear);
        assert_eq!(r.systems[0].name, "8QT-H4");
        assert!(to_report(&event(r#"{"id":"e3","kind":"report","location":{"system":"Nowhere"}}"#), &Fields::default(), "f", &geo(), 5).is_none());
    }

    #[test]
    fn a_future_timestamp_is_held_to_now() {
        let (r, _) = to_report(&event(r#"{"id":"e4","timestamp":1891831600,"location":{"system_id":30004759}}"#), &Fields::default(), "f", &geo(), 1_791_900_000).unwrap();
        assert_eq!(r.received, 1_791_900_000);
    }
}
