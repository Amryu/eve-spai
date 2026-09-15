//! Scanned and synced wormhole connections.

use super::*;

impl Store {
    pub fn upsert_wormhole(&self, incoming: &crate::wormholes::Wormhole) -> i64 {
        // The find-then-insert/update below is a read-modify-write; two scout/watcher
        // connections could both miss the row and both INSERT, and the loser's INSERT would
        // hit the dedup UNIQUE constraint and be silently dropped. BEGIN IMMEDIATE takes the
        // write lock up front so concurrent upserts serialize.
        //
        // RAII rather than a hand-rolled COMMIT: a discarded commit error left the connection
        // inside the transaction, so every later BEGIN on it failed and it stayed wedged for the
        // rest of the process. Dropping the guard rolls back instead.
        match rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        ) {
            Ok(tx) => {
                let id = self.upsert_wormhole_locked(incoming);
                if let Err(e) = tx.commit() {
                    crate::disk::note_sqlite_error(&e);
                }
                id
            }
            // No lock available, so run unserialized rather than drop the report entirely.
            Err(e) => {
                crate::disk::note_sqlite_error(&e);
                self.upsert_wormhole_locked(incoming)
            }
        }
    }

    pub(crate) fn upsert_wormhole_locked(&self, incoming: &crate::wormholes::Wormhole) -> i64 {
        use crate::wormholes::DestClass;
        // Signature-matching an intel report and the EVE-Scout entry for one Thera/Turnur connection
        // splits them (signatures differ); their system+dest dedup key collapses them instead.
        let special = matches!(incoming.dest, DestClass::Thera | DestClass::Turnur);
        if !special {
            if let Some(sig) = incoming.signature.as_deref().filter(|s| !s.is_empty()) {
                if let Some(mut near) = self.wormhole_where(
                    "system_id=?1 AND signature=?2",
                    params![incoming.system_id, sig],
                ) {
                    near.merge_from(incoming);
                    self.write_wormhole(&near);
                    return near.id;
                }
                if let Some(mut owner) = self.wormhole_where(
                    "dest_system_id=?1 AND dest_signature=?2",
                    params![incoming.system_id, sig],
                ) {
                    owner.confirm_far(incoming);
                    self.write_wormhole(&owner);
                    return owner.id;
                }
            }
        }
        let key = incoming.dedup_key();
        if let Some(mut existing) = self.wormhole_where("dedup=?1", params![key]) {
            existing.merge_from(incoming);
            self.write_wormhole(&existing);
            return existing.id;
        }
        let _ = self.conn.execute(
            "INSERT INTO wormholes(dedup, system_id, signature, wh_type, dest_class,
                dest_system_id, dest_signature, dest_wh_type, size, is_drifter, reported_at,
                explicit_expiry, source, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                key, incoming.system_id, incoming.signature, incoming.wh_type,
                incoming.dest.code(), incoming.dest_system_id, incoming.dest_signature,
                incoming.dest_wh_type, incoming.size.map(|s| s.code()), incoming.is_drifter as i64,
                incoming.reported_at, incoming.explicit_expiry, incoming.source.code(),
                incoming.updated_at,
            ],
        );
        self.conn.last_insert_rowid()
    }

    pub(crate) fn write_wormhole(&self, w: &crate::wormholes::Wormhole) {
        let _ = self.conn.execute(
            "UPDATE wormholes SET system_id=?2, signature=?3, wh_type=?4, dest_class=?5,
                dest_system_id=?6, dest_signature=?7, dest_wh_type=?8, size=?9, is_drifter=?10,
                reported_at=?11, explicit_expiry=?12, source=?13, updated_at=?14 WHERE id=?1",
            params![
                w.id, w.system_id, w.signature, w.wh_type, w.dest.code(), w.dest_system_id,
                w.dest_signature, w.dest_wh_type, w.size.map(|s| s.code()), w.is_drifter as i64,
                w.reported_at, w.explicit_expiry, w.source.code(), w.updated_at,
            ],
        );
    }

    pub(crate) fn wormhole_where(
        &self,
        cond: &str,
        params: impl rusqlite::Params,
    ) -> Option<crate::wormholes::Wormhole> {
        self.conn
            .query_row(
                &format!("SELECT {} FROM wormholes WHERE {cond}", Self::WH_COLS),
                params,
                Self::row_to_wormhole,
            )
            .ok()
    }

    /// A hole the user has marked dead stays in the table (an EVE-Scout resync would otherwise just
    /// re-add it) but is invisible to everything downstream: overlay, routing, waypoints.
    pub fn kill_wormhole(&self, id: i64) {
        let _ = self.conn.execute("UPDATE wormholes SET dead = 1 WHERE id = ?1", params![id]);
    }

    /// Collapse Thera/Turnur duplicates to one row per system. EVE-Scout's row survives (best facts),
    /// but if any duplicate was dead the survivor stays dead: an intel/manual kill wins over
    /// EVE-Scout's lag. Legacy rows predate the system+dest dedup key, so this repairs their key too.
    pub fn collapse_special_holes(&self) {
        for code in ["thera", "turnur"] {
            let systems: Vec<i64> = match self.conn.prepare(
                "SELECT system_id FROM wormholes WHERE dest_class = ?1 GROUP BY system_id HAVING COUNT(*) > 1",
            ) {
                Ok(mut stmt) => match stmt.query_map(params![code], |r| r.get::<_, i64>(0)) {
                    Ok(rows) => rows.flatten().collect(),
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            for sys in systems {
                let rows: Vec<(i64, bool, bool, i64)> = match self.conn.prepare(
                    "SELECT id, dead, source = 'eve-scout', updated_at FROM wormholes
                     WHERE dest_class = ?1 AND system_id = ?2",
                ) {
                    Ok(mut stmt) => match stmt.query_map(params![code, sys], |r| {
                        Ok((r.get(0)?, r.get::<_, i64>(1)? != 0, r.get::<_, i64>(2)? != 0, r.get(3)?))
                    }) {
                        Ok(rows) => rows.flatten().collect(),
                        Err(_) => continue,
                    },
                    Err(_) => continue,
                };
                if rows.len() < 2 {
                    continue;
                }
                let any_dead = rows.iter().any(|r| r.1);
                let survivor = rows.iter().max_by_key(|r| (r.2, r.3)).map(|r| r.0).unwrap();
                for r in &rows {
                    if r.0 != survivor {
                        let _ = self.conn.execute("DELETE FROM wormholes WHERE id = ?1", params![r.0]);
                    }
                }
                let _ = self.conn.execute(
                    "UPDATE wormholes SET dead = ?1, dedup = ?2 WHERE id = ?3",
                    params![any_dead as i64, format!("{sys}|{code}"), survivor],
                );
            }
        }
    }

    /// Delete EVE-Scout holes absent from its latest fetch (`keep` = ids just upserted): the source
    /// of truth dropped them. Deleted, not dead-marked, so a genuine re-scan re-adds cleanly.
    pub fn retire_missing_evescout(&self, keep: &std::collections::HashSet<i64>) {
        let live: Vec<i64> = {
            let mut stmt = match self.conn.prepare("SELECT id FROM wormholes WHERE source = 'eve-scout'")
            {
                Ok(s) => s,
                Err(_) => return,
            };
            let rows = stmt.query_map([], |r| r.get::<_, i64>(0));
            match rows {
                Ok(rows) => rows.flatten().collect(),
                Err(_) => return,
            }
        };
        for id in live {
            if !keep.contains(&id) {
                let _ = self.conn.execute("DELETE FROM wormholes WHERE id = ?1", params![id]);
            }
        }
    }

    pub fn wormholes(&self) -> Vec<crate::wormholes::Wormhole> {
        let mut out = Vec::new();
        if let Ok(mut stmt) =
            self.conn.prepare(&format!("SELECT {} FROM wormholes WHERE dead = 0", Self::WH_COLS))
        {
            if let Ok(rows) = stmt.query_map([], Self::row_to_wormhole) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn prune_wormholes(&self, now: i64) {
        let _ = self.conn.execute(
            "DELETE FROM wormholes WHERE
                COALESCE(explicit_expiry, reported_at + (CASE WHEN is_drifter THEN 86400 ELSE 172800 END)) <= ?1",
            params![now],
        );
    }

    pub(crate) fn row_to_wormhole(row: &rusqlite::Row) -> rusqlite::Result<crate::wormholes::Wormhole> {
        use crate::wormholes::{DestClass, ShipSize, Source, Wormhole};
        let dest_code: String = row.get(4)?;
        let size_code: Option<String> = row.get(8)?;
        let source_code: String = row.get(12)?;
        Ok(Wormhole {
            id: row.get(0)?,
            system_id: row.get(1)?,
            signature: row.get(2)?,
            wh_type: row.get(3)?,
            dest: DestClass::from_code(&dest_code),
            dest_system_id: row.get(5)?,
            dest_signature: row.get(6)?,
            dest_wh_type: row.get(7)?,
            size: size_code.and_then(|c| ShipSize::from_code(&c)),
            is_drifter: row.get::<_, i64>(9)? != 0,
            reported_at: row.get(10)?,
            explicit_expiry: row.get(11)?,
            source: Source::from_code(&source_code),
            updated_at: row.get(13)?,
        })
    }
}
