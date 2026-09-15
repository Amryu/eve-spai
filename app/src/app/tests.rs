//! Unit tests for the app module, kept out of `app.rs` so the file holds only code that ships.

use super::*;

#[cfg(test)]
mod wh_badge_tests {
    use super::wormhole_badge_label;
    use crate::intel::IntelReport;
    use crate::wormholes::DestClass;

    fn wh(f: impl FnOnce(&mut IntelReport)) -> IntelReport {
        let mut ir = IntelReport { wormhole: true, ..Default::default() };
        f(&mut ir);
        ir
    }

    #[test]
    fn wormhole_badge_composition() {
        let icon = egui_phosphor::regular::SPIRAL;
        assert_eq!(wormhole_badge_label(&wh(|_| {})), icon.to_string());
        let l = wormhole_badge_label(&wh(|ir| {
            ir.wh_sig = Some("ABC-123".into());
            ir.wh_type = Some("S899".into());
            ir.wh_size = Some(crate::wormholes::ShipSize::XLarge);
            ir.wh_drifter = true;
            ir.wh_dest = Some(DestClass::Highsec);
        }));
        assert!(l.starts_with(icon), "{l}");
        for want in ["ABC-123", "S899", "XL", "Drifter", "Highsec"] {
            assert!(l.contains(want), "missing {want} in {l:?}");
        }
        // The card must not contain literal parentheses (they only marked optionality).
        assert!(!l.contains('(') && !l.contains(')'), "unexpected parens in {l:?}");
        assert!(!wormhole_badge_label(&wh(|ir| ir.wh_type = Some("K162".into()))).contains("K162"));
        assert!(wormhole_badge_label(&wh(|ir| ir.wh_dest = Some(DestClass::Thera))).contains("Thera"));
        assert!(wormhole_badge_label(&wh(|ir| ir.wh_dest = Some(DestClass::Turnur))).contains("Turnur"));
        // A frigate-size hole reads "Small", not "Frig".
        let frig = wormhole_badge_label(&wh(|ir| ir.wh_size = Some(crate::wormholes::ShipSize::Frigate)));
        assert!(frig.contains("Small") && !frig.contains("Frig"), "{frig:?}");
    }

    #[test]
    fn only_specific_ship_classes_badge() {
        use super::interesting_ship_class;
        // Generic hull tiers are noise.
        for generic in ["Frigate", "Destroyer", "Cruiser", "Battlecruiser", "Battleship"] {
            assert!(!interesting_ship_class(generic), "{generic} should be hidden");
        }
        // T2/T3 specialisations and capitals are shown.
        for keep in ["Interdictor", "Heavy Interdictor", "Black Ops", "Strategic Cruiser", "Dreadnought", "Titan"] {
            assert!(interesting_ship_class(keep), "{keep} should show");
        }
    }

    #[test]
    fn anom_sig_badge_composition() {
        use super::anom_sig_badge_label;
        use crate::intel::AnomKind;
        let icon = egui_phosphor::regular::CROSSHAIR;
        assert_eq!(anom_sig_badge_label(AnomKind::Anomaly, ""), format!("{icon} Anom"));
        assert_eq!(anom_sig_badge_label(AnomKind::Signature, ""), format!("{icon} Sig"));
        assert_eq!(anom_sig_badge_label(AnomKind::Signature, "ABC-123"), format!("{icon} Sig ABC-123"));
    }
}

#[cfg(test)]
mod wh_overlay_tests {
    use super::*;
    use crate::wormholes::{DestClass, Source, Wormhole};

    fn conn(sys: i64, dest_sys: i64) -> Wormhole {
        Wormhole {
            id: 0,
            system_id: sys,
            signature: None,
            wh_type: None,
            dest: if is_jspace(dest_sys) { DestClass::Wspace } else { DestClass::Nullsec },
            dest_system_id: Some(dest_sys),
            dest_signature: None,
            dest_wh_type: None,
            size: None,
            is_drifter: false,
            reported_at: 0,
            explicit_expiry: None,
            source: Source::Intel,
            updated_at: 0,
        }
    }

    #[test]
    fn chains_through_jspace_and_direct_links() {
        let whs = vec![
            conn(30_000_001, 31_000_001),
            conn(31_000_001, 30_000_002),
            conn(30_000_001, 30_000_003),
        ];
        let o = WhOverlay::build(&whs);
        assert!(o.direct.contains(&(30_000_001, 30_000_003)), "direct: {:?}", o.direct);
        assert!(
            o.chains.iter().any(|&(a, b, h)| (a, b) == (30_000_001, 30_000_002) && h == 1),
            "chains: {:?}",
            o.chains
        );
        assert!(o.jspace_holes.contains(&30_000_001));
        assert!(!o.direct.iter().any(|&(a, b)| is_jspace(a) || is_jspace(b)));
    }
}

#[cfg(test)]
mod celestial_key_tests {
    use super::*;

    fn same(a: &str, b: &str) -> bool {
        let (ka, kb) = (celestial_key(a), celestial_key(b));
        ka.is_some() && ka == kb
    }

    #[test]
    fn spellings_of_one_moon_agree() {
        assert!(same(
            "Planet VI - Moon 3 - Blood Raider Chemical Laboratory",
            "Planet VI - Moon 3 - Chemical Laboratory"
        ));
        assert!(same("Moon 6-3", "Planet VI - Moon 3 - Chemical Laboratory"));
        assert!(same("Moon 6-3", "7-K5EL VI - Moon 3"));
        assert!(same("moon 6-3", "PLANET VI - MOON 3"));
        assert!(same("Jita IV - Moon 4", "Moon 4-4"));
    }

    #[test]
    fn spellings_of_one_planet_agree() {
        assert!(same("Planet 6", "Jita VI"));
        assert!(same("Planet VI", "Planet 6"));
    }

    /// Adversarial cases added on review. Merging hides a chip, and a hidden chip can mean a
    /// pilot not knowing hostiles sit at a different celestial, so the failure that matters is a
    /// false merge.
    #[test]
    fn review_adversarial_cases() {
        // Prefix collisions in either index.
        assert!(!same("Moon 6-3", "Planet VI - Moon 30 - Station"));
        assert!(!same("Moon 6-30", "Planet VI - Moon 3 - Station"));
        assert!(!same("Moon 60-3", "Planet VI - Moon 3 - Station"));
        // A roman-looking system token must not become a planet index.
        assert_eq!(celestial_key("K5EL 7"), Some(CelestialKey::Planet(7)));
        assert_eq!(celestial_key("7-K5EL"), None);
        // Digits inside a token keep it out of the roman path.
        assert_eq!(celestial_key("MJ-5F9 IV"), Some(CelestialKey::Planet(4)));
        assert_eq!(celestial_key("MJ-5F9"), None);
        // Belts and unindexed suffixes stay unmergeable in both directions.
        assert_eq!(celestial_key("Jita IV - Asteroid Belt 1"), None);
        assert!(!same("Asteroid Belt", "Jita IV - Asteroid Belt 1"));
        // A planet chip must never swallow a moon chip on the same planet.
        assert!(!same("Planet VI", "Planet VI - Moon 3 - Station"));
    }

    /// Documents a known limit rather than asserting desired behaviour: the key carries no system,
    /// so two planets with the same index in different systems collide. Both fields come from one
    /// report, so this needs a report naming two systems' planets to bite.
    #[test]
    fn review_planet_key_ignores_the_system() {
        assert!(same("Jita IV", "Amarr IV"));
    }

    #[test]
    fn suns_agree() {
        assert!(same("Sun", "Jita - Star"));
    }

    #[test]
    fn different_moons_stay_apart() {
        assert!(!same("Moon 6-4", "Planet VI - Moon 3 - Chemical Laboratory"));
        assert!(!same("Moon 5-3", "Planet VI - Moon 3 - Chemical Laboratory"));
        assert!(!same("Moon 3-6", "Planet VI - Moon 3"));
        assert!(!same("Planet VI - Moon 13", "Planet VI - Moon 1"));
    }

    #[test]
    fn a_moon_is_not_its_planet() {
        assert!(!same("Planet VI", "Planet VI - Moon 3 - Chemical Laboratory"));
        assert!(!same("Planet IV", "Jita IV - Moon 4"));
        assert!(!same("Sun", "Jita IV"));
    }

    #[test]
    fn nameless_indexes_never_match() {
        assert_eq!(celestial_key("Moon IV"), None);
        assert_eq!(celestial_key("Moon 3"), None);
        assert_eq!(celestial_key("Asteroid Belt"), None);
        assert_eq!(celestial_key("Ice Belt"), None);
        assert_eq!(celestial_key("Belt"), None);
        assert_eq!(celestial_key("Jita IV - Asteroid Belt 1"), None);
        assert_eq!(celestial_key("Jita IV - Caldari Navy Assembly Plant"), None);
        assert_eq!(celestial_key("Stargate (Perimeter)"), None);
        assert!(!same("Moon IV", "Planet II - Moon 4"));
        assert!(!same("Asteroid Belt", "Jita IV - Asteroid Belt 1"));
    }

    #[test]
    fn keys_read_the_indexes() {
        assert_eq!(celestial_key("Planet VI - Moon 3"), Some(CelestialKey::Moon(6, 3)));
        assert_eq!(celestial_key("Moon 6-3"), Some(CelestialKey::Moon(6, 3)));
        assert_eq!(celestial_key("Jita IV"), Some(CelestialKey::Planet(4)));
        assert_eq!(celestial_key("Jita - Star"), Some(CelestialKey::Star));
    }
}

#[cfg(test)]
mod kill_noise_tests {
    use super::*;

    #[test]
    fn deployables_and_unknowns_are_noise() {
        assert!(kill_is_noise("", 5_000_000.0));
        assert!(kill_is_noise("Mobile Tractor Unit", 1_000_000.0));
        assert!(kill_is_noise("Mobile Depot", 500_000.0));
        assert!(kill_is_noise("Shuttle", 10_000.0));
        assert!(kill_is_noise("Reaper", 1000.0));
        assert!(kill_is_noise("Capsule", 100_000.0));
        assert!(!kill_is_noise("Stabber", 20_000_000.0));
        assert!(!kill_is_noise("Keepstar", 1e12));
        assert!(!kill_is_noise("Capsule", 500_000_000.0));
    }
}

#[cfg(test)]
mod op_channel_tests {
    use super::*;

    #[test]
    fn op_key_canonicalizes_variants() {
        assert_eq!(op_key("Op 4").as_deref(), Some("op4"));
        assert_eq!(op_key("OP4").as_deref(), Some("op4"));
        assert_eq!(op_key("op 4 - dead keepstars").as_deref(), Some("op4"));
        assert_eq!(op_key("get to OP 9 now").as_deref(), Some("op9"));
        assert_eq!(op_key("stop shooting"), None);
        assert_eq!(op_key("no channel here"), None);
    }
}

#[cfg(test)]
mod activity_label_tests {
    use super::compact_count;

    #[test]
    fn small_counts_stay_exact() {
        assert_eq!(compact_count(0), "0");
        assert_eq!(compact_count(7), "7");
        assert_eq!(compact_count(99), "99");
    }

    #[test]
    fn three_digits_and_up_abbreviate() {
        assert_eq!(compact_count(100), "0.1k");
        assert_eq!(compact_count(234), "0.2k");
        assert_eq!(compact_count(1_234), "1.2k");
        assert_eq!(compact_count(23_400), "23.4k");
    }
}

#[cfg(test)]
mod sov_art_tests {
    use super::*;

    fn img(px: &[egui::Color32]) -> egui::ColorImage {
        egui::ColorImage::new([px.len(), 1], px.to_vec())
    }

    #[test]
    fn mean_ignores_transparent_padding() {
        let clear = egui::Color32::from_rgba_unmultiplied(0xFF, 0x00, 0x00, 0x00);
        let blue = egui::Color32::from_rgb(0x00, 0x00, 0xC0);
        // The red is fully transparent logo padding, so only the blue counts.
        let c = mean_logo_color(&img(&[clear, blue, clear])).unwrap();
        assert_eq!((c.r(), c.g()), (0, 0));
        assert!(c.b() > 0xA0, "b={}", c.b());
    }

    #[test]
    fn mean_lifts_a_dark_logo_into_view() {
        let dark = egui::Color32::from_rgb(0x10, 0x10, 0x20);
        let c = mean_logo_color(&img(&[dark])).unwrap();
        assert!(c.b() >= 100, "a near-black logo must not yield a near-black dot: {c:?}");
    }

    /// Two logos whose averages are both muddy but differently tinted must not collapse to the same
    /// grey, or every alliance looks alike on the map.
    #[test]
    fn a_washed_out_mean_comes_back_saturated() {
        let muddy_red = mean_logo_color(&img(&[egui::Color32::from_rgb(0x70, 0x5A, 0x5A)])).unwrap();
        let muddy_blue = mean_logo_color(&img(&[egui::Color32::from_rgb(0x5A, 0x5A, 0x70)])).unwrap();

        let spread = |c: egui::Color32| {
            let (r, g, b) = (c.r() as i32, c.g() as i32, c.b() as i32);
            r.max(g).max(b) - r.min(g).min(b)
        };
        assert!(spread(muddy_red) > 40, "still grey: {muddy_red:?}");
        assert!(spread(muddy_blue) > 40, "still grey: {muddy_blue:?}");
        // Hue is preserved: the reddish one stays reddish, the bluish one bluish.
        assert!(muddy_red.r() > muddy_red.b());
        assert!(muddy_blue.b() > muddy_blue.r());
    }

    #[test]
    fn a_genuinely_grey_logo_is_left_grey() {
        // No hue to recover, so boosting must not invent one.
        let c = mean_logo_color(&img(&[egui::Color32::from_rgb(0x80, 0x80, 0x80)])).unwrap();
        assert_eq!((c.r(), c.g()), (c.b(), c.b()));
    }

