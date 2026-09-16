//! Icon names, emitted from the same constants the app draws with.
//!
//! The page asks for `ico('warning')` and never writes a codepoint, since a hand-copied Phosphor
//! escape is unreviewable and a wrong one renders as tofu instead of an error.

use egui_phosphor::regular as icon;

pub const ICONS: &[(&str, &str)] = &[
    ("info", icon::INFO),
    ("warning", icon::WARNING),
    ("warning-octagon", icon::WARNING_OCTAGON),
    ("check-circle", icon::CHECK_CIRCLE),
    ("skull", icon::SKULL),
    ("crosshair", icon::CROSSHAIR),
    ("eye-slash", icon::EYE_SLASH),
    ("planet", icon::PLANET),
    ("users", icon::USERS),
    ("user", icon::USER),
    ("coins", icon::COINS),
    ("castle-turret", icon::CASTLE_TURRET),
    ("magnifying-glass", icon::MAGNIFYING_GLASS),
    ("sign-in", icon::SIGN_IN),
    ("moon", icon::MOON),
    ("map-pin-line", icon::MAP_PIN_LINE),
    ("chart-line", icon::CHART_LINE),
    ("scan", icon::SCAN),
    ("arrow-square-out", icon::ARROW_SQUARE_OUT),
    ("arrow-left", icon::ARROW_LEFT),
    ("arrow-right", icon::ARROW_RIGHT),
    ("megaphone", icon::MEGAPHONE),
    ("headset", icon::HEADSET),
    ("link", icon::LINK),
    ("copy", icon::COPY),
    ("bell", icon::BELL),
    ("broadcast", icon::BROADCAST),
    ("map-trifold", icon::MAP_TRIFOLD),
    ("squares-four", icon::SQUARES_FOUR),
    ("gear-six", icon::GEAR_SIX),
    ("campfire", icon::CAMPFIRE),
    ("spiral", icon::SPIRAL),
    ("bookmark-simple", icon::BOOKMARK_SIMPLE),
    ("article", icon::ARTICLE),
    ("arrow-up", icon::ARROW_UP),
    ("speaker-high", icon::SPEAKER_HIGH),
    ("speaker-slash", icon::SPEAKER_SLASH),
    ("arrows-in", icon::ARROWS_IN),
    ("arrows-split", icon::ARROWS_SPLIT),
    ("arrows-out", icon::ARROWS_OUT),
    ("x", icon::X),
    // Layout chrome.
    ("dots-six-vertical", icon::DOTS_SIX_VERTICAL),
    ("arrows-out-line-horizontal", icon::ARROWS_OUT_LINE_HORIZONTAL),
    ("arrows-out-line-vertical", icon::ARROWS_OUT_LINE_VERTICAL),
    ("arrows-in-line-horizontal", icon::ARROWS_IN_LINE_HORIZONTAL),
    ("dots-three", icon::DOTS_THREE),
    ("star-four", icon::STAR_FOUR),
    // Jabber.
    ("chat-circle-dots", icon::CHAT_CIRCLE_DOTS),
    ("users-three", icon::USERS_THREE),
    ("paper-plane-right", icon::PAPER_PLANE_RIGHT),
    ("plus", icon::PLUS),
    // Map markers.
    ("radioactive", icon::RADIOACTIVE),
    ("gear", icon::GEAR),
    ("cell-tower", icon::CELL_TOWER),
    ("crosshair-simple", icon::CROSSHAIR_SIMPLE),
    // Notes, tags and their folders.
    ("tag", icon::TAG),
    ("note", icon::NOTE),
    ("note-pencil", icon::NOTE_PENCIL),
    ("pencil-simple", icon::PENCIL_SIMPLE),
    ("trash", icon::TRASH),
    ("folder", icon::FOLDER),
    ("folder-open", icon::FOLDER_OPEN),
    ("folder-plus", icon::FOLDER_PLUS),
    ("download-simple", icon::DOWNLOAD_SIMPLE),
    ("upload-simple", icon::UPLOAD_SIMPLE),
    ("arrow-bend-up-right", icon::ARROW_BEND_UP_RIGHT),
    ("globe", icon::GLOBE),
    ("globe-x", icon::GLOBE_X),
    ("target", icon::TARGET),
    ("check-square", icon::CHECK_SQUARE),
    ("square", icon::SQUARE),
];

pub fn json() -> String {
    let body: Vec<String> =
        ICONS.iter().map(|(name, glyph)| format!("\"{name}\":\"{glyph}\"")).collect();
    format!("{{{}}}", body.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing name fails to compile, but an empty or multi-glyph constant would only show in the
    /// browser.
    #[test]
    fn every_icon_is_one_private_use_glyph() {
        for (name, glyph) in ICONS {
            let mut chars = glyph.chars();
            let c = chars.next().unwrap_or_else(|| panic!("{name} is empty"));
            assert!(chars.next().is_none(), "{name} is more than one glyph");
            assert!(
                ('\u{e000}'..='\u{f8ff}').contains(&c),
                "{name} is U+{:04X}, outside the private use area",
                c as u32
            );
        }
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for (name, _) in ICONS {
            assert!(seen.insert(name), "{name} listed twice");
        }
    }

    #[test]
    fn json_parses_and_round_trips() {
        let v: std::collections::HashMap<String, String> =
            serde_json::from_str(&json()).expect("valid json");
        assert_eq!(v.len(), ICONS.len());
        assert_eq!(v.get("warning").map(String::as_str), Some(icon::WARNING));
    }
}
