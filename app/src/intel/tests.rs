use super::*;
use crate::geo::{SystemInfo, Systems};

/// The ESS hack timer is a duration, and a red/neut keyword on the other side of it must not
/// turn it into a hostile count. "45 mins" counts here even though `ess_time` rejects it as
/// over the 6 minute main-bank cap: it is a duration either way.
#[test]
fn ess_timer_is_not_a_hostile_count() {
    let s = systems();
    let sh = ships_with(&[("Bellicose", 632)]);
    for (text, why) in [
        ("ess reds 5 min Rancer", "5 min is the timer, not 5 reds"),
        ("ess hostiles 2 min left in Rancer", "2 min is the timer"),
        ("ess neuts 45 mins left Rancer", "45 mins is a duration"),
        ("reds 30 seconds out Rancer", "30 seconds is an ETA"),
    ] {
        let r = analyze(text, &s, &sh, &noknown(), 1, "ch", "x");
        assert_eq!(r.count, None, "{why}: {text} gave {:?}", r.count);
    }
    // A stated count alongside a timer still counts.
    let r = analyze("ESS 5:00 3 reds Rancer", &s, &sh, &noknown(), 1, "ch", "x");
    assert_eq!(r.count, Some(3), "a real count next to a timer must survive");
    let r = analyze("ess 45 min reserve 3 reds Rancer", &s, &sh, &noknown(), 1, "ch", "x");
    assert_eq!(r.count, Some(3));
}

/// A range off a gate is a distance. Nothing else parses these, so `parse_count` is the only
/// thing standing between "reds 20 km off gate" and a card claiming 20 hostiles.
#[test]
fn distance_is_not_a_hostile_count() {
    let s = systems();
    let sh = ships_with(&[("Loki", 29990)]);
    for text in ["reds 20 km off gate Rancer", "hostiles 100 au out Rancer", "neuts 10 km gate Rancer"] {
        let r = analyze(text, &s, &sh, &noknown(), 1, "ch", "x");
        assert_eq!(r.count, None, "{text} gave {:?}", r.count);
    }
    let r = analyze("5 Loki Rancer", &s, &sh, &noknown(), 1, "ch", "x");
    assert_eq!(r.count, Some(5), "a ship count must survive");
}

/// A number the name parser swallowed belongs to the name. The guard keys on the pair actually
/// appearing in a detected pilot, which is what keeps "ESS 5 reds" counting 5: no pilot "ESS 5"
/// is ever detected there, even though "ESS" is capitalised like a name.
#[test]
fn number_in_a_pilot_name_is_not_a_hostile_count() {
    let s = systems();
    let sh = ships_with(&[("Loki", 29990)]);
    let r = analyze("Trinity 5 red in Rancer", &s, &sh, &noknown(), 1, "ch", "x");
    assert_eq!(r.count, None, "the 5 belongs to the name: {:?}", r.pilots);
    assert!(
        r.name_number_skips.iter().any(|(c, n)| c.eq_ignore_ascii_case("Trinity 5") && *n == 5),
        "held as a skip so resolution can add it back: {:?}",
        r.name_number_skips
    );
    let r = analyze("Bob 7 neut in Rancer", &s, &sh, &noknown(), 1, "ch", "x");
    assert_eq!(r.count, None, "{:?}", r.pilots);

    for (text, want) in [
        ("ESS 5 reds Rancer", 5),
        ("Rancer gate 5 reds", 5),
        ("belt 3 neuts Rancer", 3),
        ("reds 6 Rancer", 6),
    ] {
        let r = analyze(text, &s, &sh, &noknown(), 1, "ch", "x");
        assert_eq!(r.count, Some(want), "{text} gave {:?}", r.count);
    }
}

#[test]
fn sightings_counting_and_revival() {
    let now = 1_000_000;
    let mut s = Sightings::default();
    s.record("Bob", 30000001, now - 100);
    s.record("bob", 30000002, now - 200);
    s.record("BOB", 30000003, now - 300);
    s.record("bob", 30000001, now - 50);
    s.record("bob", 30000099, now - 20000);

    assert_eq!(s.distinct_systems_since("bob", 3600, now), 3);
    assert!(s.revived("bob", now));

    assert_eq!(s.distinct_systems_since("bob", SIGHTINGS_WINDOW, now), 3);
    assert_eq!(s.distinct_systems_since("bob", 999_999, now), 4);
    s.prune(now);
    assert_eq!(s.distinct_systems_since("bob", 999_999, now), 3);

    assert_eq!(s.distinct_systems_since("nobody", 3600, now), 0);
    assert!(!s.revived("nobody", now));

    let mut w = Sightings::default();
    for (i, dt) in [(1, 100), (2, 200), (3, 5000), (4, 6000), (5, 7000)] {
        w.record("roamer", 30000000 + i, now - dt);
    }
    assert_eq!(w.distinct_systems_since("roamer", 3600, now), 2);
    assert_eq!(w.distinct_systems_since("roamer", SIGHTINGS_WINDOW, now), 5);
    assert!(w.revived("roamer", now));

    let mut z = Sightings::default();
    z.record("x", 0, now);
    z.record("x", -5, now);
    assert_eq!(z.distinct_systems_since("x", 3600, now), 0);
}

fn noships() -> std::collections::HashMap<String, (i64, String)> {
    std::collections::HashMap::new()
}

fn noknown() -> std::collections::HashMap<String, i64> {
    std::collections::HashMap::new()
}

fn esi_resolve(pilots: &[String], reals: &[&str]) -> Vec<String> {
    use crate::pilot::{name_windows, PilotCache};
    let real_map: std::collections::HashMap<String, i64> =
        reals.iter().enumerate().map(|(i, r)| (r.to_lowercase(), i as i64 + 1)).collect();
    let mut c = PilotCache::default();
    c.preload(&real_map);
    let mut negs: Vec<String> = Vec::new();
    for p in pilots {
        let mut spans = name_windows(p);
        spans.push(p.clone());
        spans.extend(p.split_whitespace().map(str::to_owned));
        for w in spans {
            let lw = w.to_lowercase();
            if !real_map.contains_key(&lw) {
                negs.push(lw);
            }
        }
    }
    c.preload_negatives(&negs);
    let mut out: Vec<String> = Vec::new();
    for p in pilots {
        if is_pilot_stopword(p) {
            continue;
        }
        match c.get(p) {
            Some(Some(_)) => out.push(p.clone()),
            _ => out.extend(c.cover(p).into_iter().filter(|n| !is_pilot_stopword(n))),
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.to_lowercase()));
    out
}

fn resolve_report(
    r: &IntelReport,
    reals: &[&str],
    systems: &Systems,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let pilots = esi_resolve(&r.pilots, reals);
    let reserved: std::collections::HashSet<String> =
        pilots.iter().flat_map(|p| p.split_whitespace()).map(|w| w.to_lowercase()).collect();
    let tokens: Vec<&str> = tokenize(&r.text);
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    let (detected, gates, _) =
        detect_location(&tokens, &lower_tokens, &reserved, systems, None, &[]);
    (pilots, detected.into_iter().map(|d| d.name).collect(), gates)
}

fn apply_resolution(r: &mut IntelReport, reals: &[&str], systems: &Systems) {
    let (pilots, sysnames, gates) = resolve_report(r, reals, systems);
    r.pilots = pilots;
    r.systems = sysnames
        .iter()
        .filter_map(|n| resolve(systems, n))
        .map(|i| DetectedSystem { id: i.id, name: i.name.clone(), security: i.security })
        .collect();
    r.gates = gates;
}

fn systems() -> Systems {
    let by_name = [
        ("rancer", "Rancer", 1, 0.4),
        ("jita", "Jita", 2, 0.9),
        ("1dq1-a", "1DQ1-A", 3, -0.4),
        ("78-aaa", "78-AAA", 4, -0.5),
        ("c-j6mt", "C-J6MT", 5, -0.6),
        ("ypw-m2", "YPW-M2", 7, -0.5),
        ("amarr", "Amarr", 8, 1.0),
        ("sv5-8n", "SV5-8N", 9, -0.4),
        ("eimj-m", "EIMJ-M", 30004946, -0.4),
        ("uitra", "Uitra", 30000148, 0.9),
        ("n3-jbx", "N3-JBX", 30000669, -0.3),
        ("384-in", "384-IN", 30000535, -0.5),
        ("e-jcus", "E-JCUS", 30000531, -0.5),
        ("b-3qpd", "B-3QPD", 30001156, -0.4),
    ]
    .into_iter()
    .map(|(key, name, id, sec)| {
        (
            key.to_string(),
            SystemInfo {
                id,
                name: name.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        )
    })
    .collect();
    Systems::new(by_name, HashMap::new())
}

#[test]
fn denied_name_frees_its_tokens() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("comet".to_string(), 1i64)].into_iter().collect();
    let empty = std::collections::HashSet::new();
    let base = analyze_ctx(
        "Comet tackled in Rancer", &s, &noships(), &known, 1, "ch", "x", None, &[], &empty,
    );
    assert!(
        base.pilots.iter().any(|p| p.eq_ignore_ascii_case("comet")),
        "baseline anchors Comet: {:?}",
        base.pilots
    );
    let denied: std::collections::HashSet<String> =
        ["comet".to_string()].into_iter().collect();
    let r = analyze_ctx(
        "Comet tackled in Rancer", &s, &noships(), &known, 1, "ch", "x", None, &[], &denied,
    );
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("comet")),
        "denied name Comet must be freed, not a pilot: {:?}",
        r.pilots
    );
    assert!(r.tackled, "the tackle keyword still parses with Comet freed");
    assert!(
        r.systems.iter().any(|d| d.name == "Rancer"),
        "the system still parses with Comet freed: {:?}",
        r.systems
    );
}

#[test]
fn all_stop_word_runs_are_never_pilots() {
    assert!(is_pilot_stopword("they are"));
    assert!(is_pilot_stopword("back to"));
    assert!(is_pilot_stopword("still here"));
    assert!(is_pilot_stopword("they"));
    assert!(!is_pilot_stopword("bob"));
    assert!(!is_pilot_stopword("Navy Bob"));
    assert!(is_pilot_stopword("I'm"));
    assert!(is_pilot_stopword("im"));
    assert!(is_pilot_stopword("they're"));
    assert!(is_pilot_stopword("don't"));
    assert!(!is_pilot_stopword("O'Brien"));
    assert!(is_pilot_stopword("full"));
    assert!(is_pilot_stopword("Full"));
}

#[test]
fn common_phrases_not_parsed_as_pilots() {
    let s = systems();
    let known = std::collections::HashMap::new();
    let empty = std::collections::HashSet::new();
    let a = |t: &str| analyze_ctx(t, &s, &noships(), &known, 1, "ch", "x", None, &[], &empty);

    for (text, banned) in [
        ("They are roaming in Rancer", &["they are", "they", "are"][..]),
        ("Back to gate in Rancer", &["back to", "back", "to"][..]),
        ("Still here in Rancer", &["still here", "still", "here"][..]),
    ] {
        let r = a(text);
        for b in banned {
            assert!(
                !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(b)),
                "{text:?}: {b:?} must not be a pilot: {:?}",
                r.pilots
            );
        }
        assert!(
            r.systems.iter().any(|d| d.name == "Rancer"),
            "{text:?}: the system still parses with the prose freed: {:?}",
            r.systems
        );
    }

    let r = a("I'm tackled in Rancer");
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("i'm") || p.eq_ignore_ascii_case("im")),
        "I'm must not be a pilot: {:?}",
        r.pilots
    );
    assert!(r.tackled, "tackle keyword still parses with I'm freed");
    assert!(r.systems.iter().any(|d| d.name == "Rancer"));
}

#[test]
fn legit_names_with_a_non_stop_word_survive() {
    let s = systems();
    let known = std::collections::HashMap::new();
    let empty = std::collections::HashSet::new();
    let a = |t: &str| analyze_ctx(t, &s, &noships(), &known, 1, "ch", "x", None, &[], &empty);

    let r = a("Bob Hope tackled in Rancer");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Bob Hope")),
        "multi-word name survives: {:?}",
        r.pilots
    );
    let r = a("I-Pustelga tackled in Rancer");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("I-Pustelga")),
        "distinctive single-word name survives: {:?}",
        r.pilots
    );

    let r = a("Navy Bob tackled in Rancer");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Navy Bob")),
        "one non-stop word keeps the run: {:?}",
        r.pilots
    );
}

fn systems_with_neighbor() -> Systems {
    let by_name = [("rancer", "Rancer", 1i64, 0.4), ("f2a-3x", "F2A-3X", 100, -0.4), ("jita", "Jita", 2, 0.9)]
        .into_iter()
        .map(|(k, n, id, sec)| {
            (k.to_string(), SystemInfo { id, name: n.to_string(), security: sec, constellation: String::new(), region: String::new(), faction: String::new() })
        })
        .collect();
    let adjacency = [(1i64, vec![100i64]), (100, vec![1])].into_iter().collect();
    Systems::new(by_name, adjacency)
}

#[test]
fn short_code_resolves_as_neighbour_gate_not_pilot() {
    let s = systems_with_neighbor();
    for msg in ["Bob f2a", "hostiles F2A", "hostiles f2a-3"] {
        let r = analyze_ctx(msg, &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
        let on_f2a = r.gates.iter().any(|g| g.eq_ignore_ascii_case("F2A-3X"))
            || r.systems.iter().any(|d| d.name == "F2A-3X");
        assert!(on_f2a, "{msg}: F2A not resolved — gates={:?} systems={:?}", r.gates, r.systems.iter().map(|d| &d.name).collect::<Vec<_>>());
        assert!(!r.pilots.iter().any(|p| p.to_lowercase().contains("f2a")), "{msg}: F2A as pilot {:?}", r.pilots);
    }
    let r = analyze_ctx("Bob f2a", &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
    assert!(r.pilots.iter().any(|p| p == "Bob"), "Bob lost: {:?}", r.pilots);
}

#[test]
fn surname_that_is_a_system_is_not_a_gate() {
    let s = systems();
    let r2 = analyze("N3-JBX* alexpanda Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
    let (pilots, sysd, gates) = resolve_report(&r2, &["alexpanda Uitra"], &s);
    assert_eq!(pilots, vec!["alexpanda Uitra".to_string()]);
    assert_eq!(sysd, vec!["N3-JBX".to_string()]);
    assert!(gates.is_empty(), "gates={gates:?}");
    let r3 = analyze("N3-JBX Bob Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
    let (pilots, sysd, gates) = resolve_report(&r3, &["Bob Uitra"], &s);
    assert_eq!(pilots, vec!["Bob Uitra".to_string()]);
    assert_eq!(sysd, vec!["N3-JBX".to_string()]);
    assert!(gates.is_empty(), "gates={gates:?}");
    let r4 = analyze("N3-JBX Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
    let (pilots, sysd, gates) = resolve_report(&r4, &[], &s);
    assert!(pilots.is_empty(), "pilots={pilots:?}");
    assert_eq!(sysd, vec!["N3-JBX".to_string()]);
    assert!(gates.iter().any(|g| g == "Uitra"), "gates={gates:?}");
}

#[test]
fn confirmed_name_system_surname_not_pulled_as_gate() {
    let s = systems();
    for line in ["alexpanda Uitra", "alexpanda Uitra gate", "alexpanda Uitra tackled"] {
        let r = analyze(line, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(proposed(&r.pilots, "alexpanda Uitra"), "{line:?}: not proposed: {:?}", r.pilots);
        let (pilots, sysd, gates) = resolve_report(&r, &["alexpanda Uitra"], &s);
        assert_eq!(pilots, vec!["alexpanda Uitra".to_string()], "{line:?}: pilots={pilots:?}");
        assert!(sysd.is_empty(), "{line:?}: Uitra leaked as a system: {sysd:?}");
        assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "{line:?}: Uitra leaked as a gate: {gates:?}");
    }
    let r = analyze("Rancer alexpanda Uitra gate", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, sysd, gates) = resolve_report(&r, &["alexpanda Uitra"], &s);
    assert_eq!(pilots, vec!["alexpanda Uitra".to_string()], "pilots={pilots:?}");
    assert!(sysd.iter().any(|n| n == "Rancer"), "Rancer missing: {sysd:?}");
    assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "Uitra leaked as a gate: {gates:?}");
    let r = analyze("N3-JBX Uitra gate", &s, &noships(), &noknown(), 1, "ch", "x");
    let (_p, _sysd, gates) = resolve_report(&r, &[], &s);
    assert!(gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "genuine Uitra gate lost: {gates:?}");
}

fn proposed(pilots: &[String], name: &str) -> bool {
    let want: Vec<String> = name.split_whitespace().map(|w| w.to_lowercase()).collect();
    pilots.iter().any(|p| {
        let ws: Vec<String> = p.split_whitespace().map(|w| w.to_lowercase()).collect();
        ws.windows(want.len()).any(|w| w == want.as_slice())
    })
}

fn has_pilot_token(pilots: &[String], tok: &str) -> bool {
    pilots.iter().any(|p| p.split_whitespace().any(|w| w.eq_ignore_ascii_case(tok)))
}

#[test]
fn stray_letter_midrun_splits_pilot_list() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("willlin".to_string(), 1i64), ("qiuxiaoye".to_string(), 2i64)].into_iter().collect();
    let r = analyze(
        "willlin qiuxiaoye Micahel wu v Htguuu Htg-0 灵感级* 金鹏级*",
        &s,
        &noships(),
        &known,
        1,
        "ch",
        "Wujian",
    );
    for name in ["willlin", "qiuxiaoye", "Micahel wu", "Htguuu", "Htg-0"] {
        assert!(proposed(&r.pilots, name), "{name:?} not proposed: {:?}", r.pilots);
    }
    assert!(!has_pilot_token(&r.pilots, "v"), "stray v leaked: {:?}", r.pilots);
    assert!(!r.pilots.iter().any(|p| p == "v"));
    assert!(proposed(&r.pilots, "Htg-0"), "Htg-0 mangled: {:?}", r.pilots);
    assert!(!has_pilot_token(&r.pilots, "灵感级"), "ship as pilot: {:?}", r.pilots);
}

#[test]
fn stray_word_midrun_splits_pilot_list() {
    let s = systems();
    let r = analyze("Alpha v Bravo", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p == "Alpha"), "Alpha lost: {:?}", r.pilots);
    assert!(r.pilots.iter().any(|p| p == "Bravo"), "Bravo lost: {:?}", r.pilots);
    assert!(!has_pilot_token(&r.pilots, "v"), "stray v leaked: {:?}", r.pilots);

    let r2 = analyze("Alpha Bravo lol Charlie", &s, &noships(), &noknown(), 1, "ch", "x");
    for name in ["Alpha", "Bravo", "Charlie"] {
        assert!(proposed(&r2.pilots, name), "{name:?} not proposed: {:?}", r2.pilots);
    }
    assert!(!has_pilot_token(&r2.pilots, "lol"), "stray lol leaked: {:?}", r2.pilots);

    let r3 = loose_pilot_runs("Cult is Dead", &noships(), &s);
    assert!(r3.iter().any(|p| p == "Cult is Dead"), "Cult is Dead split: {r3:?}");
}

#[test]
fn stray_letter_before_name_with_code_system() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("ruston shackleford".to_string(), 95786689i64)].into_iter().collect();
    let rk =
        analyze("v Ruston Shackleford B-3QPD", &s, &noships(), &known, 1, "ch", "Ixen Orlenard");
    assert_eq!(rk.pilots, vec!["Ruston Shackleford".to_string()], "known pilots={:?}", rk.pilots);
    assert_eq!(
        rk.systems.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
        vec!["B-3QPD".to_string()],
        "known systems"
    );
    assert!(rk.gates.is_empty(), "gates={:?}", rk.gates);

    let r = analyze("v Ruston Shackleford B-3QPD", &s, &noships(), &noknown(), 1, "ch", "Ixen Orlenard");
    let (pilots, sysd, gates) = resolve_report(&r, &["Ruston Shackleford"], &s);
    assert_eq!(pilots, vec!["Ruston Shackleford".to_string()], "raw pilots={:?}", r.pilots);
    assert_eq!(sysd, vec!["B-3QPD".to_string()]);
    assert!(gates.is_empty(), "gates={gates:?}");
}