    #[test]
    fn a_fully_transparent_logo_has_no_mean() {
        let clear = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0);
        assert!(mean_logo_color(&img(&[clear])).is_none());
    }

    #[test]
    fn npc_factions_resolve_to_a_logo_corp() {
        use crate::factions::corporation_id;
        assert_eq!(corporation_id(500_010), Some(1_000_127)); // Guristas, holds Venal
        assert_eq!(corporation_id(500_019), Some(1_000_162)); // Sansha, holds Stain
        assert_eq!(corporation_id(1234), None);
    }
}

#[cfg(test)]
mod wh_route_tests {
    use super::*;
    use crate::geo::{SystemInfo, Systems, ZARZAKH};
    use std::collections::HashMap;

    const THERA: i64 = 31_000_005;

    /// Two gate islands, 6 gates apart the long way, with Thera reachable by a hole from each.
    fn systems() -> Systems {
        let mk = |id: i64| SystemInfo {
            id,
            name: format!("S{id}"),
            security: 0.0,
            constellation: String::new(),
            region: String::new(),
            faction: String::new(),
        };
        let ids = [1, 2, 3, 4, 5, 6, 7, THERA, ZARZAKH];
        let by_name: HashMap<String, SystemInfo> =
            ids.into_iter().map(|id| (format!("s{id}"), mk(id))).collect();
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for (a, b) in [(1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7)] {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        Systems::new(by_name, adj)
    }

    fn holes(edges: &[(i64, i64)]) -> HashMap<i64, Vec<i64>> {
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(a, b) in edges {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        adj
    }

    #[test]
    fn both_sides_of_a_hole_get_a_waypoint() {
        let g = systems();
        // 1 -gate- 2 =hole= Thera =hole= 6 -gate- 7. The client cannot route through the hole, so it
        // needs 2 (fly here, jump) and 6 (resume here), and Thera itself can hold no waypoint.
        let wp = wh_route_waypoints(&g, &holes(&[(2, THERA), (6, THERA)]), 1, 7).unwrap();
        assert_eq!(wp, vec![2, 6, 7]);
    }

    #[test]
    fn the_system_we_are_in_is_not_a_waypoint() {
        let g = systems();
        // Standing on the hole already: nothing to fly to, just jump and carry on.
        let wp = wh_route_waypoints(&g, &holes(&[(1, THERA), (6, THERA)]), 1, 7).unwrap();
        assert_eq!(wp, vec![6, 7]);
    }

    #[test]
    fn map_route_runs_through_the_hole() {
        let g = systems();
        let h = holes(&[(1, THERA), (7, THERA)]);
        let route = g.route_with(1, 7, true, true, &h, |_| true).unwrap();
        assert_eq!(route, vec![1, THERA, 7]);
        assert!(g.is_hole_step(1, THERA) && g.is_hole_step(THERA, 7));
        assert!(!g.is_hole_step(1, 2));
        // Without the holes the same trip is all six gates.
        assert_eq!(g.route(1, 7, true, true, |_| true).unwrap().len() - 1, 6);
    }

    #[test]
    fn a_thera_hub_with_many_holes_still_routes() {
        let g = systems();
        // The map overlay drops a j-space hub above degree 6. Routing must not.
        let h = holes(&[
            (2, THERA),
            (3, THERA),
            (4, THERA),
            (5, THERA),
            (6, THERA),
            (7, THERA),
        ]);
        assert_eq!(wh_route_waypoints(&g, &h, 1, 7).unwrap(), vec![2, 7]);
    }

    #[test]
    fn gates_win_when_the_hole_is_no_shortcut() {
        let g = systems();
        // 1 -> 2 -> 3 is pure gates, so no waypoints beyond the destination.
        assert_eq!(wh_route_waypoints(&g, &holes(&[(1, THERA), (5, THERA)]), 1, 3).unwrap(), vec![3]);
    }

    #[test]
    fn no_route_without_a_connecting_hole() {
        let g = systems();
        assert_eq!(wh_route_waypoints(&g, &holes(&[(1, THERA)]), 1, 99), None);
    }

    #[test]
    fn zarzakh_is_not_a_shortcut() {
        let g = systems();
        // A hole into Zarzakh does not let a route continue out of its far gate.
        let h = holes(&[(1, ZARZAKH), (ZARZAKH, 7)]);
        assert_eq!(wh_route_waypoints(&g, &h, 1, 7).unwrap(), vec![7]);
        // Zarzakh as the destination is still fine.
        assert_eq!(wh_route_waypoints(&g, &h, 1, ZARZAKH).unwrap(), vec![ZARZAKH]);
        assert_eq!(g.route_with(1, 7, true, true, &h, |_| true).unwrap().len() - 1, 6);
    }
}

#[cfg(feature = "fc-rescue")]
#[cfg(test)]
mod comms_link_tests {
    use super::*;

    /// Anything posted to a channel must carry a gnf.lt link. A raw `mumble://` URL is silently
    /// dead on clients/OSes with no protocol handler, and it would point at Command Sector Alpha.
    #[test]
    fn posted_comms_links_are_gnf_lt() {
        for op in (1u8..=12).filter(|n| *n != 8) {
            let url = op_comms_url(op);
            assert!(url.starts_with("https://gnf.lt/"), "op {op} posted link: {url}");

            let invite = rescue_comms_invite(Some("ajunta_thor"), op).unwrap();
            assert!(invite.contains("https://gnf.lt/"), "op {op} invite: {invite}");
            assert!(!invite.contains("mumble://"), "op {op} invite: {invite}");
            assert_eq!(invite, format!("ajunta_thor: Join OP {op} Comms {url}"));
            // Nothing may follow the URL, or the receiving client links the punctuation too.
            assert!(invite.ends_with(&url), "op {op} invite: {invite}");
        }
    }

    #[test]
    fn invite_needs_an_author_and_a_link() {
        assert!(rescue_comms_invite(None, 11).is_none());
        assert!(rescue_comms_invite(Some("  "), 11).is_none());
        // Op 8 has no short link, so it must not fall back to a command-comms mumble URL.
        assert!(rescue_comms_invite(Some("someone"), 8).is_none());
    }
}

#[cfg(test)]
mod ping_link_tests {
    use super::*;

    #[test]
    fn url_keeps_its_path_but_drops_sentence_punctuation() {
        assert_eq!(trim_url_tail("https://gnf.lt/abc"), "https://gnf.lt/abc");
        assert_eq!(trim_url_tail("https://gnf.lt/abc."), "https://gnf.lt/abc");
        assert_eq!(trim_url_tail("https://gnf.lt/abc)."), "https://gnf.lt/abc");
        assert_eq!(trim_url_tail("https://zkillboard.com/kill/1/"), "https://zkillboard.com/kill/1/");
    }

    /// Every branch slices `body` by byte index, so a multibyte ping body must not panic.
    #[test]
    fn muc_service_falls_back_to_the_convention() {
        // No room joined yet: the browse button still has a service to disco.
        assert_eq!(muc_domain_of("", "pilot@goonfleet.com"), "conference.goonfleet.com");
        // An explicit setting always wins, and is not double-prefixed.
        assert_eq!(muc_domain_of(" conference.example.org ", "pilot@goonfleet.com"), "conference.example.org");
        // Nothing configured at all stays empty, so the button stays disabled with its hint.
        assert_eq!(muc_domain_of("", ""), "");
        assert_eq!(muc_domain_of("", "pilot"), "");
    }

    #[test]
    fn ping_bodies_render_without_panicking() {
        let bodies = [
            "form up now",
            "https://gnf.lt/abc",
            "join https://gnf.lt/abc now",
            "bare http and https words",
            "Ö https://gnf.lt/ä, ドクトリン https://eve-spai.com/br/x.",
            "line one https://gnf.lt/a\nline two https://gnf.lt/b\n\ntail",
            "",
        ];
        egui::__run_test_ui(|ui| {
            for b in bodies {
                render_ping_body(ui, b, true);
                render_ping_body(ui, b, false);
                ui.horizontal_wrapped(|ui| render_message_body(ui, b));
            }
        });
    }
}

#[cfg(test)]
mod badge_total_tests {
    /// The badge counts messages, not conversations, and a muted conversation does not get to put a
    /// number on the taskbar.
    #[test]
    fn muted_conversations_do_not_reach_the_badge() {
        let mut counts: std::collections::BTreeMap<String, u32> = Default::default();
        counts.insert("loud@example.com".to_owned(), 3);
        counts.insert("muted@example.com".to_owned(), 40);
        let muted = |k: &str| k.starts_with("muted");
        let total: u32 =
            counts.iter().filter(|(k, _)| !muted(k)).map(|(_, c)| *c).sum();
        assert_eq!(total, 3);
    }
}

#[cfg(test)]
mod msg_row_tests {
    use super::*;

    /// Drives a real context with the pointer parked inside the rows, so the hover branch (tint,
    /// backdrop, action buttons) is exercised and not just the resting one.
    fn run_hovered(at: egui::Pos2, mut f: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 400.0),
            )),
            ..Default::default()
        };
        input.events.push(egui::Event::PointerMoved(at));
        for _ in 0..2 {
            let _ = ctx.run_ui(input.clone(), &mut f);
            input.events.clear();
        }
    }

    const BODIES: [&str; 4] =
        ["form up now", "Ö ドクトリン", "", "attention https://gnf.lt/a ドクトリン"];

    /// Click at `at` and report what the row returned.
    fn click_row(at: egui::Pos2, show: MsgActions) -> MsgRowAction {
        let ctx = egui::Context::default();
        let base = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 400.0),
            )),
            ..Default::default()
        };
        let mut got = MsgRowAction::None;
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput { events, ..base.clone() };
            let _ = ctx.run_ui(input, |ui| {
                let a = message_row(ui, ui.id().with("row"), false, show, |ui| {
                    ui.label("pilot: form up");
                });
                if a != MsgRowAction::None {
                    got = a;
                }
            });
        };
        // Two frames before pressing: the row only reports hover once it has been registered for a
        // frame, and the icons only exist while it is hovered, so they must be laid out before the
        // press or there is no widget to attribute it to.
        frame(vec![egui::Event::PointerMoved(at)]);
        frame(Vec::new());
        frame(vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        frame(vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        got
    }

    /// The action icons must not allocate. A widget placed in the row changes the width the body
    /// wraps into, so hovering silently reflows the text to a different number of lines and the
    /// list jitters between two row heights. Needs wrapping text: with a single short line a width
    /// change does not show up as a height change at all.
    #[test]
    fn hovering_a_row_does_not_reflow_it() {
        let heights = |hover: bool| -> Vec<f32> {
            let ctx = egui::Context::default();
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 200.0),
                )),
                ..Default::default()
            };
            if hover {
                input.events.push(egui::Event::PointerMoved(egui::pos2(120.0, 30.0)));
            }
            let show = MsgActions { copy: true, mention: true, dm: true };
            let body = "form up now in 1DQ we need dps and logi bring your own doctrine ship please";
            let mut out = Vec::new();
            // Four passes: hover is reported a frame late, and a reflow needs another to settle.
            for _ in 0..4 {
                out.clear();
                let _ = ctx.run_ui(input.clone(), |ui| {
                    egui::ScrollArea::vertical().id_salt("m").auto_shrink([false, false]).show(
                        ui,
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for i in 0..4 {
                                let top = ui.cursor().min.y;
                                message_row(ui, ui.id().with(("row", i)), false, show, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label("pilot:");
                                        render_message_body(ui, body);
                                    });
                                });
                                out.push(ui.cursor().min.y - top);
                            }
                        },
                    );
                });
                input.events.clear();
            }
            out
        };
        let (resting, hovered) = (heights(false), heights(true));
        assert_eq!(resting, hovered, "hovering reflowed the rows");
    }

    /// The icons are painted at hand-computed rects, so a geometry slip would leave them visible
    /// but unclickable. Sweep the row's right edge rather than hardcoding the layout.
    #[test]
    fn clicking_an_action_icon_returns_that_action() {
        for (show, want) in [
            (MsgActions { copy: true, mention: false, dm: false }, MsgRowAction::Copy),
            (MsgActions { copy: false, mention: true, dm: false }, MsgRowAction::Mention),
            (MsgActions { copy: false, mention: false, dm: true }, MsgRowAction::GoToDm),
        ] {
            let hit = (320..400)
                .map(|x| click_row(egui::pos2(x as f32, 8.0), show))
                .any(|a| a == want);
            assert!(hit, "no clickable slot returned {want:?}");
        }
        // Three at once: each icon must be reachable, so all three come back over the sweep.
        let all = MsgActions { copy: true, mention: true, dm: true };
        for want in [MsgRowAction::Copy, MsgRowAction::Mention, MsgRowAction::GoToDm] {
            let hit = (320..400)
                .map(|x| click_row(egui::pos2(x as f32, 8.0), all))
                .any(|a| a == want);
            assert!(hit, "{want:?} unreachable when all three icons are shown");
        }
    }

    #[test]
    fn message_rows_render_in_every_state() {
        let row = |ui: &mut egui::Ui, n: usize, mentioned: bool, grouped: bool, show: MsgActions, body: &str| {
            let act = message_row(ui, ui.id().with(("row", n)), mentioned, show, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if !grouped {
                        ui.label("pilot:");
                    }
                    render_message_body(ui, body);
                });
            });
            assert_eq!(act, MsgRowAction::None);
        };
        let mut states = Vec::new();
        for mentioned in [false, true] {
            for grouped in [false, true] {
                for copy in [false, true] {
                    for mention in [false, true] {
                        for dm in [false, true] {
                            for body in BODIES {
                                states.push((
                                    mentioned,
                                    grouped,
                                    MsgActions { copy, mention, dm },
                                    body,
                                ));
                            }
                        }
                    }
                }
            }
        }
        egui::__run_test_ui(|ui| {
            for (i, (mentioned, grouped, show, body)) in states.iter().enumerate() {
                row(ui, i, *mentioned, *grouped, *show, body);
            }
        });
        // One row per pass, drawn under the pointer, so every state also renders its hover branch.
        for (i, (mentioned, grouped, show, body)) in states.iter().enumerate() {
            run_hovered(egui::pos2(120.0, 6.0), |ui| {
                row(ui, i, *mentioned, *grouped, *show, body);
            });
        }
    }

    #[cfg(feature = "fc-rescue")]
    #[test]
    fn rescue_grouping_follows_sender_and_gap() {
        assert!(!rescue_grouped("a", 100, None, 0));
        assert!(rescue_grouped("a", 200, Some("a"), 100));
        assert!(!rescue_grouped("a", 500, Some("a"), 100));
        assert!(!rescue_grouped("b", 200, Some("a"), 100));
        // A message that arrives out of order must start a fresh header, not fold into the last.
        assert!(!rescue_grouped("a", 50, Some("a"), 100));
    }

    #[cfg(feature = "fc-rescue")]
    #[test]
    fn rescue_feed_renders_every_grouping_shape() {
        let msg = |who: &str, body: &str, out: bool, t: i64| {
            (who.to_owned(), body.to_owned(), out, t)
        };
        let now = chrono::Utc::now().timestamp();
        let feeds: Vec<Vec<(String, String, bool, i64)>> = vec![
            Vec::new(),
            vec![msg("Ödin", "Ö ドクトリン", false, now)],
            vec![msg("Ödin", "one", false, now - 100), msg("Ödin", "two", false, now - 10)],
            vec![msg("Ödin", "one", false, now - 900), msg("Ödin", "two", false, now)],
            vec![msg("Ödin", "one", false, now - 10), msg("Bob", "two", true, now)],
        ];
        let draw = |ui: &mut egui::Ui| {
            for (i, f) in feeds.iter().enumerate() {
                assert!(rescue_chat_feed(ui, f, &format!("feed{i}")).is_none());
            }
        };
        egui::__run_test_ui(draw);
        // One feed per pass, sampled down the column, so the hover branch renders too.
        for (i, f) in feeds.iter().enumerate() {
            for y in [6.0, 20.0, 34.0, 48.0] {
                run_hovered(egui::pos2(120.0, y), |ui| {
                    assert!(rescue_chat_feed(ui, f, &format!("feed{i}")).is_none());
                });
            }
        }
    }
}

