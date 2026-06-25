//! Three-colour theme engine.
//!
//! A theme is exactly three user-chosen colours — `background`, `foreground`,
//! `accent` (docs/DESIGN.md §6.3). Every other surface/text/state colour is
//! *derived* here at runtime so the whole UI stays cohesive from three inputs.

use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};

/// A plain sRGB colour, serialisable in settings.
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

/// The three-colour theme.
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
    /// Default EVE-flavoured dark theme: cyan accent on near-black blue.
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

    /// High-contrast light theme to prove the 3-colour model works either polarity.
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

    /// Apply this theme to an egui context by deriving a full `Visuals`.
    pub fn apply(&self, ctx: &egui::Context) {
        let bg = self.background.color();
        let fg = self.foreground.color();
        let accent = self.accent.color();

        let dark = luminance(bg) < 0.5;
        // Contrast direction: lighten surfaces on dark themes, darken on light ones.
        let contrast = if dark {
            Color32::WHITE
        } else {
            Color32::BLACK
        };

        let surface = mix(bg, contrast, 0.05);
        let surface_hi = mix(bg, contrast, 0.10);
        let surface_active = mix(bg, contrast, 0.16);
        let muted = mix(fg, bg, 0.45);
        let line = mix(bg, contrast, 0.18);

        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        v.override_text_color = Some(fg);
        v.panel_fill = surface;
        v.window_fill = surface;
        v.extreme_bg_color = bg;
        v.faint_bg_color = mix(bg, contrast, 0.03);
        v.window_stroke = Stroke::new(1.0, line);
        v.hyperlink_color = accent;

        v.selection.bg_fill = accent.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, accent);

        // Widget states.
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

        // Don't draw a loading spinner over pending remote images (pilot/corp/alliance/ship
        // icons from images.evetech.net). The spinner self-animates via request_repaint every
        // frame, so on a busy intel feed — where images are always loading (and some 404 or are
        // slow) — it pins the UI at a continuous repaint and burns CPU. The image still appears
        // once its loader thread finishes (it requests a single repaint then).
        v.image_loading_spinners = false;

        ctx.set_visuals(v);

        // Spacing/density polish — a little more breathing room than egui defaults.
        ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
            style.spacing.interact_size.y = 26.0;
            style.spacing.menu_margin = egui::Margin::same(8);
        });
    }
}

/// Semantic standing colours — a small fixed palette, separate from the 3-colour
/// theme because friend/foe meaning can't be derived from chrome colours
/// (docs/DESIGN.md §6.3). Overriding these is an Advanced setting (not yet exposed).
#[allow(dead_code)]
pub mod standing {
    use egui::Color32;
    pub const HOSTILE: Color32 = Color32::from_rgb(0xD8, 0x4C, 0x4C);
    pub const NEUTRAL: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xA8);
    pub const FRIENDLY: Color32 = Color32::from_rgb(0x5A, 0xC8, 0x6A);
    pub const CORP: Color32 = Color32::from_rgb(0x4F, 0x9B, 0xD8);
    pub const ALLIANCE: Color32 = Color32::from_rgb(0x9B, 0x6F, 0xD8);
    pub const WARNING: Color32 = Color32::from_rgb(0xE0, 0xA4, 0x3A);
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn luminance(c: Color32) -> f32 {
    (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0
}
