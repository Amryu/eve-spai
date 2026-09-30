//! Colours both apps draw with.

/// Hostile standing: red.
pub const HOSTILE: egui::Color32 = egui::Color32::from_rgb(0xD8, 0x4C, 0x4C);

/// EVE's security status colours, from 0.0 (purple-red) to 1.0 (cyan).
pub fn security_color(security: f64) -> egui::Color32 {
    const COLORS: [(u8, u8, u8); 11] = [
        (0xB0, 0x3A, 0x9A),
        (0xD7, 0x30, 0x00),
        (0xF0, 0x48, 0x00),
        (0xF0, 0x60, 0x00),
        (0xD7, 0x77, 0x00),
        (0xEF, 0xEF, 0x00),
        (0x8F, 0xEF, 0x2F),
        (0x00, 0xF0, 0x00),
        (0x00, 0xEF, 0x47),
        (0x48, 0xF0, 0xC0),
        (0x2F, 0xEF, 0xEF),
    ];
    let idx = (security * 10.0).round().clamp(0.0, 10.0) as usize;
    let (r, g, b) = COLORS[idx];
    egui::Color32::from_rgb(r, g, b)
}
