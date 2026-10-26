//! Recorded fleet movement: who was where, in what, and when.

use super::*;
use crate::fleets::movement::{Kind, MoveEvent, Via};

/// How long recorded movement is kept.
pub const FLEET_MOVES_RETENTION_SECS: i64 = 90 * 86_400;

impl Store {
    /// Starts or refreshes a fleet's recording.
    pub fn fleet_track_seen(&self, fleet_id: &str, name: &str, at: i64) {
        self.exec_historic(
            "INSERT INTO fleet_tracks (fleet_id, name, started_at, seen_at) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(fleet_id) DO UPDATE SET name = ?2, seen_at = ?3",
            params![fleet_id, name, at],
        );
    }

    pub fn fleet_track_closed(&self, fleet_id: &str, at: i64) {
        self.exec_historic(
            "UPDATE fleet_tracks SET closed_at = ?2 WHERE fleet_id = ?1",
            params![fleet_id, at],
        );
    }

    /// Fleets still being recorded when the app last ran, seen after `since`.
    pub fn fleet_tracks_open(&self, since: i64) -> Vec<(String, String)> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT fleet_id, name FROM fleet_tracks WHERE closed_at IS NULL AND seen_at >= ?1",
        ) else {
            return Vec::new();
        };
        st.query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn add_fleet_moves(&self, fleet_id: &str, events: &[MoveEvent]) {
        for e in events {
            self.exec_historic(
                "INSERT INTO fleet_moves (fleet_id, at, kind, character_id, name, system_id, from_system,
                     ship_type_id, ship_name, via, count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    fleet_id,
                    e.at,
                    e.kind.key(),
                    e.character_id,
                    e.name,
                    e.system_id,
                    e.from_system,
                    e.ship_type_id,
                    e.ship_name,
                    e.via.map(|v| v.key()),
                    e.count,
                ],
            );
        }
    }

    /// A fleet's whole record, oldest first.
    pub fn fleet_moves(&self, fleet_id: &str) -> Vec<MoveEvent> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT at, kind, character_id, name, system_id, from_system, ship_type_id, ship_name, via, count
             FROM fleet_moves WHERE fleet_id = ?1 ORDER BY at, rowid",
        ) else {
            return Vec::new();
        };
        st.query_map(params![fleet_id], |r| {
            let kind: String = r.get(1)?;
            let via: Option<String> = r.get(8)?;
            Ok((kind, via, MoveEvent {
                at: r.get(0)?,
                kind: Kind::Resume,
                character_id: r.get(2)?,
                name: r.get(3)?,
                system_id: r.get(4)?,
                from_system: r.get(5)?,
                ship_type_id: r.get(6)?,
                ship_name: r.get(7)?,
                via: None,
                count: r.get(9)?,
            }))
        })
        .map(|rows| {
            rows.flatten()
                .filter_map(|(kind, via, e)| {
                    Some(MoveEvent { kind: Kind::parse(&kind)?, via: via.as_deref().and_then(Via::parse), ..e })
                })
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn prune_fleet_moves(&self, now: i64) {
        let before = now - FLEET_MOVES_RETENTION_SECS;
        self.exec_historic("DELETE FROM fleet_moves WHERE at < ?1", params![before]);
        self.exec_historic("DELETE FROM fleet_tracks WHERE seen_at < ?1", params![before]);
        self.exec_historic("DELETE FROM fleet_kills WHERE at < ?1", params![before]);
        // A resume straight after another says nothing new, and a tracker respawned on every frame
        // once left 100k of them behind.
        self.exec_historic(
            "DELETE FROM fleet_moves WHERE rowid IN (
                 SELECT rowid FROM (
                     SELECT rowid, kind, LAG(kind) OVER (PARTITION BY fleet_id ORDER BY at, rowid) AS prev
                     FROM fleet_moves)
                 WHERE kind = 'resume' AND prev = 'resume')",
            [],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_drops_back_to_back_resumes_and_keeps_the_rest() {
        let s = Store::mem();
        let ev = |at: i64, kind: Kind| MoveEvent::new(at, kind);
        s.add_fleet_moves("f", &[ev(100, Kind::Join), ev(200, Kind::Resume), ev(200, Kind::Resume), ev(201, Kind::Resume)]);
        s.add_fleet_moves("f", &[ev(300, Kind::Leave), ev(400, Kind::Resume)]);
        s.add_fleet_moves("g", &[ev(200, Kind::Resume)]);
        s.prune_fleet_moves(500);
        let kinds = |id: &str| s.fleet_moves(id).iter().map(|e| (e.at, e.kind)).collect::<Vec<_>>();
        assert_eq!(kinds("f"), [(100, Kind::Join), (200, Kind::Resume), (300, Kind::Leave), (400, Kind::Resume)]);
        assert_eq!(kinds("g"), [(200, Kind::Resume)], "another fleet's resume is its own");
    }
}
