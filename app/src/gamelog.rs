// Combat-event alerts are disabled for now (see app.rs); kept intact for re-enabling.
#![allow(dead_code)]

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CombatKind {
    Scrambled,
    UnderAttack,
}

impl CombatKind {
    pub fn message(self) -> &'static str {
        match self {
            CombatKind::Scrambled => "You are warp scrambled!",
            CombatKind::UnderAttack => "Under attack!",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameLogLine {
    #[allow(dead_code)]
    pub time: String,
    pub kind: Option<CombatKind>,
}

pub fn read(path: &Path) -> Vec<GameLogLine> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(raw: &str) -> Option<GameLogLine> {
    let line = raw.trim_start_matches('\u{feff}').trim();
    let rest = line.strip_prefix("[ ")?;
    let (time, rest) = rest.split_once(" ] ")?;
    let rest = rest.strip_prefix('(')?;
    let (typ, message) = rest.split_once(") ")?;
    Some(GameLogLine {
        time: time.trim().to_owned(),
        kind: classify(typ.trim(), message),
    })
}

pub fn classify(typ: &str, message: &str) -> Option<CombatKind> {
    let m = message.to_lowercase();
    if m.contains("warp scramble attempt") || m.contains("warp disruption attempt") {
        return Some(CombatKind::Scrambled);
    }
    // Incoming damage reads "<n> from <source> - ..."; outgoing reads "to".
    if typ == "combat" && m.contains(" from ") {
        return Some(CombatKind::UnderAttack);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_combat_events() {
        assert_eq!(
            classify("combat", "Warp scramble attempt from <b>Ganker</b> to you!"),
            Some(CombatKind::Scrambled)
        );
        assert_eq!(
            classify("combat", "<b>240</b> from <b>Ganker</b> - Hobgoblin - Hits"),
            Some(CombatKind::UnderAttack)
        );
        assert_eq!(classify("combat", "<b>240</b> to <b>Victim</b> - Hits"), None);
        assert_eq!(classify("notify", "Some hint message"), None);
    }

    #[test]
    fn parses_a_line() {
        let l = parse_line("[ 2026.06.22 18:30:45 ] (combat) 99 from Rat - Hits").unwrap();
        assert_eq!(l.time, "2026.06.22 18:30:45");
        assert_eq!(l.kind, Some(CombatKind::UnderAttack));
    }
}
