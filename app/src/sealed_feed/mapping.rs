//! A feed event as one of Spai's own intel reports. Structured already, so the chat parser is
//! not involved: systems, pilots and hulls arrive with their ids.

use super::wire::{Event, Location};
use crate::intel::{DetectedShip, DetectedSystem, IntelReport};

fn system(loc: &Location, systems: &crate::geo::Systems) -> Option<DetectedSystem> {
    let info = loc.system_id.and_then(|id| systems.info_of(id)).or_else(|| systems.lookup(loc.system.trim()))?;
    Some(DetectedSystem { id: info.id, name: info.name.clone(), security: info.security })
}

/// Systems named anywhere in an item: a string that is a system's name, a run of up to three
/// words in one, or a number that is a system id.
fn scan(v: &serde_json::Value, systems: &crate::geo::Systems, out: &mut Vec<DetectedSystem>, budget: &mut usize) {
    use serde_json::Value;
    if *budget == 0 || out.len() >= 5 {
        return;
    }
    *budget -= 1;
    let mut add = |info: &crate::geo::SystemInfo| {
        if !out.iter().any(|x| x.id == info.id) {
            out.push(DetectedSystem { id: info.id, name: info.name.clone(), security: info.security });
        }
    };
    match v {
        Value::Number(n) => {
            if let Some(info) = n.as_i64().filter(|id| (30_000_000..33_000_000).contains(id)).and_then(|id| systems.info_of(id)) {
                add(info);
            }
        }
        Value::String(s) => {
            let words: Vec<&str> = s.split(|c: char| c.is_whitespace() || ",;:()[]!?\"'".contains(c)).filter(|w| !w.is_empty()).take(200).collect();
            for n in (1..=3).rev() {
                for w in words.windows(n) {
                    if let Some(info) = systems.lookup(&w.join(" ")) {
                        add(info);
                    }
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| scan(x, systems, out, budget)),
        Value::Object(o) => o.values().for_each(|x| scan(x, systems, out, budget)),
        _ => {}
    }
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
        scan(&ev.raw, systems, &mut found, &mut 500);
    }
    if found.is_empty() {
        return None;
    }
    let said = ev.text.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let said_lower = said.map(str::to_lowercase).unwrap_or_default();
    let pilots: Vec<String> = ev.hostiles.iter().map(|(n, _)| n.trim().to_owned()).filter(|n| !n.is_empty()).collect();
    let ids: Vec<(String, i64)> = ev.hostiles.iter().filter_map(|(n, id)| Some((n.trim().to_lowercase(), (*id)?))).filter(|(n, _)| !n.is_empty()).collect();
    let ships: Vec<DetectedShip> = ev.ships.iter().filter(|(n, _)| !n.trim().is_empty()).map(|(n, id)| DetectedShip { id: id.unwrap_or(0), name: n.trim().to_owned() }).collect();
    let kw: Vec<String> = ev.keywords.iter().map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect();
    let said_words: Vec<&str> = said_lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has = |w: &[&str]| kw.iter().any(|k| w.contains(&k.as_str())) || w.iter().any(|x| if x.contains(' ') { said_lower.contains(x) } else { said_words.contains(x) });
    let is_kind = |k: &str| !k.is_empty() && ev.kind.eq_ignore_ascii_case(k);
    let clear = is_kind(&fields.kind_clear) || kw.iter().any(|k| k == "clr");
    let count = ev.count.filter(|&c| c > 0).or_else(|| (!pilots.is_empty()).then_some(pilots.len() as u32));
    let gate = ev.gate.as_deref().map(str::trim).filter(|g| !g.is_empty());

    let mut text: Vec<String> = Vec::new();
    if !(said.is_some() && found.iter().any(|f| said_lower.contains(&f.name.to_lowercase()))) {
        text.push(found[0].name.clone());
    }
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
    text.extend(said.map(str::to_owned));

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

    /// The nested shapes a feed spec used: ids beside names, gate and pilots as objects, ids of 0
    /// and counts of 0 meaning unknown, a clear tag.
    #[test]
    fn nested_objects_and_zero_meaning_unknown() {
        let ev = event(r#"{"id":"4b0e","ts":"2026-10-12T19:00:00.482Z","kind":"report","location":{"system":"1DQ1-A","id":30004759,"region":"Delve"},
            "from":null,"systems":[{"system":"8QT-H4","id":30004758,"region":"Delve"}],
            "hostiles":[{"name":"Some Pilot","id":0,"corp":{"id":98000001,"name":"Corp","ticker":"C"},"alliance":null}],
            "count":0,"ships":[{"name":"Sabre","type_id":22456,"class":"Interdictor","count":0}],"keywords":["gatecamp","nv"],
            "gate":{"system":"8QT-H4","ansiblex":false},"kills":[]}"#);
        let (r, ids) = to_report(&ev, &Fields::default(), "f", &geo(), 1_791_900_000).unwrap();
        assert_eq!(r.received, 1_791_831_600);
        assert_eq!(r.systems.iter().map(|s| s.id).collect::<Vec<_>>(), vec![30004759, 30004758]);
        assert_eq!((r.pilots.clone(), r.ships[0].id, r.gates.clone()), (vec!["Some Pilot".to_owned()], 22456, vec!["8QT-H4".to_owned()]));
        assert_eq!(r.count, Some(1), "count 0 is not stated, so the pilots named count");
        assert!(ids.is_empty(), "id 0 is unresolved");
        assert!(r.camp && r.no_visual && !r.clear);
        let (c, _) = to_report(&event(r#"{"id":"c","kind":"report","location":{"system":"8QT-H4","id":30004758},"keywords":["clr"]}"#), &Fields::default(), "f", &geo(), 5).unwrap();
        assert!(c.clear);
    }

    #[test]
    fn a_system_is_found_anywhere_in_an_item_and_its_message_is_the_text() {
        let (r, _) = to_report(&event(r#"{"msg":"bubble camp on the 8QT-H4 gate","meta":{"near":[30004759]}}"#), &Fields::default(), "f", &geo(), 5).unwrap();
        let mut names: Vec<&str> = r.systems.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["1DQ1-A", "8QT-H4"]);
        assert_eq!(r.text, "bubble camp on the 8QT-H4 gate");
        assert!(r.bubble && r.camp);
        let plain = crate::sealed_feed::wire::read_event(&serde_json::Value::String("1dq1-a clr".into()), &Fields::default());
        assert_eq!(to_report(&plain, &Fields::default(), "f", &geo(), 5).unwrap().0.systems[0].id, 30004759);
    }

    #[test]
    fn a_future_timestamp_is_held_to_now() {
        let (r, _) = to_report(&event(r#"{"id":"e4","timestamp":1891831600,"location":{"system_id":30004759}}"#), &Fields::default(), "f", &geo(), 1_791_900_000).unwrap();
        assert_eq!(r.received, 1_791_900_000);
    }
}
