//! What sharing groups need locally: the groups and their keys, who is in them, which log entries
//! are applied, the outbox of local changes, and each hole field's last change.
//!
//! A local write marks what it changed and queues the hole; a change from a group is written with
//! `applying_remote` set, so it is never queued again. That is what keeps two installs from sending
//! the same change back and forth.

use super::*;
use std::collections::HashMap;
use crate::share::hole::{self, Clock};
use crate::share::ops::{HoleState, Member, Role, SigRow};
use crate::wormholes::{ScanSig, Source, Wormhole};

/// Live holes, recently collapsed ones, and the signatures of each system.
pub type Snapshot = (Vec<HoleState>, Vec<String>, Vec<(i64, Vec<SigRow>)>);

#[derive(Clone, Debug, PartialEq)]
pub struct ShareGroup {
    pub id: String,
    pub name: String,
    /// Which of this install's characters is the member.
    pub char_id: i64,
    pub role: Role,
    pub epoch: u32,
    pub cursor: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outgoing {
    Hole(String),
    Dead(String),
    Sigs { system_id: i64, rows: Vec<SigRow>, drop_missing: bool, at: i64 },
    SigDelete { system_id: i64, sig: String },
}

fn role_code(r: Role) -> &'static str {
    match r {
        Role::Member => "member",
        Role::Admin => "admin",
        Role::Owner => "owner",
    }
}

fn role_of(s: &str) -> Role {
    match s {
        "owner" => Role::Owner,
        "admin" => Role::Admin,
        _ => Role::Member,
    }
}

impl Store {
    fn sharing(&self) -> bool {
        !self.applying_remote.get()
            && self.conn.query_row("SELECT 1 FROM share_groups LIMIT 1", [], |_| Ok(())).is_ok()
    }

    /// Marks the fields a local write changed and queues the hole for the groups.
    pub(crate) fn share_track(&self, before: Option<&Wormhole>, after: &Wormhole) {
        if after.source == Source::EveScout || after.uid.is_empty() || !self.sharing() {
            return;
        }
        let fields = hole::changed(before, after);
        if fields.is_empty() {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        for f in fields {
            let _ = self.conn.execute(
                "INSERT INTO wh_field_clock (uid, field, at, by) VALUES (?1, ?2, ?3, 0)
                 ON CONFLICT(uid, field) DO UPDATE SET at = MAX(excluded.at, wh_field_clock.at + 1), by = 0",
                params![after.uid, f, now],
            );
        }
        self.queue("hole", Some(&after.uid), None);
    }

    pub(crate) fn share_track_dead(&self, w: &Wormhole) {
        if w.source != Source::EveScout && !w.uid.is_empty() && self.sharing() {
            self.queue("dead", Some(&w.uid), None);
        }
    }

    pub(crate) fn share_track_sigs(&self, system_id: i64, scan: &[ScanSig], drop_missing: bool, at: i64) {
        if !self.sharing() {
            return;
        }
        let rows: Vec<SigRow> = scan
            .iter()
            .map(|s| SigRow { sig: s.id.clone(), kind: s.kind.clone(), group: s.group.clone(), name: s.name.clone(), added_at: at })
            .collect();
        let payload = serde_json::json!({ "system_id": system_id, "rows": rows, "drop_missing": drop_missing, "at": at });
        self.queue("sigs", None, Some(&payload.to_string()));
    }

    pub(crate) fn share_track_sig_delete(&self, system_id: i64, sig: &str) {
        if self.sharing() {
            let payload = serde_json::json!({ "system_id": system_id, "sig": sig });
            self.queue("sigdel", None, Some(&payload.to_string()));
        }
    }

    fn queue(&self, kind: &str, uid: Option<&str>, payload: Option<&str>) {
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO share_outbox (kind, uid, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, uid, payload, chrono::Utc::now().timestamp()],
        );
    }