#[cfg(test)]
mod chat_window_tests {
    use super::*;

    fn win(id: u64, tabs: &[&str], active: Option<&str>) -> ChatWindow {
        ChatWindow {
            id,
            tabs: tabs.iter().map(|s| (*s).to_owned()).collect(),
            active: active.map(str::to_owned),
            ..Default::default()
        }
    }

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    struct Fix {
        main: Vec<String>,
        active: Option<String>,
        popouts: Vec<ChatWindow>,
    }
    impl Fix {
        fn set(&mut self) -> TabSet<'_> {
            TabSet {
                main: &mut self.main,
                main_active: &mut self.active,
                popouts: &mut self.popouts,
            }
        }
    }

    fn fix() -> Fix {
        Fix {
            main: v(&["a", "b"]),
            active: Some("b".to_owned()),
            popouts: vec![win(1, &["c", "d"], Some("d"))],
        }
    }

    #[test]
    fn owner_finds_main_and_popouts() {
        let mut f = fix();
        let t = f.set();
        assert_eq!(t.owner("a"), Some(ChatWinKey::Main));
        assert_eq!(t.owner("d"), Some(ChatWinKey::Popout(1)));
        assert_eq!(t.owner("zz"), None);
    }

    #[test]
    fn detach_from_main_falls_back_left_then_to_pings() {
        let mut f = fix();
        assert_eq!(f.set().detach("b"), Some(ChatWinKey::Main));
        assert_eq!(f.main, v(&["a"]));
        assert_eq!(f.active.as_deref(), Some("a"));
        // Removing the left-most tab leaves the pings pseudo-tab selected.
        f.active = Some("a".to_owned());
        f.set().detach("a");
        assert!(f.main.is_empty());
        assert_eq!(f.active, None);
    }

    #[test]
    fn detach_from_popout_never_yields_the_pings_sentinel() {
        let mut f = fix();
        f.popouts[0].active = Some("c".to_owned());
        assert_eq!(f.set().detach("c"), Some(ChatWinKey::Popout(1)));
        assert_eq!(f.popouts[0].active.as_deref(), Some("d"));
        f.set().detach("d");
        assert!(f.popouts[0].tabs.is_empty());
        assert_eq!(f.popouts[0].active, None);
    }

    #[test]
    fn attach_respects_index_and_is_idempotent() {
        let mut f = fix();
        f.set().attach("z", ChatWinKey::Main, Some(1));
        assert_eq!(f.main, v(&["a", "z", "b"]));
        f.set().attach("z", ChatWinKey::Main, Some(0));
        assert_eq!(f.main, v(&["a", "z", "b"]));
        // Out-of-range index clamps to the end; a missing window is a no-op.
        f.set().attach("y", ChatWinKey::Main, Some(99));
        assert_eq!(f.main, v(&["a", "z", "b", "y"]));
        f.set().attach("q", ChatWinKey::Popout(42), None);
        assert_eq!(f.set().owner("q"), None);
    }

    #[test]
    fn move_tab_between_windows_fixes_both_actives() {
        let mut f = fix();
        f.set().move_tab("b", ChatWinKey::Popout(1), Some(0));
        assert_eq!(f.main, v(&["a"]));
        assert_eq!(f.active.as_deref(), Some("a"));
        assert_eq!(f.popouts[0].tabs, v(&["b", "c", "d"]));
        // Dragging a conversation into a window shows it there.
        assert_eq!(f.popouts[0].active.as_deref(), Some("b"));
        // Moving something nobody owns does not invent a tab.
        f.set().move_tab("nope", ChatWinKey::Main, None);
        assert_eq!(f.main, v(&["a"]));
    }

    #[test]
    fn reordering_within_a_window_keeps_the_selection() {
        let mut f = Fix {
            main: v(&["a", "b", "c"]),
            active: Some("a".to_owned()),
            popouts: vec![win(1, &["x", "y"], Some("x"))],
        };
        // The tab being dragged is the one you are reading; it must stay selected.
        f.set().move_tab("a", ChatWinKey::Main, Some(2));
        assert_eq!(f.main, v(&["b", "c", "a"]));
        assert_eq!(f.active.as_deref(), Some("a"));
        // Reordering some other tab leaves the selection where it was.
        f.set().move_tab("c", ChatWinKey::Main, Some(0));
        assert_eq!(f.main, v(&["c", "b", "a"]));
        assert_eq!(f.active.as_deref(), Some("a"));
        f.set().move_tab("y", ChatWinKey::Popout(1), Some(0));
        assert_eq!(f.popouts[0].tabs, v(&["y", "x"]));
        assert_eq!(f.popouts[0].active.as_deref(), Some("x"));
    }

    #[test]
    fn normalize_strips_cross_window_dupes_and_pins_active() {
        let mut f = Fix {
            main: v(&["a", "b", "a"]),
            active: Some("gone".to_owned()),
            popouts: vec![win(1, &["b", "c"], Some("b")), win(2, &[], Some("x"))],
        };
        assert!(f.set().normalize());
        assert_eq!(f.main, v(&["a", "b"]));
        assert_eq!(f.popouts[0].tabs, v(&["c"]));
        assert_eq!(f.active, None);
        assert_eq!(f.popouts[0].active.as_deref(), Some("c"));
        assert_eq!(f.popouts[1].active, None);
        // Idempotent.
        assert!(!f.set().normalize());
    }

    #[test]
    fn empty_popouts_and_fresh_id() {
        let mut f = Fix {
            main: Vec::new(),
            active: None,
            popouts: vec![win(3, &[], None), win(7, &["a"], Some("a"))],
        };
        assert_eq!(f.set().empty_popouts(), vec![3]);
        assert_eq!(f.set().fresh_id(), 8);
        let mut empty = Fix { main: Vec::new(), active: None, popouts: Vec::new() };
        assert_eq!(empty.set().fresh_id(), 1);
    }

    #[test]
    fn reconcile_leaves_a_popout_owned_tab_where_it_is() {
        // Regression guard: reconciliation must not re-add a popped-out conversation to the main
        // tab bar, which would duplicate it and yank it back every frame.
        let mut f = fix();
        let want = v(&["a", "b", "c", "d"]);
        reconcile_tabs(&mut f.set(), &want);
        assert_eq!(f.main, v(&["a", "b"]));
        assert_eq!(f.popouts[0].tabs, v(&["c", "d"]));
        // And again, unchanged.
        reconcile_tabs(&mut f.set(), &want);
        assert_eq!(f.main, v(&["a", "b"]));
        assert_eq!(f.popouts[0].tabs, v(&["c", "d"]));
    }

    #[test]
    fn reconcile_adds_new_jids_to_main_in_order() {
        let mut f = fix();
        reconcile_tabs(&mut f.set(), &v(&["a", "b", "c", "d", "e", "f"]));
        assert_eq!(f.main, v(&["a", "b", "e", "f"]));
        assert_eq!(f.popouts[0].tabs, v(&["c", "d"]));
    }

    #[test]
    fn reconcile_drops_from_any_window() {
        let mut f = fix();
        reconcile_tabs(&mut f.set(), &v(&["a", "c"]));
        assert_eq!(f.main, v(&["a"]));
        assert_eq!(f.active.as_deref(), Some("a"));
        assert_eq!(f.popouts[0].tabs, v(&["c"]));
        assert_eq!(f.popouts[0].active.as_deref(), Some("c"));
    }

    #[test]
    fn reconcile_with_nothing_wanted_clears_everything() {
        let mut f = fix();
        reconcile_tabs(&mut f.set(), &[]);
        assert!(f.main.is_empty());
        assert_eq!(f.active, None);
        assert!(f.popouts[0].tabs.is_empty());
        assert_eq!(f.set().empty_popouts(), vec![1]);
    }

    #[test]
    fn drop_target_picks_the_smallest_containing_rect() {
        let big = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let small = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(20.0, 20.0));
        let rects = [(ChatWinKey::Main, big), (ChatWinKey::Popout(1), small)];
        let p = Some(egui::pos2(15.0, 15.0));
        assert_eq!(drop_target(&rects, ChatWinKey::Popout(9), p), Some(ChatWinKey::Popout(1)));
        // The source window is never its own drop target.
        assert_eq!(drop_target(&rects, ChatWinKey::Popout(1), p), Some(ChatWinKey::Main));
        assert_eq!(drop_target(&rects, ChatWinKey::Popout(9), None), None);
        assert_eq!(
            drop_target(&rects, ChatWinKey::Popout(9), Some(egui::pos2(500.0, 500.0))),
            None
        );
    }

    #[test]
    fn detach_to_new_never_leaves_an_empty_window() {
        // A pop-out observed empty even once is pruned by `jabber_reconcile`, so the window must
        // be born holding its tab.
        let mut f = fix();
        let id = f.set().detach_to_new("b", Some((10.0, 20.0))).unwrap();
        assert_eq!(id, 2);
        assert_eq!(f.main, v(&["a"]));
        assert_eq!(f.active.as_deref(), Some("a"));
        let w = f.popouts.iter().find(|w| w.id == id).unwrap();
        assert_eq!(w.tabs, v(&["b"]));
        assert_eq!(w.active.as_deref(), Some("b"));
        assert_eq!(w.pos, Some((10.0, 20.0)));
        assert!(f.set().empty_popouts().is_empty());
        // Nothing owns "zz", so no window is invented for it.
        assert_eq!(f.set().detach_to_new("zz", None), None);
        assert_eq!(f.popouts.len(), 2);
    }

    #[test]
    fn dissolve_returns_tabs_to_main_and_leaves_its_active_alone() {
        let mut f = fix();
        assert!(f.set().dissolve(1));
        assert_eq!(f.main, v(&["a", "b", "c", "d"]));
        assert_eq!(f.active.as_deref(), Some("b"));
        assert!(f.popouts.is_empty());
        assert!(!f.set().dissolve(1));
    }

    #[test]
    fn tearing_off_the_last_tab_of_a_window_leaves_it_prunable() {
        let mut f = fix();
        f.set().move_tab("c", ChatWinKey::Main, None);
        f.set().move_tab("d", ChatWinKey::Main, None);
        assert_eq!(f.set().empty_popouts(), vec![1]);
        // A fresh id never reuses the emptied one while it is still listed.
        assert_eq!(f.set().fresh_id(), 2);
    }

    #[test]
    fn popout_settings_round_trip() {
        let live = [
            ChatWindow {
                id: 4,
                tabs: v(&["a@x", "b@x"]),
                active: Some("b@x".to_owned()),
                pos: Some((3.0, 4.0)),
                size: Some((640.0, 480.0)),
                // Runtime-only state must not survive, and must not block the round-trip.
                geom_applied: true,
                focused: true,
                outer: Some(egui::Rect::EVERYTHING),
                inner: Some(egui::Rect::EVERYTHING),
            },
            ChatWindow { id: 9, tabs: v(&["c@x"]), ..Default::default() },
        ];
        let cfg: Vec<_> = live.iter().map(popout_cfg).collect();
        let back = popouts_from_cfg(&cfg);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].tabs, live[0].tabs);
        assert_eq!(back[0].active.as_deref(), Some("b@x"));
        assert_eq!(back[0].pos, Some((3.0, 4.0)));
        assert_eq!(back[0].size, Some((640.0, 480.0)));
        assert!(!back[0].geom_applied);
        assert!(back[0].outer.is_none());
        // A window with no explicit active tab lands on its first one.
        assert_eq!(back[1].active.as_deref(), Some("c@x"));
        // Stable after one pass: a blank `active` canonicalises to the first tab, and everything
        // then round-trips unchanged.
        let cfg2: Vec<_> = back.iter().map(popout_cfg).collect();
        assert_eq!(cfg2[1].active, "c@x");
        assert_eq!(popouts_from_cfg(&cfg2).iter().map(popout_cfg).collect::<Vec<_>>(), cfg2);
    }

    #[test]
    fn popouts_from_cfg_drops_junk_entries() {
        use crate::settings::ChatWindowCfg;
        let cfg = vec![
            ChatWindowCfg { id: 1, tabs: v(&["a"]), ..Default::default() },
            // Empty window: nothing to show, and it would be pruned on the first frame anyway.
            ChatWindowCfg { id: 2, tabs: Vec::new(), ..Default::default() },
            // Duplicate id: two windows would share one viewport.
            ChatWindowCfg { id: 1, tabs: v(&["b"]), ..Default::default() },
            // Active tab this window does not hold.
            ChatWindowCfg {
                id: 3,
                tabs: v(&["c"]),
                active: "elsewhere".to_owned(),
                ..Default::default()
            },
        ];
        let out = popouts_from_cfg(&cfg);
        assert_eq!(out.iter().map(|w| w.id).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(out[1].active.as_deref(), Some("c"));
    }

    #[test]
    fn popouts_from_cfg_honours_the_cap() {
        let cfg: Vec<crate::settings::ChatWindowCfg> = (1..=MAX_POPOUTS as u64 + 3)
            .map(|id| crate::settings::ChatWindowCfg {
                id,
                tabs: vec![format!("t{id}")],
                ..Default::default()
            })
            .collect();
        assert_eq!(popouts_from_cfg(&cfg).len(), MAX_POPOUTS);
    }

    #[test]
    fn reorder_index_maps_visible_slots_onto_the_full_tab_list() {
        let tabs = v(&["a", "b", "c", "d"]);
        // Only b, c and d fit on the bar; "a" is in the overflow dropdown.
        let centers =
            vec![("b".to_owned(), 10.0), ("c".to_owned(), 30.0), ("d".to_owned(), 50.0)];
        // Dropping "d" left of "b" puts it before "b" in the FULL list, i.e. after "a".
        assert_eq!(reorder_index(&tabs, &centers, "d", 0.0), 1);
        // Between c and d -> right before "d"'s old slot, which is the end once "d" is removed.
        assert_eq!(reorder_index(&tabs, &centers, "d", 40.0), 3);
        // Past the right edge -> the end.
        assert_eq!(reorder_index(&tabs, &centers, "b", 999.0), 3);
        // No other visible tab: nothing to order against.
        assert_eq!(reorder_index(&tabs, &[("b".to_owned(), 10.0)], "b", 5.0), 0);
    }

    #[test]
    fn insertion_index_boundaries() {
        let centers = [10.0, 30.0, 50.0];
        assert_eq!(insertion_index(&centers, 0.0), 0);
        assert_eq!(insertion_index(&centers, 10.0), 1);
        assert_eq!(insertion_index(&centers, 20.0), 1);
        assert_eq!(insertion_index(&centers, 49.9), 2);
        assert_eq!(insertion_index(&centers, 999.0), 3);
        assert_eq!(insertion_index(&[], 5.0), 0);
    }

    const TAB_LABELS: [&str; 4] = ["mgmt", "Ö ドクトリン", "", "a-very-long-channel-name"];

    /// Drives a real context with the pointer parked in the bar, so the hovered branch (which is
    /// the only one that draws the pop-out and close icons) renders too.
    fn run_tabs(at: egui::Pos2, mut f: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 200.0),
            )),
            ..Default::default()
        };
        input.events.push(egui::Event::PointerMoved(at));
        for _ in 0..2 {
            let _ = ctx.run_ui(input.clone(), &mut f);
            input.events.clear();
        }
    }

    fn draw_tab_matrix(ui: &mut egui::Ui) {
        let mut n = 0u32;
        for label in TAB_LABELS {
            for selected in [false, true] {
                for unread in [false, true] {
                    for mention in [false, true] {
                        for (closable, can_popout) in
                            [(false, false), (true, false), (true, true)]
                        {
                            for lead in
                                [TabLead::Dot(egui::Color32::RED), TabLead::Icon("\u{e000}")]
                            {
                                n += 1;
                                let hit = jabber_tab_box(
                                    ui,
                                    egui::Id::new(("t", n)),
                                    selected,
                                    unread,
                                    mention,
                                    lead,
                                    closable,
                                    can_popout,
                                    label,
                                );
                                let w = jabber_tab_width(ui, closable, unread, label);
                                assert!((hit.resp.rect.width() - w).abs() < 0.5);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn tab_box_renders_every_state_and_matches_its_measured_width() {
        egui::__run_test_ui(draw_tab_matrix);
        // Sweep the row so the hovered branch, and the icons it reveals, are laid out too.
        for x in [4.0, 40.0, 90.0, 160.0] {
            run_tabs(egui::pos2(x, 8.0), draw_tab_matrix);
        }
    }

    #[test]
    fn a_closable_tab_reserves_both_trailing_icons() {
        egui::__run_test_ui(|ui| {
            for label in TAB_LABELS {
                let plain = jabber_tab_width(ui, false, false, label);
                let unread = jabber_tab_width(ui, false, true, label);
                let closable = jabber_tab_width(ui, true, false, label);
                assert!(unread > plain);
                // Close X plus the pop-out slot, both with their leading gap.
                assert!((closable - plain - (TAB_GAP + TAB_POP_W + TAB_GAP + TAB_CLOSE_W)).abs() < 0.01);
            }
        });
    }

    #[test]
    fn ellipsized_tabs_stay_inside_the_minimum_width() {
        // Guards the MIN_TAB_W bump that paid for the pop-out icon: the boundary tab is ellipsized
        // rather than dropped, so its rendered width must never exceed the budget it was given.
        egui::__run_test_ui(|ui| {
            for label in TAB_LABELS {
                for budget in [96.0f32, 100.0, 140.0, 240.0] {
                    let lbl = ellipsize_tab_label(ui, true, false, label, budget);
                    let w = jabber_tab_width(ui, true, false, &lbl);
                    assert!(w <= budget + 0.01 || lbl == "\u{2026}", "{label:?} {budget} -> {lbl:?} {w}");
                }
            }
        });
    }
}

#[cfg(all(test, feature = "fc-rescue"))]
mod rescue_range_tests {
    use crate::geo::{SystemInfo, Systems};
    use crate::map::LY_METERS;
    use crate::store::MapSystem;
    use std::collections::HashMap;

    /// One system on the x axis, `ly` lightyears from the origin where staging sits.
    fn sys(id: i64, name: &str, ly: f64) -> MapSystem {
        MapSystem {
            id,
            name: name.to_owned(),
            security: -0.5,
            region_id: 10_000_060,
            x: ly * LY_METERS,
            y: 0.0,
            z: 0.0,
            x2d: 0.0,
            z2d: 0.0,
        }
    }

    const STAGING: i64 = 30_000_001;
    const TARGET: i64 = 30_000_100;
    const NEAR_A: i64 = 30_000_101;
    const NEAR_B: i64 = 30_000_102;
    const FAR_BUT_CLOSE: i64 = 30_000_110;

    /// Staging at 0 ly, the stranded capital at 10 ly, so the target is out of the 6 ly titan range
    /// and the warning is live. Three systems sit inside titan range of staging:
    ///
    /// - `FAR-BUT-CLOSE` at 5.9 ly, the nearest to the target on the map, but ten gates away.
    /// - `NEAR-A` at 3.0 ly and `NEAR-B` at 3.5 ly, both one gate from the target.
    ///
    /// The chain that connects `FAR-BUT-CLOSE` to the target is parked at 50+ ly so it cannot be
    /// chosen as a jump-off point itself.
    fn fixture() -> (Systems, Vec<MapSystem>) {
        let mut coords = vec![
            sys(STAGING, "STAGING", 0.0),
            sys(TARGET, "5J4K-9", 10.0),
            sys(NEAR_A, "NEAR-A", 3.0),
            sys(NEAR_B, "NEAR-B", 3.5),
            sys(FAR_BUT_CLOSE, "ZH-GKG", 5.9),
        ];
        let chain: Vec<i64> = (0..9).map(|i| 30_000_120 + i).collect();
        for (i, id) in chain.iter().enumerate() {
            coords.push(sys(*id, &format!("CHAIN-{i}"), 50.0 + i as f64));
        }

        let by_name: HashMap<String, SystemInfo> = coords
            .iter()
            .map(|s| {
                (
                    s.name.to_lowercase(),
                    SystemInfo {
                        id: s.id,
                        name: s.name.clone(),
                        security: s.security,
                        constellation: "C".into(),
                        region: "Delve".into(),
                        faction: String::new(),
                    },
                )
            })
            .collect();

        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut link = |a: i64, b: i64| {
            adjacency.entry(a).or_default().push(b);
            adjacency.entry(b).or_default().push(a);
        };
        link(TARGET, NEAR_A);
        link(TARGET, NEAR_B);
        let mut prev = TARGET;
        for id in &chain {
            link(prev, *id);
            prev = *id;
        }
        link(prev, FAR_BUT_CLOSE);
        (Systems::new(by_name, adjacency), coords)
    }

    fn pick(systems: &Systems, coords: &[MapSystem]) -> Option<String> {
        let at = |id: i64| coords.iter().find(|s| s.id == id).unwrap();
        super::best_jump_off(systems, coords, at(STAGING), TARGET, at(TARGET), 40)
            .map(|s| s.name.clone())
    }

    /// The defect: the jump-off point was ranked by lightyears to the target, so the fleet was sent
    /// to the system that merely looks nearest on the map.
    #[test]
    fn jump_off_is_the_fewest_jumps_out_not_the_nearest_on_the_map() {
        let (systems, coords) = fixture();
        let at = |id: i64| coords.iter().find(|s| s.id == id).unwrap();
        let target_pos = at(TARGET);

        // The old ranking's answer, stated here so the test fails loudly if it ever comes back.
        assert!(
            crate::map::ly_distance(at(FAR_BUT_CLOSE), target_pos)
                < crate::map::ly_distance(at(NEAR_B), target_pos),
            "fixture must keep the wrong system looking closest on the map"
        );
        assert_eq!(systems.jumps(FAR_BUT_CLOSE, TARGET, 40), Some(10));
        assert_eq!(systems.jumps(NEAR_B, TARGET, 40), Some(1));

        assert_eq!(pick(&systems, &coords).as_deref(), Some("NEAR-B"));
    }

    /// Two systems one jump out, which is the reported case. The nearer of the two in lightyears
    /// wins, so the choice is stable rather than dependent on system id order.
    #[test]
    fn ties_at_the_same_jump_count_go_to_the_nearer_one() {
        let (systems, coords) = fixture();
        assert_eq!(systems.jumps(NEAR_A, TARGET, 40), Some(1));
        assert_eq!(systems.jumps(NEAR_B, TARGET, 40), Some(1));
        // NEAR_B sits 6.5 ly from the target, NEAR_A 7.0.
        assert_eq!(pick(&systems, &coords).as_deref(), Some("NEAR-B"));
    }

    /// No route at all from anything in range: the warning still has to name a system and report
    /// no route, rather than vanishing and leaving the FC with nothing.
    #[test]
    fn unroutable_target_falls_back_to_the_nearest_on_the_map() {
        let (_systems, coords) = fixture();
        let by_name: HashMap<String, SystemInfo> = coords
            .iter()
            .map(|s| {
                (
                    s.name.to_lowercase(),
                    SystemInfo {
                        id: s.id,
                        name: s.name.clone(),
                        security: s.security,
                        constellation: "C".into(),
                        region: "Delve".into(),
                        faction: String::new(),
                    },
                )
            })
            .collect();
        let stranded = Systems::new(by_name, HashMap::new());
        assert_eq!(stranded.jumps(NEAR_B, TARGET, 40), None);
        assert_eq!(pick(&stranded, &coords).as_deref(), Some("ZH-GKG"));
    }
}

#[cfg(test)]
mod char_rings_tests {
    use super::*;
    use crate::uitest::fixtures;

    const DQ: i64 = 30_004_759; // 1DQ1-A
    const THREE: i64 = 30_004_608; // 319-3D, one gate from 1DQ1-A
    const K5: i64 = 30_003_704; // 7-K5EL, two gates from 1DQ1-A, one bridge in the bridged graph
    const JITA: i64 = 30_000_142; // no gate adjacency at all in this graph

    fn chars() -> Vec<(String, i64)> {
        [("Amryu", 90_000_001_i64), ("Scout", 90_000_002), ("Hauler", 90_000_003)]
            .into_iter()
            .map(|(n, id)| (n.to_owned(), id))
            .collect()
    }

    fn locs(at: &[(&str, i64, bool)]) -> std::collections::HashMap<String, (i64, bool)> {
        at.iter().map(|(n, s, d)| ((*n).to_owned(), (*s, *d))).collect()
    }

    fn rings(
        systems: &std::sync::Arc<crate::geo::Systems>,
        at: &[(&str, i64, bool)],
        disabled: &[&str],
        only_undocked: bool,
        use_bridges: bool,
    ) -> CharRings {
        let disabled: Vec<String> = disabled.iter().map(|s| (*s).to_owned()).collect();
        build_char_rings(
            &Some(systems.clone()),
            &chars(),
            &locs(at),
            "Amryu",
            None,
            &disabled,
            only_undocked,
            use_bridges,
        )
    }

    fn names_and_jumps(c: &CardChars) -> Vec<(String, Option<u32>)> {
        c.hops.iter().map(|h| (h.name.clone(), h.jumps)).collect()
    }

    #[test]
    fn alert_candidate_truth_table() {
        let off = ["scout".to_owned()];
        assert!(alert_candidate("Amryu", false, &off, false));
        assert!(!alert_candidate("Scout", false, &off, false), "deny list is case-insensitive");
        assert!(alert_candidate("Amryu", true, &off, false), "docked counts while the gate is off");
        assert!(!alert_candidate("Amryu", true, &off, true), "docked is out while the gate is on");
        assert!(alert_candidate("Amryu", false, &off, true));
    }

    #[test]
    fn rings_are_nearest_first_and_carry_the_selected_index() {
        let sys = fixtures::systems();
        let at = [("Amryu", K5, false), ("Scout", THREE, false), ("Hauler", DQ, false)];
        let c = rings(&sys, &at, &[], false, false).card(Some(DQ));
        assert_eq!(
            names_and_jumps(&c),
            vec![
                ("Hauler".to_owned(), Some(0)),
                ("Scout".to_owned(), Some(1)),
                ("Amryu".to_owned(), Some(2)),
            ]
        );
        assert_eq!(c.selected, Some(2), "the active character is Amryu, two jumps out");
        assert_eq!(c.nearest().map(|h| h.name.as_str()), Some("Hauler"));
        assert_eq!(c.second().map(|h| h.name.as_str()), Some("Amryu"));
    }

    #[test]
    fn a_deny_listed_character_is_dropped_but_never_the_active_one() {
        let sys = fixtures::systems();
        let at = [("Amryu", K5, false), ("Scout", THREE, false), ("Hauler", DQ, false)];

        let c = rings(&sys, &at, &["Scout"], false, false).card(Some(DQ));
        assert_eq!(
            names_and_jumps(&c),
            vec![("Hauler".to_owned(), Some(0)), ("Amryu".to_owned(), Some(2))]
        );

        // Alerts off for the character you are looking through: the card still has to quote it,
        // or the number the rest of the app shows has no owner on screen.
        let c = rings(&sys, &at, &["Amryu"], false, false).card(Some(DQ));
        assert_eq!(c.hops.len(), 3);
        assert_eq!(c.selected, Some(2));
    }

    #[test]
    fn docked_characters_follow_the_only_undocked_setting() {
        let sys = fixtures::systems();
        let at = [("Amryu", K5, false), ("Scout", THREE, true), ("Hauler", DQ, false)];
        let c = rings(&sys, &at, &[], true, false).card(Some(DQ));
        assert_eq!(
            names_and_jumps(&c),
            vec![("Hauler".to_owned(), Some(0)), ("Amryu".to_owned(), Some(2))],
            "Scout is docked and the setting is on"
        );
        let c = rings(&sys, &at, &[], false, false).card(Some(DQ));
        assert_eq!(c.hops.len(), 3, "the same character counts while the setting is off");
    }

    #[test]
    fn a_tie_goes_to_the_active_character() {
        let sys = fixtures::systems();
        let at = [("Amryu", THREE, false), ("Scout", THREE, false)];
        let c = rings(&sys, &at, &[], false, false).card(Some(DQ));
        assert_eq!(c.hops[0].name, "Amryu", "an alt beside you must not take the badge off you");
        assert_eq!(c.selected, Some(0));
        assert_eq!(c.second(), None, "nearest is the selected one, so there is no second slot");
    }

    #[test]
    fn card_carries_light_years_from_staging_and_you() {
        let sys = fixtures::systems();
        let r = fixtures::intel_typical();
        let c = rings(&sys, &[("Amryu", K5, false)], &[], false, false)
            .with_staging(Some(" 319-3d "))
            .card_for(&r);
        assert!(c.hops.is_empty(), "one character still draws the plain number");
        assert_eq!(c.ly.staging, "319-3D");
        assert_eq!(c.ly.you, "Amryu");
        assert_eq!(c.ly.of(DQ), Some((Some(210), Some(533))));
        assert_eq!(ly_line(533, "Amryu"), "5.33 ly from Amryu");
        assert_eq!(ly_line(7, "x"), "0.07 ly from x");
    }

    #[test]
    fn unknown_staging_keeps_only_your_distance() {
        let sys = fixtures::systems();
        let r = fixtures::intel_typical();
        let c = rings(&sys, &[("Amryu", THREE, false)], &[], false, false)
            .with_staging(Some("Nowhere"))
            .card_for(&r);
        assert_eq!(c.ly.of(DQ), Some((None, Some(210))));
        let c = rings(&sys, &[], &[], false, false).with_staging(None).card_for(&r);
        assert_eq!(c.ly, CardLy::default(), "no staging and no location, nothing to show");
    }

    #[test]
    fn one_character_leaves_the_card_alone() {
        let sys = fixtures::systems();
        let c = rings(&sys, &[("Amryu", K5, false)], &[], false, false).card(Some(DQ));
        assert_eq!(c, CardChars::default(), "nothing to disambiguate, so no badge and no hops");
    }

    #[test]
    fn each_character_gets_its_own_bridge_verdict() {
        let sys = fixtures::systems_bridged();
        let at = [("Amryu", DQ, false), ("Scout", THREE, false)];

        let c = rings(&sys, &at, &[], false, true).card(Some(K5));
        assert_eq!(names_and_jumps(&c), vec![
            ("Amryu".to_owned(), Some(1)),
            ("Scout".to_owned(), Some(1)),
        ]);
        assert_eq!(c.hops[0].via, JumpVia::BridgeShorter(2), "Amryu rides the bridge");
        assert_eq!(c.hops[1].via, JumpVia::Gates, "Scout walks one gate either way");

        let c = rings(&sys, &at, &[], false, false).card(Some(K5));
        assert!(c.hops.iter().all(|h| h.via == JumpVia::Gates), "bridges off, nothing to flag");
        assert_eq!(c.hops[0].jumps, Some(1), "Scout is now the nearest, at one gate");
    }

    #[test]
    fn an_unreachable_character_sorts_last_and_keeps_no_number() {
        // An alt parked in Jita while you are in Delve: this graph has no gate route between them.
        let sys = fixtures::systems();
        let at = [("Amryu", DQ, false), ("Scout", JITA, false)];
        let c = rings(&sys, &at, &[], false, false).card(Some(DQ));
        assert_eq!(names_and_jumps(&c), vec![
            ("Amryu".to_owned(), Some(0)),
            ("Scout".to_owned(), None),
        ]);
    }

    #[test]
    fn a_bridge_only_target_is_flagged_for_every_character_that_rides_it() {
        let sys = fixtures::systems_bridged_island();
        let at = [("Amryu", DQ, false), ("Scout", THREE, false)];
        let c = rings(&sys, &at, &[], false, true).card(Some(JITA));
        assert_eq!(names_and_jumps(&c), vec![
            ("Amryu".to_owned(), Some(1)),
            ("Scout".to_owned(), Some(2)),
        ]);
        assert!(
            c.hops.iter().all(|h| h.via == JumpVia::BridgeOnly),
            "no gate route reaches Jita in this graph, so both numbers only exist as bridges"
        );

        let c = rings(&sys, &at, &[], false, false).card(Some(JITA));
        assert!(c.hops.iter().all(|h| h.jumps.is_none()), "gates alone reach nobody");
    }

    #[test]
    fn a_ball_is_cached_per_graph_and_origin() {
        let sys = fixtures::systems();
        let a = distance_ball(&sys, DQ, false);
        assert!(std::sync::Arc::ptr_eq(&a, &distance_ball(&sys, DQ, false)));
        assert!(!std::sync::Arc::ptr_eq(&a, &distance_ball(&sys, THREE, false)), "keyed on origin");
        assert!(!std::sync::Arc::ptr_eq(&a, &distance_ball(&sys, DQ, true)), "keyed on the graph");

        let rebuilt = fixtures::systems();
        assert!(
            !std::sync::Arc::ptr_eq(&a, &distance_ball(&rebuilt, DQ, false)),
            "a rebuilt graph invalidates, which is how a bridge edit lands"
        );
    }

    /// The alert engine's own distance, which had no test at all.
    #[test]
    fn min_jumps_from_takes_the_nearest_source() {
        let sys = Some(fixtures::systems());
        assert_eq!(min_jumps_from(&sys, &[K5, THREE], Some(DQ), false), Some(1));
        assert_eq!(min_jumps_from(&sys, &[K5], Some(DQ), false), Some(2));
        assert_eq!(min_jumps_from(&sys, &[], Some(DQ), false), None, "no character, no distance");
        assert_eq!(min_jumps_from(&sys, &[DQ], Some(JITA), false), None, "unreachable");

        let bridged = Some(fixtures::systems_bridged());
        assert_eq!(min_jumps_from(&bridged, &[DQ], Some(K5), true), Some(1));
        assert_eq!(min_jumps_from(&bridged, &[DQ], Some(K5), false), Some(2), "gates only");
    }
}

#[cfg(test)]
mod jabber_room_tests {
    use super::*;

    const ROOM: &str = "delve@conference.goonfleet.com";
    const DM: &str = "someguy@goonfleet.com";

    fn app() -> (egui::Context, SpaiApp) {
        let ctx = egui::Context::default();
        let app = SpaiApp::build(&ctx, true);
        (ctx, app)
    }

    fn frame(rooms: &[&str], unread: &[&str], mentions: &[&str]) -> JabberFrame {
        JabberFrame {
            configured: true,
            ever_online: true,
            connected: true,
            status: String::new(),
            convos: Vec::new(),
            pings: Vec::new(),
            rooms: rooms.iter().map(|s| (*s).to_owned()).collect(),
            dm_keys: Vec::new(),
            unread: unread.iter().map(|s| (*s).to_owned()).collect(),
            mentions: mentions.iter().map(|s| (*s).to_owned()).collect(),
            pings_unread: false,
            channels: Vec::new(),
            inaccessible: Vec::new(),
            subjects: Default::default(),
        }
    }

    #[test]
    fn hidden_room_survives_ordinary_traffic() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned()];
        a.settings.jabber_closed_rooms = vec![ROOM.to_owned()];
        a.jabber_reconcile(&frame(&[ROOM], &[ROOM], &[]));
        assert!(a.jabber_tabs.is_empty(), "a message un-hid a room hidden on purpose");
        assert_eq!(a.settings.jabber_closed_rooms, vec![ROOM.to_owned()]);
    }

    #[test]
    fn being_named_in_a_hidden_room_brings_the_tab_back() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned()];
        a.settings.jabber_closed_rooms = vec![ROOM.to_owned()];
        a.jabber_reconcile(&frame(&[ROOM], &[ROOM], &[ROOM]));
        assert_eq!(a.jabber_tabs, vec![ROOM.to_owned()]);
        assert!(a.settings.jabber_closed_rooms.is_empty());
    }

    #[test]
    fn a_closed_dm_still_reopens_on_a_plain_message() {
        let (_ctx, mut a) = app();
        a.settings.jabber_closed_dms = vec![DM.to_owned()];
        let mut f = frame(&[], &[DM], &[]);
        f.dm_keys = vec![DM.to_owned()];
        a.jabber_reconcile(&f);
        assert_eq!(a.jabber_tabs, vec![DM.to_owned()]);
    }

    /// The X is not a leave any more: it hides, and the room stays joined.
    #[test]
    fn closing_a_room_tab_only_hides_it() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned()];
        a.jabber.lock().unwrap().rooms.insert(ROOM.to_owned());
        a.jabber_tabs = vec![ROOM.to_owned()];
        a.close_jabber_tab(ROOM, true);
        assert_eq!(a.settings.jabber_closed_rooms, vec![ROOM.to_owned()]);
        assert_eq!(a.settings.jabber_rooms, vec![ROOM.to_owned()], "the X left the room");
        assert!(a.settings.jabber_left_rooms.is_empty(), "the X left the room");
        assert!(a.jabber.lock().unwrap().rooms.contains(ROOM), "the X left the room");
        assert!(a.jabber_tabs.is_empty());
        // Still hidden after a reconcile that sees it joined.
        a.jabber_reconcile(&frame(&[ROOM], &[ROOM], &[]));
        assert!(a.jabber_tabs.is_empty());
    }

    #[test]
    fn leaving_a_room_is_permanent() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned()];
        a.jabber_tabs = vec![ROOM.to_owned()];
        // Leaving is the sidebar's remove button now, the only path that leaves.
        a.jabber_forget(ROOM, true);
        assert_eq!(a.settings.jabber_left_rooms, vec![ROOM.to_owned()]);
        assert!(a.settings.jabber_rooms.is_empty(), "we would rejoin it on the next start");
        assert!(!a.jabber.lock().unwrap().rooms.contains(ROOM));

        // A room message still in flight past the leave must not resurrect it.
        assert!(!crate::jabber::note_room_seen(&a.jabber, ROOM));
        assert!(!a.jabber.lock().unwrap().rooms.contains(ROOM));

        // Nor may a reconcile with no live rooms put it back in the join list.
        a.jabber_reconcile(&frame(&[], &[], &[]));
        assert!(a.settings.jabber_rooms.is_empty());
        assert_eq!(a.settings.jabber_left_rooms, vec![ROOM.to_owned()]);
        assert!(a.jabber_tabs.is_empty());
    }

    #[test]
    fn a_left_room_is_not_joined_on_the_next_start() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned(), "other@conference.x".to_owned()];
        a.settings.jabber_left_rooms = vec![ROOM.to_owned()];
        assert_eq!(a.jabber_rooms_to_join(), vec!["other@conference.x".to_owned()]);
    }

    #[test]
    fn a_server_force_join_overrides_a_leave() {
        let (_ctx, mut a) = app();
        a.settings.jabber_left_rooms = vec![ROOM.to_owned()];
        a.jabber.lock().unwrap().rooms_left.insert(ROOM.to_owned());
        // Self-presence from the MUC: the server put us back in.
        crate::jabber::note_room_joined(&a.jabber, ROOM);
        a.jabber_reconcile(&frame(&[ROOM], &[], &[]));
        assert!(a.settings.jabber_left_rooms.is_empty());
        assert_eq!(a.settings.jabber_rooms, vec![ROOM.to_owned()]);
        assert!(a.jabber_frame(false).channels.iter().any(|c| c.jid == ROOM));
        // First sight of a server-driven join opens the tab once (UI-047).
        assert_eq!(a.jabber_tabs, vec![ROOM.to_owned()]);
    }

    #[test]
    fn a_left_room_is_neither_a_dm_nor_a_channel_row() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned()];
        {
            let mut st = a.jabber.lock().unwrap();
            st.chats.insert(ROOM.to_owned(), Vec::new());
            st.rooms.insert(ROOM.to_owned());
        }
        let f = a.jabber_frame(false);
        assert!(f.channels.iter().any(|c| c.jid == ROOM));

        a.jabber_forget(ROOM, true);
        let f = a.jabber_frame(false);
        assert!(!f.dm_keys.contains(&ROOM.to_owned()), "left room came back as a DM");
        assert!(!f.channels.iter().any(|c| c.jid == ROOM));
    }
}

