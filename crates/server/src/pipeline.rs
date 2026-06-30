//! The upload pipeline's pure, DB-free core: bounded gzip decompression (gzip-bomb
//! guard), server-side re-derivation of the battle snapshot, canonical hashing,
//! id generation, and column extraction. Everything here is unit-tested without a
//! database or network.

use br_core::battle::{Battle, BattleReportDoc, BATTLE_BREAK_SECS};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::io::Read;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("decompressed body exceeds the {0}-byte cap")]
    TooLarge(usize),
    #[error("gzip decode failed: {0}")]
    Gzip(std::io::Error),
    #[error("invalid battle report: {0}")]
    Parse(String),
}

/// Base62-ish id alphabet with visually ambiguous glyphs (0/O, 1/I/l) removed, so a
/// shared id is unambiguous when read aloud or copied. URL-safe.
const ID_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const ID_LEN: usize = 10;

/// A random ~10-char public id.
pub fn generate_id() -> String {
    let mut rng = rand::rng();
    (0..ID_LEN)
        .map(|_| ID_ALPHABET[rng.random_range(0..ID_ALPHABET.len())] as char)
        .collect()
}

/// Decompress gzip into memory, aborting once output would exceed `cap`. Reads at
/// most `cap + 1` bytes from the decoder, so a gzip bomb can never allocate beyond
/// the ceiling. Returns [`PipelineError::TooLarge`] when the cap is breached.
pub fn decompress_bounded(gz: &[u8], cap: usize) -> Result<Vec<u8>, PipelineError> {
    let mut reader = flate2::read::GzDecoder::new(gz).take(cap as u64 + 1);
    let mut out = Vec::new();
    reader.read_to_end(&mut out).map_err(PipelineError::Gzip)?;
    if out.len() > cap {
        return Err(PipelineError::TooLarge(cap));
    }
    Ok(out)
}

/// Parse the decompressed bytes into a document (rejecting unknown `format_version`,
/// which `from_json` already enforces).
pub fn parse_doc(bytes: &[u8]) -> Result<BattleReportDoc, PipelineError> {
    let s = std::str::from_utf8(bytes).map_err(|e| PipelineError::Parse(e.to_string()))?;
    BattleReportDoc::from_json(s).map_err(|e| PipelineError::Parse(format!("{e:#}")))
}

/// Replace the client-supplied battle snapshot with one re-derived server-side from
/// the raw engagements. Client tallies are never trusted: the stored `battle` (and
/// every extracted column) comes from `br_core`'s own clustering of `engagements`.
pub fn rederive(doc: &mut BattleReportDoc) {
    doc.battle = br_core::battle::preview_battle(doc.engagements.clone(), BATTLE_BREAK_SECS);
}

