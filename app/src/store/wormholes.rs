//! Scanned and synced wormhole connections.

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SystemSig {
    pub sig: String,
    pub kind: String,
    pub group: String,
    pub name: String,
    pub added_at: i64,
    pub updated_at: i64,
    pub who: String,
}

/// A random id for a new entry, stable wherever it is later synced to.
pub(crate) fn new_uid() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("the OS random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Store {
    pub fn upsert_wormhole(&self, incoming: &crate::wormholes::Wormhole) -> i64 {
        // BEGIN IMMEDIATE so concurrent find-then-insert upserts serialize; otherwise two
        // connections both miss the row and the loser's INSERT is dropped by the UNIQUE
        // constraint. The RAII guard rolls back on drop, so a failed commit cannot leave the
        // connection wedged inside the transaction.
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
        let uid = if incoming.uid.is_empty() { new_uid() } else { incoming.uid.clone() };
        let _ = self.conn.execute(
            "INSERT INTO wormholes(dedup, system_id, signature, wh_type, dest_class,
                dest_system_id, dest_signature, dest_wh_type, size, is_drifter, reported_at,
                explicit_expiry, source, updated_at, seen_by, detected_by, jumped_at, mass, note, uid,
                life, observed_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
            params![
                key, incoming.system_id, incoming.signature, incoming.wh_type,
                incoming.dest.code(), incoming.dest_system_id, incoming.dest_signature,
                incoming.dest_wh_type, incoming.size.map(|s| s.code()), incoming.is_drifter as i64,
                incoming.reported_at, incoming.explicit_expiry, incoming.source.code(),
                incoming.updated_at, (incoming.seen_by | incoming.source.bit()) as i64,
                incoming.detected_by, incoming.jumped_at, incoming.mass.map(|m| m.code()), incoming.note, uid,
                incoming.life.map(|l| l.code()), incoming.observed_at,
            ],
        );
        let id = self.conn.last_insert_rowid();
        let inserted = crate::wormholes::Wormhole { uid, ..incoming.clone() };
        self.share_track(None, &inserted);
        id
    }

    pub(crate) fn write_wormhole(&self, w: &crate::wormholes::Wormhole) {
        let before = self.wormhole_by_id(w.id);
        let _ = self.conn.execute(
            "UPDATE wormholes SET system_id=?2, signature=?3, wh_type=?4, dest_class=?5,
                dest_system_id=?6, dest_signature=?7, dest_wh_type=?8, size=?9, is_drifter=?10,
                reported_at=?11, explicit_expiry=?12, source=?13, updated_at=?14, seen_by=?15,
                detected_by=?16, jumped_at=?17, mass=?18, note=?19, life=?20, observed_at=?21 WHERE id=?1",
            params![
                w.id, w.system_id, w.signature, w.wh_type, w.dest.code(), w.dest_system_id,
                w.dest_signature, w.dest_wh_type, w.size.map(|s| s.code()), w.is_drifter as i64,
                w.reported_at, w.explicit_expiry, w.source.code(), w.updated_at,
                (w.seen_by | w.source.bit()) as i64, w.detected_by, w.jumped_at, w.mass.map(|m| m.code()), w.note,
                w.life.map(|l| l.code()), w.observed_at,
            ],
        );
        if let Some(before) = before {
            let after = crate::wormholes::Wormhole { uid: before.uid.clone(), ..w.clone() };
            self.share_track(Some(&before), &after);
        }
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

    /// Records who said what about hole `uid`, one row per field.
    pub fn audit_wormhole(&self, uid: &str, who: &str, source: crate::wormholes::Source, changes: &[(&str, String)]) {
        let at = chrono::Utc::now().timestamp();
        for (field, value) in changes {
            self.exec_historic(
                "INSERT INTO wormhole_audit (uid, at, who, source, field, value) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![uid, at, who, source.code(), field, value],
            );
        }
    }

    pub fn wormhole_audit(&self, uid: &str) -> Vec<(i64, String, String, String, String)> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT at, who, source, field, value FROM wormhole_audit WHERE uid = ?1 ORDER BY at, rowid",
        ) else {
            return Vec::new();
        };
        st.query_map(params![uid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn system_sigs(&self, system_id: i64) -> Vec<SystemSig> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT sig, kind, grp, name, added_at, updated_at, who FROM system_sigs WHERE system_id = ?1 ORDER BY added_at DESC, sig",
        ) else {
            return Vec::new();
        };
        st.query_map(params![system_id], |r| {
            Ok(SystemSig {
                sig: r.get(0)?,
                kind: r.get(1)?,
                group: r.get(2)?,
                name: r.get(3)?,
                added_at: r.get(4)?,
                updated_at: r.get(5)?,
                who: r.get(6)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    /// Folds a probe scanner paste into a system's list. A group or name already known is not
    /// blanked by a paste that has not scanned it yet. With `drop_missing`, whatever the paste
    /// lacks is gone from space. Returns (new, updated, removed).
    pub fn merge_system_sigs(
        &self,
        system_id: i64,
        scan: &[crate::wormholes::ScanSig],
        who: &str,
        now: i64,
        drop_missing: bool,
    ) -> (usize, usize, usize) {
        let old: std::collections::HashMap<String, SystemSig> = self.system_sigs(system_id).into_iter().map(|s| (s.sig.clone(), s)).collect();
        let (mut added, mut updated) = (0, 0);
        for s in scan {
            match old.get(&s.id) {
                Some(o) => {
                    let group = if s.group.is_empty() { &o.group } else { &s.group };
                    let name = if s.name.is_empty() { &o.name } else { &s.name };
                    if *group != o.group || *name != o.name {
                        updated += 1;
                    }
                    self.exec_historic(
                        "UPDATE system_sigs SET kind=?3, grp=?4, name=?5, updated_at=?6, who=?7 WHERE system_id=?1 AND sig=?2",
                        params![system_id, s.id, s.kind, group, name, now, who],
                    );
                }
                None => {
                    added += 1;
                    self.exec_historic(
                        "INSERT INTO system_sigs (system_id, sig, kind, grp, name, added_at, updated_at, who) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
                        params![system_id, s.id, s.kind, s.group, s.name, now, who],
                    );
                }
            }
        }
        self.share_track_sigs(system_id, scan, drop_missing, now);
        let mut removed = 0;
        if drop_missing {
            for sig in old.keys().filter(|k| !scan.iter().any(|s| &s.id == *k)) {
                self.exec_historic("DELETE FROM system_sigs WHERE system_id=?1 AND sig=?2", params![system_id, sig]);
                removed += 1;
            }
        }
        (added, updated, removed)
    }

    pub fn delete_system_sig(&self, system_id: i64, sig: &str) {
        self.exec_historic("DELETE FROM system_sigs WHERE system_id=?1 AND sig=?2", params![system_id, sig]);
        self.share_track_sig_delete(system_id, sig);
    }

    /// Signatures unseen for days are long gone from space.
    pub fn prune_system_sigs(&self, older_than: i64) {
        self.exec_historic("DELETE FROM system_sigs WHERE updated_at < ?1", params![older_than]);
    }

    pub fn wh_layout(&self) -> std::collections::HashMap<i64, (f32, f32)> {
        let Ok(mut st) = self.conn.prepare("SELECT system_id, x, y FROM wh_layout") else {
            return Default::default();
        };
        st.query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, f64>(1)? as f32, r.get::<_, f64>(2)? as f32))))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn set_wh_layout(&self, system_id: i64, x: f32, y: f32) {
        self.exec_historic(
            "INSERT INTO wh_layout (system_id, x, y) VALUES (?1, ?2, ?3) ON CONFLICT(system_id) DO UPDATE SET x=excluded.x, y=excluded.y",
            params![system_id, x as f64, y as f64],
        );
    }

    pub fn clear_wh_layout(&self) {
        self.exec_historic("DELETE FROM wh_layout", []);
    }

    pub fn wormhole_by_id(&self, id: i64) -> Option<crate::wormholes::Wormhole> {
        self.wormhole_where("id=?1", params![id])
    }

    /// A hole the user has marked dead stays in the table (an EVE-Scout resync would otherwise just
    /// re-add it) but is invisible to everything downstream: overlay, routing, waypoints.
    pub fn kill_wormhole(&self, id: i64) {
        let _ = self.conn.execute("UPDATE wormholes SET dead = 1 WHERE id = ?1", params![id]);
        if let Some(w) = self.wormhole_by_id(id) {
            self.share_track_dead(&w);
        }
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
                    "SELECT id, dead, (source = 'eve-scout' OR (seen_by & 1) != 0), updated_at FROM wormholes
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
                COALESCE(explicit_expiry, reported_at + (CASE WHEN is_drifter THEN 3600 ELSE 172800 END)) <= ?1",
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
            seen_by: row.get::<_, i64>(14)? as u8,
            detected_by: row.get(15)?,
            jumped_at: row.get(16)?,
            mass: row.get::<_, Option<String>>(17)?.and_then(|m| crate::wormholes::Mass::from_code(&m)),
            note: row.get(18)?,
            uid: row.get::<_, Option<String>>(19)?.unwrap_or_default(),
            life: row.get::<_, Option<String>>(20)?.and_then(|l| crate::wormholes::Life::from_code(&l)),
            observed_at: row.get(21)?,
        })
    }
}
