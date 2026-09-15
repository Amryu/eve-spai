use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
    pub fn color(self) -> Color32 {
        Color32::from_rgb(self.r, self.g, self.b)
    }
    pub fn array(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
    pub fn from_array(a: [u8; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub background: Rgb,
    pub foreground: Rgb,
    pub accent: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        Self::caldari()
    }
}

impl Theme {
    pub fn caldari() -> Self {
        Self {
            name: "Caldari".into(),
            background: Rgb::new(0x0B, 0x0F, 0x12),
            foreground: Rgb::new(0xC8, 0xD2, 0xD8),
            accent: Rgb::new(0x3F, 0xA9, 0xC9),
        }
    }

    pub fn amarr() -> Self {
        Self {
            name: "Amarr".into(),
            background: Rgb::new(0x12, 0x0E, 0x08),
            foreground: Rgb::new(0xE6, 0xD8, 0xB8),
            accent: Rgb::new(0xD2, 0xA6, 0x4B),
        }
    }

    pub fn minmatar() -> Self {
        Self {
            name: "Minmatar".into(),
            background: Rgb::new(0x12, 0x0A, 0x08),
            foreground: Rgb::new(0xE2, 0xD2, 0xC6),
            accent: Rgb::new(0xB7, 0x4A, 0x36),
        }
    }

    pub fn gallente() -> Self {
        Self {
            name: "Gallente".into(),
            background: Rgb::new(0x0A, 0x10, 0x0C),
            foreground: Rgb::new(0xCB, 0xD8, 0xCC),
            accent: Rgb::new(0x4F, 0xB0, 0x6A),
        }
    }

    pub fn daylight() -> Self {
        Self {
            name: "Daylight".into(),
            background: Rgb::new(0xF4, 0xF6, 0xF8),
            foreground: Rgb::new(0x18, 0x20, 0x26),
            accent: Rgb::new(0x16, 0x6E, 0x8C),
        }
    }

    pub fn presets() -> Vec<Theme> {
        vec![
            Self::caldari(),
            Self::amarr(),
            Self::minmatar(),
            Self::gallente(),
            Self::daylight(),
        ]
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let Derived {
            dark,
            bg,
            fg,
            accent,
            surface,
            surface_hi,
            surface_active,
            faint,
            muted,
            line,
        } = derived(self);

        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        v.override_text_color = Some(fg);
        v.panel_fill = surface;
        v.window_fill = surface;
        v.extreme_bg_color = bg;
        v.faint_bg_color = faint;
        v.window_stroke = Stroke::new(1.0, line);
        v.hyperlink_color = accent;

        v.selection.bg_fill = accent.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, accent);

        v.widgets.noninteractive.bg_fill = surface;
        v.widgets.noninteractive.weak_bg_fill = surface;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, muted);

        v.widgets.inactive.bg_fill = surface_hi;
        v.widgets.inactive.weak_bg_fill = surface;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, line);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, fg);

        v.widgets.hovered.bg_fill = surface_active;
        v.widgets.hovered.weak_bg_fill = surface_hi;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.6));
        v.widgets.hovered.fg_stroke = Stroke::new(1.5, fg);

        v.widgets.active.bg_fill = accent.gamma_multiply(0.45);
        v.widgets.active.weak_bg_fill = accent.gamma_multiply(0.30);
        v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
        v.widgets.active.fg_stroke = Stroke::new(1.5, fg);

        v.widgets.open.bg_fill = surface_hi;
        v.widgets.open.weak_bg_fill = surface_hi;
        v.widgets.open.bg_stroke = Stroke::new(1.0, line);
        v.widgets.open.fg_stroke = Stroke::new(1.0, fg);

        // The loading spinner requests a repaint every frame, so on a busy intel feed, where remote
        // images are always loading, it pins the UI at continuous repaint. The image still appears
        // once its loader requests a single repaint.
        v.image_loading_spinners = false;

        ctx.set_visuals(v);

        ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
            style.spacing.interact_size.y = 26.0;
            style.spacing.menu_margin = egui::Margin::same(8);
        });
    }
}

/// Every colour `Theme::apply` puts into the egui `Visuals`, derived from the theme's three.
/// Separate from `apply` so the web view emits the same palette as CSS custom properties instead of
/// re-deriving it in JavaScript, where the two would drift apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Derived {
    pub dark: bool,
    pub bg: Color32,
    pub fg: Color32,
    pub accent: Color32,
    pub surface: Color32,
    pub surface_hi: Color32,
    pub surface_active: Color32,
    pub faint: Color32,
    pub muted: Color32,
    pub line: Color32,
}