#[cfg(test)]
mod jabber_forget_tests {
    use super::*;

    const ROOM: &str = "delve@conference.goonfleet.com";
    const DM: &str = "someguy@goonfleet.com";

    fn app() -> (egui::Context, SpaiApp) {
        let ctx = egui::Context::default();
        let app = SpaiApp::build(&ctx, true);
        (ctx, app)
    }

    fn seed_history(a: &SpaiApp, jid: &str) {
        a.jabber.lock().unwrap().chats.insert(
            jid.to_owned(),
            vec![crate::jabber::ChatMsg {
                from: "someone".to_owned(),
                body: "hi".to_owned(),
                time: 0,
                outgoing: false,
            }],
        );
    }

    #[test]
    fn forgetting_a_room_acts_as_if_we_were_never_in_it() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned()];
        a.settings.jabber_closed_rooms = vec![ROOM.to_owned()];
        a.settings.jabber_inaccessible_rooms = vec![ROOM.to_owned()];
        a.settings.jabber_room_subjects.insert(ROOM.to_owned(), "MOTD".to_owned());
        a.settings.jabber_contacts = vec![ROOM.to_owned()];
        seed_history(&a, ROOM);
        {
            let mut st = a.jabber.lock().unwrap();
            st.rooms.insert(ROOM.to_owned());
            st.room_subjects.insert(ROOM.to_owned(), "MOTD".to_owned());
        }
        a.jabber_tabs = vec![ROOM.to_owned()];
        a.jabber_chat = Some(ROOM.to_owned());

        a.jabber_forget(ROOM, true);

        assert!(a.settings.jabber_rooms.is_empty());
        assert!(a.settings.jabber_closed_rooms.is_empty());
        assert!(a.settings.jabber_inaccessible_rooms.is_empty());
        assert!(a.settings.jabber_room_subjects.is_empty(), "the MOTD outlived the room");
        assert!(a.settings.jabber_contacts.is_empty());
        assert_eq!(a.settings.jabber_forgotten, vec![ROOM.to_owned()]);
        assert!(a.jabber_tabs.is_empty());
        assert_eq!(a.jabber_chat, None);

        // Gone from every list, in both panes.
        let f = a.jabber_frame(false);
        assert!(!f.channels.iter().any(|c| c.jid == ROOM));
        assert!(!f.convos.iter().any(|c| c.jid == ROOM));
        assert!(!f.dm_keys.contains(&ROOM.to_owned()));
    }

    /// Removing a channel from the sidebar closes its tab, in whichever window holds it, and the
    /// pop-out that is left empty goes with it.
    #[test]
    fn forgetting_closes_the_tab_in_a_popout_too() {
        let (_ctx, mut a) = app();
        a.jabber_tabs = vec![DM.to_owned()];
        a.jabber_chat = Some(DM.to_owned());
        a.jabber_popouts = vec![ChatWindow {
            id: 1,
            tabs: vec![ROOM.to_owned()],
            active: Some(ROOM.to_owned()),
            ..Default::default()
        }];
        a.jabber.lock().unwrap().rooms.insert(ROOM.to_owned());

        a.jabber_forget(ROOM, true);
        assert!(a.jabber_popouts[0].tabs.is_empty());
        assert_eq!(a.jabber_popouts[0].active, None);
        // The main window is untouched.
        assert_eq!(a.jabber_tabs, vec![DM.to_owned()]);
        assert_eq!(a.jabber_chat.as_deref(), Some(DM));

        // reconcile prunes the emptied pop-out and must not resurrect the tab.
        let mut f = a.jabber_frame(false);
        f.configured = true;
        f.ever_online = true;
        a.jabber_reconcile(&f);
        assert!(a.jabber_popouts.is_empty(), "an empty pop-out was left on screen");
        assert_eq!(a.jabber_tabs, vec![DM.to_owned()]);
    }

    #[test]
    fn forgetting_keeps_the_chat_history() {
        let (_ctx, mut a) = app();
        seed_history(&a, ROOM);
        a.jabber.lock().unwrap().rooms.insert(ROOM.to_owned());
        a.jabber_forget(ROOM, true);
        let st = a.jabber.lock().unwrap();
        assert_eq!(st.chats.get(ROOM).map(Vec::len), Some(1), "history was destroyed");
    }

    #[test]
    fn forgetting_a_joined_room_leaves_it() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![ROOM.to_owned()];
        a.jabber.lock().unwrap().rooms.insert(ROOM.to_owned());
        a.jabber_forget(ROOM, true);
        assert_eq!(a.settings.jabber_left_rooms, vec![ROOM.to_owned()]);
        assert!(!a.jabber.lock().unwrap().rooms.contains(ROOM));
        assert!(a.jabber_rooms_to_join().is_empty());
    }

    #[test]
    fn a_forgotten_dm_comes_back_on_a_new_message() {
        let (_ctx, mut a) = app();
        seed_history(&a, DM);
        a.jabber_forget(DM, false);
        assert_eq!(a.settings.jabber_forgotten, vec![DM.to_owned()]);

        let mut f = a.jabber_frame(false);
        assert!(!f.convos.iter().any(|c| c.jid == DM));
        // jabber_frame reports the headless app as unconfigured, and reconcile no-ops on that.
        f.configured = true;
        f.ever_online = true;
        f.unread.insert(DM.to_owned());
        f.dm_keys = vec![DM.to_owned()];
        a.jabber_reconcile(&f);
        assert!(a.settings.jabber_forgotten.is_empty(), "a new message was swallowed");
        assert_eq!(a.jabber_tabs, vec![DM.to_owned()]);
    }

    #[test]
    fn a_forgotten_room_comes_back_on_a_force_join() {
        let (_ctx, mut a) = app();
        a.jabber.lock().unwrap().rooms.insert(ROOM.to_owned());
        a.jabber_forget(ROOM, true);
        crate::jabber::note_room_joined(&a.jabber, ROOM);
        let mut f = a.jabber_frame(false);
        f.configured = true;
        f.ever_online = true;
        a.jabber_reconcile(&f);
        assert!(a.settings.jabber_forgotten.is_empty());
        assert!(a.settings.jabber_left_rooms.is_empty());
        assert_eq!(a.settings.jabber_rooms, vec![ROOM.to_owned()]);
        assert!(a.jabber_frame(false).channels.iter().any(|c| c.jid == ROOM));
        assert_eq!(a.jabber_tabs, vec![ROOM.to_owned()], "a force-join did not surface");
    }

    /// The button is offered per row, and a roster row must not carry one: the server owns the
    /// roster and would push the contact straight back.
    #[test]
    fn roster_rows_are_not_forgettable_but_remembered_ones_are() {
        let (_ctx, a) = app();
        seed_history(&a, DM);
        a.jabber.lock().unwrap().roster.insert(
            "friend@goonfleet.com".to_owned(),
            crate::jabber::Contact {
                name: Some("Friend".to_owned()),
                groups: vec!["Corp".to_owned()],
                presence: crate::jabber::Presence::default(),
                status_text: String::new(),
            },
        );
        let f = a.jabber_frame(false);
        let roster = f.convos.iter().find(|c| c.jid == "friend@goonfleet.com").unwrap();
        let remembered = f.convos.iter().find(|c| c.jid == DM).unwrap();
        assert!(roster.in_roster);
        assert!(!remembered.in_roster);
        assert_eq!(remembered.group, "Other");
    }
}