    /// Queues every live hole this install knows, for a group that was just joined or made.
    pub fn share_queue_all(&self) {
        let uids: Vec<String> = self
            .conn
            .prepare("SELECT uid FROM wormholes WHERE dead = 0 AND source != 'eve-scout' AND uid IS NOT NULL")
            .and_then(|mut st| st.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default();
        let now = chrono::Utc::now().timestamp();
        for uid in uids {
            if let Some(w) = self.wormhole_where("uid=?1", params![uid]) {
                for f in hole::changed(None, &w) {
                    let _ = self.conn.execute(
                        "INSERT OR IGNORE INTO wh_field_clock (uid, field, at, by) VALUES (?1, ?2, ?3, 0)",
                        params![uid, f, w.observed_at.unwrap_or(w.updated_at).min(now)],
                    );
                }
                self.queue("hole", Some(&uid), None);
            }
        }
    }

    /// The oldest queued changes, with their outbox ids.
    pub fn share_outbox(&self, limit: usize) -> Vec<(i64, Outgoing)> {
        let Ok(mut st) = self.conn.prepare("SELECT id, kind, uid, payload FROM share_outbox ORDER BY id LIMIT ?1") else {
            return Vec::new();
        };
        let rows: Vec<(i64, String, Option<String>, Option<String>)> = st
            .query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|(id, kind, uid, payload)| {
                let p = || payload.as_deref().and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok());
                let out = match kind.as_str() {
                    "hole" => Outgoing::Hole(uid?),
                    "dead" => Outgoing::Dead(uid?),
                    "sigs" => {
                        let v = p()?;
                        Outgoing::Sigs {
                            system_id: v["system_id"].as_i64()?,
                            rows: serde_json::from_value(v["rows"].clone()).ok()?,
                            drop_missing: v["drop_missing"].as_bool().unwrap_or(false),
                            at: v["at"].as_i64()?,
                        }
                    }
                    "sigdel" => {
                        let v = p()?;
                        Outgoing::SigDelete { system_id: v["system_id"].as_i64()?, sig: v["sig"].as_str()?.to_owned() }
                    }
                    _ => return None,
                };
                Some((id, out))
            })
            .collect()
    }

    pub fn share_outbox_len(&self) -> i64 {
        self.conn.query_row("SELECT COUNT(*) FROM share_outbox", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn share_outbox_done(&self, id: i64) {
        let _ = self.conn.execute("DELETE FROM share_outbox WHERE id = ?1", params![id]);
    }

    fn clocks(&self, uid: &str) -> HashMap<String, Clock> {
        self.conn
            .prepare("SELECT field, at, by FROM wh_field_clock WHERE uid = ?1")
            .and_then(|mut st| {
                st.query_map(params![uid], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?)))).map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default()
    }

    /// A hole as the group log carries it, local changes signed over to `me`.
    pub fn share_hole_state(&self, uid: &str, me: i64) -> Option<HoleState> {
        let w = self.wormhole_where("uid=?1", params![uid])?;
        (w.source != Source::EveScout).then(|| hole::state(&w, &self.clocks(uid), me))
    }

    /// After sending: local changes now carry `me` as their author.
    pub fn share_settle(&self, uid: &str, me: i64) {
        let _ = self.conn.execute("UPDATE wh_field_clock SET by = ?2 WHERE uid = ?1 AND by = 0", params![uid, me]);
    }

    fn resolve_uid(&self, uid: &str) -> String {
        self.conn
            .query_row("SELECT uid FROM wh_uid_alias WHERE alias = ?1", params![uid], |r| r.get(0))
            .unwrap_or_else(|_| uid.to_owned())
    }

    /// One hole may reach two installs under two uids. Both settle on the smaller, so every
    /// member ends with the same one without anything being re-sent.
    fn rename_uid(&self, from: &str, to: &str) {
        for sql in [
            "UPDATE wormholes SET uid = ?2 WHERE uid = ?1",
            "UPDATE OR REPLACE wh_field_clock SET uid = ?2 WHERE uid = ?1",
            "UPDATE OR IGNORE share_outbox SET uid = ?2 WHERE uid = ?1",
            "UPDATE wormhole_audit SET uid = ?2 WHERE uid = ?1",
        ] {
            let _ = self.conn.execute(sql, params![from, to]);
        }
        let _ = self.conn.execute("INSERT OR REPLACE INTO wh_uid_alias (alias, uid) VALUES (?1, ?2)", params![from, to]);
    }

    fn with_remote<T>(&self, f: impl FnOnce() -> T) -> T {
        let was = self.applying_remote.replace(true);
        let out = f();
        self.applying_remote.set(was);
        out
    }

    /// Folds a hole from `group` in. Returns whether anything here changed.
    pub fn share_apply_hole(&self, remote: &HoleState, group: &str, who: &str) -> bool {
        self.with_remote(|| {
            let uid = self.resolve_uid(&remote.uid);
            let mut row = match self.wormhole_where("uid=?1", params![uid]) {
                Some(r) => r,
                None => {
                    let id = self.upsert_wormhole(&hole::fresh(remote));
                    let Some(r) = self.wormhole_by_id(id) else { return false };
                    if r.uid != remote.uid {
                        if remote.uid < r.uid {
                            self.rename_uid(&r.uid.clone(), &remote.uid);
                        } else {
                            let _ = self.conn.execute(
                                "INSERT OR REPLACE INTO wh_uid_alias (alias, uid) VALUES (?1, ?2)",
                                params![remote.uid, r.uid],
                            );
                        }
                    }
                    match self.wormhole_by_id(id) {
                        Some(r) => r,
                        None => return false,
                    }
                }
            };
            let mut clocks = self.clocks(&row.uid);
            let taken = hole::merge(&mut row, &mut clocks, remote);
            for (field, (at, by)) in &taken {
                let _ = self.conn.execute(
                    "INSERT OR REPLACE INTO wh_field_clock (uid, field, at, by) VALUES (?1, ?2, ?3, ?4)",
                    params![row.uid, field, at, by],
                );
            }
            let _ = self.conn.execute("UPDATE wormholes SET group_id = COALESCE(group_id, ?2) WHERE id = ?1", params![row.id, group]);
            if taken.is_empty() {
                return false;
            }
            row.seen_by |= remote_source_bit(&remote.source);
            row.updated_at = row.updated_at.max(taken.iter().map(|(_, (at, _))| *at).max().unwrap_or(0));
            self.write_wormhole(&row);
            let changes: Vec<(&str, String)> =
                taken.iter().map(|(f, _)| (f.as_str(), hole::get(&row, f).to_string().trim_matches('"').to_owned())).collect();
            self.audit_wormhole(&row.uid, who, Source::from_code(&remote.source), &changes);
            true
        })
    }

    pub fn share_apply_dead(&self, uid: &str) {
        self.with_remote(|| {
            let uid = self.resolve_uid(uid);
            let _ = self.conn.execute("UPDATE wormholes SET dead = 1 WHERE uid = ?1", params![uid]);
        });
    }

    pub fn share_apply_sigs(&self, system_id: i64, rows: &[SigRow], drop_missing: bool, at: i64, who: &str) {
        self.with_remote(|| {
            let scan: Vec<ScanSig> =
                rows.iter().map(|r| ScanSig { id: r.sig.clone(), kind: r.kind.clone(), group: r.group.clone(), name: r.name.clone() }).collect();
            self.merge_system_sigs(system_id, &scan, who, at, drop_missing);
        });
    }

    pub fn share_apply_sig_delete(&self, system_id: i64, sig: &str) {
        self.with_remote(|| self.delete_system_sig(system_id, sig));
    }

    /// Everything a new member needs that the log may no longer hold.
    pub fn share_snapshot(&self, me: i64) -> Snapshot {
        let live: Vec<String> = self
            .conn
            .prepare("SELECT uid FROM wormholes WHERE dead = 0 AND source != 'eve-scout' AND uid IS NOT NULL")
            .and_then(|mut st| st.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default();
        let holes = live.iter().filter_map(|u| self.share_hole_state(u, me)).collect();
        let dead: Vec<String> = self
            .conn
            .prepare("SELECT uid FROM wormholes WHERE dead = 1 AND uid IS NOT NULL AND updated_at > ?1")
            .and_then(|mut st| {
                st.query_map(params![chrono::Utc::now().timestamp() - 3 * 86_400], |r| r.get(0)).map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default();
        let systems: Vec<i64> = self
            .conn
            .prepare("SELECT DISTINCT system_id FROM system_sigs")
            .and_then(|mut st| st.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default();
        let sigs = systems
            .into_iter()
            .map(|sys| {
                let rows = self
                    .system_sigs(sys)
                    .into_iter()
                    .map(|s| SigRow { sig: s.sig, kind: s.kind, group: s.group, name: s.name, added_at: s.added_at })
                    .collect();
                (sys, rows)
            })
            .collect();
        (holes, dead, sigs)
    }

    pub fn share_groups(&self) -> Vec<ShareGroup> {
        self.conn
            .prepare("SELECT id, name, char_id, role, epoch, cursor FROM share_groups ORDER BY joined_at")
            .and_then(|mut st| {
                st.query_map([], |r| {
                    Ok(ShareGroup {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        char_id: r.get(2)?,
                        role: role_of(&r.get::<_, String>(3)?),
                        epoch: r.get(4)?,
                        cursor: r.get(5)?,
                    })
                })
                .map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default()
    }

    pub fn share_group_save(&self, g: &ShareGroup) {
        let _ = self.conn.execute(
            "INSERT INTO share_groups (id, name, char_id, role, epoch, cursor, joined_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, char_id=excluded.char_id, role=excluded.role, epoch=excluded.epoch, cursor=excluded.cursor",
            params![g.id, g.name, g.char_id, role_code(g.role), g.epoch, g.cursor, chrono::Utc::now().timestamp()],
        );
    }

    pub fn share_group_forget(&self, id: &str) {
        for sql in [
            "DELETE FROM share_groups WHERE id = ?1",
            "DELETE FROM share_keys WHERE group_id = ?1",
            "DELETE FROM share_members WHERE group_id = ?1",
            "DELETE FROM share_applied WHERE group_id = ?1",
        ] {
            let _ = self.conn.execute(sql, params![id]);
        }
    }

    pub fn share_key(&self, group: &str, epoch: u32) -> Option<crate::share::crypto::Key> {
        let text: String =
            self.conn.query_row("SELECT key FROM share_keys WHERE group_id = ?1 AND epoch = ?2", params![group, epoch], |r| r.get(0)).ok()?;
        crate::share::crypto::unb64_32(&text).ok()
    }

    pub fn share_key_save(&self, group: &str, epoch: u32, key: &crate::share::crypto::Key) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO share_keys (group_id, epoch, key) VALUES (?1, ?2, ?3)",
            params![group, epoch, crate::share::crypto::b64(key)],
        );
    }

    pub fn share_members(&self, group: &str) -> Vec<Member> {
        self.conn
            .prepare("SELECT body FROM share_members WHERE group_id = ?1")
            .and_then(|mut st| {
                st.query_map(params![group], |r| r.get::<_, String>(0))
                    .map(|rows| rows.flatten().filter_map(|b| serde_json::from_str(&b).ok()).collect())
            })
            .unwrap_or_default()
    }

    pub fn share_members_save(&self, group: &str, members: &[Member]) {
        let _ = self.conn.execute("DELETE FROM share_members WHERE group_id = ?1", params![group]);
        for m in members {
            let _ = self.conn.execute(
                "INSERT INTO share_members (group_id, char_id, body) VALUES (?1, ?2, ?3)",
                params![group, m.char_id, serde_json::to_string(m).unwrap_or_default()],
            );
        }
    }

    /// Records an entry as applied. False when it already was.
    pub fn share_mark_applied(&self, op_id: &str, group: &str) -> bool {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO share_applied (op_id, group_id, at) VALUES (?1, ?2, ?3)",
                params![op_id, group, chrono::Utc::now().timestamp()],
            )
            .is_ok_and(|n| n == 1)
    }

    pub fn share_unmark_applied(&self, op_id: &str) -> bool {
        self.conn.execute("DELETE FROM share_applied WHERE op_id = ?1", params![op_id]).is_ok()
    }

    pub fn share_set_group(&self, uid: &str, group: &str) {
        let _ = self.conn.execute("UPDATE wormholes SET group_id = COALESCE(group_id, ?2) WHERE uid = ?1", params![uid, group]);
    }

    pub fn share_cursor_save(&self, group: &str, cursor: i64) {
        let _ = self.conn.execute("UPDATE share_groups SET cursor = ?2 WHERE id = ?1", params![group, cursor]);
    }

    pub fn share_invite_save(&self, id: &str, group: &str, secret: &crate::share::crypto::Key, for_char: i64, for_name: &str) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO share_invites (id, group_id, secret, for_char, for_name, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, group, crate::share::crypto::b64(secret), for_char, for_name, chrono::Utc::now().timestamp()],
        );
    }

    /// An invite this install made: its secret and the character it is for.
    pub fn share_invite(&self, id: &str) -> Option<(crate::share::crypto::Key, i64, String)> {
        let (s, c, n): (String, i64, String) = self
            .conn
            .query_row("SELECT secret, for_char, for_name FROM share_invites WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .ok()?;
        Some((crate::share::crypto::unb64_32(&s).ok()?, c, n))
    }

    pub fn wormhole_groups(&self) -> HashMap<String, String> {
        self.conn
            .prepare("SELECT uid, group_id FROM wormholes WHERE group_id IS NOT NULL AND uid IS NOT NULL")
            .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()))
            .unwrap_or_default()
    }

    /// The group a hole first came from, if it came from one.
    pub fn wormhole_group(&self, uid: &str) -> Option<String> {
        self.conn.query_row("SELECT group_id FROM wormholes WHERE uid = ?1", params![uid], |r| r.get(0)).ok().flatten()
    }
}

