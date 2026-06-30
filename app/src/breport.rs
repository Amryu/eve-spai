//! Battle-report sharing — the versioned document used to save a battle report to disk (and, in a
//! later milestone, to upload it to the server).
//!
//! Milestone 1 keeps the document type itself in [`crate::battle`] alongside the battle model it
//! embeds; this module re-exports it so the rest of the app refers to a single "battle report"
//! home. Milestone 2 will lift [`BattleReportDoc`] into a shared crate and grow this module with
//! the upload / public-id machinery.

pub use crate::battle::BattleReportDoc;

/// Default file name for an exported report: `<system>-<date>.evespai-br.json`, where `system`
/// is the battle's first system (sanitised) and `date` is the battle start in UTC `YYYY-MM-DD`.
pub fn default_file_name(battle: &crate::battle::Battle) -> String {
    let system = battle
        .systems
        .first()
        .map(|(_, name, _)| name.as_str())
        .unwrap_or("battle");
    // Keep the name filesystem-safe: letters/digits/dash/underscore, others -> '-'.
    let safe: String = system
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect();
    let date = chrono::DateTime::from_timestamp(battle.start, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    format!("{safe}-{date}.evespai-br.json")
}