#[cfg(test)]
mod eve_time_label_tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn same_day_carries_seconds() {
        // 2026-09-01 12:02:35 UTC
        let ts = 1_788_264_155;
        assert_eq!(eve_time_label(ts, ts), "EVE 12:02:35");
    }

    #[test]
    fn an_older_message_keeps_its_date_and_gains_seconds() {
        let ts = 1_788_264_155;
        assert_eq!(eve_time_label(ts, ts + DAY), "EVE 2026/09/01 12:02:35");
    }

    /// The whole point: two messages inside the same minute must read differently. +10s, not +30,
    /// because 12:02:35 + 30 lands in 12:03 and the old format would have passed this vacuously.
    #[test]
    fn two_messages_in_one_minute_are_distinguishable() {
        let ts = 1_788_264_155;
        let a = eve_time_label(ts, ts);
        let b = eve_time_label(ts + 10, ts);
        assert!(a.starts_with("EVE 12:02:") && b.starts_with("EVE 12:02:"), "{a} / {b} not one minute");
        assert_ne!(a, b, "{a} and {b} stamp identically");
    }

    /// Both windows read the same helper, so neither can drift.
    #[test]
    fn the_jabber_and_rescue_windows_share_one_format() {
        let ts = 1_788_264_155;
        assert_eq!(eve_time_label(ts, ts).matches(':').count(), 2);
    }

    #[test]
    fn an_unrepresentable_timestamp_renders_nothing() {
        assert_eq!(eve_time_label(i64::MAX, 0), "");
    }
}