/// Canonical JSON (sorted object keys via `serde_json::Value`'s `BTreeMap`) so the
/// hash and stored bytes are stable across runs. Returns `(canonical_value, sha256)`.
pub fn canonicalize(doc: &BattleReportDoc) -> Result<(serde_json::Value, String), PipelineError> {
    let value = serde_json::to_value(doc).map_err(|e| PipelineError::Parse(e.to_string()))?;
    let bytes = serde_json::to_vec(&value).map_err(|e| PipelineError::Parse(e.to_string()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok((value, hex::encode(h.finalize())))
}

/// The flat columns extracted from a re-derived battle for indexed querying.
#[derive(Debug, Clone)]
pub struct Columns {
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub systems: Vec<String>,
    pub system_ids: Vec<i64>,
    pub total_isk: f64,
    pub kills: i32,
    pub participants: i32,
    pub side_names: Vec<String>,
}

/// Derive the stored columns from a (server-computed) battle.
pub fn extract_columns(battle: &Battle) -> Columns {
    let systems: Vec<String> = battle.systems.iter().map(|(_, name, _)| name.clone()).collect();
    let system_ids: Vec<i64> = battle.systems.iter().map(|(id, _, _)| *id).collect();

    // A side's display name: its coalition, else its most-involved party.
    let side_names: Vec<String> = battle
        .sides
        .iter()
        .map(|s| {
            s.coalition
                .clone()
                .or_else(|| s.parties.first().map(|p| p.name.clone()))
                .unwrap_or_else(|| "Unknown".to_string())
        })
        .collect();

    // Distinct pilots across the battle (victims + attackers), ignoring NPCs (id 0).
    let mut pilots = std::collections::HashSet::new();
    for e in &battle.engagements {
        if e.victim_char != 0 {
            pilots.insert(e.victim_char);
        }
        for a in &e.attackers {
            if a.char_id != 0 {
                pilots.insert(a.char_id);
            }
        }
    }

    Columns {
        started_at: chrono::DateTime::from_timestamp(battle.start, 0),
        ended_at: chrono::DateTime::from_timestamp(battle.end, 0),
        systems,
        system_ids,
        total_isk: battle.isk,
        kills: battle.kills as i32,
        participants: pilots.len() as i32,
        side_names,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use br_core::battle::{Attacker, Engagement, Overrides, Party, PartyKind};
    use std::io::Write;

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(bytes).unwrap();
        enc.finish().unwrap()
    }

    fn party(id: i64, name: &str) -> Party {
        Party { id, name: name.to_string(), kind: PartyKind::Alliance }
    }

    // A kill: `victim_side` loses a ship to `killer_side`.
    fn eng(kill_id: i64, time: i64, victim_side: (i64, &str), killer_side: (i64, &str)) -> Engagement {
        Engagement {
            kill_id,
            time,
            system_id: 30000142,
            system_name: "Jita".to_string(),
            security: 0.9,
            victim: party(victim_side.0, victim_side.1),
            victim_char: 1000 + kill_id,
            victim_pilot: format!("Victim {kill_id}"),
            victim_ship: 587,
            attackers: vec![Attacker {
                party: party(killer_side.0, killer_side.1),
                char_id: 2000 + kill_id,
                ship: 588,
                pilot: format!("Killer {kill_id}"),
                final_blow: true,
            }],
            isk: 1_000_000.0,
            anchored: true,
        }
    }

    fn real_doc() -> BattleReportDoc {
        let red = (100, "Red Alliance");
        let blue = (200, "Blue Alliance");
        let engs = vec![
            eng(1, 0, red, blue),
            eng(2, 30, blue, red),
            eng(3, 60, red, blue),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        BattleReportDoc::new(battle, engs, Overrides::default(), Some("Test".into()), 1_700_000_000)
    }

    #[test]
    fn gzip_bomb_aborts_past_cap() {
        let bomb = gzip(&vec![0u8; 1_000_000]); // ~1 MB of zeros, compresses tiny
        match decompress_bounded(&bomb, 1024) {
            Err(PipelineError::TooLarge(1024)) => {}
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn decompress_within_cap_ok() {
        let payload = b"hello battle report";
        let out = decompress_bounded(&gzip(payload), 1024).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn unknown_format_version_rejected() {
        let mut doc = real_doc();
        doc.format_version = 999;
        let json = serde_json::to_string(&doc).unwrap();
        assert!(matches!(parse_doc(json.as_bytes()), Err(PipelineError::Parse(_))));
    }

    #[test]
    fn sha256_is_deterministic() {
        let doc = real_doc();
        let (_, a) = canonicalize(&doc).unwrap();
        let (_, b) = canonicalize(&doc).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn rederivation_overwrites_forged_tallies() {
        let mut doc = real_doc();
        // Forge garbage client tallies: absurd isk, wrong kill count, bogus systems.
        doc.battle.isk = 9.9e30;
        doc.battle.kills = 99999;
        doc.battle.systems = vec![(1, "Forged System".into(), -1.0)];
        doc.battle.start = 1;
        doc.battle.end = 2;

        rederive(&mut doc);

        // The stored battle must match br_core's own clustering of the engagements,
        // not the forged numbers.
        let expected = br_core::battle::preview_battle(doc.engagements.clone(), BATTLE_BREAK_SECS);
        assert_eq!(doc.battle, expected);
        assert_eq!(doc.battle.kills, 3);
        assert!((doc.battle.isk - 3_000_000.0).abs() < 1e-6);
        assert_eq!(doc.battle.systems, vec![(30000142, "Jita".to_string(), 0.9)]);
    }

    #[test]
    fn extracted_columns_come_from_battle() {
        let doc = real_doc();
        let cols = extract_columns(&doc.battle);
        assert_eq!(cols.kills, 3);
        assert_eq!(cols.systems, vec!["Jita".to_string()]);
        assert_eq!(cols.system_ids, vec![30000142]);
        assert!((cols.total_isk - 3_000_000.0).abs() < 1e-6);
        assert!(cols.participants >= 2);
        assert_eq!(cols.side_names.len(), doc.battle.sides.len());
    }

    #[test]
    fn id_shape() {
        let id = generate_id();
        assert_eq!(id.len(), ID_LEN);
        assert!(id.bytes().all(|b| ID_ALPHABET.contains(&b)));
        assert_ne!(generate_id(), generate_id());
    }
}
