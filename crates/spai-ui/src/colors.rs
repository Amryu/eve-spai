//! Colours both apps draw with.

/// Hostile standing: red.
pub const HOSTILE: egui::Color32 = egui::Color32::from_rgb(0xD8, 0x4C, 0x4C);

/// Fitting resources as the fitting window tints them: CPU blue, powergrid amber, calibration teal.
pub const CPU: egui::Color32 = egui::Color32::from_rgb(0x5A, 0xA9, 0xE0);
pub const POWERGRID: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x9A, 0x45);
pub const CALIBRATION: egui::Color32 = egui::Color32::from_rgb(0x6B, 0xC9, 0xA0);

/// Damage types in the game's order: EM, thermal, kinetic, explosive.
pub const DAMAGE: [egui::Color32; 4] = [
    egui::Color32::from_rgb(0x5A, 0xA9, 0xE0),
    egui::Color32::from_rgb(0xD6, 0x45, 0x45),
    egui::Color32::from_rgb(0x9A, 0xA3, 0xA8),
    egui::Color32::from_rgb(0xD6, 0xA6, 0x45),
];

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