#[cfg(all(test, feature = "fc-rescue"))]
mod jabber_rescue_room_tests {
    use super::*;

    const RESCUE: &str = "delve911@conference.goonfleet.com";
    const SKIRMISH: &str = "skirmish_commanders@conference.goonfleet.com";
    const OTHER: &str = "corp@conference.goonfleet.com";

    fn app(rescue_on: bool) -> (egui::Context, SpaiApp) {
        let ctx = egui::Context::default();
        let mut a = SpaiApp::build(&ctx, true);
        a.settings.fc_rescue_enabled = rescue_on;
        (ctx, a)
    }

    fn frame(rooms: &[&str]) -> JabberFrame {
        JabberFrame {
            configured: true,
            ever_online: true,
            connected: true,
            status: String::new(),
            convos: Vec::new(),
            pings: Vec::new(),
            rooms: rooms.iter().map(|s| (*s).to_owned()).collect(),
            dm_keys: Vec::new(),
            unread: Default::default(),
            mentions: Default::default(),
            pings_unread: false,
            channels: Vec::new(),
            inaccessible: Vec::new(),
            subjects: Default::default(),
        }
    }

    #[test]
    fn rescue_mode_pins_both_rooms() {
        let (_ctx, a) = app(true);
        assert_eq!(a.jabber_rescue_rooms(), vec![RESCUE.to_owned(), SKIRMISH.to_owned()]);
        // Joined on connect even though nothing put them in jabber_rooms.
        assert_eq!(a.jabber_rooms_to_join(), vec![RESCUE.to_owned(), SKIRMISH.to_owned()]);
    }

    #[test]
    fn explicit_room_jids_override_the_defaults() {
        let (_ctx, mut a) = app(true);
        a.settings.rescue_delve911_jid = "rescue@conference.example.com".to_owned();
        a.settings.rescue_skirmish_jid = "fc@conference.example.com".to_owned();
        assert_eq!(
            a.jabber_rescue_rooms(),
            vec!["rescue@conference.example.com".to_owned(), "fc@conference.example.com".to_owned()]
        );
    }

    #[test]
    fn nothing_is_pinned_with_rescue_mode_off() {
        let (_ctx, mut a) = app(false);
        assert!(a.jabber_rescue_rooms().is_empty());
        a.settings.jabber_rooms = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        a.settings.jabber_left_rooms = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        assert!(a.jabber_rooms_to_join().is_empty(), "a left room was joined with rescue off");
    }

    #[test]
    fn neither_pinned_room_can_be_forgotten() {
        for room in [RESCUE, SKIRMISH] {
            let (_ctx, mut a) = app(true);
            a.settings.jabber_rooms = vec![room.to_owned()];
            a.jabber.lock().unwrap().rooms.insert(room.to_owned());
            a.jabber_forget(room, true);
            assert!(a.settings.jabber_forgotten.is_empty(), "{room} was forgotten");
            assert!(a.settings.jabber_left_rooms.is_empty(), "{room} was left");
            assert_eq!(a.settings.jabber_rooms, vec![room.to_owned()]);
            assert!(a.jabber.lock().unwrap().rooms.contains(room), "we left {room}");
        }
    }

