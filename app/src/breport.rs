pub use crate::battle::BattleReportDoc;

pub fn default_file_name(battle: &crate::battle::Battle) -> String {
    let system = battle
        .systems
        .first()
        .map(|(_, name, _)| name.as_str())
        .unwrap_or("battle");
    let safe: String = system
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect();
    let date = chrono::DateTime::from_timestamp(battle.start, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    format!("{safe}-{date}.evespai-br.json")
}