#[test]
fn apostrophe_name_does_not_leak_bare_first_word() {
    // "Jennifer' Thyron" is one pilot. Tokenizing strips the ', and a bare "Jennifer" would
    // survive as a candidate because it matches a real EVE name via ESI.
    let s = systems();
    let text = "Biggi Harry Jae-ha Jennifer' Thyron Talon Karrdex";
    let r = analyze(text, &s, &noships(), &noknown(), 1, "ch", "Woosi");
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Jennifer")),
        "bare Jennifer leaked: {:?}",
        r.pilots
    );
    assert!(
        r.pilots.iter().any(|p| p.to_lowercase().contains("jennifer' thyron")),
        "full run missing: {:?}",
        r.pilots
    );
}

#[test]
fn sfi_is_ambiguous_ship_not_pilot() {
    let s = systems();
    let ships =
        ships_with(&[("Scythe Fleet Issue", 17812), ("Stabber Fleet Issue", 17726)]);
    let r = analyze("SFI in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(esi_resolve(&r.pilots, &[]).is_empty(), "SFI as pilot: {:?}", r.pilots);
    assert!(r.ships.is_empty(), "SFI resolved to a hull: {:?}", r.ships);
    assert_eq!(r.ambiguous_ships.len(), 1, "ambiguous: {:?}", r.ambiguous_ships);
    assert_eq!(r.ambiguous_ships[0].abbrev, "SFI");
    assert_eq!(r.ambiguous_ships[0].candidates.len(), 2);
}

#[test]
fn amending_full_name_clears_ambiguous_badge() {
    let mut dst = vec![AmbiguousShip {
        abbrev: "SFI".to_owned(),
        candidates: vec![
            (17812, "Scythe Fleet Issue".to_owned()),
            (17726, "Stabber Fleet Issue".to_owned()),
        ],
    }];
    let ships = vec![DetectedShip { id: 17726, name: "Stabber Fleet Issue".to_owned() }];
    merge_ambiguous_ships(&mut dst, &[], &ships);
    assert!(dst.is_empty(), "badge should clear once a hull is named: {dst:?}");
}

#[test]
fn held_model_lowercase_name_with_system() {
    let s = systems();
    let r = analyze("C-J6MT bob uitra", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.systems.is_empty(), "location must be held: {:?}", r.systems);
    assert!(has_held_system(&r, &s), "report should be parked");
    let (pilots, sysd, gates) = resolve_report(&r, &["bob uitra"], &s);
    assert_eq!(pilots, vec!["bob uitra".to_string()]);
    assert_eq!(sysd, vec!["C-J6MT".to_string()]);
    assert!(gates.is_empty(), "gates={gates:?}");
    let (pilots, sysd, _) = resolve_report(&r, &[], &s);
    assert!(pilots.is_empty(), "pilots={pilots:?}");
    assert!(sysd.iter().any(|n| n == "C-J6MT"), "systems={sysd:?}");
}

#[test]
fn fly_catcher_is_the_flycatcher_hull() {
    let s = systems();
    let mut by_name = std::collections::HashMap::new();
    by_name.insert("flycatcher".to_string(), (16242i64, "Flycatcher".to_string()));
    let mut ships = by_name.clone();
    for (slug, e) in crate::shipnames::aliases(&by_name) {
        ships.entry(slug).or_insert(e);
    }
    let r = analyze("Fly Catcher on gate in Jita", &s, &ships, &noknown(), 1, "ch", "Scout");
    assert!(r.ships.iter().any(|sh| sh.name == "Flycatcher"), "ships={:?}", r.ships);
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("fly") || p.eq_ignore_ascii_case("catcher")),
        "pilots={:?}",
        r.pilots
    );
}

#[test]
fn system_gate_token_is_not_also_a_pilot() {
    let mut by_name = std::collections::HashMap::new();
    for (key, name, id, sec) in
        [("o3-4mn", "O3-4MN", 100i64, -0.5f64), ("ias-x", "IAS-X", 101, -0.5)]
    {
        by_name.insert(
            key.to_string(),
            SystemInfo {
                id,
                name: name.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        );
    }
    let adjacency = [(100i64, vec![101i64]), (101, vec![100])].into_iter().collect();
    let s = Systems::new(by_name, adjacency);
    let r = analyze("O3-4MN gang on the IAS gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
    assert!(r.gates.iter().any(|g| g == "IAS-X"), "gates={:?}", r.gates);
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("ias")), "pilots={:?}", r.pilots);
    assert_eq!(
        r.systems.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
        vec!["O3-4MN"],
        "systems={:?}",
        r.systems
    );
}

#[test]
fn all_caps_names_are_pilots_regardless_of_length() {
    let s = systems();
    let r = analyze("C-J6MT  PORTOS11", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(esi_resolve(&r.pilots, &["PORTOS11"]), vec!["PORTOS11".to_string()]);
    let r2 = analyze("XEN in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(esi_resolve(&r2.pilots, &["XEN"]), vec!["XEN".to_string()]);
}

#[test]
fn safe_is_a_clear_and_question_mark_suppresses_it() {
    let s = systems();
    assert!(analyze("Rancer safe", &s, &noships(), &noknown(), 1, "ch", "x").clear);
    assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x").clear);
    assert!(!analyze("Rancer clear?", &s, &noships(), &noknown(), 1, "ch", "x").clear);
    assert!(!analyze("is Rancer safe?", &s, &noships(), &noknown(), 1, "ch", "x").clear);
}

#[test]
fn long_dictionary_prose_is_not_pilots() {
    let s = systems();
    let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").pilots;
    assert!(a("Just another front row seat").is_empty(), "{:?}", a("Just another front row seat"));
    assert!(a("front").is_empty());
    // Short runs stay candidates for ESI, even all-dictionary ones.
    assert_eq!(a("Silent Hunter in Rancer"), vec!["Silent Hunter".to_string()]);
    assert_eq!(a("Cult is Dead in Rancer"), vec!["Cult is Dead".to_string()]);
}

#[test]
fn paste_segment_is_not_unglued_by_the_cache() {
    let s = systems();
    let mut known = noknown();
    known.insert("ghost".into(), 1);
    known.insert("magician".into(), 2);
    let paste = analyze("C-J6MT  Ghost Magician", &s, &noships(), &known, 1, "ch", "x");
    assert_eq!(paste.pilots, vec!["Ghost Magician".to_string()], "{:?}", paste.pilots);
    let typed = analyze("Ghost Magician in Rancer", &s, &noships(), &known, 1, "ch", "x");
    assert_eq!(typed.pilots, vec!["Ghost Magician".to_string()], "{:?}", typed.pilots);
    let mut k3 = known.clone();
    k3.insert("gliar".into(), 3);
    k3.insert("mliarvis".into(), 4);
    k3.insert("sliarhia".into(), 5);
    let list = analyze("Gliar Mliarvis Sliarhia in Rancer", &s, &noships(), &k3, 1, "ch", "x");
    assert_eq!(list.pilots.len(), 1, "kept whole at parse time: {:?}", list.pilots);
    let split = esi_resolve(&list.pilots, &["Gliar", "Mliarvis", "Sliarhia"]);
    assert_eq!(split.len(), 3, "ESI-rejected whole + confirmed handles → list: {:?}", split);
    assert!(esi_resolve(&paste.pilots, &["Ghost", "Magician"]).is_empty());
}

#[test]
fn paste_segment_drops_typed_location_tail() {
    let s = systems();
    let r = analyze("C-J6MT  Garen Willow at taj", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.pilots, vec!["Garen Willow".to_string()], "pilots={:?}", r.pilots);
    assert!(r.systems.iter().any(|d| d.name == "C-J6MT"));
    assert_eq!(trim_paste_location_tail("Man in Black", &ships_with(&[])), "Man in Black");
    assert_eq!(trim_paste_location_tail("Lord of War", &ships_with(&[])), "Lord of War");
    assert_eq!(trim_paste_location_tail("Garen Willow at taj", &ships_with(&[])), "Garen Willow");
}

#[test]
fn paste_segment_drops_trailing_count() {
    let s = systems();
    let r = analyze("C-J6MT  01XcerberusX01 +3", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.pilots, vec!["01XcerberusX01".to_string()], "pilots={:?}", r.pilots);
    assert_eq!(r.count, Some(4), "count={:?}", r.count);
    assert_eq!(trim_paste_location_tail("Malcolm 41", &ships_with(&[])), "Malcolm 41");
    assert_eq!(trim_paste_location_tail("01XcerberusX01 +3", &ships_with(&[])), "01XcerberusX01");
    assert_eq!(trim_paste_location_tail("Drake x4", &ships_with(&[])), "Drake");
}

#[test]
fn pasted_urls_are_not_parsed_as_pilots() {
    let s = systems();
    let r = analyze("https://dscan.info/v/a626d009ffc3  Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.is_empty(), "url leaked as pilots: {:?}", r.pilots);
    assert_eq!(r.links.len(), 1, "dscan link should be captured");
    assert!(r.systems.iter().any(|d| d.name == "Rancer"));
    let r2 = analyze("Bob https://example.com/Foo-Bar in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!r2.pilots.iter().any(|p| p.to_lowercase().contains("foo") || p.contains("example") || p.contains("http")), "url fragments leaked: {:?}", r2.pilots);
    assert!(r2.pilots.iter().any(|p| p == "Bob"), "real name dropped: {:?}", r2.pilots);
}

#[test]
fn belt_is_a_location_badge_not_a_pilot() {
    let s = systems();
    let r = analyze("Ice Belt in Jita", &s, &noships(), &noknown(), 1, "ch", "Scout");
    assert!(r.celestials.iter().any(|c| c == "Ice Belt"), "celestials={:?}", r.celestials);
    assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);

    let r2 =
        analyze("hostiles at Asteroid Belt in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        r2.celestials.iter().any(|c| c == "Asteroid Belt"),
        "celestials={:?}",
        r2.celestials
    );
    assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("belt")), "pilots={:?}", r2.pilots);

    let r3 = analyze("camp on belt in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r3.celestials.iter().any(|c| c == "Belt"), "celestials={:?}", r3.celestials);
}

#[test]
fn lowercase_full_name_not_truncated_to_surname() {
    let s = systems();
    let mut known2 = noknown();
    known2.insert("ji wuming".into(), 2112339969);
    known2.insert("wuming".into(), 999);
    let r2 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known2, 1, "ch", "x");
    assert_eq!(r2.pilots, vec!["ji wuming".to_string()], "got {:?}", r2.pilots);
    let mut known3 = noknown();
    known3.insert("wuming".into(), 999);
    let r3 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known3, 1, "ch", "x");
    assert_eq!(r3.pilots, vec!["ji wuming".to_string()], "got {:?}", r3.pilots);
}

#[test]
fn full_name_not_split_into_ship_and_pilot() {
    let s = systems();
    let mut ships = noships();
    ships.insert("wolf".into(), (11371, "Wolf".into()));
    let mut known = noknown();
    known.insert("wolf e kristjansson".into(), 2122822665);
    let r2 = analyze("Wolf E Kristjansson nv", &s, &ships, &known, 1, "ch", "x");
    assert_eq!(r2.pilots, vec!["Wolf E Kristjansson".to_string()]);
    assert!(r2.ships.is_empty(), "ships={:?}", r2.ships);
}

#[test]
fn rest_keyword_not_a_pilot_even_if_known() {
    let s = systems();
    let mut known = noknown();
    known.insert("rest".into(), 999);
    let mut ships = noships();
    ships.insert("jackdaw".into(), (34562, "Jackdaw".into()));
    let r = analyze("1 jackdaw, rest NV in Jita", &s, &ships, &known, 1, "ch", "Anaz");
    assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
    assert!(r.no_visual);
    assert!(r.ships.iter().any(|sh| sh.name == "Jackdaw"));
}