    /// The tab X stays usable on it. Hiding keeps the room joined, so the parser keeps reading.
    /// Held open, not merely joined: the FC must not have to go looking for these.
    #[test]
    fn the_pinned_rooms_cannot_be_closed() {
        let (_ctx, mut a) = app(true);
        a.settings.jabber_rooms = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        {
            let mut st = a.jabber.lock().unwrap();
            st.rooms.insert(RESCUE.to_owned());
            st.rooms.insert(SKIRMISH.to_owned());
        }
        a.jabber_tabs = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        a.close_jabber_tab(RESCUE, true);
        a.close_jabber_tab(SKIRMISH, true);
        assert!(a.settings.jabber_closed_rooms.is_empty());
        assert_eq!(a.jabber_tabs, vec![RESCUE.to_owned(), SKIRMISH.to_owned()]);
    }

    /// The whole guarantee in one walk: joined, held open, un-leavable, un-forgettable, and
    /// receiving. Every link in the chain the rescue parser depends on.
    #[test]
    fn the_rescue_rooms_are_open_and_working_end_to_end() {
        let (_ctx, mut a) = app(true);
        // A profile that had left and forgotten both, with no tabs and nothing in jabber_rooms.
        a.settings.jabber_left_rooms = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        a.settings.jabber_forgotten = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        a.settings.jabber_closed_rooms = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        a.jabber.lock().unwrap().rooms_left.insert(RESCUE.to_owned());

        // Offline reconcile: the join list is repaired before we ever connect.
        let mut off = frame(&[]);
        off.configured = false;
        off.ever_online = false;
        a.jabber_reconcile(&off);
        assert_eq!(a.jabber_rooms_to_join(), vec![RESCUE.to_owned(), SKIRMISH.to_owned()]);

        // Connected: both joined, both hold a tab.
        crate::jabber::note_room_joined(&a.jabber, RESCUE);
        crate::jabber::note_room_joined(&a.jabber, SKIRMISH);
        a.jabber_reconcile(&frame(&[RESCUE, SKIRMISH]));
        assert!(a.jabber_tabs.contains(&RESCUE.to_owned()), "delve911 has no tab");
        assert!(a.jabber_tabs.contains(&SKIRMISH.to_owned()), "skirmish has no tab");

        // Receiving: neither is muted at the store gate, which is what feeds the parser.
        assert!(crate::jabber::note_room_seen(&a.jabber, RESCUE));
        assert!(crate::jabber::note_room_seen(&a.jabber, SKIRMISH));

        // Every removal path refuses, and the tabs survive another reconcile.
        for room in [RESCUE, SKIRMISH] {
            a.jabber_forget(room, true);
            a.close_jabber_tab(room, true);
        }
        a.jabber_reconcile(&frame(&[RESCUE, SKIRMISH]));
        assert!(a.settings.jabber_left_rooms.is_empty());
        assert!(a.settings.jabber_forgotten.is_empty());
        assert!(a.settings.jabber_closed_rooms.is_empty());
        assert!(a.jabber_tabs.contains(&RESCUE.to_owned()));
        assert!(a.jabber_tabs.contains(&SKIRMISH.to_owned()));
        assert!(a.jabber.lock().unwrap().rooms.contains(RESCUE));
        assert!(a.jabber.lock().unwrap().rooms.contains(SKIRMISH));

        // And they survive a restart: the tab bar is saved with them in it.
        a.sync_popout_settings();
        assert!(a.settings.jabber_main_tabs.contains(&RESCUE.to_owned()));
        assert!(a.settings.jabber_main_tabs.contains(&SKIRMISH.to_owned()));
    }

    /// The reported profile's shape: the room was left or forgotten before it was pinned.
    #[test]
    fn previously_left_rescue_rooms_heal_on_reconcile() {
        let (_ctx, mut a) = app(true);
        a.settings.jabber_left_rooms =
            vec![RESCUE.to_owned(), OTHER.to_owned(), SKIRMISH.to_owned()];
        a.settings.jabber_forgotten = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        a.jabber.lock().unwrap().rooms_left.insert(RESCUE.to_owned());

        a.jabber_reconcile(&frame(&[]));

        assert_eq!(a.settings.jabber_left_rooms, vec![OTHER.to_owned()], "healed the wrong room");
        assert!(a.settings.jabber_forgotten.is_empty());
        assert_eq!(a.settings.jabber_rooms, vec![RESCUE.to_owned(), SKIRMISH.to_owned()]);
        assert!(!a.jabber.lock().unwrap().rooms_left.contains(RESCUE));
        assert_eq!(a.jabber_rooms_to_join(), vec![RESCUE.to_owned(), SKIRMISH.to_owned()]);
    }

    /// Healing must not depend on being online: the join list has to be right before we connect.
    #[test]
    fn healing_happens_while_offline_too() {
        let (_ctx, mut a) = app(true);
        a.settings.jabber_left_rooms = vec![RESCUE.to_owned(), SKIRMISH.to_owned()];
        let mut f = frame(&[]);
        f.configured = false;
        f.ever_online = false;
        a.jabber_reconcile(&f);
        assert!(a.settings.jabber_left_rooms.is_empty());
    }
}

#[cfg(test)]
mod jabber_tab_persist_tests {
    use super::*;

    const ROOM: &str = "delve@conference.goonfleet.com";
    const OTHER: &str = "corp@conference.goonfleet.com";
    const DM: &str = "someguy@goonfleet.com";

    fn frame(rooms: &[&str], dms: &[&str], unread: &[&str], mentions: &[&str]) -> JabberFrame {
        JabberFrame {
            configured: true,
            ever_online: true,
            connected: true,
            status: String::new(),
            convos: Vec::new(),
            pings: Vec::new(),
            rooms: rooms.iter().map(|s| (*s).to_owned()).collect(),
            dm_keys: dms.iter().map(|s| (*s).to_owned()).collect(),
            unread: unread.iter().map(|s| (*s).to_owned()).collect(),
            mentions: mentions.iter().map(|s| (*s).to_owned()).collect(),
            pings_unread: false,
            channels: Vec::new(),
            inaccessible: Vec::new(),
            subjects: Default::default(),
        }
    }

    fn app_with(settings: crate::settings::Settings) -> (egui::Context, SpaiApp) {
        let ctx = egui::Context::default();
        let mut a = SpaiApp::build(&ctx, true);
        a.settings = settings;
        // Same call `build` makes, so the helper cannot drift from the real restore.
        let (tabs, active) = restored_main_tabs(&a.settings);
        a.jabber_tabs = tabs;
        a.jabber_chat = active;
        (ctx, a)
    }

    /// The headline: five joined rooms and a DM with history, none of them opened by the user.
    #[test]
    fn a_joined_room_does_not_open_a_tab_by_itself() {
        let mut s = crate::settings::Settings::default();
        // Already known, so not a first-sight force-join (UI-047).
        s.jabber_rooms = vec![ROOM.to_owned(), OTHER.to_owned()];
        let (_ctx, mut a) = app_with(s);
        a.jabber_reconcile(&frame(&[ROOM, OTHER], &[DM], &[], &[]));
        assert!(a.jabber_tabs.is_empty(), "reconcile opened {:?}", a.jabber_tabs);
    }

    #[test]
    fn only_the_previously_open_tabs_come_back() {
        let mut s = crate::settings::Settings::default();
        s.jabber_main_tabs = vec![ROOM.to_owned()];
        s.jabber_main_active = ROOM.to_owned();
        s.jabber_rooms = vec![ROOM.to_owned(), OTHER.to_owned()];
        let (_ctx, mut a) = app_with(s);
        // OTHER and DM are just as reachable, and stay shut.
        a.jabber_reconcile(&frame(&[ROOM, OTHER], &[DM], &[], &[]));
        assert_eq!(a.jabber_tabs, vec![ROOM.to_owned()]);
        assert_eq!(a.jabber_chat.as_deref(), Some(ROOM));
    }

    /// A restored tab must survive the first reconcile, which is where `reconcile_tabs` prunes
    /// anything not in the wanted set.
    #[test]
    fn a_restored_tab_is_not_pruned_on_the_first_frame() {
        let mut s = crate::settings::Settings::default();
        s.jabber_main_tabs = vec![ROOM.to_owned(), DM.to_owned()];
        let (_ctx, mut a) = app_with(s);
        a.jabber_reconcile(&frame(&[ROOM], &[DM], &[], &[]));
        assert_eq!(a.jabber_tabs, vec![ROOM.to_owned(), DM.to_owned()]);
    }

    /// A room we are no longer in keeps its tab: the history is still readable.
    #[test]
    fn a_restored_tab_survives_the_room_being_gone() {
        let mut s = crate::settings::Settings::default();
        s.jabber_main_tabs = vec![ROOM.to_owned()];
        let (_ctx, mut a) = app_with(s);
        a.jabber_reconcile(&frame(&[], &[], &[], &[]));
        assert_eq!(a.jabber_tabs, vec![ROOM.to_owned()]);
    }

    #[test]
    fn a_closed_tab_stays_closed_across_a_restart() {
        let mut s = crate::settings::Settings::default();
        s.jabber_rooms = vec![ROOM.to_owned()];
        let (_ctx, mut a) = app_with(s);
        a.jabber_tabs = vec![ROOM.to_owned(), DM.to_owned()];
        a.close_jabber_tab(ROOM, true);
        a.close_jabber_tab(DM, false);
        a.sync_popout_settings();
        assert!(a.settings.jabber_main_tabs.is_empty());

        // Restart with exactly those settings.
        let (_ctx2, mut b) = app_with(a.settings.clone());
        assert!(b.jabber_tabs.is_empty());
        b.jabber_reconcile(&frame(&[ROOM], &[DM], &[], &[]));
        assert!(b.jabber_tabs.is_empty(), "a closed tab came back as {:?}", b.jabber_tabs);
    }

    #[test]
    fn an_incoming_dm_still_surfaces_a_tab() {
        let (_ctx, mut a) = app_with(Default::default());
        a.jabber_reconcile(&frame(&[], &[DM], &[DM], &[]));
        assert_eq!(a.jabber_tabs, vec![DM.to_owned()]);
    }

    #[test]
    fn room_traffic_surfaces_a_tab_only_on_a_mention() {
        let mut s = crate::settings::Settings::default();
        s.jabber_rooms = vec![ROOM.to_owned()];
        let (_ctx, mut a) = app_with(s);
        a.jabber_reconcile(&frame(&[ROOM], &[], &[ROOM], &[]));
        assert!(a.jabber_tabs.is_empty(), "plain room traffic opened a tab");
        a.jabber_reconcile(&frame(&[ROOM], &[], &[ROOM], &[ROOM]));
        assert_eq!(a.jabber_tabs, vec![ROOM.to_owned()]);
    }

    #[test]
    fn the_restore_drops_an_active_tab_that_is_not_in_the_bar() {
        let mut s = crate::settings::Settings::default();
        s.jabber_main_tabs = vec![ROOM.to_owned()];
        s.jabber_main_active = DM.to_owned();
        assert_eq!(restored_main_tabs(&s), (vec![ROOM.to_owned()], None));
        s.jabber_main_active = ROOM.to_owned();
        assert_eq!(restored_main_tabs(&s), (vec![ROOM.to_owned()], Some(ROOM.to_owned())));
        assert_eq!(
            restored_main_tabs(&crate::settings::Settings::default()),
            (Vec::new(), None),
            "a fresh profile restored something"
        );
    }

    #[test]
    fn the_tab_bar_is_mirrored_into_settings() {
        let (_ctx, mut a) = app_with(Default::default());
        a.jabber_tabs = vec![ROOM.to_owned(), DM.to_owned()];
        a.jabber_chat = Some(DM.to_owned());
        a.sync_popout_settings();
        assert_eq!(a.settings.jabber_main_tabs, vec![ROOM.to_owned(), DM.to_owned()]);
        assert_eq!(a.settings.jabber_main_active, DM);
        // The Fleet pings pseudo-tab round-trips as an empty string, not as a missing tab.
        a.jabber_chat = None;
        a.sync_popout_settings();
        assert!(a.settings.jabber_main_active.is_empty());
    }
}

#[cfg(test)]
mod jabber_force_join_tests {
    use super::*;

    const NEW: &str = "mandatory@conference.goonfleet.com";
    const KNOWN: &str = "corp@conference.goonfleet.com";

    fn app() -> (egui::Context, SpaiApp) {
        let ctx = egui::Context::default();
        (ctx.clone(), SpaiApp::build(&ctx, true))
    }

    fn frame(rooms: &[&str]) -> JabberFrame {
        JabberFrame {
            configured: true,
            ever_online: true,
            connected: true,
            status: String::new(),
            convos: Vec::new(),
            pings: Vec::new(),
            rooms: rooms.iter().map(|s| (*s).to_owned()).collect(),
            dm_keys: Vec::new(),
            unread: Default::default(),
            mentions: Default::default(),
            pings_unread: false,
            channels: Vec::new(),
            inaccessible: Vec::new(),
            subjects: Default::default(),
        }
    }

    #[test]
    fn a_force_join_opens_the_tab() {
        let (_ctx, mut a) = app();
        a.jabber_reconcile(&frame(&[NEW]));
        assert_eq!(a.jabber_tabs, vec![NEW.to_owned()]);
        assert_eq!(a.settings.jabber_rooms, vec![NEW.to_owned()]);
    }

    /// "Once" is the whole requirement: close it and it must stay closed.
    #[test]
    fn it_opens_once_and_the_close_sticks() {
        let (_ctx, mut a) = app();
        a.jabber_reconcile(&frame(&[NEW]));
        assert_eq!(a.jabber_tabs, vec![NEW.to_owned()]);

        a.close_jabber_tab(NEW, true);
        assert!(a.jabber_tabs.is_empty());

        // Same session, many frames.
        for _ in 0..5 {
            a.jabber_reconcile(&frame(&[NEW]));
        }
        assert!(a.jabber_tabs.is_empty(), "the force-join reopened the tab");

        // And across a restart, with the room still joined.
        a.sync_popout_settings();
        let (_ctx2, mut b) = app();
        b.settings = a.settings.clone();
        let (tabs, active) = restored_main_tabs(&b.settings);
        b.jabber_tabs = tabs;
        b.jabber_chat = active;
        b.jabber_reconcile(&frame(&[NEW]));
        assert!(b.jabber_tabs.is_empty(), "the force-join reopened after a restart");
    }

