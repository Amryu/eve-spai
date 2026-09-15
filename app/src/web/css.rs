//! The palette, emitted from the app's own colours.
//!
//! Every colour the page uses is a custom property generated here, so a theme, severity or security
//! colour changed in Rust reaches the page without editing CSS.

use egui::Color32;

use crate::theme::{self, Theme};

fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

pub fn theme_css(theme: &Theme) -> String {
    let d = theme::derived(theme);
    let mut out = String::with_capacity(1200);
    out.push_str(":root{\n");
    for (name, c) in [
        ("bg", d.bg),
        ("fg", d.fg),
        ("accent", d.accent),
        ("surface", d.surface),
        ("surface-hi", d.surface_hi),
        ("surface-active", d.surface_active),
        ("faint", d.faint),
        ("muted", d.muted),
        ("line", d.line),
    ] {
        out.push_str(&format!("  --{name}: {};\n", hex(c)));
    }
    for (name, c) in [
        ("hostile", theme::standing::HOSTILE),
        ("neutral", theme::standing::NEUTRAL),
        ("friendly", theme::standing::FRIENDLY),
        ("corp", theme::standing::CORP),
        ("alliance", theme::standing::ALLIANCE),
        ("warning", theme::standing::WARNING),
    ] {
        out.push_str(&format!("  --{name}: {};\n", hex(c)));
    }
    for sev in [
        crate::settings::Severity::Info,
        crate::settings::Severity::Warning,
        crate::settings::Severity::Danger,
        crate::settings::Severity::Critical,
    ] {
        let name = format!("{sev:?}").to_lowercase();
        out.push_str(&format!("  --sev-{name}: {};\n", hex(crate::app::severity_color(sev))));
    }
    for (name, c) in [
        ("chip-clear", theme::chip::CLEAR),
        ("chip-kill-icon", theme::chip::KILL_ICON),
        ("chip-celestial", theme::chip::CELESTIAL),
        ("chip-celestial-bg", theme::chip::CELESTIAL_BG),
        ("chip-isk", theme::chip::ISK),
        ("chip-isk-bg", theme::chip::ISK_BG),
        ("chip-structure", theme::chip::STRUCTURE),
        ("chip-structure-bg", theme::chip::STRUCTURE_BG),
        ("chip-probes", theme::chip::PROBES),
        ("chip-probes-bg", theme::chip::PROBES_BG),
        ("chip-tackled", theme::chip::TACKLED),
        ("chip-tackled-bg", theme::chip::TACKLED_BG),
        ("chip-uncertain", theme::chip::UNCERTAIN),
        ("chip-uncertain-bg", theme::chip::UNCERTAIN_BG),
        ("chip-kill-card-bg", theme::chip::KILL_CARD_BG),
    ] {
        out.push_str(&format!("  --{name}: {};\n", hex(c)));
    }
    // Indexed like `security_color`, so the page picks a stop with the same arithmetic.
    for i in 0..=10 {
        let c = crate::app::security_color(i as f64 / 10.0);
        out.push_str(&format!("  --sec-{i}: {};\n", hex(c)));
    }
    out.push_str(&format!("  color-scheme: {};\n", if d.dark { "dark" } else { "light" }));
    out.push_str("}\n");
    out.push_str(&format!(
        "@font-face{{font-family:phosphor;src:url({}) format('truetype');font-display:block}}\n",
        super::assets::font_path()
    ));
    out
}

/// A per-device colour override. Derived here in Rust so it cannot drift from the app's derivation.
pub fn theme_from_query(query: &str, fallback: &Theme) -> Theme {
    let pick = |key: &str, dflt: theme::Rgb| {
        super::routes::query_param(query, key)
            .and_then(parse_hex)
            .unwrap_or(dflt)
    };
    Theme {
        name: "Custom".to_owned(),
        background: pick("bg", fallback.background),
        foreground: pick("fg", fallback.foreground),
        accent: pick("accent", fallback.accent),
    }
}

fn parse_hex(s: &str) -> Option<theme::Rgb> {
    let s = s.trim_start_matches("%23").trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some(theme::Rgb::new(n(0)?, n(2)?, n(4)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_every_token_the_page_needs() {
        let css = theme_css(&Theme::caldari());
        for token in [
            "--bg", "--fg", "--accent", "--surface", "--surface-hi", "--surface-active", "--faint",
            "--muted", "--line", "--hostile", "--neutral", "--friendly", "--corp", "--alliance",
            "--warning", "--sev-info", "--sev-warning", "--sev-danger", "--sev-critical",
            "--sec-0", "--sec-5", "--sec-10", "--chip-clear", "--chip-celestial",
            "--chip-isk-bg", "--chip-uncertain", "--chip-kill-card-bg", "--chip-tackled-bg",
        ] {
            assert!(css.contains(token), "{token} missing");
        }
        assert!(css.contains("font-family:phosphor"));
    }

    #[test]
    fn the_emitted_values_are_the_app_values() {
        let t = Theme::caldari();
        let css = theme_css(&t);
        let d = theme::derived(&t);
        assert!(css.contains(&format!("--surface: {}", hex(d.surface))));
        assert!(css.contains(&format!("--muted: {}", hex(d.muted))));
        assert!(css.contains(&format!(
            "--sev-critical: {}",
            hex(crate::app::severity_color(crate::settings::Severity::Critical))
        )));
        assert!(css.contains(&format!("--sec-10: {}", hex(crate::app::security_color(1.0)))));
        assert!(css.contains(&format!("--chip-uncertain: {}", hex(theme::chip::UNCERTAIN))));
        assert!(css.contains("color-scheme: dark"));
    }

    #[test]
    fn a_light_theme_says_so() {
        assert!(theme_css(&Theme::daylight()).contains("color-scheme: light"));
    }

    #[test]
    fn every_preset_emits_a_distinct_sheet() {
        let mut seen = std::collections::HashSet::new();
        for t in Theme::presets() {
            assert!(seen.insert(theme_css(&t)), "{} collides with another preset", t.name);
        }
    }

    #[test]
    fn a_query_override_replaces_only_what_it_supplies() {
        let base = Theme::caldari();
        let t = theme_from_query("bg=101010&accent=ff0000", &base);
        assert_eq!(t.background, theme::Rgb::new(0x10, 0x10, 0x10));
        assert_eq!(t.accent, theme::Rgb::new(0xff, 0x00, 0x00));
        assert_eq!(t.foreground, base.foreground, "untouched keys keep the app's value");
    }

    #[test]
    fn a_bad_override_is_ignored_rather_than_rendering_black() {
        let base = Theme::caldari();
        for bad in ["zzzzzz", "fff", "", "12345678", "#gg0011"] {
            let t = theme_from_query(&format!("bg={bad}"), &base);
            assert_eq!(t.background, base.background, "{bad} should have been refused");
        }
        assert_eq!(
            theme_from_query("bg=%23101010", &base).background,
            theme::Rgb::new(0x10, 0x10, 0x10),
            "a url-encoded hash is what a colour input actually sends"
        );
    }
}
