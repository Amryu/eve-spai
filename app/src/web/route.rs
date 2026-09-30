pub use spai_core::route::*;

/// Warnings from raw reports, because the desktop has no published snapshot when the web view is
/// off.
pub fn danger_from_reports(
    reports: &[crate::intel::IntelReport],
    rules: &crate::settings::SeverityRules,
    ttl: i64,
    now: i64,
    kills: &[(i64, u32, u32)],
) -> std::collections::HashMap<i64, HopWarning> {
    let marks: Vec<(i64, u8, i64)> = reports
        .iter()
        .filter(|r| ttl <= 0 || now - r.received <= ttl)
        .filter(|r| !r.clear)
        .flat_map(|r| {
            let sev = crate::app::severity_of(r, rules) as u8;
            r.systems.iter().map(move |s| (s.id, sev, r.received))
        })
        .collect();
    danger_from_marks(&marks, kills)
}