fn remote_source_bit(code: &str) -> u8 {
    Source::from_code(code).bit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wormholes::{DestClass, Mass};

    fn joined(s: &Store) {
        s.share_group_save(&ShareGroup { id: "g".into(), name: "Chain".into(), char_id: 1, role: Role::Owner, epoch: 0, cursor: 0 });
    }

    fn hole() -> Wormhole {
        Wormhole {
            system_id: 31_000_001,
            signature: Some("ABC".into()),
            dest: DestClass::Highsec,
            dest_system_id: Some(30_000_142),
            source: Source::Manual,
            reported_at: 100,
            updated_at: 100,
            ..Default::default()
        }
    }

    #[test]
    fn local_edits_are_queued_and_remote_ones_are_not() {
        let a = Store::mem();
        joined(&a);
        let id = a.upsert_wormhole(&hole());
        let uid = a.wormhole_by_id(id).unwrap().uid;
        assert_eq!(a.share_outbox(10).len(), 1);
        let (oid, _) = a.share_outbox(10)[0].clone();
        a.share_outbox_done(oid);

        // The same hole arriving from the group on another install: not sent back.
        let b = Store::mem();
        joined(&b);
        let mut state = a.share_hole_state(&uid, 1).unwrap();
        assert!(b.share_apply_hole(&state, "g", "Pilot 1"));
        assert!(b.share_outbox(10).is_empty(), "a remote change must not echo");
        assert!(!b.share_apply_hole(&state, "g", "Pilot 1"), "applying twice changes nothing");

        // B degrades it; A takes that without queuing it again.
        let mut w = b.wormhole_where("uid=?1", params![uid]).unwrap();
        w.mass = Some(Mass::Critical);
        b.write_wormhole(&w);
        state = b.share_hole_state(&uid, 2).unwrap();
        assert!(a.share_apply_hole(&state, "g", "Pilot 2"));
        assert_eq!(a.wormhole_where("uid=?1", params![uid]).unwrap().mass, Some(Mass::Critical));
        assert!(a.share_outbox(10).is_empty());
    }

    #[test]
    fn one_hole_under_two_uids_settles_on_the_smaller() {
        let a = Store::mem();
        joined(&a);
        let mine = a.upsert_wormhole(&Wormhole { uid: "ffff".into(), ..hole() });
        let theirs = HoleState { uid: "0000".into(), ..a.share_hole_state("ffff", 2).unwrap() };
        a.share_apply_hole(&theirs, "g", "Pilot 2");
        assert_eq!(a.wormhole_by_id(mine).unwrap().uid, "0000");
    }
}