pub fn derived(theme: &Theme) -> Derived {
    let bg = theme.background.color();
    let fg = theme.foreground.color();
    let dark = luminance(bg) < 0.5;
    let contrast = if dark { Color32::WHITE } else { Color32::BLACK };
    Derived {
        dark,
        bg,
        fg,
        accent: theme.accent.color(),
        surface: mix(bg, contrast, 0.05),
        surface_hi: mix(bg, contrast, 0.10),
        surface_active: mix(bg, contrast, 0.16),
        faint: mix(bg, contrast, 0.03),
        muted: mix(fg, bg, 0.45),
        line: mix(bg, contrast, 0.18),
    }
}

/// The intel card's chip palette, shared so the egui card and `web::css` read the same constants.
pub mod chip {
    use egui::Color32;

    /// A "clear" report, and the icon that goes with it.
    pub const CLEAR: Color32 = Color32::from_rgb(0x5A, 0xC8, 0x6A);
    /// A zKill-derived card's icon.
    pub const KILL_ICON: Color32 = Color32::from_rgb(0xEF, 0x53, 0x50);

    /// Celestials and the near-celestial chip.
    pub const CELESTIAL: Color32 = Color32::from_rgb(0x8E, 0xD6, 0xE6);
    pub const CELESTIAL_BG: Color32 = Color32::from_rgb(0x10, 0x32, 0x3A);

    pub const ISK: Color32 = Color32::from_rgb(0xFF, 0xD9, 0x6B);
    pub const ISK_BG: Color32 = Color32::from_rgb(0x4A, 0x3D, 0x10);

    pub const STRUCTURE: Color32 = Color32::from_rgb(0xC4, 0xB5, 0xFD);
    pub const STRUCTURE_BG: Color32 = Color32::from_rgb(0x2E, 0x24, 0x4A);

    pub const PROBES: Color32 = Color32::from_rgb(0x7D, 0xD3, 0xDE);
    pub const PROBES_BG: Color32 = Color32::from_rgb(0x10, 0x3A, 0x40);

    pub const TACKLED: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x8A);
    pub const TACKLED_BG: Color32 = Color32::from_rgb(0x5A, 0x18, 0x18);

    /// An unresolved or uncertain name: the `?` on a pilot badge, and an ambiguous hull.
    pub const UNCERTAIN: Color32 = Color32::from_rgb(0xFB, 0xBF, 0x24);
    pub const UNCERTAIN_BG: Color32 = Color32::from_rgb(0x3D, 0x30, 0x14);

    /// A zKill card's own background, darker than any theme surface so it reads as foreign.
    pub const KILL_CARD_BG: Color32 = Color32::from_rgb(0x0C, 0x0C, 0x0C);
}

pub mod standing {
    use egui::Color32;
    pub const HOSTILE: Color32 = Color32::from_rgb(0xD8, 0x4C, 0x4C);
    pub const NEUTRAL: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xA8);
    pub const FRIENDLY: Color32 = Color32::from_rgb(0x5A, 0xC8, 0x6A);
    pub const CORP: Color32 = Color32::from_rgb(0x4F, 0x9B, 0xD8);
    pub const ALLIANCE: Color32 = Color32::from_rgb(0x9B, 0x6F, 0xD8);
    pub const WARNING: Color32 = Color32::from_rgb(0xE0, 0xA4, 0x3A);
}

/// Build and install the shared font set: egui's defaults, Phosphor icons and a CJK fallback so
/// Chinese names render instead of tofu. The CJK face is loaded from a system font file (embedding
/// it would add ~15 MB) and appended last in both the Proportional and Monospace families, so
/// Latin and icon glyphs keep their fonts and metrics. Without a CJK font, CJK text stays tofu.
pub fn install_fonts(ctx: &egui::Context) {
    install_fonts_opts(ctx, true);
}

/// `include_cjk = false` skips the system font probe, whose result depends on which fonts the
/// machine happens to have installed. The UI harness needs layout to be machine-independent.
pub fn install_fonts_opts(ctx: &egui::Context, include_cjk: bool) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    if let Some(data) = include_cjk.then(load_cjk_font).flatten() {
        const NAME: &str = "cjk-fallback";
        fonts.font_data.insert(NAME.to_owned(), std::sync::Arc::new(data));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(NAME.to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

fn load_cjk_font() -> Option<egui::FontData> {
    for path in cjk_font_candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            // `.ttc` collections load at face index 0 (the full CJK face) via ab_glyph.
            return Some(egui::FontData::from_owned(bytes));
        }
    }
    eprintln!("fonts: no system CJK font found; Chinese/Japanese text will render as boxes");
    None
}