#[test]
fn lowercase_chat_words_not_pilots() {
    let s = systems();
    let mut known = noknown();
    for (w, id) in [("sry", 1i64), ("gg", 2), ("ez", 3), ("neo", 4)] {
        known.insert(w.into(), id);
    }
    let r = analyze("sry gg ez that was ez in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
    assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
    let r2 = analyze("Neo tackled in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
    assert!(r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("neo")), "pilots={:?}", r2.pilots);
}

#[test]
fn name_with_bubble_keyword_is_a_pilot_not_a_bubble() {
    let mut by_name = std::collections::HashMap::new();
    by_name.insert("r0-dmm".to_string(), SystemInfo { id: 30000563, name: "R0-DMM".into(),
        security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
    let s = Systems::new(by_name, HashMap::new());
    let r = analyze("R0-DMM  The Bubble Boy", &s, &noships(), &noknown(), 1, "ch", "Anniken");
    assert_eq!(r.pilots, vec!["The Bubble Boy".to_string()]);
    assert!(!r.bubble);
    assert!(analyze("bubble up on gate R0-DMM", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
    assert!(!analyze("2 Dragoons on gate R0-DMM", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
    assert!(analyze("drag bubble on the R0-DMM gate", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
}

#[test]
fn standing_color_led_name_reaches_the_cover() {
    let mut by_name = std::collections::HashMap::new();
    by_name.insert("9olq-6".to_string(), SystemInfo { id: 30000800, name: "9OLQ-6".into(),
        security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
    let s = Systems::new(by_name, HashMap::new());
    let r = analyze("Blue RandomAttac Redhorn Mastro 9OLQ-6", &s, &noships(), &noknown(), 1, "ch", "Ariel Afuran");
    let (pilots, sysd, _) = resolve_report(&r, &["Blue RandomAttac", "Redhorn Mastro"], &s);
    assert_eq!(pilots, vec!["Blue RandomAttac".to_string(), "Redhorn Mastro".to_string()]);
    assert!(sysd.iter().any(|d| d == "9OLQ-6"), "systems={sysd:?}");
}

#[test]
fn suffix_subphrase_pilot_is_dropped() {
    let s = systems();
    let r = analyze("Dr Chen Chen in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.pilots, vec!["Dr Chen Chen".to_string()]);
}

#[test]
fn isk_amount_is_not_a_count() {
    let s = systems();
    let ships = ships_with(&[("Bellicose", 632)]);
    let r = analyze("ESS raid 2 Bellicose 334 million 6:00 Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert_eq!(r.count, Some(2), "ISK amount must not inflate the count");
    let r2 = analyze("ESS raid 2 Bellicose 300 kk 6:00 Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert_eq!(r2.count, Some(2), "300 kk must not be counted: {:?}", r2.count);
    let r3 = analyze("ESS raid 2 Bellicose 5 bill 6:00 Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert_eq!(r3.count, Some(2), "5 bill must not be counted: {:?}", r3.count);
}

#[test]
fn adjacent_names_not_leaked_as_subword() {
    let s = systems();
    let r = analyze("Bunk Boi Bunk Helper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Bunk Boi Bunk Helper")),
        "pilots={:?}",
        r.pilots
    );
    assert!(!r.pilots.iter().any(|p| p == "Helper"), "pilots={:?}", r.pilots);
}

#[test]
fn parses_regions_from_motd() {
    let known: std::collections::HashSet<String> =
        ["tenerifis", "immensea", "impass", "catch", "wicked creek"]
            .iter().map(|s| s.to_string()).collect();
    let motd = "[ 2026.06.24 ] EVE System > Channel MOTD: TENERIFIS // IMMENSEA // IMPASS // CATCH\nPlease contact Corps Diplomatique";
    assert_eq!(parse_motd_regions(motd, &known), vec!["tenerifis", "immensea", "impass", "catch"]);
    let glued = "EVE System > Channel MOTD: TENERIFIS // IMMENSEA // IMPASS // CATCHPlease contact Corps";
    assert_eq!(parse_motd_regions(glued, &known), vec!["tenerifis", "immensea", "impass", "catch"]);
    assert_eq!(parse_motd_regions("Channel MOTD:  Wicked Creek //  Cache", &known), vec!["wicked creek"]);
    let utf8 = "Channel MOTD: Привет диплома // CATCH glued";
    assert_eq!(parse_motd_regions(utf8, &known), vec!["catch"]);
}

#[test]
fn motd_region_disambiguates_abbreviation() {
    use crate::geo::{SystemInfo, Systems};
    let mk = |id, name: &str, region: &str| SystemInfo {
        id,
        name: name.into(),
        security: -0.6,
        constellation: String::new(),
        region: region.into(),
        faction: String::new(),
    };
    let by_name: std::collections::HashMap<String, SystemInfo> = [
        ("c-j6mt", mk(1, "C-J6MT", "Tenerifis")),
        ("c-j7cr", mk(2, "C-J7CR", "Vale of the Silent")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let sys = Systems::new(by_name, std::collections::HashMap::new());
    let r0 = analyze_ctx("hostiles in C-J", &sys, &noships(), &noknown(), 1, "ch", "x", None, &[], &std::collections::HashSet::new());
    assert!(r0.systems.is_empty(), "should stay ambiguous: {:?}", r0.systems);
    let regions = vec!["Tenerifis".to_string()];
    let r = analyze_ctx(
        "hostiles in C-J", &sys, &noships(), &noknown(), 1, "ch", "x", None, &regions,
        &std::collections::HashSet::new(),
    );
    assert!(r.systems.iter().any(|s| s.name == "C-J6MT"), "systems={:?}", r.systems);
    assert!(!r.systems.iter().any(|s| s.name == "C-J7CR"), "systems={:?}", r.systems);
}

#[test]
fn digit_handle_is_a_pilot_candidate() {
    let s = systems();
    let r = analyze("0xtomorrow AGCP-I", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["0xtomorrow"], &s);
    assert_eq!(pilots, vec!["0xtomorrow".to_string()], "pilots={pilots:?}");
    let junk = analyze("334m 88A 1DH-SX in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(esi_resolve(&junk.pilots, &[]).is_empty(), "junk pilots: {:?}", junk.pilots);
    assert!(is_time_token("4min") && is_time_token("30s") && is_time_token("2h"));
    assert!(!is_time_token("0xtomorrow") && !is_time_token("c137m"));
}

#[test]
fn trailing_apostrophe_stripped_from_name() {
    let s = systems();
    let r = analyze("MO-I1W PeshyHod'", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["PeshyHod"], &s);
    assert_eq!(pilots, vec!["PeshyHod".to_string()], "pilots={pilots:?}");
    let r2 = analyze("O'Brien in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r2, &["O'Brien"], &s);
    assert_eq!(pilots, vec!["O'Brien".to_string()], "pilots={pilots:?}");
}

#[test]
fn detects_chinese_keywords() {
    let s = systems();
    let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(a("J5A 蹲门").camp, "蹲 = camp");
    assert!(a("泡泡 on gate").bubble, "泡泡 = bubble");
    assert!(a("诱导信标").cyno, "诱导 = cyno");
    assert!(a("求救").help, "求救 = help");
    assert!(a("红名被抓了").tackled, "抓 = tackled");
}

#[test]
fn currently_is_never_a_pilot() {
    let s = systems();
    let r = analyze("currently in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(is_pilot_stopword("currently"));
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("currently")),
        "currently parsed as pilot: {:?}",
        r.pilots
    );
    let r2 = analyze("Currently camped", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(esi_resolve(&r2.pilots, &[]).is_empty(), "pilots: {:?}", r2.pilots);
}

#[test]
fn anom_sig_keyword_alone_raises_a_bare_badge() {
    let s = systems();
    let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
    for (kw, kind) in [
        ("anom", AnomKind::Anomaly),
        ("sig", AnomKind::Signature),
        ("anomaly", AnomKind::Anomaly),
        ("signature", AnomKind::Signature),
    ] {
        let r = a(kw);
        assert_eq!(r.anom_sigs, vec![(kind, String::new())], "{kw}: anom_sigs={:?}", r.anom_sigs);
        assert!(!r.diamond_rats, "{kw}: diamond_rats");
        assert!(esi_resolve(&r.pilots, &[]).is_empty(), "{kw}: pilots={:?}", r.pilots);
        assert!(r.systems.is_empty(), "{kw}: systems={:?}", r.systems);
    }
}

#[test]
fn diamond_rats_badge_not_a_pilot() {
    let s = systems();
    for txt in ["diamond rats in Rancer", "dia rats", "Diamond Rats", "diamond rat"] {
        let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.diamond_rats, "{txt}: diamond_rats not set");
        let pilots = esi_resolve(&r.pilots, &["Diamond", "Dia", "Rat", "Rats"]);
        assert!(
            !pilots.iter().any(|p| {
                matches!(p.to_lowercase().as_str(), "diamond" | "dia" | "rat" | "rats")
            }),
            "{txt}: rats word as pilot: {pilots:?}"
        );
    }
    let plain = analyze("rats on gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!plain.diamond_rats, "plain rats set diamond flag");
    assert!(esi_resolve(&plain.pilots, &["Rats"]).is_empty(), "plain pilots: {:?}", plain.pilots);
}

#[test]
fn anom_sig_code_badge_both_orders() {
    let s = systems();
    let before = analyze("anom ABC-123", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(before.anom_sigs, vec![(AnomKind::Anomaly, "ABC-123".to_string())]);
    let after = analyze("ABC-123 sig", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(after.anom_sigs, vec![(AnomKind::Signature, "ABC-123".to_string())]);
    for r in [&before, &after] {
        assert!(esi_resolve(&r.pilots, &[]).is_empty(), "pilots: {:?}", r.pilots);
        assert!(r.systems.is_empty(), "systems: {:?}", r.systems);
    }
    assert_eq!(alert_label(&before.anom_sigs[0]), "Anom ABC-123");
    assert_eq!(alert_label(&after.anom_sigs[0]), "Sig ABC-123");
}

fn alert_label((kind, code): &(AnomKind, String)) -> String {
    match kind {
        AnomKind::Anomaly => format!("Anom {code}"),
        AnomKind::Signature => format!("Sig {code}"),
    }
}

#[test]
fn distance_token_not_a_pilot_and_anomaly_badge() {
    let by_name = [
        ("27-hp0", "27-HP0", 30000832i64, -0.4),
        ("mordunium", "Mordunium", 30000833, -0.4),
    ]
    .into_iter()
    .map(|(k, n, id, sec)| {
        (
            k.to_string(),
            SystemInfo {
                id,
                name: n.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        )
    })
    .collect();
    let s = Systems::new(by_name, HashMap::new());
    let ships = ships_with(&[("Vagabond", 11999)]);
    let known: std::collections::HashMap<String, i64> =
        [("tinde erkkinen".to_string(), 1i64)].into_iter().collect();
    let r = analyze(
        "27-HP0  tinde Erkkinen (Vagabond) 100km off Mordunium anomaly",
        &s,
        &ships,
        &known,
        1,
        "ch",
        "Lancer Maelstorm",
    );
    assert!(r.pilots.iter().any(|p| p == "tinde Erkkinen"), "pilots={:?}", r.pilots);
    assert!(
        !r.pilots.iter().any(|p| p.to_lowercase().contains("km")),
        "distance leaked into a pilot: {:?}",
        r.pilots
    );
    assert!(
        r.anom_sigs.iter().any(|(k, _)| *k == AnomKind::Anomaly),
        "no anomaly badge: {:?}",
        r.anom_sigs
    );
    assert!(r.ships.iter().any(|sh| sh.name == "Vagabond"), "ships={:?}", r.ships);
}

#[test]
fn anom_sig_code_shape_letters_optional_digits() {
    let s = systems();
    let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        a("anomaly ABX").anom_sigs.iter().any(|(k, c)| *k == AnomKind::Anomaly && c == "ABX"),
        "ABX: {:?}",
        a("anomaly ABX").anom_sigs
    );
    assert!(
        a("ABC-123 sig").anom_sigs.iter().any(|(k, c)| *k == AnomKind::Signature && c == "ABC-123"),
        "ABC-123: {:?}",
        a("ABC-123 sig").anom_sigs
    );
    assert_eq!(a("the anomaly").anom_sigs, vec![(AnomKind::Anomaly, String::new())]);
}

#[test]
fn anom_code_that_is_a_real_system_stays_a_system() {
    let by_name = [("abc-123", "ABC-123", 50001, -0.5)]
        .into_iter()
        .map(|(key, name, id, sec)| {
            (
                key.to_string(),
                SystemInfo {
                    id,
                    name: name.to_string(),
                    security: sec,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            )
        })
        .collect();
    let s = Systems::new(by_name, HashMap::new());
    let r = analyze("anom ABC-123", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        !r.anom_sigs.iter().any(|(_, c)| !c.is_empty()),
        "real system made an anom code: {:?}",
        r.anom_sigs
    );
    assert!(
        r.systems.iter().any(|d| d.name == "ABC-123"),
        "real system not detected: {:?}",
        r.systems
    );
}

#[test]
fn chinese_hull_name_resolves_as_ship() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> =
        [("审判者级".to_string(), (17738i64, "Retribution".to_string()))].into_iter().collect();
    let r = analyze("审判者级 in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Retribution"), "ships={:?}", r.ships);
    assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
}

#[test]
fn detects_tackled_with_target() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> =
        [("loki".to_string(), (29990i64, "Loki".to_string()))].into_iter().collect();
    let r = analyze("Loki tackled on the gate", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.tackled, "tackled keyword should fire");
    assert!(r.tackled_targets.iter().any(|t| t == "Loki"), "targets={:?}", r.tackled_targets);
    assert!(!r.cap_tackled, "Loki is not a capital");
    let r2 = analyze("2 marauders pointed", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r2.tackled && r2.tackled_targets.iter().any(|t| t == "Marauder"), "targets={:?}", r2.tackled_targets);
    let r3 = analyze("recon scrammed", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r3.tackled && r3.tackled_targets.iter().any(|t| t == "Recon"), "targets={:?}", r3.tackled_targets);
    let r4 = analyze("dread tackled", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r4.cap_tackled, "cap_tackled escalated");
    assert!(r4.tackled, "tackled also fires for a cap");
}

#[test]
fn detects_ship_classes() {
    let s = systems();
    let r = analyze("2 dics and a recon on the gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.classes.iter().any(|c| c == "Interdictor"), "classes={:?}", r.classes);
    assert!(r.classes.iter().any(|c| c == "Recon"), "classes={:?}", r.classes);
    for w in ["dics", "recon"] {
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
    }
    let r2 = analyze("hic plus 2 logi and a bomber", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r2.classes.iter().any(|c| c == "Heavy Interdictor"), "classes={:?}", r2.classes);
    let r3 = analyze("3 t3s and a t3 roaming, etc", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r3.classes.iter().any(|c| c == "Strategic Cruiser"), "classes={:?}", r3.classes);
    assert!(!r3.pilots.iter().any(|p| p.eq_ignore_ascii_case("etc")), "pilots={:?}", r3.pilots);
    let r4 = analyze("CRUISERS and battleships in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    // Generic hull tiers are just a size, not a class badge (only T2/T3 + capitals are).
    assert!(!r4.classes.iter().any(|c| c == "Cruiser"), "classes={:?}", r4.classes);
    assert!(!r4.classes.iter().any(|c| c == "Battleship"), "classes={:?}", r4.classes);
    assert!(esi_resolve(&r4.pilots, &[]).is_empty(), "pilots={:?}", r4.pilots);
    let mut ships = noships();
    ships.insert("dni".into(), (37457, "Drake Navy Issue".into()));
    let r5 = analyze("DNI in Jita", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r5.ships.iter().any(|sh| sh.name == "Drake Navy Issue"), "ships={:?}", r5.ships);
    assert!(r5.pilots.is_empty(), "pilots={:?}", r5.pilots);
    assert!(r2.classes.iter().any(|c| c == "Logistics"), "classes={:?}", r2.classes);
    assert!(r2.classes.iter().any(|c| c == "Stealth Bomber"), "classes={:?}", r2.classes);
}

#[test]
fn class_word_in_pilot_name_is_not_a_class() {
    let s = systems();
    // A generic hull word in a pilot's name never becomes a class badge.
    let r = analyze("Bob Destroyer tackled in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!r.classes.iter().any(|c| c == "Destroyer"), "classes={:?} pilots={:?}", r.classes, r.pilots);
    // A bare hull tier is never a class badge.
    let r2 = analyze("battleship gang in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r2.classes.is_empty(), "classes={:?}", r2.classes);
    // A standalone specialised class word still detects the class.
    let r3 = analyze("2 dictors on gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r3.classes.iter().any(|c| c == "Interdictor"), "classes={:?}", r3.classes);
}

#[test]
fn alliance_name_not_double_consumed_as_pilots() {
    let s = systems();
    let r = analyze("Shadow Cartel gang in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.alliances.iter().any(|(n, _)| n == "Shadow Cartel"), "alliances={:?}", r.alliances);
    for w in ["Shadow", "Cartel", "Shadow Cartel"] {
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)),
            "{w:?} leaked as a pilot: {:?}",
            r.pilots
        );
    }
}

#[test]
fn ceno_resolves_to_cenotaph() {
    let s = systems();
    // Aliases are folded into the ship index (store.rs does this from the SDE); mimic that.
    let mut by: std::collections::HashMap<String, (i64, String)> = std::collections::HashMap::new();
    by.insert("cenotaph".into(), (85062i64, "Cenotaph".into()));
    for (slug, e) in crate::shipnames::aliases(&by) {
        by.insert(slug, e);
    }
    for msg in ["2 ceno on gate in Rancer", "cenos in Rancer"] {
        let r = analyze(msg, &s, &by, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Cenotaph"), "{msg:?}: ships={:?}", r.ships);
    }
}

#[test]
fn cleared_and_shiptype_ignored() {
    let s = systems();
    // "cleared" registers as a clear and is not a pilot.
    let r = analyze("Rancer cleared", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.clear, "cleared should register as clear: {:?}", r.text);
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("cleared")), "pilots={:?}", r.pilots);
    // "ship type" / "shiptypes" is a common question, never a pilot.
    for msg in ["ship type?", "what shiptypes?"] {
        let r = analyze(msg, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            !r.pilots.iter().any(|p| {
                let l = p.to_lowercase();
                l.contains("type") || l.contains("ship")
            }),
            "{msg:?}: pilots={:?}",
            r.pilots
        );
    }
}

#[test]
fn complex_ship_report_not_misparsed() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [
        ("eris", (22460i64, "Eris")),
        ("vedmak", (47271, "Vedmak")),
        ("eni", (40072, "Exequror Navy Issue")),
    ]
    .into_iter()
    .map(|(k, (id, n))| (k.to_string(), (id, n.to_string())))
    .collect();
    let r = analyze(
        "O3-4MN Eris ENI Vedmak mobile bubble on the gate close to ansi warp via cyno beacon",
        &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.bubble, "bubble should fire");
    assert!(r.cyno, "cyno should fire");
    for sh in ["Eris", "Vedmak", "Exequror Navy Issue"] {
        assert!(r.ships.iter().any(|x| x.name == sh), "missing {sh}: {:?}", r.ships);
    }
    let resolved = esi_resolve(&r.pilots, &[]);
    assert!(resolved.is_empty(), "ships/keywords resolved as pilots: {resolved:?}");
}

#[test]
fn adjacent_ship_names_are_ships_not_pilots() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [
        ("sabre", (22456i64, "Sabre")),
        ("orthrus", (33157, "Orthrus")),
        ("stabber", (622, "Stabber")),
        ("deimos", (12023, "Deimos")),
    ]
    .into_iter()
    .map(|(k, (id, n))| (k.to_string(), (id, n.to_string())))
    .collect();
    let r = analyze("ZD1-Z2 Sabre Orthrus in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    for w in ["Sabre", "Orthrus", "Sabre Orthrus"] {
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
    }
    assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "ships={:?}", r.ships);
    assert!(r.ships.iter().any(|sh| sh.name == "Orthrus"), "ships={:?}", r.ships);
    let r2 = analyze("Stabber and Deimos in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r2.ships.iter().any(|sh| sh.name == "Stabber"), "ships={:?}", r2.ships);
    assert!(r2.ships.iter().any(|sh| sh.name == "Deimos"), "ships={:?}", r2.ships);
    assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Stabber") || p.eq_ignore_ascii_case("Deimos")), "pilots={:?}", r2.pilots);
}

#[test]
fn fuzzy_typo_multiword_hull_is_a_ship_not_pilots() {
    let s = systems();
    let ships = ships_with(&[
        ("Scythe Fleet Issue", 17812),
        ("Scythe", 631),
        ("Cyclone Fleet Issue", 17634),
        ("Drake", 24698),
    ]);
    let r = analyze("cythe fleet issue tackled in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r.ships);
    for w in ["cythe", "fleet issue", "fleet", "issue", "scythe", "cythe fleet issue"] {
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
    }
    assert!(!r.ships.iter().any(|sh| sh.name == "Scythe"), "ships={:?}", r.ships);
    assert!(r.systems.iter().any(|d| d.name == "Rancer"), "systems={:?}", r.systems);
    assert!(r.tackled, "tackled keyword should fire");

    let r2 = analyze("Scythe Fleet Issue in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r2.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r2.ships);

    let r3 = analyze("draek in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r3.ships.iter().any(|sh| sh.name == "Drake"), "ships={:?}", r3.ships);
    let r4 = analyze("drak in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(!r4.ships.iter().any(|sh| sh.name == "Drake"), "ships={:?}", r4.ships);
}

#[test]
fn confirmed_pilot_near_a_hull_stays_a_pilot() {
    let s = systems();
    let ships = ships_with(&[("Cyclone Fleet Issue", 17634)]);
    let known: std::collections::HashMap<String, i64> =
        [("cyclon fleet issue".to_string(), 4242i64)].into_iter().collect();
    let r = analyze("Cyclon Fleet Issue in Rancer", &s, &ships, &known, 1, "ch", "x");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Cyclon Fleet Issue")),
        "pilots={:?}",
        r.pilots
    );
    assert!(!r.ships.iter().any(|sh| sh.name == "Cyclone Fleet Issue"), "ships={:?}", r.ships);
}

#[test]
fn ansi_resolves_to_jump_bridge_destination() {
    use crate::geo::{SystemInfo, Systems};
    let mk = |id, name: &str| SystemInfo {
        id,
        name: name.into(),
        security: -0.5,
        constellation: String::new(),
        region: String::new(),
        faction: String::new(),
    };
    let by_name: std::collections::HashMap<String, SystemInfo> =
        [("o3-4mn", mk(1, "O3-4MN")), ("rancer", mk(2, "Rancer"))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
    let mut sys = Systems::new(by_name, std::collections::HashMap::new());
    sys.add_bridges(&[(1, 2)]);
    let r = analyze("O3-4MN Gate camp on Ansi", &sys, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.camp, "gate-camp keyword should fire");
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ansi")), "pilots={:?}", r.pilots);
    assert!(r.gates.iter().any(|g| g == "Rancer"), "the Ansi should lead to Rancer: {:?}", r.gates);
}

#[test]
fn ansi_destination_abbrev_is_not_a_pilot() {
    use crate::geo::{SystemInfo, Systems};
    let mk = |id, name: &str| SystemInfo {
        id,
        name: name.into(),
        security: -0.5,
        constellation: String::new(),
        region: String::new(),
        faction: String::new(),
    };
    // Destination abbreviation "EFM" carries no digit: a null code need not contain numbers.
    let by_name: std::collections::HashMap<String, SystemInfo> =
        [("o3-4mn", mk(1, "O3-4MN")), ("efm-j6", mk(2, "EFM-J6"))]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
    let mut sys = Systems::new(by_name, std::collections::HashMap::new());
    sys.add_bridges(&[(1, 2)]);
    let r = analyze("O3-4MN EFM Ansi", &sys, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.gates.iter().any(|g| g == "EFM-J6"), "the Ansi should lead to EFM-J6: {:?}", r.gates);
    for w in ["EFM", "Ansi"] {
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)),
            "{w} should not be a pilot: {:?}",
            r.pilots
        );
    }
}

#[test]
fn system_code_known_as_pilot_is_not_a_pilot() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("c-j".to_string(), 2119528359i64)].into_iter().collect();
    let r = analyze("Gorika Galrog C-J in Rancer", &s, &noships(), &known, 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["Gorika Galrog"], &s);
    assert_eq!(pilots, vec!["Gorika Galrog".to_string()], "pilots={pilots:?}");
}

#[test]
fn plus_count_adds_to_named_pilots() {
    let s = systems();
    let r = analyze("Gorika Galrog +20 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p == "Gorika Galrog"), "pilots={:?}", r.pilots);
    assert_eq!(r.count, Some(21), "pilots={:?}", r.pilots);
    let r2 = analyze("Gorika Galrog in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r2.count, None, "pilots={:?}", r2.pilots);
}

#[test]
fn derive_count_tracks_surviving_pilots() {
    // 3+ named pilots -> that many; a drop below 3 shows no bare count (parse semantics).
    assert_eq!(derive_count(None, 0, 0, 4, false), Some(4));
    assert_eq!(derive_count(None, 0, 0, 3, false), Some(3));
    assert_eq!(derive_count(None, 0, 0, 2, false), None);
    // A +N addend survives when its named pilots are discarded.
    assert_eq!(derive_count(None, 20, 0, 1, false), Some(21));
    assert_eq!(derive_count(None, 20, 0, 0, false), Some(20));
    // An explicit total (x5 / ship count) stands on its own, ignoring named pilots.
    assert_eq!(derive_count(Some(5), 0, 0, 2, false), Some(5));
    // Resolved ship counts add on top; solo seeds 1; nothing -> None.
    assert_eq!(derive_count(None, 0, 3, 0, false), Some(3));
    assert_eq!(derive_count(None, 0, 0, 0, true), Some(1));
    assert_eq!(derive_count(None, 0, 0, 0, false), None);
}

#[test]
fn discarded_pilot_stops_inflating_count() {
    let s = systems();
    let r = analyze("Gorika Galrog +20 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.count, Some(21));
    assert_eq!(r.count_plus, 20);
    assert_eq!(r.count_extra, None);
    // If "Gorika Galrog" is later discarded (ESI: not a character), re-deriving from the
    // surviving pilots gives 0 named + 20 = 20, not the stale 21.
    let after = derive_count(r.count_extra, r.count_plus, r.count_ships, 0, r.solo);
    assert_eq!(after, Some(20));
}

#[test]
fn combat_prob_is_probes_not_pilots() {
    let s = systems();
    let r = analyze("combat prob in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.probes, Some(Probes::Combat), "probes={:?}", r.probes);
    assert!(
        !r.pilots.iter().any(|p| {
            p.eq_ignore_ascii_case("combat") || p.eq_ignore_ascii_case("prob")
        }),
        "pilots={:?}",
        r.pilots
    );
}

#[test]
fn thera_hole_is_a_wormhole() {
    let s = systems();
    let r = analyze("thera hole in Rancer", &s, &noships(), &noknown(), 1, "ch", "wwhh");
    assert!(r.wormhole, "should be a wormhole message");
    assert!(matches!(r.wh_dest, Some(crate::wormholes::DestClass::Thera)), "dest={:?}", r.wh_dest);
}

#[test]
fn nullified_is_a_flag_not_a_pilot() {
    let s = systems();
    let ships = ships_with(&[("Loki", 29990)]);
    let r = analyze("Nullified Loki on gate in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.nullified, "nullified flag should fire");
    assert!(
        !proposed(&r.pilots, "Nullified") && !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("nullified")),
        "nullified leaked as a pilot: {:?}",
        r.pilots
    );
    // "nullifier" (the module name) also triggers it.
    assert!(analyze("ceptor with interdiction nullifier", &s, &noships(), &noknown(), 1, "ch", "x").nullified);
    // A plain nullsec mention does not trigger it.
    assert!(!analyze("hostiles in null in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").nullified);
}

#[test]
fn wormhole_size_parsed_from_words() {
    use crate::wormholes::ShipSize;
    let s = systems();
    let sz = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").wh_size;
    // "Extra large" (and xl / xlarge) is XL, and must beat the "large" substring.
    assert_eq!(sz("K162 nullsec extra large EOL"), Some(ShipSize::XLarge));
    assert_eq!(sz("wormhole xl to nullsec"), Some(ShipSize::XLarge));
    assert_eq!(sz("large wormhole in Rancer"), Some(ShipSize::Large));
    assert_eq!(sz("medium hole"), Some(ShipSize::Medium));
    assert_eq!(sz("frig hole in Rancer"), Some(ShipSize::Frigate));
    // Size is only read inside a wormhole message, so a normal gang report is unaffected.
    assert_eq!(sz("large gang in Rancer"), None);
}

#[test]
fn wormhole_sig_not_duplicated_as_sig_badge() {
    let s = systems();
    // A wormhole sig shows on the wormhole badge, so it must not also raise a Sig badge.
    let r = analyze("sig ABC-123 wormhole to nullsec", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.wormhole, "should be a wormhole");
    assert_eq!(r.wh_sig.as_deref(), Some("ABC-123"), "wh_sig={:?}", r.wh_sig);
    assert!(
        !r.anom_sigs.iter().any(|(_, c)| c.eq_ignore_ascii_case("ABC-123")),
        "duplicate Sig badge: {:?}",
        r.anom_sigs
    );
    // A non-wormhole signature still raises its Sig badge.
    let r2 = analyze("sig XYZ-456 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        r2.anom_sigs.iter().any(|(_, c)| c.eq_ignore_ascii_case("XYZ-456")),
        "anom_sigs={:?}",
        r2.anom_sigs
    );
}

#[test]
fn sisters_combat_scanner_is_probes_not_pilots() {
    let s = systems();
    let r = analyze("Sisters Combat Scanner in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.probes, Some(Probes::Combat), "probes={:?}", r.probes);
    assert!(esi_resolve(&r.pilots, &[]).is_empty(), "pilots={:?}", r.pilots);
}

#[test]
fn drops_subphrase_pilots_works() {
    let mut p = vec!["Nine".to_string(), "Nine -3".to_string()];
    drop_subphrase_pilots(&mut p, &std::collections::HashSet::new(), "Nine -3");
    assert_eq!(p, vec!["Nine -3".to_string()]);
    let mut q = vec!["Callas Plaude".to_string(), "Callas Plaude Wolf".to_string()];
    let protect: std::collections::HashSet<String> = ["callas plaude".to_string()].into();
    drop_subphrase_pilots(&mut q, &protect, "Callas Plaude Wolf");
    assert!(q.contains(&"Callas Plaude".to_string()), "q={q:?}");
    let mut t = vec!["Tiffanbrill".to_string(), "Tiffanbrill Dragon".to_string()];
    drop_subphrase_pilots(
        &mut t,
        &std::collections::HashSet::new(),
        "Tiffanbrill Tiffanbrill Dragon",
    );
    assert_eq!(t, vec!["Tiffanbrill".to_string(), "Tiffanbrill Dragon".to_string()], "t={t:?}");
    let mut u = vec!["Ruston Shackleford".to_string(), "Ruston Shackleford B-3QPD".to_string()];
    drop_subphrase_pilots(
        &mut u,
        &std::collections::HashSet::new(),
        "Ruston Shackleford B-3QPD",
    );
    assert_eq!(u, vec!["Ruston Shackleford B-3QPD".to_string()], "u={u:?}");
}

#[test]
fn standalone_ship_word_known_as_pilot_is_not_a_pilot() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> =
        [("buzzard".to_string(), (11192i64, "Buzzard".to_string()))].into_iter().collect();
    let known: std::collections::HashMap<String, i64> =
        [("buzzard".to_string(), 794250917i64)].into_iter().collect();
    let r = analyze("hostiles in a Buzzard in Rancer", &s, &ships, &known, 1, "ch", "x");
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Buzzard")), "pilots={:?}", r.pilots);
    assert!(r.ships.iter().any(|sh| sh.name == "Buzzard"), "ships={:?}", r.ships);
}

#[test]
fn ansiblex_jump_bridge_is_not_a_pilot() {
    let s = systems();
    let r = analyze("Ansiblex Jump Bridge in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    for w in ["Ansi", "Ansiblex", "Jump", "Bridge"] {
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
    }
    let r2 = analyze("reds on the Ansi in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ansi")), "pilots={:?}", r2.pilots);
}

#[test]
fn system_codes_and_state_words_not_pilots() {
    let s = systems();
    let r = analyze("88A-RA C-J gate bubbled", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.bubble, "bubble keyword should fire");
    assert!(
        analyze("drag on gate", &s, &noships(), &noknown(), 1, "ch", "x").bubble,
        "drag = drag bubble"
    );
    assert!(
        !analyze("no drag", &s, &noships(), &noknown(), 1, "ch", "x").bubble,
        "negated drag"
    );
    for w in ["C-J", "88A-RA", "bubbled"] {
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)),
            "{w} must not be a pilot: {:?}",
            r.pilots
        );
    }
}

#[test]
fn cloaked_is_a_state_not_a_pilot() {
    let s = systems();
    let r = analyze("Psychopathic beemaster cloaked in bubble", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["Psychopathic beemaster"], &s);
    assert_eq!(pilots, vec!["Psychopathic beemaster".to_string()], "pilots={pilots:?}");
}

#[test]
fn wormhole_word_is_keyword_not_a_pilot() {
    let s = systems();
    let r = analyze("Wormhole in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.wormhole, "wormhole keyword should fire");
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Wormhole")),
        "Wormhole must not be a pilot: {:?}",
        r.pilots
    );
}

#[test]
fn known_pilot_cache_respects_ship_and_stopwords() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [(
        "federation navy comet".to_string(),
        (17841i64, "Federation Navy Comet".to_string()),
    )]
    .into_iter()
    .collect();
    let known: std::collections::HashMap<String, i64> =
        [("navy".to_string(), 1i64), ("comet".to_string(), 2i64)].into_iter().collect();
    let r = analyze("Federation Navy Comet Docteur West in Rancer", &s, &ships, &known, 1, "ch", "x");
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Navy")), "pilots={:?}", r.pilots);
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Comet")), "pilots={:?}", r.pilots);
}

#[test]
fn hedging_think_not_a_pilot_even_if_known() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("think".to_string(), 1i64)].into_iter().collect();
    let r = analyze("i think Sevra is in Rancer", &s, &noships(), &known, 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["Sevra"], &s);
    assert!(!pilots.iter().any(|p| p.eq_ignore_ascii_case("think")), "pilots={pilots:?}");
    assert!(pilots.iter().any(|p| p == "Sevra"), "pilots={pilots:?}");
}

#[test]
fn content_keyword_kept_inside_name() {
    let s = systems();
    let r = analyze("High Plains Drifter in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["High Plains Drifter"], &s);
    assert_eq!(pilots, vec!["High Plains Drifter".to_string()], "pilots={pilots:?}");
}

#[test]
fn other_side_and_theft_are_not_pilots() {
    let s = systems();
    for m in ["Other Side in Jita", "skyhook Theft in Jita", "Other Side gang in Jita"] {
        let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            !r.pilots.iter().any(|p| ["side", "other", "theft"].contains(&p.to_lowercase().as_str())),
            "{m} -> spurious pilot: {:?}",
            r.pilots
        );
    }
}

#[test]
fn mid_name_connector_keeps_name_whole() {
    let s = systems();
    for (m, want) in [
        ("Cult is Dead in Rancer", "Cult is Dead"),
        ("Lord of War in Rancer", "Lord of War"),
    ] {
        let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == want), "{m} -> {:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p == "Cult" || p == "Dead" || p == "War"), "{m} -> {:?}", r.pilots);
    }
    let r = analyze("Sevra is in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
    assert!(!r.pilots.iter().any(|p| p == "Sevra is"), "pilots={:?}", r.pilots);
    let r2 = analyze("gate is camped in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r2.pilots.is_empty(), "pilots={:?}", r2.pilots);
}

#[test]
fn uppercase_x_multiplier_is_a_count() {
    let s = systems();
    for m in ["x5 in Rancer", "X5 in Rancer", "X12 hostiles Rancer"] {
        let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.count.is_some(), "{m} -> count {:?}", r.count);
    }
    assert_eq!(analyze("X5 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").count, Some(5));
    assert_eq!(analyze("x5 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").count, Some(5));
}

#[test]
fn skyhook_typo_still_detected() {
    let s = systems();
    let r = analyze("skhook theft in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.skyhook, "skyhook flag not set: {:?}", r.text);
    assert!(r.structures.iter().any(|(n, _)| n.as_str() == "Skyhook"), "structs={:?}", r.structures);
    assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
    let r2 = analyze("Schook in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!r2.skyhook, "schook wrongly flagged as skyhook");
}

#[test]
fn descriptor_and_verb_words_are_not_pilots() {
    let s = systems();
    let r = analyze("Sevra jumped Navy Issue in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    let resolved = esi_resolve(&r.pilots, &["Sevra"]);
    assert_eq!(resolved, vec!["Sevra".to_string()], "resolved={resolved:?} from {:?}", r.pilots);
}

#[test]
fn detects_structures_and_distance() {
    assert_eq!(
        detect_structures("Keepstar 500 off"),
        vec![("Keepstar".to_string(), Some("500km".to_string()))]
    );
    assert_eq!(
        detect_structures("Astra 2AU"),
        vec![("Astrahus".to_string(), Some("2AU".to_string()))]
    );
    assert_eq!(detect_structures("Fort tackled"), vec![("Fortizar".to_string(), None)]);
    assert_eq!(
        detect_structures("merc den anchoring"),
        vec![("Mercenary Den".to_string(), None)]
    );
    assert_eq!(
        detect_structures("Sotiyo 1000km"),
        vec![("Sotiyo".to_string(), Some("1000km".to_string()))]
    );
    let p1 = analyze("planet 1 Jita", &systems(), &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(p1.celestials, vec!["Planet 1".to_string()]);
    assert!(p1.count.is_none(), "count={:?}", p1.count);
    let m = analyze("moon IV in Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(m.celestials, vec!["Moon IV".to_string()]);
    let m53 = analyze("moon 5-3 Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(m53.celestials, vec!["Moon 5-3".to_string()]);
    assert!(m53.count.is_none(), "count={:?}", m53.count);
    let paste = analyze("Rancer VI - Moon 12", &systems(), &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(paste.celestials, vec!["Moon 6-12".to_string()], "cels={:?}", paste.celestials);
    let mi = analyze("moon I think it's clear Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
    assert!(mi.celestials.is_empty(), "phantom celestial: {:?}", mi.celestials);
    let sun = analyze("camped at the sun Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(sun.celestials, vec!["Sun".to_string()]);
    assert_eq!(detect_structures("POS bash Rancer"), vec![("POS".to_string(), None)]);
    assert!(is_structure_word("pos"));
    assert!(detect_structures("hostiles in Rancer").is_empty());
    assert!(is_structure_word("fort") && is_structure_word("keep") && is_structure_word("astra"));
    let cb = analyze("Cyno Beacon online in Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
    assert!(cb.structures.iter().any(|(n, _)| n == "Cyno Beacon"), "structures={:?}", cb.structures);
    assert!(
        !cb.pilots.iter().any(|p| p.eq_ignore_ascii_case("beacon") || p.eq_ignore_ascii_case("cyno")),
        "structure word leaked as a pilot: {:?}",
        cb.pilots
    );
}

#[test]
fn scanner_probes_badge_not_ship_or_pilot() {
    assert_eq!(detect_probes("Sisters Core Scanner Probe on dscan"), Some(Probes::Core));
    assert_eq!(detect_probes("Combat Scanner Probe I"), Some(Probes::Combat));
    assert_eq!(detect_probes("Core Probes"), Some(Probes::Core));
    assert_eq!(detect_probes("combat probes out"), Some(Probes::Combat));
    assert_eq!(detect_probes("probes on dscan"), Some(Probes::Any));
    assert_eq!(detect_probes("Probe tackled"), None);
    assert_eq!(detect_probes("hostiles in Rancer"), None);

    let si =
        std::collections::HashMap::from([("probe".to_string(), (587i64, "Probe".to_string()))]);
    let s = systems();
    let r = analyze("Sisters Core Scanner Probe on dscan", &s, &si, &noknown(), 1, "ch", "x");
    assert_eq!(r.probes, Some(Probes::Core));
    assert!(r.ships.iter().all(|sh| !sh.name.eq_ignore_ascii_case("probe")), "{:?}", r.ships);
    assert!(
        !r.pilots.iter().any(|p| p.to_lowercase().contains("probe")),
        "{:?}",
        r.pilots
    );
    let r2 = analyze("Probe tackled", &s, &si, &noknown(), 1, "ch", "x");
    assert!(r2.ships.iter().any(|sh| sh.name.eq_ignore_ascii_case("probe")));
    assert!(analyze("prob cyno in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").probes.is_none());
    assert_eq!(analyze("combat probes on dscan", &s, &noships(), &noknown(), 1, "ch", "x").probes, Some(Probes::Combat));
    let rp = analyze("RSS Scanner Probe tackled in Rancer", &s, &si, &noknown(), 1, "ch", "x");
    assert_eq!(rp.probes, None, "pilot name triggered a probe badge: {:?}", rp.probes);
    assert!(
        rp.pilots.iter().any(|p| p == "RSS Scanner Probe"),
        "RSS Scanner Probe not a pilot: {:?}",
        rp.pilots
    );
    assert_eq!(
        analyze("RSS Scanner Probe and Sisters Combat Scanner Probe on dscan", &s, &si, &noknown(), 1, "ch", "x").probes,
        Some(Probes::Combat),
        "real probes after the pilot name should still fire"
    );
}

#[test]
fn parses_isk_amounts() {
    assert_eq!(parse_isk("ess 300kk 5 min", true), Some(300_000_000));
    assert_eq!(parse_isk("ess worth 1.5b", true), Some(1_500_000_000));
    assert_eq!(parse_isk("ess 300 mil tag", true), Some(300_000_000));
    assert_eq!(parse_isk("worth 1.5b", false), None);
    assert_eq!(parse_isk("300 mil tag", false), None);
    assert_eq!(parse_isk("ess 750m", true), Some(750_000_000));
    assert_eq!(parse_isk("loot 750m", false), None);
    assert_eq!(parse_isk("ess hostiles in 4M-HGW", true), None);
    assert_eq!(parse_isk("5 min", false), None);
    assert_eq!(parse_isk("Rancer 3 Drake +2", false), None);
    assert_eq!(parse_isk("ess robbed 30m", true), None);
    assert_eq!(parse_isk("ess reserve 30m bank", true), None);
    assert_eq!(parse_isk("ess 50m", true), Some(50_000_000));
    assert_eq!(parse_isk("ess 77m bank", true), Some(77_000_000));
    assert_eq!(parse_isk("30m loot", false), None);
}

#[test]
fn ni_fi_abbreviations_match_faction_ships() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [
        ("vexor navy issue".to_string(), (1i64, "Vexor Navy Issue".to_string())),
        ("scythe fleet issue".to_string(), (2i64, "Scythe Fleet Issue".to_string())),
    ]
    .into_iter()
    .collect();
    let r = analyze("Vexor NI and Scythe FI in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Vexor Navy Issue"), "ships={:?}", r.ships);
    assert!(r.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r.ships);
}

#[test]
fn min_minutes_is_a_stop_word() {
    assert!(is_pilot_stopword("min"));
    assert!(is_pilot_stopword("heading"));
    assert!(is_pilot_stopword("towards"));
    let s = systems();
    let runs = loose_pilot_runs("ess 300kk 5 min", &noships(), &s);
    assert!(
        !runs.iter().any(|r| r.split_whitespace().any(|w| w.eq_ignore_ascii_case("min"))),
        "runs={:?}",
        runs
    );
}

#[test]
fn navy_issue_short_form_matches_ship() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [
        ("brutix navy issue".to_string(), (1i64, "Brutix Navy Issue".to_string())),
        ("stabber fleet issue".to_string(), (2i64, "Stabber Fleet Issue".to_string())),
    ]
    .into_iter()
    .collect();
    let r = analyze("Brutix Navy and Stabber Fleet in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Brutix Navy Issue"), "ships={:?}", r.ships);
    assert!(r.ships.iter().any(|sh| sh.name == "Stabber Fleet Issue"), "ships={:?}", r.ships);
}

#[test]
fn connector_stop_word_kept_in_multiword_name() {
    assert!(extract_pilots("384-IN The Meek").iter().any(|r| r == "The Meek"));
}

#[test]
fn intel_descriptor_breaks_a_name_run() {
    let out = extract_pilots("Cloaked Predator");
    assert!(!out.iter().any(|r| r.to_lowercase().contains("cloaked")), "out={:?}", out);
    assert!(extract_pilots("The").is_empty());
}

#[test]
fn covered_prefix_name_is_dropped() {
    let pilots = vec![
        "Gallente Citizen".to_string(),
        "Gallente Citizen 17120704".to_string(),
    ];
    let out = drop_covered_prefixes(&pilots, "Gallente Citizen 17120704 8-WYQZ");
    assert_eq!(out, vec!["Gallente Citizen 17120704".to_string()]);
}

#[test]
fn standalone_name_sharing_a_prefix_is_kept() {
    let pilots = vec!["Bob".to_string(), "Bob Smith".to_string()];
    let out = drop_covered_prefixes(&pilots, "Bob and Bob Smith inc");
    assert!(out.contains(&"Bob".to_string()));
    assert!(out.contains(&"Bob Smith".to_string()));
}

#[test]
fn multiword_ship_word_not_a_pilot() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [(
        "federation navy comet".to_string(),
        (17841i64, "Federation Navy Comet".to_string()),
    )]
    .into_iter()
    .collect();
    let r = analyze("Federation Navy Comet Docteur West in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Comet")), "pilots={:?}", r.pilots);
    assert!(r.ships.iter().any(|sh| sh.name == "Federation Navy Comet"));
}

#[test]
fn long_name_list_forms_one_run() {
    let s = systems();
    let r = analyze(
        "Noki Saken Ris Etor Ryko Erukka Saratoga Forge Urhi Hita nv in Rancer",
        &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case(
            "Noki Saken Ris Etor Ryko Erukka Saratoga Forge Urhi Hita")),
        "pilots={:?}", r.pilots);
}

#[test]
fn lowercase_name_with_digit_part_is_a_candidate() {
    let s = systems();
    let r = analyze("rick c137 sancgez in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("rick c137 sancgez")),
        "pilots={:?}",
        r.pilots
    );
}

#[test]
fn single_word_name_is_a_candidate() {
    let s = systems();
    let r = analyze("Sevra in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
}

#[test]
fn keywords_no_substring_false_trigger() {
    let s = systems();
    let r = analyze("Bunk Boi Bunk Helper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!r.help, "Helper must not trigger help");
    let ships: std::collections::HashMap<String, (i64, String)> =
        [("cynabal".to_string(), (17720i64, "Cynabal".to_string()))].into_iter().collect();
    let r2 = analyze("Cynabal in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(!r2.cyno, "Cynabal must not trigger cyno");
    assert!(analyze("cyno up Rancer", &s, &noships(), &noknown(), 1, "ch", "x").cyno);
}

#[test]
fn detects_help_keyword() {
    let s = systems();
    assert!(analyze("help in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
    assert!(analyze("sos Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
    assert!(analyze("need backup in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
    assert!(!analyze("clear Rancer", &s, &noships(), &noknown(), 1, "ch", "x").help);
}

#[test]
fn loose_run_catches_lowercase_name() {
    let s = systems();
    let r = analyze("bigfoott Kepplet in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("bigfoott Kepplet")),
        "pilots={:?}",
        r.pilots
    );
}

#[test]
fn multiword_ship_not_double_counted() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [
        ("catalyst".to_string(), (16240i64, "Catalyst".to_string())),
        ("catalyst navy issue".to_string(), (33470i64, "Catalyst Navy Issue".to_string())),
    ]
    .into_iter()
    .collect();
    let r = analyze("Rancer Catalyst Navy Issue", &s, &ships, &noknown(), 1, "ch", "x");
    let names: Vec<_> = r.ships.iter().map(|sh| sh.name.clone()).collect();
    assert_eq!(names, vec!["Catalyst Navy Issue"], "got {:?}", names);
}

#[test]
fn detects_localised_kill_keyword() {
    let s = systems();
    let r = analyze("DZ Sharisa > 击杀：Wolf E Kristjansson", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.killmail, "should flag a kill from the Chinese keyword");
}

#[test]
fn detects_single_ship_name() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> =
        [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
    let r = analyze("E-JCUS sabre", &s, &ships, &noknown(), 1, "ch", "x");
    assert_eq!(r.ships.iter().map(|sh| sh.name.clone()).collect::<Vec<_>>(), vec!["Sabre"]);
    let r2 = analyze("Sabre Smith in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r2.ships.is_empty());
    assert_eq!(r2.systems.iter().map(|d| d.name.clone()).collect::<Vec<_>>(), vec!["Rancer"]);
}

#[test]
fn extracts_pilot_candidates() {
    let s = systems();
    let r = analyze("Some Pilot tackled in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(esi_resolve(&r.pilots, &["Some Pilot"]), vec!["Some Pilot".to_string()]);
    let r2 = analyze("Gate Camp in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(esi_resolve(&r2.pilots, &[]).is_empty(), "pilots={:?}", r2.pilots);
}

#[test]
fn amend_merges_ship_when_system_held_in_name_blob() {
    let s = systems();
    let sh: std::collections::HashMap<String, (i64, String)> =
        [("gila".to_string(), (17715i64, "Gila".to_string()))].into_iter().collect();
    let orig = analyze("C-J6MT Keeves nv", &s, &sh, &noknown(), 100, "ch", "Super Logico");
    let amend = analyze("Keeves C-J6MT Gila ?", &s, &sh, &noknown(), 140, "ch", "Yhana Malkav 2");
    let mut state = IntelState::default();
    state.push(orig);
    assert!(state.try_amend(&amend, 60, &s), "should amend on the shared pilot word");
    assert_eq!(state.reports.len(), 1);
    assert!(
        state.reports[0].ships.iter().any(|x| x.name == "Gila"),
        "Gila not merged: {:?}",
        state.reports[0].ships
    );
}

#[test]
fn leading_digit_pilot_name_is_consistent_and_amends() {
    let s = systems_with(&[("kzfv-4", "KZFV-4", 30100, -0.5)]);
    let ships = ships_with(&[("Exequror Navy Issue", 29344)]);
    let known: std::collections::HashMap<String, i64> =
        [("1 tap machine".to_string(), 1i64)].into_iter().collect();
    let a = analyze("1 Tap Machine ENI", &s, &ships, &known, 100, "ch", "Corn SilkTea");
    let b = analyze("KZFV-4* 1 Tap Machine", &s, &ships, &known, 130, "ch", "jhouzy");
    assert!(proposed(&a.pilots, "1 Tap Machine"), "A pilots={:?}", a.pilots);
    assert!(proposed(&b.pilots, "1 Tap Machine"), "B pilots={:?}", b.pilots);
    assert_eq!(a.count, None, "leading digit counted in A: {:?}", a.pilots);
    assert_eq!(b.count, None, "leading digit counted in B: {:?}", b.pilots);
    assert!(b.systems.iter().any(|d| d.name == "KZFV-4"), "B system={:?}", b.systems);
    let mut state = IntelState::default();
    state.push(a);
    assert!(state.try_amend(&b, 60, &s), "second mention should amend the first");
    assert_eq!(state.reports.len(), 1, "split into separate cards: {:?}", state.reports);
    assert!(state.reports[0].systems.iter().any(|d| d.name == "KZFV-4"), "system not merged");

    let drake = ships_with(&[("Drake", 24698)]);
    let c = analyze("3 Drake", &s, &drake, &noknown(), 1, "ch", "x");
    assert_eq!(c.count, Some(3), "3 Drake should be a count: {:?}", c);
    assert!(!proposed(&c.pilots, "3 Drake"), "3 Drake leaked as a pilot: {:?}", c.pilots);
}

#[test]
fn wilen_amend_keeps_full_three_word_name() {
    let s = systems();
    let ships = ships_with(&[("Stabber", 622)]);
    let known: std::collections::HashMap<String, i64> =
        [("elizabeth van wilen".to_string(), 1i64)].into_iter().collect();
    let a =
        analyze("Rancer Elizabeth van Wilen", &s, &noships(), &known, 100, "ch", "Savant Solette");
    let b =
        analyze("Elizabeth van Wilen Stabber", &s, &ships, &known, 130, "ch", "Jeff Kali");
    assert!(proposed(&a.pilots, "Elizabeth van Wilen"), "A pilots={:?}", a.pilots);
    assert!(proposed(&b.pilots, "Elizabeth van Wilen"), "B pilots={:?}", b.pilots);
    assert!(
        !b.pilots.iter().any(|p| p.eq_ignore_ascii_case("Wilen") || p.eq_ignore_ascii_case("Wilen Stabber")),
        "B leaked bare 'Wilen': {:?}",
        b.pilots
    );
    let mut state = IntelState::default();
    state.push(a);
    assert!(state.try_amend(&b, 60, &s), "second mention should amend the first");
    assert_eq!(state.reports.len(), 1, "split into separate cards: {:?}", state.reports);
    assert!(
        proposed(&state.reports[0].pilots, "Elizabeth van Wilen"),
        "merged pilots={:?}",
        state.reports[0].pilots
    );
    assert!(
        !state.reports[0].pilots.iter().any(|p| p.eq_ignore_ascii_case("Wilen") || p.eq_ignore_ascii_case("Wilen Stabber")),
        "merged leaked bare 'Wilen': {:?}",
        state.reports[0].pilots
    );
}

#[test]
fn kill_paste_extracts_victim_and_ship() {
    let s = systems();
    let ships = ships_with(&[("Loki", 29990)]);
    let known: std::collections::HashMap<String, i64> =
        [("lord road".to_string(), 1i64), ("road".to_string(), 2i64)].into_iter().collect();
    let r = analyze("Kill: Lord Road (Loki)", &s, &ships, &known, 1, "ch", "x");
    assert!(r.killmail, "killmail flag");
    assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "victim: {:?}", r.pilots);
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")), "bare Road: {:?}", r.pilots);
    assert!(r.ships.iter().any(|sh| sh.name == "Loki"), "ship: {:?}", r.ships);
    let r2 = analyze("击杀：Lord Road (洛基级)", &s, &noships(), &known, 1, "ch", "x");
    assert!(r2.killmail);
    assert!(r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "victim: {:?}", r2.pilots);
    assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")), "bare Road: {:?}", r2.pilots);
}

#[test]
fn kill_paste_amend_no_double_consume() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("lord road".to_string(), 1i64), ("road".to_string(), 2i64)].into_iter().collect();
    let m1 = analyze("击杀：Lord Road (洛基级)  Rancer", &s, &noships(), &known, 100, "ch", "yuyexf");
    let m2 = analyze("击杀：Lord Road (洛基级)", &s, &noships(), &known, 130, "ch", "Aurelius Caracalla");
    let mut st = IntelState::default();
    st.push(m1);
    assert!(st.try_amend(&m2, 60, &s), "should amend on the shared victim");
    let merged = &st.reports[0].pilots;
    assert!(merged.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "victim: {:?}", merged);
    assert!(!merged.iter().any(|p| p.eq_ignore_ascii_case("Road")), "double-consumed Road: {:?}", merged);
}

#[test]
fn paste_linked_dictionary_name_survives() {
    let s = systems();
    let r = analyze("fibular  detective spider  Q-K2T7", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("fibular")), "dropped: {:?}", r.pilots);
    let known: std::collections::HashMap<String, i64> =
        [("fibular".to_string(), 5i64)].into_iter().collect();
    let r2 = analyze("fibular in Rancer", &s, &noships(), &known, 1, "ch", "x");
    assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("fibular")), "prose kept: {:?}", r2.pilots);
}

#[test]
fn keyword_in_pasted_name_does_not_bail_paste() {
    let s = systems();
    let r = analyze(
        "Bsjsisnjs  buzuo333  detective spider  feng fenghua  fliet98 cyno  Q-K2T7",
        &s, &noships(), &noknown(), 1, "ch", "Muchchi",
    );
    for name in ["Bsjsisnjs", "buzuo333", "detective spider", "feng fenghua"] {
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
            "missing {name}: {:?}",
            r.pilots
        );
    }
    assert!(
        !r.pilots.iter().any(|p| p.split_whitespace().count() > 3),
        "glued blob leaked: {:?}",
        r.pilots
    );
}

#[test]
fn subname_not_reused_when_full_name_resolved() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("lord road".to_string(), 1i64), ("road".to_string(), 2i64), ("capitaine onaga".to_string(), 3i64)]
            .into_iter()
            .collect();
    let r = analyze("Lord Road in Rancer", &s, &noships(), &known, 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "pilots={:?}", r.pilots);
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")),
        "bare 'Road' leaked: {:?}",
        r.pilots
    );
    let ships = ships_with(&[("Nereus", 650)]);
    let r2 = analyze(
        "JV1V-O  Kill: Capitaine Onaga (Nereus)  Lord Road he's happy now",
        &s, &ships, &known, 1, "ch", "Capitaine Onaga",
    );
    assert!(
        !r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")),
        "bare 'Road' leaked from kill paste: {:?}",
        r2.pilots
    );
}

#[test]
fn duplicate_line_flags_only_repeats() {
    let mut st = IntelState::default();
    assert!(!st.duplicate_line("Delve", "2026.07.06 14:30:22", "Pilot X", "Delve Prober"));
    assert!(st.duplicate_line("Delve", "2026.07.06 14:30:22", "Pilot X", "Delve Prober"));
    assert!(!st.duplicate_line("Delve", "2026.07.06 14:30:23", "Pilot X", "Delve Prober"));
    assert!(!st.duplicate_line("Delve", "2026.07.06 14:30:22", "Pilot Y", "Delve Prober"));
    assert!(!st.duplicate_line("Querious", "2026.07.06 14:30:22", "Pilot X", "Delve Prober"));
}

#[test]
fn identical_lines_across_accounts_make_one_card() {
    let s = systems();
    // Mirror the watcher: dedup the raw line, else analyze + amend-or-push.
    let ingest = |st: &mut IntelState, ts: &str, who: &str, text: &str| {
        if st.duplicate_line("ch", ts, who, text) {
            return;
        }
        let r = analyze(text, &s, &noships(), &noknown(), 100, "ch", who);
        if !st.try_amend(&r, 60, &s) {
            st.push(r);
        }
    };
    // Same regular line from three account logs (relog makes a 3rd file for 2 accounts).
    let mut st = IntelState::default();
    for _ in 0..3 {
        ingest(&mut st, "2026.07.06 14:30:22", "Scout", "Rancer Slasher hostile");
    }
    assert_eq!(st.reports.len(), 1, "{:?}", st.reports);
    // Clears duplicate worst (try_amend never merges them), but dedup must still collapse.
    let mut st2 = IntelState::default();
    for _ in 0..3 {
        ingest(&mut st2, "2026.07.06 14:31:00", "Scout", "Rancer clear");
    }
    assert_eq!(st2.reports.len(), 1, "{:?}", st2.reports);
    // Genuinely different lines are not over-deduped.
    let mut st3 = IntelState::default();
    ingest(&mut st3, "2026.07.06 14:32:00", "Scout", "Rancer Slasher hostile");
    ingest(&mut st3, "2026.07.06 14:33:30", "Scout", "Jita clear");
    assert_eq!(st3.reports.len(), 2, "{:?}", st3.reports);
}

#[test]
fn amends_successive_reporter_messages() {
    let s = systems();
    let mut state = IntelState::default();
    state.push(analyze("hostile in Rancer", &s, &noships(), &noknown(), 100, "ch", "Scout"));
    let follow = analyze("on 78- gate", &s, &noships(), &noknown(), 130, "ch", "Scout");
    assert!(state.try_amend(&follow, 60, &s));
    assert_eq!(state.reports.len(), 1);
    assert!(!state.reports[0].gates.is_empty());
    let other = analyze("hostile in Jita", &s, &noships(), &noknown(), 140, "ch", "Scout");
    assert!(!state.try_amend(&other, 60, &s));
    let clear = analyze("Rancer clear", &s, &noships(), &noknown(), 150, "ch", "Scout");
    assert!(!state.try_amend(&clear, 60, &s));
}

#[test]
fn clear_card_is_not_amended_by_later_sighting() {
    let s = systems();
    let mut state = IntelState::default();
    state.push(analyze("Rancer clear", &s, &noships(), &noknown(), 100, "ch", "Scout"));
    let follow = analyze("3 reds in Rancer", &s, &noships(), &noknown(), 120, "ch", "Scout");
    assert!(!state.try_amend(&follow, 60, &s));
    assert_eq!(state.reports.len(), 1);
    assert!(state.reports[0].clear);
}

#[test]
fn known_pilots_match_with_subset_protection() {
    let s = systems();
    let k1: std::collections::HashMap<String, i64> =
        [("bigfoott".to_string(), 2i64)].into_iter().collect();
    let r = analyze("Rancer bigfoott", &s, &noships(), &k1, 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["bigfoott"], &s);
    assert!(pilots.iter().any(|p| p.eq_ignore_ascii_case("bigfoott")), "{pilots:?}");
    let k2: std::collections::HashMap<String, i64> =
        [("hold me balls".to_string(), 1i64), ("hold".to_string(), 3i64)].into_iter().collect();
    let r2 = analyze("E-JCUS HOLD ME BALLS", &s, &noships(), &k2, 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r2, &["hold me balls"], &s);
    assert!(pilots.iter().any(|p| p.eq_ignore_ascii_case("hold me balls")), "{pilots:?}");
    assert!(!pilots.iter().any(|p| p.eq_ignore_ascii_case("hold")), "{pilots:?}");
}

#[test]
fn dscan_drop_ship_is_not_a_pilot() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [(
        "council diplomatic shuttle".to_string(),
        (670i64, "Council Diplomatic Shuttle".to_string()),
    )]
    .into_iter()
    .collect();
    let r = analyze(
        "I-Pustelga (Council Diplomatic Shuttle)",
        &s,
        &ships,
        &noknown(),
        1,
        "ch",
        "x",
    );
    assert!(r.pilots.iter().any(|p| p == "I-Pustelga"));
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Council Diplomatic Shuttle")));
    assert!(r.ships.iter().any(|sh| sh.name == "Council Diplomatic Shuttle"));
}

#[test]
fn dscan_drop_extracts_pilot_name() {
    assert_eq!(
        extract_dscan_drops("YI-GV6 SokoleOko (鱼鹰级海军型)"),
        vec![("SokoleOko".to_string(), "鱼鹰级海军型".to_string())]
    );
    assert_eq!(
        extract_dscan_drops("0UBC-R I-Pustelga (Council Diplomatic Shuttle)"),
        vec![("I-Pustelga".to_string(), "Council Diplomatic Shuttle".to_string())]
    );
}

#[test]
fn amends_by_shared_pilot_across_reporters() {
    let s = systems();
    let loki: std::collections::HashMap<String, (i64, String)> =
        [("loki".to_string(), (29990i64, "Loki".to_string()))].into_iter().collect();
    let mut state = IntelState::default();
    let mut a = analyze("C-J6MT Pericle No1", &s, &noships(), &noknown(), 100, "ch", "Kobayashi Mika");
    apply_resolution(&mut a, &["Pericle No1"], &s);
    assert_eq!(a.pilots, vec!["Pericle No1".to_string()]);
    state.push(a);
    let mut follow = analyze("Pericle No1 loki", &s, &loki, &noknown(), 130, "ch", "Wallie Warptunnel");
    apply_resolution(&mut follow, &["Pericle No1"], &s);
    assert!(state.try_amend(&follow, 60, &s));
    assert_eq!(state.reports.len(), 1);
    assert!(state.reports[0].ships.iter().any(|sh| sh.name == "Loki"));
}

#[test]
fn quoting_forces_pilot_not_keyword() {
    let s = systems();
    let r = analyze("'clear' in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p == "clear"));
    assert!(!r.clear);
    assert_eq!(r.systems.len(), 1);
    let r2 = analyze("`Some Guy\" tackled", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r2.pilots.iter().any(|p| p == "Some Guy"));
}

#[test]
fn name_with_trailing_number_isnt_a_count() {
    let s = systems();
    assert_eq!(analyze("8X6T-8 Malcolm 41", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
    assert_eq!(analyze("Adama 80 pls help", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
    // A lone trailing number after a system is too error-prone to be a count.
    assert_eq!(analyze("Rancer 5", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
}

#[test]
fn bare_numbers_need_a_qualifier() {
    let s = systems();
    let ships = ships_with(&[("Drake", 24698)]);
    let pos = |t: &str, ships: &std::collections::HashMap<String, (i64, String)>| {
        analyze(t, &s, ships, &noknown(), 1, "ch", "x").count
    };
    // Positive: a number qualified by +, x/X, a hostile keyword, or a ship counts.
    assert_eq!(pos("+3 in Rancer", &noships()), Some(3), "attached +N");
    assert_eq!(pos("3+ in Rancer", &noships()), Some(3), "attached N+");
    assert_eq!(pos("Rancer + 5", &noships()), Some(5), "+ before number, spaced");
    assert_eq!(pos("Rancer 5 +", &noships()), Some(5), "+ after number, spaced");
    assert_eq!(pos("x5 in Rancer", &noships()), Some(5), "x multiplier");
    assert_eq!(pos("X5 in Rancer", &noships()), Some(5), "X multiplier");
    assert_eq!(pos("5 reds in Rancer", &noships()), Some(5), "keyword after number");
    assert_eq!(pos("Rancer neuts 3", &noships()), Some(3), "keyword before number");
    assert_eq!(pos("hostiles 10 in Rancer", &noships()), Some(10), "hostile keyword");
    assert_eq!(pos("2 marauders in Rancer", &noships()), Some(2), "ship class");
    assert_eq!(pos("3 Drake in Rancer", &ships), Some(3), "known hull");
    assert_eq!(pos("2 Drakes in Rancer", &ships), Some(2), "plural hull");
    assert_eq!(pos("5 in system", &noships()), Some(5), "N in system");
    assert_eq!(pos("10 in local", &noships()), Some(10), "N in local");
    assert_eq!(pos("6 in sys", &noships()), Some(6), "N in sys");
    // Negative: a lone number, with no +/x/keyword/ship beside it, does not count.
    assert_eq!(pos("Rancer 5", &noships()), None, "lone trailing number");
    assert_eq!(pos("5 in Rancer", &noships()), None, "lone leading number");
    assert_eq!(pos("Rancer 5 gate", &noships()), None, "number between system and gate");
    assert_eq!(pos("camp in Rancer 8", &noships()), None, "stray number");
    assert_eq!(pos("3 Drake in Rancer", &noships()), None, "unknown word is not a ship");
}

#[test]
fn loose_runs_keep_short_name_parts() {
    let s = systems();
    let runs = loose_pilot_runs("Adama 80 Lopatich R", &noships(), &s);
    assert!(runs.iter().any(|r| r.contains("80")), "runs={:?}", runs);
    assert!(runs.iter().any(|r| r.split_whitespace().last() == Some("R")), "runs={:?}", runs);
    assert!(loose_pilot_runs("80 90", &noships(), &s).is_empty());
}

#[test]
fn system_detection_coverage() {
    let s = systems();
    let det = |m: &str| {
        let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
        (r.systems.iter().map(|x| x.name.clone()).collect::<Vec<String>>(), r.gates.clone())
    };
    assert_eq!(det("hostiles in Jita").0, vec!["Jita"]);
    assert_eq!(det("Jita").0, vec!["Jita"]);
    assert_eq!(det("5 reds Jita").0, vec!["Jita"]);
    assert_eq!(det("C-J6MT clear").0, vec!["C-J6MT"]);
    assert_eq!(det("Jita* hostiles").0, vec!["Jita"]);
    let (sysd, gates) = det("N3-JBX Uitra");
    assert_eq!(sysd, vec!["N3-JBX"]);
    assert!(gates.iter().any(|g| g == "Uitra"), "gates={gates:?}");
    assert!(det("on C-J gate").1.iter().any(|g| g == "C-J6MT"), "{:?}", det("on C-J gate"));
    assert_eq!(det("Sevra in Jita").0, vec!["Jita"]);
    assert!(det("hostiles incoming").0.is_empty());
    assert_eq!(det("c-j6mt clear").0, vec!["C-J6MT"]);
}

#[test]
fn detects_systems_count_and_flags() {
    let s = systems();

    let drake = ships_with(&[("Drake", 24698)]);
    let r = analyze("hostile in Rancer, 3 Drake +2", &s, &drake, &noknown(), 100, "ch", "Scout");
    assert_eq!(r.systems.len(), 1);
    assert_eq!(r.systems[0].name, "Rancer");
    assert_eq!(r.count, Some(5));
    assert!(!r.clear);

    assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x").clear);
    assert!(analyze("nv in Jita", &s, &noships(), &noknown(), 1, "ch", "x").no_visual);
    assert!(analyze("gate camp 1DQ1-A bubble up", &s, &noships(), &noknown(), 1, "ch", "x").camp);
    let gc = analyze("gatecamp in 1DQ1-A", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(gc.camp, "gatecamp should fire camp");
    assert!(gc.pilots.is_empty(), "gatecamp is not a pilot");
    assert!(analyze("https://zkillboard.com/kill/123/", &s, &noships(), &noknown(), 1, "ch", "x").killmail);
    assert!(analyze("cyno up in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").cyno);
    assert!(analyze("hotdropper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
    assert!(analyze("blops on scan Jita", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
    assert!(analyze("watch for hot drop", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
    assert!(!analyze("just a dropbear in Jita", &s, &noships(), &noknown(), 1, "ch", "x").dropper);
    assert!(analyze("wh in Jita k162", &s, &noships(), &noknown(), 1, "ch", "x").wormhole);
    assert!(analyze("ess being robbed", &s, &noships(), &noknown(), 1, "ch", "x").ess);
    assert!(analyze("skyhook theft Rancer", &s, &noships(), &noknown(), 1, "ch", "x").skyhook);
    for w in ["filament", "needlejack", "trace", "filaments", "needlejacks"] {
        let r = analyze(&format!("{w} in Rancer"), &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.filament, "{w} should set filament");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w} is a keyword, not a pilot");
    }
    assert!(analyze("clear in here", &s, &noships(), &noknown(), 1, "ch", "x").systems.is_empty());
}

#[test]
fn recognizes_battle_report_links_including_our_site() {
    let ours = extract_links("gf all https://eve-spai.com/br/abc123def nice fight");
    assert!(
        ours.iter().any(|l| l.kind == LinkKind::BattleReport && l.url.contains("eve-spai.com/br/")),
        "{ours:?}"
    );
    assert!(extract_links("https://br.evetools.org/br/xyz")
        .iter()
        .any(|l| l.kind == LinkKind::BattleReport));
    assert!(!extract_links("https://eve-spai.com/about").iter().any(|l| l.kind == LinkKind::BattleReport));
}

#[test]
fn pronoun_i_never_a_pilot() {
    let s = systems();
    let has_i = |names: &[String]| names.iter().any(|p| p.split_whitespace().any(|w| w == "I"));
    for txt in [
        "I think 5 reds in Jita",
        "I guess they left",
        "tackled one, I saw him warp Jita",
        "Rancer clear, I am going afk",
        "dunno where they went, I missed it",
        "I see a Sabre and I think a Loki",
        "i think reds incoming",
        "Bishopi I think he docked",
        "warp to I and hold",
    ] {
        let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "Spai");
        let resolved = esi_resolve(&r.pilots, &["Bishopi", "Sabre"]);
        assert!(!has_i(&resolved), "pronoun 'I' leaked as a pilot in {txt:?}: {resolved:?}");
    }
    let r = analyze("Bishopi I think he docked", &s, &noships(), &noknown(), 1, "ch", "Spai");
    let resolved = esi_resolve(&r.pilots, &["Bishopi"]);
    assert_eq!(resolved, vec!["Bishopi".to_string()], "glued pronoun: {resolved:?}");
}

fn sys_map(rows: &[(&str, &str, i64, f64)]) -> std::collections::HashMap<String, SystemInfo> {
    rows.iter()
        .map(|(k, n, id, sec)| {
            (k.to_string(), SystemInfo { id: *id, name: n.to_string(), security: *sec, constellation: String::new(), region: String::new(), faction: String::new() })
        })
        .collect()
}

#[test]
fn glued_oneword_handles_split_via_cache() {
    let s = Systems::new(sys_map(&[("9-ougj", "9-OUGJ", 30000454, -0.5)]), std::collections::HashMap::new());
    let known: std::collections::HashMap<String, i64> = [
        ("clol23".to_string(), 2124249172i64),
        ("rm712".to_string(), 2117556515),
        ("wenmg".to_string(), 2121075688),
    ]
    .into_iter()
    .collect();
    let plain = "clol23 MuskQAQ rm712 wenmg 9-OUGJ";
    let r = analyze(plain, &s, &noships(), &known, 1, "ch", "TreeBeard Elderling");
    let split = esi_resolve(&r.pilots, &["clol23", "MuskQAQ", "rm712", "wenmg"]);
    let lc: Vec<String> = split.iter().map(|p| p.to_lowercase()).collect();
    for want in ["clol23", "rm712", "wenmg", "muskqaq"] {
        assert!(lc.contains(&want.to_string()), "missing {want}: {:?}", split);
    }

    let known2: std::collections::HashMap<String, i64> =
        [("comet".to_string(), 90i64)].into_iter().collect();
    let r2 = analyze("Comet Rider in 9-OUGJ", &s, &noships(), &known2, 1, "ch", "x");
    let resolved = esi_resolve(&r2.pilots, &["Comet Rider"]);
    assert!(resolved.iter().any(|p| p.eq_ignore_ascii_case("Comet Rider")), "pilots: {resolved:?}");
    assert!(!resolved.iter().any(|p| p.eq_ignore_ascii_case("Rider")), "wrongly split: {resolved:?}");
}

#[test]
fn double_space_paste_recognises_pilots() {
    let s = Systems::new(
        sys_map(&[
            ("l-fm3p", "L-FM3P", 30000540, -0.5),
            ("9-ougj", "9-OUGJ", 30000454, -0.5),
            ("ypw-m4", "YPW-M4", 30000785, -0.5),
        ]),
        std::collections::HashMap::new(),
    );
    let lc = |r: &IntelReport| {
        let mut v: Vec<String> = r.pilots.iter().map(|p| p.to_lowercase()).collect();
        v.sort();
        v
    };

    let r = analyze("L-FM3P  Gliar  Mliarvis  Sliarhia", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(lc(&r), vec!["gliar", "mliarvis", "sliarhia"]);
    assert!(r.systems.iter().any(|d| d.name == "L-FM3P"), "system kept: {:?}", r.systems);

    let r = analyze("clol23  MuskQAQ  rm712  wenmg  9-OUGJ", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(lc(&r), vec!["clol23", "muskqaq", "rm712", "wenmg"]);

    let r = analyze("YPW-M4*  Boris95  BorisDread95  Destroyer95", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(lc(&r), vec!["boris95", "borisdread95", "destroyer95"]);
    assert!(r.systems.iter().any(|d| d.name == "YPW-M4"), "system: {:?}", r.systems);

    let r = analyze("L-FM3P  First Last  Second Guy", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(lc(&r), vec!["first last", "second guy"]);

    let r = analyze("L-FM3P    Gliar    Mliarvis", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(lc(&r), vec!["gliar", "mliarvis"]);

    let known: std::collections::HashMap<String, i64> =
        [("gliar".to_string(), 1i64), ("mliarvis".to_string(), 2), ("sliarhia".to_string(), 3)]
            .into_iter()
            .collect();
    let r = analyze("L-FM3P  Gliar  Mliarvis  Sliarhia", &s, &noships(), &known, 1, "ch", "x");
    assert_eq!(lc(&r), vec!["gliar", "mliarvis", "sliarhia"]);

    let ships: std::collections::HashMap<String, (i64, String)> =
        [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
    let r = analyze("L-FM3P  Gliar  Sabre", &s, &ships, &noknown(), 1, "ch", "x");
    assert_eq!(lc(&r), vec!["gliar"], "sabre must not be a pilot");
    assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "sabre is a ship: {:?}", r.ships);
}

#[test]
fn double_space_falls_back_on_prose_and_bad_grammar() {
    let s = systems();
    let pilots = |t: &str| {
        let mut v = analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").pilots;
        v.sort();
        v
    };
    assert!(
        analyze("rorqual  pointed in Jita", &s, &noships(), &noknown(), 1, "ch", "x").cap_tackled,
        "cap detection must survive a stray double space"
    );
    for t in [
        "reds  pointed in Jita",
        "they  warped off to Jita",
        "got him  tackled in Jita now",
        "Rancer  is clear now lads",
    ] {
        let resolved = esi_resolve(&pilots(t), &[]);
        assert!(resolved.is_empty(), "prose treated as paste for {t:?}: {resolved:?}");
    }
    for (dbl, sgl) in [
        ("reds  pointed in Jita", "reds pointed in Jita"),
        ("they  warped off to Jita", "they warped off to Jita"),
        ("lol  gg  wp", "lol gg wp"),
        ("he  said  hi", "he said hi"),
        ("idk  man  lol", "idk man lol"),
        ("u  see  them", "u see them"),
        ("ok  ok  sure", "ok ok sure"),
        ("cats  love  fish", "cats love fish"),
        ("nice  one  mate", "nice one mate"),
        ("Rancer  is clear now lads", "Rancer is clear now lads"),
    ] {
        assert_eq!(pilots(dbl), pilots(sgl), "double-space hint changed a non-paste parse: {dbl:?}");
    }
    let r = analyze("Rancer  Gliar  they all warped off already", &s, &noships(), &noknown(), 1, "ch", "x");
    let resolved = esi_resolve(&r.pilots, &["Gliar"]);
    assert_eq!(resolved, vec!["Gliar".to_string()], "prose tail leaked: {resolved:?}");
}

#[test]
fn lowercase_clear_rain_pilot_detected() {
    let s = systems();
    let r = analyze("Rancer clear rain nemesis on gate", &s, &noships(), &noknown(), 1, "ch", "Spai");
    assert!(r.pilots.iter().any(|p| p == "clear rain"), "clear rain not a pilot: {:?}", r.pilots);
    assert!(!r.clear, "pilot name 'clear rain' spoofed a clear status");
    assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "Spai").clear);
}

#[test]
fn clear_loses_to_threats() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> =
        [("nemesis".to_string(), (11377i64, "Nemesis".to_string()))].into_iter().collect();
    let r = analyze("Rancer hot dropper bubble clear rain nemesis", &s, &ships, &noknown(), 1, "ch", "Spai");
    assert!(!r.clear, "clear should lose to threats");
    let r2 = analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "Spai");
    assert!(r2.clear, "pure clear lost");
    let r3 = analyze("got Clear Rain on gate", &s, &noships(), &noknown(), 1, "ch", "Spai");
    let (pilots, _, _) = resolve_report(&r3, &["Clear Rain"], &s);
    assert!(pilots.iter().any(|p| p == "Clear Rain"), "name split: {pilots:?}");
    assert!(!r3.clear, "name 'Clear Rain' spoofed clear");
}

#[test]
fn different_main_systems_do_not_amend_on_shared_pilot() {
    let by_name = [("rz-ti6", "RZ-TI6", 30000834i64, -0.4), ("fx4l-2", "FX4L-2", 30000835, -0.4)]
        .into_iter()
        .map(|(k, n, id, sec)| {
            (
                k.to_string(),
                SystemInfo {
                    id,
                    name: n.to_string(),
                    security: sec,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            )
        })
        .collect();
    let s = Systems::new(by_name, HashMap::new());
    let mut state = IntelState::default();
    let mut a =
        analyze("RZ-TI6  Cloister Cobon-Han", &s, &noships(), &noknown(), 100, "ch", "BiGsnorlax");
    apply_resolution(&mut a, &["Cloister Cobon-Han"], &s);
    assert_eq!(a.primary_system().map(|d| d.id), Some(30000834), "msg1 system");
    state.push(a);
    let mut b = analyze(
        "Cloister Cobon-Han  FX4L-2 imucs",
        &s,
        &noships(),
        &noknown(),
        130,
        "ch",
        "utsumi ota",
    );
    apply_resolution(&mut b, &["Cloister Cobon-Han"], &s);
    assert_eq!(b.primary_system().map(|d| d.id), Some(30000835), "msg2 system");
    assert!(!state.try_amend(&b, 60, &s), "different main systems must not amend");
    assert_eq!(state.reports.len(), 1, "the two sightings stay separate");
}

#[test]
fn pasted_name_sharing_a_word_with_a_system_still_resolves() {
    let by_name = [("moh", "Moh", 30000750i64, 0.3), ("4ds-oi", "4DS-OI", 30000749, -0.5)]
        .into_iter()
        .map(|(key, name, id, sec)| {
            (
                key.to_string(),
                SystemInfo {
                    id,
                    name: name.to_string(),
                    security: sec,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            )
        })
        .collect();
    let s = Systems::new(by_name, HashMap::new());
    for t in ["4DS-OI  Moh Lut nv", "Moh Lut  4DS-OI nv core probes out"] {
        let r = analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Moh Lut"), "{t}: pilots={:?}", r.pilots);
        assert!(r.systems.iter().any(|d| d.name == "4DS-OI"), "{t}: systems={:?}", r.systems);
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Moh")),
            "{t}: 'Moh' leaked as a pilot: {:?}",
            r.pilots
        );
    }
}

#[test]
fn pilot_name_with_keyword_words_and_trailing_ship_note() {
    let s = systems();
    let ships = ships_with(&[("Prospect", 33468)]);
    let known: std::collections::HashMap<String, i64> =
        [("roadman highsec cynolighter".to_string(), 1i64)].into_iter().collect();
    for t in ["DUO-51  Roadman HighSec CynoLighter likely prospect"] {
        let r = analyze(t, &s, &ships, &known, 1, "ch", "Rage Starscythe");
        assert!(
            r.pilots.iter().any(|p| p == "Roadman HighSec CynoLighter"),
            "{t}: pilots={:?}",
            r.pilots
        );
        assert!(
            !r.pilots.iter().any(|p| {
                p.to_lowercase().contains("likely") || p.to_lowercase().contains("prospect")
            }),
            "{t}: leaked prose/ship into a pilot: {:?}",
            r.pilots
        );
        assert!(r.ships.iter().any(|sh| sh.name == "Prospect"), "{t}: ships={:?}", r.ships);
    }
}

#[test]
fn gate_variants_nameless_gate_and_solo() {
    let s = systems();
    let r = analyze("1DQ1-A camp gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.camp, "camp keyword");
    assert!(r.gates.iter().any(|g| g.is_empty()), "nameless gate expected: {:?}", r.gates);
    assert!(
        !r.gates.iter().any(|g| g.eq_ignore_ascii_case("camp")),
        "camp captured as gate: {:?}",
        r.gates
    );
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("camp")), "camp pilot: {:?}", r.pilots);
    for kw in ["gates", "stargate", "stargates"] {
        let r = analyze(&format!("Rancer {kw} clear"), &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(kw)), "{kw} pilot: {:?}", r.pilots);
    }
    let r = analyze("Rancer solo Sabre", &s, &ships_with(&[("Sabre", 22456)]), &noknown(), 1, "ch", "x");
    assert_eq!(r.count, Some(1), "solo count: {:?}", r.count);
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("solo")), "solo pilot: {:?}", r.pilots);
}

#[test]
fn nvm_camper_whoever_are_not_pilots() {
    let s = systems();
    let r = analyze("Rancer nvm", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("nvm")), "nvm: {:?}", r.pilots);
    let r = analyze("Rancer campers on gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.camp, "campers should fire camp");
    assert!(
        !r.pilots.iter().any(|p| p.to_lowercase().contains("camper")),
        "camper as pilot: {:?}",
        r.pilots
    );
    let r = analyze("Whoever is tackling in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("whoever")),
        "whoever as pilot: {:?}",
        r.pilots
    );
}

#[test]
fn pluralised_multiword_and_ies_hulls() {
    let s = systems();
    let ships = ships_with(&[("Osprey Navy Issue", 29990), ("Osprey", 620), ("Harpy", 11381)]);
    for t in ["osprey navys in Rancer", "osprey navies in Rancer"] {
        let r = analyze(t, &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Osprey Navy Issue"), "{t}: {:?}", r.ships);
        assert!(
            !r.pilots.iter().any(|p| {
                p.eq_ignore_ascii_case("navys") || p.eq_ignore_ascii_case("navies")
            }),
            "{t} pilot: {:?}",
            r.pilots
        );
    }
    let r = analyze("harpies on grid", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Harpy"), "{:?}", r.ships);
    let ships2 = ships_with(&[("Ares", 11196), ("Bellicose", 29344)]);
    let r = analyze("areses on grid", &s, &ships2, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Ares"), "areses: {:?}", r.ships);
    let r = analyze("bellicoses on grid", &s, &ships2, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Bellicose"), "bellicoses: {:?}", r.ships);
}

#[test]
fn plural_sabres_and_on_grid_not_pilots() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> =
        [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
    let r = analyze("Rancer 5 Sabres on grid", &s, &ships, &noknown(), 1, "ch", "Spai");
    assert!(!r.pilots.iter().any(|p| p.to_lowercase().contains("sabres")), "sabres as pilot: {:?}", r.pilots);
    assert!(!r.pilots.iter().any(|p| p.to_lowercase().contains("grid")), "grid as pilot: {:?}", r.pilots);
    assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "Sabre ship missing: {:?}", r.ships);
}

#[test]
fn multiword_ship_is_not_a_pilot() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [(
        "exequror navy issue".to_string(),
        (29344i64, "Exequror Navy Issue".to_string()),
    )]
    .into_iter()
    .collect();
    let r = analyze("78-0R6 Exequror Navy Issue", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Exequror Navy Issue"));
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Exequror Navy Issue")));
}

#[test]
fn pilot_name_keeps_alt_suffix() {
    let s = systems();
    let r = analyze("hostiles Nine -L in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["Nine -L"], &s);
    assert!(pilots.iter().any(|p| p == "Nine -L"), "pilots: {pilots:?}");
    let r2 = analyze("Nine -3", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r2, &["Nine -3"], &s);
    assert!(pilots.iter().any(|p| p == "Nine -3"), "pilots: {pilots:?}");
}

#[test]
fn multiword_ship_via_known_cache_is_not_a_pilot() {
    let s = systems();
    let ships: std::collections::HashMap<String, (i64, String)> = [(
        "harbinger navy issue".to_string(),
        (24692i64, "Harbinger Navy Issue".to_string()),
    )]
    .into_iter()
    .collect();
    let known: std::collections::HashMap<String, i64> =
        [("harbinger navy issue".to_string(), 1i64)].into_iter().collect();
    let r = analyze("J5A-IX Harbinger Navy Issue", &s, &ships, &known, 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Harbinger Navy Issue"));
    assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Harbinger Navy Issue")));
}

#[test]
fn lowercase_family_name_recognised() {
    let s = systems();
    let r = analyze("78-0R6 Psychopathic beemaster", &s, &noships(), &noknown(), 1, "ch", "x");
    let (pilots, _, _) = resolve_report(&r, &["Psychopathic beemaster"], &s);
    assert!(pilots.iter().any(|p| p == "Psychopathic beemaster"), "{pilots:?}");
}

#[test]
fn numbered_name_with_system_prefix_plain_text() {
    let s = systems();
    let r = analyze("SV5-8N Amarr slave 3424", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p == "Amarr slave 3424"), "pilots: {:?}", r.pilots);
    assert!(!r.systems.iter().any(|d| d.name == "Amarr"), "systems: {:?}", r.systems);
    assert!(r.systems.iter().any(|d| d.name == "SV5-8N"));
}

#[test]
fn detects_wormhole_code() {
    let s = systems();
    let r = analyze("Rancer K162 just appeared", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(r.wormhole, "wormhole flag");
    assert_eq!(r.wh_type.as_deref(), Some("K162"));
    assert!(r.systems.iter().any(|d| d.name == "Rancer"));
    let r2 = analyze("Jita N968 sig", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r2.wh_type.as_deref(), Some("N968"));
    let r3 = analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(!r3.wormhole);
    assert!(r3.wh_type.is_none());
}

#[test]
fn neighbour_second_system_becomes_gate() {
    let by_name: std::collections::HashMap<String, SystemInfo> = [
        ("rancer", "Rancer", 1, 0.4),
        ("jita", "Jita", 2, 0.9),
    ]
    .into_iter()
    .map(|(k, n, id, sec)| {
        (
            k.to_string(),
            SystemInfo {
                id,
                name: n.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        )
    })
    .collect();
    let adj = std::collections::HashMap::from([(1i64, vec![2i64]), (2, vec![1])]);
    let s = Systems::new(by_name, adj);
    let r = analyze("hostiles in Rancer heading Jita", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["Rancer"]);
    assert_eq!(r.gates.first().map(|s| s.as_str()), Some("Jita"));
}

#[test]
fn non_adjacent_second_system_is_not_a_gate() {
    let by_name: std::collections::HashMap<String, SystemInfo> = [
        ("r959-u", "R959-U", 1, -0.2),
        ("agaullores", "Agaullores", 2, 0.3),
    ]
    .into_iter()
    .map(|(k, n, id, sec)| {
        (
            k.to_string(),
            SystemInfo {
                id,
                name: n.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        )
    })
    .collect();
    let adj = std::collections::HashMap::from([(1i64, vec![99]), (2, vec![99])]);
    let s = Systems::new(by_name, adj);
    let r = analyze("R959-U WH to Agaullores", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["R959-U"]);
    assert!(r.gates.is_empty(), "non-adjacent WH destination wrongly demoted to a gate: {:?}", r.gates);
}

#[test]
fn lowercase_codes_vs_hyphenated_names() {
    assert!(looks_like_system_code("c-j"));
    assert!(looks_like_system_code("4m-"));
    assert!(looks_like_system_code("1dq1-a"));
    assert!(looks_like_system_code("C-J6MT"));
    assert!(!looks_like_system_code("Jean-Luc"));
    assert!(!looks_like_system_code("Mary-Jo"));
}

#[test]
fn lowercase_code_gate_is_not_a_pilot() {
    let by_name: std::collections::HashMap<String, SystemInfo> = [
        ("c-j6mt", "C-J6MT", 5, -0.6),
        ("c-j7cr", "C-J7CR", 6, -0.5),
        ("rancer", "Rancer", 1, 0.4),
    ]
    .into_iter()
    .map(|(k, n, id, sec)| {
        (
            k.to_string(),
            SystemInfo {
                id,
                name: n.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        )
    })
    .collect();
    let adj = std::collections::HashMap::from([(1i64, vec![5i64]), (5, vec![1])]);
    let s = Systems::new(by_name, adj);
    let known = std::collections::HashMap::from([("c-j".to_string(), 999i64)]);
    let r = analyze("Rancer c-j gate camped", &s, &noships(), &known, 1, "ch", "x");
    assert!(
        !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("c-j")),
        "c-j is the gate's system, not a pilot: {:?}",
        r.pilots
    );
    assert_eq!(r.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
}

#[test]
fn negation_gate_abbrev_and_status() {
    let s = systems();
    let r = analyze(
        "C-J6MT YPW gate clear,no bubble,where the neuts went?",
        &s,
        &noships(),
        &noknown(),
        1,
        "ch",
        "x",
    );
    assert!(!r.bubble, "negated bubble");
    assert_eq!(r.gates.first().map(|s| s.as_str()), Some("YPW-M2"));
    let q = analyze("status in Rancer?", &s, &noships(), &noknown(), 1, "ch", "x");
    assert!(q.status);
    assert!(!q.pilots.iter().any(|p| p.eq_ignore_ascii_case("status")));
}

#[test]
fn lowercase_english_word_dropped_but_names_and_multiword_kept() {
    let s = systems();
    let known: std::collections::HashMap<String, i64> =
        [("carpet".to_string(), 100i64), ("silent hunter".to_string(), 200i64)]
            .into_iter()
            .collect();
    let low = analyze("carpet in Rancer", &s, &noships(), &known, 1, "ch", "x");
    assert!(
        !low.pilots.iter().any(|p| p.eq_ignore_ascii_case("carpet")),
        "lowercase word should be dropped, pilots={:?}",
        low.pilots
    );
    let cap = analyze("Carpet in Rancer", &s, &noships(), &known, 1, "ch", "x");
    assert!(
        cap.pilots.iter().any(|p| p.eq_ignore_ascii_case("carpet")),
        "Capitalised name should be kept, pilots={:?}",
        cap.pilots
    );
    let multi = analyze("silent hunter in Rancer", &s, &noships(), &known, 1, "ch", "x");
    assert!(
        multi.pilots.iter().any(|p| p.eq_ignore_ascii_case("silent hunter")),
        "multi-word lowercase run should still be tested, pilots={:?}",
        multi.pilots
    );
}

#[test]
fn gate_resolves_neighbour_prefix() {
    use std::collections::HashMap;
    let by_name = [
        ("c-j6mt", "C-J6MT", 5i64, -0.6),
        ("5e-cfl", "5E-CFL", 10, -0.5),
        ("sv5-8n", "SV5-8N", 9, -0.4),
    ]
    .into_iter()
    .map(|(k, n, id, sec)| {
        (
            k.to_string(),
            SystemInfo {
                id,
                name: n.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        )
    })
    .collect();
    let adj = HashMap::from([(5i64, vec![10i64, 9]), (10, vec![5]), (9, vec![5])]);
    let s = Systems::new(by_name, adj);
    let r = analyze("C-J6MT 5e gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.gates.first().map(|s| s.as_str()), Some("5E-CFL"));
    let r2 = analyze("C-J6MT sv gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("SV5-8N"));
}

#[test]
fn gate_disambiguates_abbrev_via_context() {
    use std::collections::HashMap;
    let by_name = [
        ("d-pnsn", "D-PNSN", 1i64, -0.4),
        ("c-j6mt", "C-J6MT", 2, -0.6),
        ("c-jeez", "C-JEEZ", 3, -0.5),
    ]
    .into_iter()
    .map(|(k, n, id, sec)| {
        (
            k.to_string(),
            SystemInfo {
                id,
                name: n.to_string(),
                security: sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        )
    })
    .collect();
    let adj = HashMap::from([(1i64, vec![2i64]), (2, vec![1])]);
    let s = Systems::new(by_name, adj);
    let r = analyze("D-PNSN C-J gate", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
    let r2 = analyze_ctx("C-J gate", &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
    assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
}

#[test]
fn detects_cap_tackled_variations() {
    let s = systems();
    let cap = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").cap_tackled;
    assert!(cap("Rancer cap tackled"));
    assert!(cap("rorqual  pointed in Jita"));
    assert!(cap("dread scrammed on gate"));
    assert!(cap("carrier takled"));
    assert!(cap("super got scram"));
    assert!(!cap("cap stable"));
    assert!(!cap("tackled a frigate"));
}

#[test]
fn ess_time_ignores_isk_amount() {
    let s = systems();
    let r = analyze("TPG-DD ESS 5 min 77m bank", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.ess_time.as_deref(), Some("5m"));
    let r2 = analyze("ESS reserve 30 min", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r2.ess_time.as_deref(), Some("30m"));
    let r3 = analyze("ESS robbed 30m", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r3.ess_time, None);
    let r4 = analyze("ESS robbed 30seconds left", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r4.ess_time.as_deref(), Some("30s"));
    assert!(
        !r4.pilots.iter().any(|p| p.to_lowercase().contains("30seconds")),
        "time token leaked as a pilot: {:?}",
        r4.pilots
    );
}

#[test]
fn sums_separate_hostile_groups() {
    let s = systems();
    let r = analyze("PDF-3Z 7 red; 1 neut", &s, &noships(), &noknown(), 1, "ch", "x");
    assert_eq!(r.count, Some(8));
}

#[test]
fn detects_gate_and_abbreviated_systems() {
    let s = systems();
    let r = analyze("C-J +20 on 78- gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
    assert_eq!(r.count, Some(20));
    assert_eq!(r.gates.first().map(|s| s.as_str()), Some("78-AAA"));
    assert_eq!(
        r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["C-J6MT"],
    );

    let r2 = analyze("20 reds on 78 gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
    assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("78-AAA"));
    assert_eq!(r2.count, Some(20));
}

#[test]
fn clear_outdates_prior_sighting_but_not_later_ones() {
    let s = systems();
    let mut st = IntelState::default();
    let prior = analyze("hostile in Rancer", &s, &noships(), &noknown(), 100, "ch", "A");
    let clear = analyze("Rancer clear", &s, &noships(), &noknown(), 112, "ch", "B");
    let later = analyze("hostile back in Rancer", &s, &noships(), &noknown(), 120, "ch", "C");
    st.push(prior.clone());
    st.push(clear.clone());
    st.push(later.clone());

    assert_eq!(st.reports.len(), 3);
    assert!(st.is_stale(&prior));
    assert!(!st.is_stale(&clear));
    assert!(!st.is_stale(&later));
}

fn systems_with(extra: &[(&str, &str, i64, f64)]) -> Systems {
    let mut by_name: std::collections::HashMap<String, SystemInfo> = std::collections::HashMap::new();
    for (key, name, id, sec) in extra {
        by_name.insert(
            key.to_string(),
            SystemInfo {
                id: *id,
                name: name.to_string(),
                security: *sec,
                constellation: String::new(),
                region: String::new(),
                faction: String::new(),
            },
        );
    }
    Systems::new(by_name, HashMap::new())
}

#[test]
fn parse_isk_handles_mio_million_abbreviation() {
    assert_eq!(parse_isk("ess 346mio", true), Some(346_000_000));
    assert_eq!(parse_isk("346mio", false), None);
    assert_eq!(parse_isk("ess worth 120 mio", true), Some(120_000_000));
    assert_eq!(parse_isk("ess 50mio.", true), Some(50_000_000));
    assert_eq!(parse_isk("loot 750m", false), None);
    assert_eq!(parse_isk("ess hostiles in 4M-HGW", true), None);
}

#[test]
fn htg0_is_a_pilot_mskr1_stays_a_system() {
    let s = systems_with(&[("mskr-1", "MSKR-1", 99, -0.5)]);
    let ships = ships_with(&[("Gnosis", 3756), ("Slasher", 585)]);
    let r = analyze(
        "MSKR-1 Htg-0 +5 gnosis 3x, Slasher, ESS 346mio",
        &s, &ships, &noknown(), 1, "ch", "Duke Dekker",
    );
    assert!(r.ships.iter().any(|sh| sh.name == "Gnosis"), "ships={:?}", r.ships);
    assert!(r.ships.iter().any(|sh| sh.name == "Slasher"), "ships={:?}", r.ships);
    assert!(r.ess, "ESS flag should fire: {:?}", r.text);
    assert_eq!(r.isk, Some(346_000_000), "isk={:?}", r.isk);
    assert!(!has_pilot_token(&r.pilots, "346mio"), "amount leaked as pilot: {:?}", r.pilots);
    assert!(proposed(&r.pilots, "Htg-0"), "Htg-0 not proposed: {:?}", r.pilots);
    let (pilots, sysd, gates) = resolve_report(&r, &["Htg-0"], &s);
    assert_eq!(pilots, vec!["Htg-0".to_string()], "resolved pilots={pilots:?}");
    assert_eq!(sysd, vec!["MSKR-1".to_string()], "resolved systems={sysd:?}");
    assert!(gates.is_empty(), "gates={gates:?}");
}

#[test]
fn code_pattern_name_without_real_system_is_a_pilot() {
    for t in ["Htg-0", "htg-0", "HTG-0", "MSKR-1", "Zzz-9", "zzz-9"] {
        assert!(looks_like_system_code(t), "{t} should match the code pattern");
    }
    assert!(!looks_like_system_code("Jean-Luc"), "a long-segment name is not a code");
    let s = systems_with(&[("mskr-1", "MSKR-1", 99, -0.5)]);
    let known: std::collections::HashMap<String, i64> =
        [("zzz-9".to_string(), 42i64), ("mskr-1".to_string(), 7i64)].into_iter().collect();
    let r = analyze("Zzz-9 MSKR-1 tackled", &s, &noships(), &known, 1, "ch", "x");
    assert!(proposed(&r.pilots, "Zzz-9"), "code-shaped name not a pilot: {:?}", r.pilots);
    assert!(!has_pilot_token(&r.pilots, "MSKR-1"), "system code leaked as pilot: {:?}", r.pilots);
    assert!(r.systems.iter().any(|d| d.name == "MSKR-1"), "MSKR-1 not the system: {:?}", r.systems);
}

#[test]
fn pilot_word_that_is_a_hull_is_not_also_a_ship() {
    let s = systems();
    let ships = ships_with(&[("Worm", 17619)]);
    let known: std::collections::HashMap<String, i64> =
        [("bovine worm".to_string(), 1i64)].into_iter().collect();
    let r = analyze("bovine worm", &s, &ships, &known, 1, "ch", "x");
    assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("bovine worm")), "pilots={:?}", r.pilots);
    assert!(!r.ships.iter().any(|sh| sh.name == "Worm"), "Worm inside pilot span leaked: {:?}", r.ships);
    let ctrl = analyze("Bob in a Worm", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(ctrl.ships.iter().any(|sh| sh.name == "Worm"), "control Worm missing: {:?}", ctrl.ships);
}

#[test]
fn noise_punctuation_between_tokens_does_not_break_detection() {
    assert_eq!(tokenize("Slasher, ESS* 346mio"), vec!["Slasher", "ESS", "346mio"]);
    assert_eq!(tokenize("O'Brien I-Pustelga Htg-0"), vec!["O'Brien", "I-Pustelga", "Htg-0"]);
    let s = systems();
    let ships = ships_with(&[("Slasher", 585)]);
    let r = analyze("Slasher*, ESS 60mio", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Slasher"), "ships={:?}", r.ships);
    assert!(r.ess, "ESS flag should fire through the punctuation: {:?}", r.text);
    assert_eq!(r.isk, Some(60_000_000), "isk={:?}", r.isk);
}

fn ships_with(names: &[(&str, i64)]) -> std::collections::HashMap<String, (i64, String)> {
    names
        .iter()
        .map(|(n, id)| (n.to_lowercase(), (*id, n.to_string())))
        .collect()
}

#[test]
fn wyf8_kill_list_keeps_all_five_pilots() {
    let s = systems_with(&[("wyf8-8", "WYF8-8", 30002126, -0.4)]);
    let reals =
        ["BoneChilling Chelien", "Gonzilla", "Krombopulous Jaynara", "Rollboy", "ShadowClown-Z"];
    let known: std::collections::HashMap<String, i64> =
        reals.iter().enumerate().map(|(i, r)| (r.to_lowercase(), i as i64 + 1)).collect();
    for line in [
        "BoneChilling Chelien  Gonzilla  Krombopulous Jaynara  Rollboy  ShadowClown-Z  WYF8-8",
        "BoneChilling Chelien Gonzilla Krombopulous Jaynara Rollboy ShadowClown-Z WYF8-8",
    ] {
        let r = analyze(line, &s, &noships(), &known, 1, "ch", "Volltz");
        let (pilots, sysd, _gates) = resolve_report(&r, &reals, &s);
        for name in reals {
            assert!(
                pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
                "{line:?}: pilot {name:?} dropped: {pilots:?}",
            );
        }
        assert_eq!(pilots.len(), 5, "{line:?}: expected exactly five pilots: {pilots:?}");
        assert_eq!(sysd, vec!["WYF8-8".to_string()], "{line:?}: system: {sysd:?}");
    }
}

#[test]
fn wyf8_amend_unions_pilots() {
    let s = systems_with(&[("wyf8-8", "WYF8-8", 30002126, -0.4)]);
    let sys = vec![DetectedSystem { id: 30002126, name: "WYF8-8".into(), security: -0.4 }];
    let mut state = IntelState::default();
    state.push(IntelReport {
        pilots: vec!["Krombopulous Jaynara".into(), "Rollboy".into()],
        systems: sys.clone(),
        reporter: "Volltz".into(),
        received: 1,
        text: "Krombopulous Jaynara Rollboy WYF8-8".into(),
        ..Default::default()
    });
    let second = IntelReport {
        pilots: vec!["BoneChilling Chelien".into(), "Gonzilla".into(), "ShadowClown-Z".into()],
        systems: sys,
        reporter: "Volltz".into(),
        received: 10,
        text: "BoneChilling Chelien Gonzilla ShadowClown-Z WYF8-8".into(),
        ..Default::default()
    };
    assert!(state.try_amend(&second, 60, &s), "second WYF8-8 message should amend the first");
    for name in
        ["Krombopulous Jaynara", "Rollboy", "BoneChilling Chelien", "Gonzilla", "ShadowClown-Z"]
    {
        assert!(
            state.reports[0].pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
            "amend dropped {name:?}: {:?}",
            state.reports[0].pilots
        );
    }
}

#[test]
fn reverse_amend_revives_systemless_content() {
    let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
    let ships = ships_with(&[("Rifter", 587), ("Punisher", 597)]);
    let mut state = IntelState::default();

    let orphan = analyze("Rifter Punisher +5", &s, &ships, &noknown(), 100, "intel", "Scout");
    assert!(orphan.systems.is_empty(), "orphan should have no system: {:?}", orphan.systems);
    assert!(!orphan.ships.is_empty(), "orphan should carry ships: {:?}", orphan.ships);
    assert_eq!(orphan.count, Some(5), "orphan count: {:?}", orphan.count);
    state.stash_orphan(orphan, 60, 100);

    let mut sysmsg = analyze("FN0-QS", &s, &ships, &noknown(), 105, "intel", "Scout");
    assert_eq!(state.reverse_amend(&mut sysmsg, 60), 1, "one orphan should merge");
    assert!(sysmsg.systems.iter().any(|d| d.name == "FN0-QS"), "system lost: {:?}", sysmsg.systems);
    for hull in ["Rifter", "Punisher"] {
        assert!(sysmsg.ships.iter().any(|sh| sh.name == hull), "ship {hull} missing: {:?}", sysmsg.ships);
    }
    assert_eq!(sysmsg.count, Some(5), "count not carried: {:?}", sysmsg.count);
    assert!(state.orphans.is_empty(), "orphan buffer not emptied: {:?}", state.orphans.len());
}

#[test]
fn reverse_amend_ors_status_flags() {
    let _s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
    let mut state = IntelState::default();
    let orphan = IntelReport {
        reporter: "Scout".into(),
        channel: "intel".into(),
        received: 100,
        text: "Loki bubbled cyno nv".into(),
        ships: vec![DetectedShip { id: 29990, name: "Loki".into() }],
        bubble: true,
        cyno: true,
        no_visual: true,
        tackled: true,
        ..Default::default()
    };
    state.stash_orphan(orphan, 60, 100);
    let mut sysmsg = IntelReport {
        reporter: "Scout".into(),
        channel: "intel".into(),
        received: 130,
        text: "FN0-QS".into(),
        systems: vec![DetectedSystem { id: 30004111, name: "FN0-QS".into(), security: -0.4 }],
        ..Default::default()
    };
    assert_eq!(state.reverse_amend(&mut sysmsg, 60), 1);
    assert!(sysmsg.bubble && sysmsg.cyno && sysmsg.no_visual && sysmsg.tackled, "flags not OR-ed");
    assert!(sysmsg.ships.iter().any(|sh| sh.name == "Loki"), "ship missing: {:?}", sysmsg.ships);
}

#[test]
fn reverse_amend_ignores_stale_orphan() {
    let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
    let ships = ships_with(&[("Rifter", 587)]);
    let mut state = IntelState::default();
    let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
    state.stash_orphan(orphan, 60, 100);
    let mut sysmsg = analyze("FN0-QS", &s, &ships, &noknown(), 161, "intel", "Scout");
    assert_eq!(state.reverse_amend(&mut sysmsg, 60), 0, "stale orphan should not merge");
    assert!(sysmsg.ships.is_empty(), "stale ship leaked: {:?}", sysmsg.ships);
    assert!(state.orphans.is_empty(), "stale orphan should be dropped: {:?}", state.orphans.len());
}

#[test]
fn reverse_amend_only_same_reporter_and_channel() {
    let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
    let ships = ships_with(&[("Rifter", 587)]);
    let mut state = IntelState::default();
    let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
    state.stash_orphan(orphan, 60, 100);

    let mut other_rep = analyze("FN0-QS", &s, &ships, &noknown(), 110, "intel", "SomeoneElse");
    assert_eq!(state.reverse_amend(&mut other_rep, 60), 0, "different reporter merged");
    assert!(other_rep.ships.is_empty(), "leaked into other reporter: {:?}", other_rep.ships);
    assert_eq!(state.orphans.len(), 1, "fresh non-matching orphan should be kept");

    let mut other_chan = analyze("FN0-QS", &s, &ships, &noknown(), 115, "other", "Scout");
    assert_eq!(state.reverse_amend(&mut other_chan, 60), 0, "different channel merged");
    assert_eq!(state.orphans.len(), 1, "orphan should still be kept");

    let mut mine = analyze("FN0-QS", &s, &ships, &noknown(), 120, "intel", "Scout");
    assert_eq!(state.reverse_amend(&mut mine, 60), 1, "same reporter/channel should merge");
    assert!(mine.ships.iter().any(|sh| sh.name == "Rifter"), "ship missing: {:?}", mine.ships);
    assert!(state.orphans.is_empty(), "orphan should be consumed");
}

#[test]
fn reverse_amend_skips_clear_report() {
    let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
    let ships = ships_with(&[("Rifter", 587)]);
    let mut state = IntelState::default();
    let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
    state.stash_orphan(orphan, 60, 100);
    let mut clr = analyze("FN0-QS clear", &s, &ships, &noknown(), 110, "intel", "Scout");
    assert!(clr.clear, "should be a clear: {:?}", clr.text);
    assert_eq!(state.reverse_amend(&mut clr, 60), 0, "clear must not reverse-amend");
    assert!(clr.ships.is_empty(), "orphan leaked into clear: {:?}", clr.ships);
    assert_eq!(state.orphans.len(), 1, "orphan should be kept after a clear: {:?}", state.orphans.len());
}

#[test]
fn stash_orphan_prunes_stale() {
    let mut state = IntelState::default();
    let mk = |t: i64| IntelReport {
        reporter: "Scout".into(),
        channel: "intel".into(),
        received: t,
        ships: vec![DetectedShip { id: 587, name: "Rifter".into() }],
        ..Default::default()
    };
    state.stash_orphan(mk(100), 60, 100);
    state.stash_orphan(mk(120), 60, 120);
    state.stash_orphan(mk(200), 60, 200);
    assert_eq!(state.orphans.len(), 1, "only the fresh orphan should remain");
    assert_eq!(state.orphans[0].received, 200);
}

#[test]
fn thera_wormhole_ref_does_not_override_primary() {
    let by_name = [
        ("rancer", "Rancer", 1i64, 0.4),
        ("jita", "Jita", 2, 0.9),
        ("thera", "Thera", 31000005, -1.0),
        ("c-j6mt", "C-J6MT", 5, -0.6),
    ]
    .into_iter()
    .map(|(k, n, id, sec)| {
        (k.to_string(), SystemInfo { id, name: n.to_string(), security: sec, constellation: String::new(), region: String::new(), faction: String::new() })
    })
    .collect();
    let adjacency = [(1i64, vec![2i64]), (2, vec![1])].into_iter().collect();
    let s = Systems::new(by_name, adjacency);
    for (line, primary) in [
        ("Rancer Thera hole", "Rancer"),
        ("Jita Thera hole", "Jita"),
        ("Rancer wh to Thera", "Rancer"),
        ("3 reds Rancer Thera hole", "Rancer"),
        ("C-J Thera hole", "C-J6MT"),
        ("Thera hole Rancer", "Rancer"),
    ] {
        let r = analyze(line, &s, &noships(), &noknown(), 1, "ch", "x");
        let (_p, sysd, gates) = resolve_report(&r, &[], &s);
        assert_eq!(sysd, vec![primary.to_string()], "{line:?}: primary system: {sysd:?}");
        assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Thera")), "{line:?}: Thera became a gate: {gates:?}");
        assert!(matches!(r.wh_dest, Some(crate::wormholes::DestClass::Thera)), "{line:?}: wh_dest: {:?}", r.wh_dest);
    }
    let r = analyze("hostiles in Thera camped", &s, &noships(), &noknown(), 1, "ch", "x");
    let (_p, sysd, _g) = resolve_report(&r, &[], &s);
    assert_eq!(sysd, vec!["Thera".to_string()], "genuine Thera location: {sysd:?}");
}

#[test]
fn system_code_matching_inactive_char_is_the_system() {
    let s = systems_with(&[("ualx", "UALX", 30009003, -0.5), ("dt-", "DT-", 30009004, -0.5)]);
    let known: std::collections::HashMap<String, i64> =
        [("ualx".to_string(), 9i64)].into_iter().collect();
    for denied_ualx in [false, true] {
        let denied: std::collections::HashSet<String> =
            if denied_ualx { ["ualx".to_string()].into() } else { Default::default() };
        let r = analyze_ctx(
            "DT- gate to UALX camped", &s, &noships(), &known, 1, "ch", "x", None, &[], &denied,
        );
        assert!(!has_pilot_token(&r.pilots, "UALX"), "UALX leaked as a pilot: {:?}", r.pilots);
        assert!(r.systems.iter().any(|d| d.name == "UALX"), "UALX not the system: {:?}", r.systems);
        assert!(r.camp, "camp keyword should still fire: {:?}", r.text);
    }
}

#[test]
fn x50em_count_and_ships_after_pilot_all_detected() {
    let s = systems_with(&[("x5-0em", "X5-0EM", 30000777, -0.5)]);
    let ships: std::collections::HashMap<String, (i64, String)> = [
        ("kiki".to_string(), (49711i64, "Kikimora".to_string())),
        ("flycatcher".to_string(), (22464i64, "Flycatcher".to_string())),
        ("kirin".to_string(), (37460i64, "Kirin".to_string())),
    ]
    .into_iter()
    .collect();
    let r = analyze("X5-0EM  dix otto  +12 kikis flycatcher kirin", &s, &ships, &noknown(), 1, "ch", "st0rkant");
    for hull in ["Kikimora", "Flycatcher", "Kirin"] {
        assert!(r.ships.iter().any(|sh| sh.name == hull), "hull {hull} missing: {:?}", r.ships);
    }
    assert_eq!(r.count, Some(13), "count: {:?}", r.count);
    let (pilots, sysd, _g) = resolve_report(&r, &["dix otto"], &s);
    assert_eq!(pilots, vec!["dix otto".to_string()], "pilots: {pilots:?}");
    assert_eq!(sysd, vec!["X5-0EM".to_string()], "system: {sysd:?}");
}

#[test]
fn unresolved_caps_code_in_gate_not_a_pilot() {
    let s = systems();
    let txt = "DT gate to UALX Camped";
    let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "Frizank2");
    let resolved = esi_resolve(&r.pilots, &[]);
    assert!(
        !resolved.iter().any(|p| p == "UALX" || p == "DT"),
        "unresolved system code became a pilot: {resolved:?}",
    );
    assert!(r.camp, "camped should set the camp keyword: {:?}", r.text);
}

mod session_regressions {
    use super::*;

    #[test]
    fn line7_repeated_subphrase_pilot_survives() {
        let s = systems_with(&[("hl-vzx", "HL-VZX", 30002, -0.4)]);
        let ships = ships_with(&[("Stabber", 622), ("Orthrus", 33470), ("Stiletto", 11198)]);
        let known: std::collections::HashMap<String, i64> = [
            ("furry for life".to_string(), 1i64),
            ("tiffanbrill".to_string(), 2i64),
            ("tiffanbrill dragon".to_string(), 3i64),
        ]
        .into_iter()
        .collect();
        let line =
            "HL-VZX Furry For Life Tiffanbrill Tiffanbrill Dragon stabber orthrus stiletto";
        let r = analyze(line, &s, &ships, &known, 1, "ch", "Shadow McLane");
        for hull in ["Stabber", "Orthrus", "Stiletto"] {
            assert!(r.ships.iter().any(|sh| sh.name == hull), "missing {hull}: {:?}", r.ships);
        }
        let reals = ["Furry For Life", "Tiffanbrill", "Tiffanbrill Dragon"];
        let (pilots, sysd, _gates) = resolve_report(&r, &reals, &s);
        for name in reals {
            assert!(
                pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
                "pilot {name:?} missing: {pilots:?}",
            );
        }
        assert_eq!(sysd, vec!["HL-VZX".to_string()], "system: {sysd:?}");

        let mut prev = analyze(line, &s, &ships, &known, 1, "ch", "Shadow McLane");
        prev.pilots = reals.iter().map(|n| (*n).to_string()).collect();
        let merge_src = format!("{} {}", prev.text, prev.text);
        drop_subphrase_pilots(
            &mut prev.pilots,
            &std::collections::HashSet::new(),
            &merge_src,
        );
        assert!(prev.pilots.iter().any(|p| p == "Tiffanbrill"), "standalone dropped: {:?}", prev.pilots);
        assert!(prev.pilots.iter().any(|p| p == "Tiffanbrill Dragon"), "two-word dropped: {:?}", prev.pilots);
    }

    #[test]
    fn line8_parse_isk_ess_amounts() {
        assert_eq!(parse_isk("ess robbed 30m", true), None, "'robbed' is not an amount");
        assert_eq!(parse_isk("ess 346mio", true), Some(346_000_000));
        assert_eq!(parse_isk("ess 77m", true), Some(77_000_000));
        assert_eq!(parse_isk("ess 300kk 5 min", true), Some(300_000_000));
    }
}

#[test]
fn full_hull_name_detected_in_any_case() {
    let s = systems();
    let ships = ships_with(&[("Rifter", 587), ("Naga", 4306)]);
    for variant in ["RIFTER", "rifter", "RiFtEr", "Rifter"] {
        let r = analyze(variant, &s, &ships, &noknown(), 1, "ch", "x");
        assert!(
            r.ships.iter().any(|sh| sh.name == "Rifter"),
            "{variant:?}: Rifter not detected: {:?}",
            r.ships
        );
    }
    let r = analyze("tackled a NAGA on the gate", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Naga"), "ships={:?}", r.ships);
}

#[test]
fn hulls_after_decorated_count_detected() {
    let s = systems_with(&[("rancer", "Rancer", 1, 0.4)]);
    let ships = ships_with(&[("Vagabond", 11999), ("Cerberus", 11993)]);
    for line in [
        "Rancer  +5 vagabond cerberus",
        "Rancer +5 VAGABOND CERBERUS",
        "Rancer +5 Vagabond Cerberus",
    ] {
        let r = analyze(line, &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Vagabond"), "{line:?}: {:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Cerberus"), "{line:?}: {:?}", r.ships);
    }
}

#[test]
fn hull_next_to_confirmed_pilot_still_detected() {
    let s = systems();
    let ships = ships_with(&[("Rifter", 587), ("Sabre", 22456), ("Worm", 17619)]);
    let known: std::collections::HashMap<String, i64> =
        [("bob".to_string(), 1i64), ("wolf e kristjansson".to_string(), 2i64)]
            .into_iter()
            .collect();
    for line in ["Bob Rifter", "Rifter Bob"] {
        let r = analyze(line, &s, &ships, &known, 1, "ch", "x");
        assert!(
            r.ships.iter().any(|sh| sh.name == "Rifter"),
            "{line:?}: hull next to confirmed pilot dropped: {:?}",
            r.ships
        );
        let (pilots, _sys, _g) = resolve_report(&r, &["Bob"], &s);
        assert_eq!(pilots, vec!["Bob".to_string()], "{line:?}: pilot: {pilots:?}");
    }
    let ships2 = ships_with(&[("Wolf", 11371)]);
    let r = analyze("Wolf E Kristjansson nv", &s, &ships2, &known, 1, "ch", "x");
    assert!(r.ships.is_empty(), "confirmed-name hull word leaked: {:?}", r.ships);
    let r = analyze("Sabre Smith in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.is_empty(), "unconfirmed blob forced a ship: {:?}", r.ships);
}

#[test]
fn multiword_hull_names_case_insensitive() {
    let s = systems();
    let ships =
        ships_with(&[("Cyclone Fleet Issue", 17634), ("Naga", 4306), ("Vagabond", 11999)]);
    for line in ["cyclone fleet issue", "CYCLONE FLEET ISSUE", "Cyclone Fleet Issue"] {
        let r = analyze(line, &s, &ships, &noknown(), 1, "ch", "x");
        assert!(
            r.ships.iter().any(|sh| sh.name == "Cyclone Fleet Issue"),
            "{line:?}: {:?}",
            r.ships
        );
    }
    let r = analyze("naga and a CYCLONE FLEET ISSUE", &s, &ships, &noknown(), 1, "ch", "x");
    assert!(r.ships.iter().any(|sh| sh.name == "Naga"), "ships={:?}", r.ships);
    assert!(
        r.ships.iter().any(|sh| sh.name == "Cyclone Fleet Issue"),
        "ships={:?}",
        r.ships
    );
}

#[test]
fn multiword_system_is_the_system_not_a_pilot() {
    let s = systems_with(&[
        ("sanctified vidette", "Sanctified Vidette", 31000123, -1.0),
        ("rancer", "Rancer", 1, 0.4),
    ]);
    let ships = ships_with(&[("Rifter", 587)]);
    for line in ["Sanctified Vidette", "sanctified vidette", "SANCTIFIED VIDETTE"] {
        let r = analyze(line, &s, &ships, &noknown(), 1, "ch", "x");
        assert!(
            r.systems.iter().any(|d| d.name == "Sanctified Vidette"),
            "{line:?}: system missing: {:?}",
            r.systems
        );
        assert!(
            !r.pilots.iter().any(|p| p.to_lowercase().contains("vidette")
                || p.to_lowercase().contains("sanctified")),
            "{line:?}: leaked as pilot: {:?}",
            r.pilots
        );
    }
    let known: std::collections::HashMap<String, i64> =
        [("bob".to_string(), 1i64)].into_iter().collect();
    let r = analyze("Bob Rifter Sanctified Vidette", &s, &ships, &known, 1, "ch", "x");
    assert!(
        r.systems.iter().any(|d| d.name == "Sanctified Vidette"),
        "system missing: {:?}",
        r.systems
    );
    assert!(r.ships.iter().any(|sh| sh.name == "Rifter"), "ship missing: {:?}", r.ships);
    let (pilots, _sys, _g) = resolve_report(&r, &["Bob"], &s);
    assert_eq!(pilots, vec!["Bob".to_string()], "pilots: {pilots:?}");

    let known2: std::collections::HashMap<String, i64> =
        [("john smith".to_string(), 7i64)].into_iter().collect();
    let r = analyze("John Smith in Rancer", &s, &noships(), &known2, 1, "ch", "x");
    assert!(
        r.pilots.iter().any(|p| p.eq_ignore_ascii_case("John Smith")),
        "normal 2-word pilot dropped: {:?}",
        r.pilots
    );
    assert!(r.systems.iter().any(|d| d.name == "Rancer"), "systems={:?}", r.systems);
}

#[test]
fn paste_segment_trailing_tag_stripped_ben_walker() {
    let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
    let known: std::collections::HashMap<String, i64> =
        [("ben walker".to_string(), 1i64)].into_iter().collect();
    for line in ["FN0-QS  Ben Walker NV", "FN0-QS Ben Walker NV", "FN0-QS  Ben Walker  NV"] {
        let r = analyze(line, &s, &noships(), &known, 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ben Walker")),
            "{line:?}: Ben Walker not recognized: {:?}",
            r.pilots
        );
        assert!(
            !r.pilots.iter().any(|p| p.to_lowercase().contains("nv")),
            "{line:?}: NV glued into a pilot: {:?}",
            r.pilots
        );
        assert!(r.no_visual, "{line:?}: no_visual not set: {:?}", r.text);
        assert!(
            r.systems.iter().any(|d| d.name == "FN0-QS"),
            "{line:?}: system missing: {:?}",
            r.systems
        );
        let (pilots, _sys, _g) = resolve_report(&r, &["Ben Walker"], &s);
        assert!(pilots.iter().any(|p| p == "Ben Walker"), "{line:?}: cover: {pilots:?}");
    }
    assert_eq!(trim_paste_location_tail("Ben Walker NV", &ships_with(&[])), "Ben Walker");
    assert_eq!(trim_paste_location_tail("Ben Walker nv", &ships_with(&[])), "Ben Walker");
    assert_eq!(trim_paste_location_tail("Clear Rain", &ships_with(&[])), "Clear Rain");
    assert_eq!(trim_paste_location_tail("Blue Skies", &ships_with(&[])), "Blue Skies");
    assert_eq!(trim_paste_location_tail("Lopatich R", &ships_with(&[])), "Lopatich R");
    assert_eq!(trim_paste_location_tail("Malcolm 41", &ships_with(&[])), "Malcolm 41");
}