    #[test]
    fn an_already_known_room_is_not_a_force_join() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![KNOWN.to_owned()];
        a.jabber_reconcile(&frame(&[KNOWN]));
        assert!(a.jabber_tabs.is_empty(), "a known room opened a tab");
    }

    /// A hand-join puts the room in `jabber_rooms` before the join lands, so it must not also be
    /// treated as a first-sight force-join.
    #[test]
    fn a_hand_joined_room_does_not_double_open() {
        let (_ctx, mut a) = app();
        a.settings.jabber_rooms = vec![KNOWN.to_owned()];
        a.jabber_open(KNOWN, ChatWinKey::Main);
        assert_eq!(a.jabber_tabs, vec![KNOWN.to_owned()]);
        a.jabber_reconcile(&frame(&[KNOWN]));
        assert_eq!(a.jabber_tabs, vec![KNOWN.to_owned()]);
        a.close_jabber_tab(KNOWN, true);
        a.jabber_reconcile(&frame(&[KNOWN]));
        assert!(a.jabber_tabs.is_empty());
    }

    /// The force-join has to clear a stale hidden flag, or the tab is opened and then pruned.
    #[test]
    fn a_force_join_survives_a_stale_hidden_flag() {
        let (_ctx, mut a) = app();
        a.settings.jabber_closed_rooms = vec![NEW.to_owned()];
        a.jabber_reconcile(&frame(&[NEW]));
        assert_eq!(a.jabber_tabs, vec![NEW.to_owned()]);
        a.jabber_reconcile(&frame(&[NEW]));
        assert_eq!(a.jabber_tabs, vec![NEW.to_owned()], "opened then pruned");
    }

    /// Pinning is not a force-join: the rescue rooms are added to `jabber_rooms` by the healing
    /// step before the branch runs, so enabling Rescue Mode must not go through this path twice.
    #[cfg(feature = "fc-rescue")]
    #[test]
    fn enabling_rescue_mode_is_not_a_force_join() {
        let (_ctx, mut a) = app();
        a.settings.fc_rescue_enabled = true;
        let rescue = a.jabber_rescue_rooms();
        a.jabber_reconcile(&frame(&[]));
        // Held open by the pin, and already recorded, so a later join is not "first sight".
        assert_eq!(a.jabber_tabs, rescue);
        let before = a.settings.jabber_rooms.clone();
        a.jabber_reconcile(&frame(&rescue.iter().map(String::as_str).collect::<Vec<_>>()));
        assert_eq!(a.settings.jabber_rooms, before, "pinning re-added the rooms");
        assert_eq!(a.jabber_tabs, rescue);
    }
}

#[cfg(test)]
mod active_character_tests {
    use super::{CharacterRow, motd_one_line, motd_preview, resolve_active_character, shows_in_dm_list};

    fn rows(names: &[&str]) -> Vec<CharacterRow> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| CharacterRow {
                id: i as i64 + 1,
                name: (*n).to_owned(),
                expires_at: 0,
                scopes: String::new(),
            })
            .collect()
    }

    #[test]
    fn a_remembered_pilot_survives_the_restart() {
        let chars = rows(&["Amryu", "Scout"]);
        assert_eq!(resolve_active_character("Scout", &chars), "Scout");
    }

    #[test]
    fn case_differences_still_match_the_remembered_pilot() {
        let chars = rows(&["Amryu"]);
        assert_eq!(resolve_active_character("amryu", &chars), "amryu");
    }

    #[test]
    fn an_empty_list_keeps_the_pick_because_nothing_is_authed_yet() {
        assert_eq!(resolve_active_character("Amryu", &[]), "Amryu");
        assert_eq!(resolve_active_character("No character", &[]), "No character");
    }

    #[test]
    fn a_removed_pilot_falls_back_to_the_first_authed_one() {
        let chars = rows(&["Amryu", "Scout"]);
        assert_eq!(resolve_active_character("Deleted", &chars), "Amryu");
    }

    #[test]
    fn no_character_picks_the_first_authed_one() {
        let chars = rows(&["Amryu", "Scout"]);
        assert_eq!(resolve_active_character("No character", &chars), "Amryu");
    }

    const SAMPLE_MOTD: &str = "DEFENCE FLEETS FORM IN 1DQ1-A\n\
                               \n\
                               ------------------------------\n\
                               Ping format: [FLEET] FC / staging\n\
                               Comms: Mumble\n\
                               Doctrines: https://example.invalid\n\
                               No AFK cloaking";

    /// The title bar has one line, and a MOTD is a notice board. Blank lines and the rule of dashes
    /// carry nothing once it is one line, and the rule took a third of the bar when it was kept.
    #[test]
    fn one_line_drops_the_blank_lines_and_the_rule() {
        let one = motd_one_line(SAMPLE_MOTD);
        assert!(one.starts_with("DEFENCE FLEETS FORM IN 1DQ1-A"), "{one}");
        assert!(!one.contains("---"), "the rule is not content: {one}");
        assert!(!one.contains("  ·    ·"), "no empty piece between separators: {one}");
        assert!(one.contains("No AFK cloaking"), "the last line survives: {one}");
    }

    /// The tooltip is a preview, and has to say so: without the marker a capped MOTD reads as the
    /// whole thing, which is worse than not showing it.
    #[test]
    fn the_preview_caps_the_lines_and_says_there_are_more() {
        let p = motd_preview(SAMPLE_MOTD, 3);
        assert_eq!(p.lines().count(), 4, "three lines plus the marker: {p:?}");
        assert!(p.ends_with('…'), "{p:?}");
        assert!(!p.contains("Comms: Mumble"), "the fourth line is behind the cap: {p:?}");

        // Under the cap there is nothing behind it, so there is no marker.
        let short = motd_preview("one\ntwo", 6);
        assert_eq!(short, "one\ntwo");
    }

    /// A room is never a direct message, however it got into the sticky set.
    ///
    /// Reported from chat: "Direct messages in jabber shows uninteractable rooms instead (the
    /// duplicates on the rooms work just fine)". Every room that went unread was stuck into the DM
    /// list and listed under both headings, and the duplicate was dead because two rows sharing a
    /// jid share an egui id and only one wins the hit test.
    #[test]
    fn a_room_is_never_listed_as_a_direct_message() {
        use std::collections::{BTreeSet, HashSet};
        let dm = "wingmate@goonfleet.com".to_owned();
        let room = "delve.imperium@conference.goonfleet.com".to_owned();
        let dm_keys: HashSet<&String> = HashSet::from([&dm]);
        let contacts: HashSet<&String> = HashSet::new();
        let closed: HashSet<&String> = HashSet::new();

        // The state the bug left behind: the room went unread, so it is sticky.
        let sticky = BTreeSet::from([dm.clone(), room.clone()]);
        assert!(shows_in_dm_list(&dm, &dm_keys, &contacts, &closed, &sticky));
        assert!(
            !shows_in_dm_list(&room, &dm_keys, &contacts, &closed, &sticky),
            "a room stays out of the DM list even while it is sticky"
        );
    }

    /// The sticky rule exists so closing a DM is curation and not a way to lose mail. It has to keep
    /// working, because the fix narrows what stickiness is allowed to override.
    #[test]
    fn a_closed_dm_comes_back_when_it_goes_unread() {
        use std::collections::{BTreeSet, HashSet};
        let dm = "wingmate@goonfleet.com".to_owned();
        let dm_keys: HashSet<&String> = HashSet::from([&dm]);
        let contacts: HashSet<&String> = HashSet::new();
        let closed: HashSet<&String> = HashSet::from([&dm]);

        assert!(
            !shows_in_dm_list(&dm, &dm_keys, &contacts, &closed, &BTreeSet::new()),
            "closed and quiet stays closed"
        );
        assert!(
            shows_in_dm_list(&dm, &dm_keys, &contacts, &closed, &BTreeSet::from([dm.clone()])),
            "closed but unread comes back"
        );
    }

    /// A contact with no history is a person you have not spoken to yet, and the list is how you
    /// start. `dm_keys` only covers conversations that already exist.
    #[test]
    fn a_contact_is_a_direct_message_without_any_history() {
        use std::collections::{BTreeSet, HashSet};
        let friend = "logilead@goonfleet.com".to_owned();
        let contacts: HashSet<&String> = HashSet::from([&friend]);
        assert!(shows_in_dm_list(
            &friend,
            &HashSet::new(),
            &contacts,
            &HashSet::new(),
            &BTreeSet::new()
        ));
    }

    /// A room with no subject must not draw an empty separator and a dead button.
    #[test]
    fn an_empty_motd_is_empty_not_a_separator() {
        assert_eq!(motd_one_line(""), "");
        assert_eq!(motd_one_line("\n\n   \n"), "");
        assert_eq!(motd_preview("   \n\n", 6), "");
    }
}

#[cfg(test)]
mod notes_behaviour_tests {
    use super::{intel_query_matches, rule_matches};
    use crate::notes::ANY_TAG;
    use crate::settings::{AlertRule, Severity};
    use crate::uitest::fixtures;

    fn view() -> crate::notes::NotesView {
        fixtures::notebook().view("")
    }

    fn rule(pilot_tags: &[&str], system_tags: &[&str]) -> AlertRule {
        AlertRule {
            min_severity: Severity::Info,
            pilot_tags: pilot_tags.iter().map(|s| (*s).to_owned()).collect(),
            system_tags: system_tags.iter().map(|s| (*s).to_owned()).collect(),
            ..Default::default()
        }
    }

    fn fires(ru: &AlertRule, r: &crate::intel::IntelReport, v: &crate::notes::NotesView) -> bool {
        rule_matches(ru, r, Severity::Danger, None, &Some(fixtures::systems()), v)
    }

    #[test]
    fn pilot_tag_condition_matches_by_name_before_resolution() {
        let v = view();
        let hunter = v.pilot_tags.iter().find(|t| t.name == "Hunter").unwrap().id.clone();
        let r = fixtures::intel_typical();
        assert!(fires(&rule(&[&hunter], &[]), &r, &v));
        assert!(fires(&rule(&["d:pilot:cyno"], &[]), &r, &v));
        assert!(fires(&rule(&[ANY_TAG], &[]), &r, &v));
        assert!(!fires(&rule(&["d:pilot:titan"], &[]), &r, &v));
        assert!(!fires(&rule(&["deleted-tag-id"], &[]), &r, &v), "a dangling id fails closed");
    }

    #[test]
    fn system_and_pilot_tag_conditions_both_have_to_hold() {
        let v = view();
        let r = fixtures::intel_typical();
        assert!(fires(&rule(&["d:pilot:cyno"], &["d:sys:staging"]), &r, &v));
        assert!(!fires(&rule(&["d:pilot:cyno"], &["d:sys:mining"]), &r, &v));
        let mut elsewhere = r.clone();
        elsewhere.systems = vec![crate::intel::DetectedSystem { id: 30_000_142, name: "Jita".into(), security: 0.95 }];
        assert!(!fires(&rule(&[], &[ANY_TAG]), &elsewhere, &v));
    }

    #[test]
    fn empty_tag_lists_leave_a_rule_alone() {
        let r = fixtures::intel_typical();
        assert!(fires(&rule(&[], &[]), &r, &crate::notes::NotesView::default()));
    }

    #[test]
    fn intel_search_finds_tags_and_notes() {
        let v = view();
        let r = fixtures::intel_typical();
        assert!(intel_query_matches(&r, "hunter", &v));
        assert!(intel_query_matches(&r, "keepstar", &v), "system note text");
        assert!(intel_query_matches(&r, "cloaky", &v), "pilot note text");
        assert!(intel_query_matches(&r, "delve.imp", &v), "the old channel match still works");
        assert!(!intel_query_matches(&r, "hidden while", &v), "offline folders are not searched");
    }

    #[test]
    fn a_rule_saved_before_tags_existed_still_parses() {
        let mut json = serde_json::to_value(AlertRule::default()).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("pilot_tags");
        obj.remove("system_tags");
        let back: AlertRule = serde_json::from_value(json).unwrap();
        assert!(back.pilot_tags.is_empty() && back.system_tags.is_empty());
    }

    #[test]
    fn an_alert_frame_without_notes_still_parses() {
        let msg = crate::ipc::AlertMsg {
            feed: Vec::new(),
            from_you: Vec::new(),
            via: Vec::new(),
            chars: Vec::new(),
            status: Default::default(),
            resolved_pilots: Default::default(),
            uncertain: Default::default(),
            last_ship: Default::default(),
            kills: Default::default(),
            affil: Default::default(),
            notes: view(),
            secs: 1.0,
            focus: false,
        };
        let mut json = serde_json::to_value(&msg).unwrap();
        json.as_object_mut().unwrap().remove("notes");
        let back: crate::ipc::AlertMsg = serde_json::from_value(json).unwrap();
        assert!(back.notes.systems.is_empty());
        let op = crate::ipc::OverlayToMain::Click(super::IntelClick::Notes(crate::notes::NotesOp::SetTag {
            folder: String::new(),
            subject: crate::notes::Subject::Pilot { id: 5, name: "Bob".into() },
            tag: "d:pilot:cyno".into(),
            on: true,
        }));
        let text = serde_json::to_string(&op).unwrap();
        assert!(serde_json::from_str::<crate::ipc::OverlayToMain>(&text).is_ok());
    }
}