fn cjk_font_candidates() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
            r"C:\Windows\Fonts\simhei.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ]
    } else {
        &[
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.otf",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansSC-Regular.otf",
            "/usr/share/fonts/opentype/noto/NotoSansSC-Regular.otf",
            "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/wqy-microhei/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ]
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn luminance(c: Color32) -> f32 {
    (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the arithmetic itself. Everything else here only checks that `apply` routes the derived
    /// values to the right `Visuals` fields, which stays true if the derivation silently changes.
    #[test]
    fn derived_pins_the_default_palette() {
        let d = derived(&Theme::caldari());
        assert!(d.dark);
        assert_eq!(d.bg, Color32::from_rgb(0x0B, 0x0F, 0x12));
        assert_eq!(d.fg, Color32::from_rgb(0xC8, 0xD2, 0xD8));
        assert_eq!(d.accent, Color32::from_rgb(0x3F, 0xA9, 0xC9));
        assert_eq!(d.surface, Color32::from_rgb(23, 27, 30));
        assert_eq!(d.surface_hi, Color32::from_rgb(35, 39, 42));
        assert_eq!(d.surface_active, Color32::from_rgb(50, 53, 56));
        assert_eq!(d.faint, Color32::from_rgb(18, 22, 25));
        assert_eq!(d.muted, Color32::from_rgb(115, 122, 127));
        assert_eq!(d.line, Color32::from_rgb(55, 58, 61));
    }

    #[test]
    fn derived_reads_light_themes_as_light() {
        assert!(!derived(&Theme::daylight()).dark);
        for t in Theme::presets().iter().filter(|t| t.name != "Daylight") {
            assert!(derived(t).dark, "{} should be dark", t.name);
        }
    }

    /// The web view emits `derived()` as CSS, so a `Visuals` field that stops agreeing with it is the
    /// two surfaces drifting apart. Checked for every preset, since a light theme takes the other
    /// contrast branch.
    #[test]
    fn apply_puts_the_derived_palette_into_visuals() {
        for theme in Theme::presets() {
            let d = derived(&theme);
            let ctx = egui::Context::default();
            theme.apply(&ctx);
            let v = ctx.global_style().visuals.clone();
            let n = &theme.name;
            assert_eq!(v.dark_mode, d.dark, "{n} dark_mode");
            assert_eq!(v.override_text_color, Some(d.fg), "{n} text");
            assert_eq!(v.panel_fill, d.surface, "{n} panel_fill");
            assert_eq!(v.window_fill, d.surface, "{n} window_fill");
            assert_eq!(v.extreme_bg_color, d.bg, "{n} extreme_bg_color");
            assert_eq!(v.faint_bg_color, d.faint, "{n} faint_bg_color");
            assert_eq!(v.window_stroke.color, d.line, "{n} window_stroke");
            assert_eq!(v.hyperlink_color, d.accent, "{n} hyperlink_color");
            assert_eq!(v.widgets.noninteractive.bg_fill, d.surface, "{n} noninteractive fill");
            assert_eq!(v.widgets.noninteractive.fg_stroke.color, d.muted, "{n} muted");
            assert_eq!(v.widgets.inactive.bg_fill, d.surface_hi, "{n} inactive fill");
            assert_eq!(v.widgets.hovered.bg_fill, d.surface_active, "{n} hovered fill");
            assert_eq!(v.widgets.active.bg_stroke.color, d.accent, "{n} active stroke");
        }
    }

    /// Pins every chip colour to its literal: a wrong chip colour is invisible until someone
    /// notices a card looks off.
    #[test]
    fn chip_colours_match_the_literals_they_replaced() {
        use chip::*;
        for (name, got, want) in [
            ("CLEAR", CLEAR, (0x5A, 0xC8, 0x6A)),
            ("KILL_ICON", KILL_ICON, (0xEF, 0x53, 0x50)),
            ("CELESTIAL", CELESTIAL, (0x8E, 0xD6, 0xE6)),
            ("CELESTIAL_BG", CELESTIAL_BG, (0x10, 0x32, 0x3A)),
            ("ISK", ISK, (0xFF, 0xD9, 0x6B)),
            ("ISK_BG", ISK_BG, (0x4A, 0x3D, 0x10)),
            ("STRUCTURE", STRUCTURE, (0xC4, 0xB5, 0xFD)),
            ("STRUCTURE_BG", STRUCTURE_BG, (0x2E, 0x24, 0x4A)),
            ("PROBES", PROBES, (0x7D, 0xD3, 0xDE)),
            ("PROBES_BG", PROBES_BG, (0x10, 0x3A, 0x40)),
            ("TACKLED", TACKLED, (0xFF, 0x8A, 0x8A)),
            ("TACKLED_BG", TACKLED_BG, (0x5A, 0x18, 0x18)),
            ("UNCERTAIN", UNCERTAIN, (0xFB, 0xBF, 0x24)),
            ("UNCERTAIN_BG", UNCERTAIN_BG, (0x3D, 0x30, 0x14)),
            ("KILL_CARD_BG", KILL_CARD_BG, (0x0C, 0x0C, 0x0C)),
        ] {
            assert_eq!(
                (got.r(), got.g(), got.b()),
                want,
                "{name} does not match the literal it was extracted from"
            );
        }
    }

    #[test]
    fn install_fonts_lays_out_cjk_without_panicking() {
        let ctx = egui::Context::default();
        super::install_fonts(&ctx);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label("中文测试 — CJK 字体 ABC");
        });
    }
}
