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

pub use spai_share::store::{Outgoing, ShareGroup, SharePrefs, ShareStore, Snapshot};

fn role_code(r: Role) -> &'static str {
    r.code()
}

fn role_of(s: &str) -> Role {
    match s {
        "owner" => Role::Owner,
        "admin" => Role::Admin,
        "viewer" => Role::Viewer,
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
        let now = crate::clock::utc().timestamp();
        for f in fields {
            let _ = self.conn.execute(
                "INSERT INTO wh_field_clock (uid, field, at, by) VALUES (?1, ?2, ?3, 0)
                 ON CONFLICT(uid, field) DO UPDATE SET at = MAX(excluded.at, wh_field_clock.at + 1), by = 0",
                params![after.uid, f, now],
            );
        }
        self.queue("hole", Some(&after.uid), None, self.wormhole_group(&after.uid).as_deref());
    }

    pub(crate) fn share_track_dead(&self, w: &Wormhole) {
        if w.source != Source::EveScout && !w.uid.is_empty() && self.sharing() {
            self.queue("dead", Some(&w.uid), None, self.wormhole_group(&w.uid).as_deref());
        }
    }

    pub(crate) fn share_track_sigs(&self, system_id: i64, scan: &[ScanSig], drop_missing: bool, at: i64) {
        if !self.sharing() {
            return;
        }
        let found: std::collections::HashMap<String, (i64, Option<i64>)> =
            self.sigs_in(system_id, false).into_iter().map(|s| (s.sig, (s.added_at, s.fresh_after))).collect();
        let rows: Vec<SigRow> = scan
            .iter()
            .map(|s| {
                let (added_at, fresh_after) = found.get(&s.id).copied().unwrap_or((at, None));
                SigRow { sig: s.id.clone(), kind: s.kind.clone(), group: s.group.clone(), name: s.name.clone(), added_at, fresh_after }
            })
            .collect();
        let payload = serde_json::json!({ "system_id": system_id, "rows": rows, "drop_missing": drop_missing, "at": at });
        self.queue("sigs", None, Some(&payload.to_string()), None);
    }

    pub(crate) fn share_track_sig_restore(&self, system_id: i64, sigs: &[SystemSig]) {
        if !self.sharing() {
            return;
        }
        let mut origins: Vec<Option<&str>> = sigs.iter().map(|s| s.origin.as_deref()).collect();
        origins.sort_unstable();
        origins.dedup();
        for origin in origins {
            let some: Vec<&SystemSig> = sigs.iter().filter(|s| s.origin.as_deref() == origin).collect();
            let at = some.iter().map(|s| s.updated_at).max().unwrap_or_default();
            let payload = serde_json::json!({ "system_id": system_id, "rows": sig_rows(some), "drop_missing": false, "at": at });
            self.queue("sigs", None, Some(&payload.to_string()), origin);
        }
    }

    pub(crate) fn share_track_sig_delete(&self, system_id: i64, sig: &str, origin: Option<&str>) {
        if self.sharing() {
            let payload = serde_json::json!({ "system_id": system_id, "sig": sig });
            self.queue("sigdel", None, Some(&payload.to_string()), origin);
        }
    }

    /// One outbox row for each group that takes the change.
    fn queue(&self, kind: &str, uid: Option<&str>, payload: Option<&str>, origin: Option<&str>) {
        for g in self.share_groups() {
            if g.takes(kind, origin) && self.share_key(&g.id, g.epoch).is_some() {
                self.queue_to(kind, uid, payload, &g.id);
            }
        }
    }

    fn queue_to(&self, kind: &str, uid: Option<&str>, payload: Option<&str>, group: &str) {
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO share_outbox (kind, uid, payload, created_at, group_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![kind, uid, payload, crate::clock::utc().timestamp(), group],
        );
    }

    /// Queues what `group` takes of what this install knows, for a group just joined or made or
    /// one that was just set to get more.
    /// `holes` and `sigs` narrow it to what was just switched on.
    pub fn share_queue_group(&self, group: &str, holes: bool, sigs: bool) {
        let Some(g) = self.share_groups().into_iter().find(|g| g.id == group) else { return };
        if holes && g.prefs.send_holes {
            self.queue_holes_to(&g.id);
        }
        if sigs && g.prefs.send_sigs {
            let systems: Vec<i64> = self
                .conn
                .prepare("SELECT DISTINCT system_id FROM system_sigs WHERE origin IS NULL OR origin = ?1")
                .and_then(|mut st| st.query_map(params![g.id], |r| r.get(0)).map(|rows| rows.flatten().collect()))
                .unwrap_or_default();
            for sys in systems {
                let sigs: Vec<SystemSig> = self.sigs_in(sys, false).into_iter().filter(|s| s.origin.as_deref().is_none_or(|o| o == g.id)).collect();
                let at = sigs.iter().map(|s| s.updated_at).max().unwrap_or_default();
                let payload = serde_json::json!({ "system_id": sys, "rows": sig_rows(sigs.iter()), "drop_missing": false, "at": at });
                self.queue_to("sigs", None, Some(&payload.to_string()), &g.id);
            }
        }
    }

    fn queue_holes_to(&self, group: &str) {
        let uids: Vec<String> = self
            .conn
            .prepare("SELECT uid FROM wormholes WHERE dead = 0 AND source != 'eve-scout' AND uid IS NOT NULL AND (group_id IS NULL OR group_id = ?1)")
            .and_then(|mut st| st.query_map(params![group], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default();
        let now = crate::clock::utc().timestamp();
        for uid in uids {
            if let Some(w) = self.wormhole_where("uid=?1", params![uid]) {
                for f in hole::changed(None, &w) {
                    let _ = self.conn.execute(
                        "INSERT OR IGNORE INTO wh_field_clock (uid, field, at, by) VALUES (?1, ?2, ?3, 0)",
                        params![uid, f, w.observed_at.unwrap_or(w.updated_at).min(now)],
                    );
                }
                self.queue_to("hole", Some(&uid), None, group);
            }
        }
    }

    /// The oldest queued changes, with their outbox ids and the group each is for (`None`:
    /// queued before sends were per group, so for every group that takes it).
    pub fn share_outbox(&self, limit: usize) -> Vec<(i64, Option<String>, Outgoing)> {
        let Ok(mut st) = self.conn.prepare("SELECT id, kind, uid, payload, group_id FROM share_outbox ORDER BY id LIMIT ?1") else {
            return Vec::new();
        };
        let rows: Vec<(i64, String, Option<String>, Option<String>, Option<String>)> = st
            .query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|(id, kind, uid, payload, group)| {
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
                Some((id, group, out))
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
        if !hole::system_id_ok(remote.system_id) || remote.uid.is_empty() || remote.uid.len() > 64 {
            return false;
        }
        self.with_remote(|| {
            let uid = self.resolve_uid(&remote.uid);
            let mut row = match self.wormhole_where("uid=?1", params![uid]) {
                Some(r) => r,
                None => {
                    // The same jump seen here too, under this install's uid: one hole, not two,
                    // even when only one side has its signature yet.
                    let fresh = hole::fresh(remote);
                    let now = crate::clock::utc().timestamp();
                    let twin = fresh
                        .dest_system_id
                        .and_then(|b| self.wormhole_where("dead = 0 AND system_id=?1 AND dest_system_id=?2", params![fresh.system_id, b]))
                        .filter(|w| w.same_connection(&fresh) && !w.is_expired(now));
                    let id = match twin {
                        Some(w) => w.id,
                        None => self.upsert_wormhole(&fresh),
                    };
                    let Some(r) = self.wormhole_by_id(id) else { return false };
                    // New here: it belongs to the group. A hole this install already had stays
                    // its own, so it is still shared with the other groups and never hidden.
                    if r.uid == remote.uid {
                        let _ = self.conn.execute("UPDATE wormholes SET group_id = ?2 WHERE id = ?1", params![r.id, group]);
                    }
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

    pub fn share_apply_sigs(&self, system_id: i64, rows: &[SigRow], drop_missing: bool, at: i64, who: &str, group: &str) {
        // Bigger than any probe scan, or not a system: not something an honest client sends.
        if !hole::system_id_ok(system_id) || rows.len() > 2000 {
            return;
        }
        let fits = |r: &&SigRow| r.sig.chars().count() <= 16 && [&r.kind, &r.group, &r.name].iter().all(|s| s.chars().count() <= 100);
        self.with_remote(|| {
            let scan: Vec<ScanSig> = rows
                .iter()
                .filter(fits)
                .map(|r| ScanSig { id: r.sig.clone(), kind: r.kind.clone(), group: r.group.clone(), name: r.name.clone() })
                .collect();
            // A member's clock may be off, but not by a month.
            let found = rows
                .iter()
                .filter(fits)
                .filter(|r| r.added_at > at - 30 * 86_400)
                .map(|r| (r.sig.clone(), (r.added_at, r.fresh_after.filter(|t| *t > r.added_at - 30 * 86_400))))
                .collect();
            self.merge_sigs_found(system_id, &scan, who, at, drop_missing, Some(group), &found);
        });
    }

    pub fn share_apply_sig_delete(&self, system_id: i64, sig: &str) {
        self.with_remote(|| self.delete_system_sig(system_id, sig));
    }

    fn strings(&self, sql: &str, p: impl rusqlite::Params) -> Vec<String> {
        self.conn.prepare(sql).and_then(|mut st| st.query_map(p, |r| r.get(0)).map(|rows| rows.flatten().collect())).unwrap_or_default()
    }

    /// Everything a new member of `g` needs that the log may no longer hold: what `g` takes, never
    /// what came from another group.
    pub fn share_snapshot(&self, g: &ShareGroup) -> Snapshot {
        let (mut holes, mut dead, mut sigs) = (Vec::new(), Vec::new(), Vec::new());
        if g.prefs.send_holes {
            let live = self.strings(
                "SELECT uid FROM wormholes WHERE dead = 0 AND source != 'eve-scout' AND uid IS NOT NULL AND (group_id IS NULL OR group_id = ?1)",
                params![g.id],
            );
            holes = live.iter().filter_map(|u| self.share_hole_state(u, g.char_id)).collect();
            dead = self.strings(
                "SELECT uid FROM wormholes WHERE dead = 1 AND uid IS NOT NULL AND (group_id IS NULL OR group_id = ?1) AND updated_at > ?2",
                params![g.id, crate::clock::utc().timestamp() - 3 * 86_400],
            );
        }
        if g.prefs.send_sigs {
            let systems: Vec<i64> = self
                .conn
                .prepare("SELECT DISTINCT system_id FROM system_sigs WHERE origin IS NULL OR origin = ?1")
                .and_then(|mut st| st.query_map(params![g.id], |r| r.get(0)).map(|rows| rows.flatten().collect()))
                .unwrap_or_default();
            sigs = systems
                .into_iter()
                .map(|sys| {
                    let own: Vec<SystemSig> = self.sigs_in(sys, false).into_iter().filter(|s| s.origin.as_deref().is_none_or(|o| o == g.id)).collect();
                    (sys, sig_rows(own.iter()))
                })
                .collect();
        }
        (holes, dead, sigs)
    }

    pub fn share_groups(&self) -> Vec<ShareGroup> {
        self.conn
            .prepare(
                "SELECT id, name, char_id, role, epoch, cursor, COALESCE(send_holes, 0), COALESCE(send_sigs, 0), recv_holes, recv_sigs, hidden
                 FROM share_groups ORDER BY joined_at",
            )
            .and_then(|mut st| {
                st.query_map([], |r| {
                    Ok(ShareGroup {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        char_id: r.get(2)?,
                        role: role_of(&r.get::<_, String>(3)?),
                        epoch: r.get(4)?,
                        cursor: r.get(5)?,
                        prefs: SharePrefs {
                            send_holes: r.get(6)?,
                            send_sigs: r.get(7)?,
                            recv_holes: r.get(8)?,
                            recv_sigs: r.get(9)?,
                            hidden: r.get(10)?,
                        },
                    })
                })
                .map(|rows| rows.flatten().collect())
            })
            .unwrap_or_default()
    }

    /// Saves a group. Its choices are taken only when it is new: after that they are the sharing
    /// window's, set with [`Self::share_prefs_save`].
    pub fn share_group_save(&self, g: &ShareGroup) {
        let p = &g.prefs;
        let _ = self.conn.execute(
            "INSERT INTO share_groups (id, name, char_id, role, epoch, cursor, joined_at, send_holes, send_sigs, recv_holes, recv_sigs, hidden)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, char_id=excluded.char_id, role=excluded.role, epoch=excluded.epoch, cursor=excluded.cursor",
            params![
                g.id,
                g.name,
                g.char_id,
                role_code(g.role),
                g.epoch,
                g.cursor,
                crate::clock::utc().timestamp(),
                p.send_holes,
                p.send_sigs,
                p.recv_holes,
                p.recv_sigs,
                p.hidden
            ],
        );
    }

    pub fn share_prefs_save(&self, group: &str, p: SharePrefs) {
        let _ = self.conn.execute(
            "UPDATE share_groups SET send_holes = ?2, send_sigs = ?3, recv_holes = ?4, recv_sigs = ?5, hidden = ?6 WHERE id = ?1",
            params![group, p.send_holes, p.send_sigs, p.recv_holes, p.recv_sigs, p.hidden],
        );
        // What is no longer sent there stops waiting to be.
        if !p.send_holes {
            let _ = self.conn.execute("DELETE FROM share_outbox WHERE group_id = ?1 AND kind IN ('hole', 'dead')", params![group]);
        }
        if !p.send_sigs {
            let _ = self.conn.execute("DELETE FROM share_outbox WHERE group_id = ?1 AND kind IN ('sigs', 'sigdel')", params![group]);
        }
    }

    /// Groups from before sends were chosen per group: the one that was "send my changes to"
    /// keeps getting everything, the others nothing, as before.
    pub fn share_prefs_migrate(&self, target: Option<&str>) {
        let _ = self.conn.execute(
            "UPDATE share_groups SET send_holes = COALESCE(send_holes, id = ?1, 0), send_sigs = COALESCE(send_sigs, id = ?1, 0)
             WHERE send_holes IS NULL OR send_sigs IS NULL",
            params![target],
        );
    }

    /// The groups whose data is hidden.
    pub fn share_hidden_groups(&self) -> Vec<String> {
        self.conn
            .prepare("SELECT id FROM share_groups WHERE hidden = 1")
            .and_then(|mut st| st.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default()
    }

    /// How many live holes and signatures came from `group`.
    pub fn share_group_counts(&self, group: &str) -> (i64, i64) {
        let n = |sql: &str| self.conn.query_row(sql, params![group], |r| r.get(0)).unwrap_or(0);
        (n("SELECT COUNT(*) FROM wormholes WHERE group_id = ?1 AND dead = 0"), n("SELECT COUNT(*) FROM system_sigs WHERE origin = ?1"))
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
                params![op_id, group, crate::clock::utc().timestamp()],
            )
            .is_ok_and(|n| n == 1)
    }

    pub fn share_unmark_applied(&self, op_id: &str) -> bool {
        self.conn.execute("DELETE FROM share_applied WHERE op_id = ?1", params![op_id]).is_ok()
    }

    pub fn share_cursor_save(&self, group: &str, cursor: i64) {
        let _ = self.conn.execute("UPDATE share_groups SET cursor = ?2 WHERE id = ?1", params![group, cursor]);
    }

    pub fn share_invite_save(&self, id: &str, group: &str, secret: &crate::share::crypto::Key, for_char: i64, for_name: &str) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO share_invites (id, group_id, secret, for_char, for_name, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, group, crate::share::crypto::b64(secret), for_char, for_name, crate::clock::utc().timestamp()],
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

    pub fn share_invite_role_save(&self, id: &str, role: crate::share::ops::Role) {
        let _ = self.conn.execute("UPDATE share_invites SET role = ?2 WHERE id = ?1", params![id, role.code()]);
    }

    pub fn share_invite_role(&self, id: &str) -> Option<crate::share::ops::Role> {
        let code: Option<String> = self.conn.query_row("SELECT role FROM share_invites WHERE id = ?1", params![id], |r| r.get(0)).ok()?;
        crate::share::ops::Role::from_code(&code?)
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

fn sig_rows<'a>(sigs: impl IntoIterator<Item = &'a SystemSig>) -> Vec<SigRow> {
    sigs.into_iter()
        .map(|s| SigRow { sig: s.sig.clone(), kind: s.kind.clone(), group: s.group.clone(), name: s.name.clone(), added_at: s.added_at, fresh_after: s.fresh_after })
        .collect()
}

fn remote_source_bit(code: &str) -> u8 {
    Source::from_code(code).bit()
}

/// The engine's storage is this database: each method is the `Store` one of the same name.
impl ShareStore for Store {
    fn share_groups(&self) -> Vec<ShareGroup> {
        Store::share_groups(self)
    }
    fn share_group_save(&self, g: &ShareGroup) {
        Store::share_group_save(self, g)
    }
    fn share_group_forget(&self, id: &str) {
        Store::share_group_forget(self, id)
    }
    fn share_key(&self, group: &str, epoch: u32) -> Option<crate::share::crypto::Key> {
        Store::share_key(self, group, epoch)
    }
    fn share_key_save(&self, group: &str, epoch: u32, key: &crate::share::crypto::Key) {
        Store::share_key_save(self, group, epoch, key)
    }
    fn share_members(&self, group: &str) -> Vec<Member> {
        Store::share_members(self, group)
    }
    fn share_members_save(&self, group: &str, members: &[Member]) {
        Store::share_members_save(self, group, members)
    }
    fn share_cursor_save(&self, group: &str, cursor: i64) {
        Store::share_cursor_save(self, group, cursor)
    }
    fn share_mark_applied(&self, op_id: &str, group: &str) -> bool {
        Store::share_mark_applied(self, op_id, group)
    }
    fn share_unmark_applied(&self, op_id: &str) -> bool {
        Store::share_unmark_applied(self, op_id)
    }
    fn share_invite_save(&self, id: &str, group: &str, secret: &crate::share::crypto::Key, for_char: i64, for_name: &str) {
        Store::share_invite_save(self, id, group, secret, for_char, for_name)
    }
    fn share_invite(&self, id: &str) -> Option<(crate::share::crypto::Key, i64, String)> {
        Store::share_invite(self, id)
    }
    fn share_invite_role_save(&self, id: &str, role: crate::share::ops::Role) {
        Store::share_invite_role_save(self, id, role)
    }
    fn share_invite_role(&self, id: &str) -> Option<crate::share::ops::Role> {
        Store::share_invite_role(self, id)
    }
    fn share_queue_group(&self, group: &str, holes: bool, sigs: bool) {
        Store::share_queue_group(self, group, holes, sigs)
    }
    fn share_snapshot(&self, g: &ShareGroup) -> Snapshot {
        Store::share_snapshot(self, g)
    }
    fn share_apply_hole(&self, remote: &HoleState, group: &str, who: &str) -> bool {
        Store::share_apply_hole(self, remote, group, who)
    }
    fn share_apply_dead(&self, uid: &str) {
        Store::share_apply_dead(self, uid)
    }
    fn share_apply_sigs(&self, system_id: i64, rows: &[SigRow], drop_missing: bool, at: i64, who: &str, group: &str) {
        Store::share_apply_sigs(self, system_id, rows, drop_missing, at, who, group)
    }
    fn share_apply_sig_delete(&self, system_id: i64, sig: &str) {
        Store::share_apply_sig_delete(self, system_id, sig)
    }
    fn share_outbox(&self, limit: usize) -> Vec<(i64, Option<String>, Outgoing)> {
        Store::share_outbox(self, limit)
    }
    fn share_outbox_done(&self, id: i64) {
        Store::share_outbox_done(self, id)
    }
    fn share_hole_state(&self, uid: &str, me: i64) -> Option<HoleState> {
        Store::share_hole_state(self, uid, me)
    }
    fn share_settle(&self, uid: &str, me: i64) {
        Store::share_settle(self, uid, me)
    }
    fn wormhole_group(&self, uid: &str) -> Option<String> {
        Store::wormhole_group(self, uid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wormholes::{DestClass, Mass};

    fn joined(s: &Store) {
        join(s, "g", SharePrefs::default());
    }

    /// In group `id` with its key, so changes are queued for it.
    fn join(s: &Store, id: &str, prefs: SharePrefs) {
        s.share_group_save(&ShareGroup { id: id.into(), name: id.into(), char_id: 1, role: Role::Owner, epoch: 0, cursor: 0, prefs });
        s.share_key_save(id, 0, &crate::share::crypto::random32());
    }

    /// Each queued change with the group it is for.
    fn outbox(s: &Store) -> Vec<(String, Outgoing)> {
        s.share_outbox(100).into_iter().map(|(_, g, o)| (g.unwrap_or_default(), o)).collect()
    }

    fn sigs_from(s: &Store, group: &str, sig: &str) {
        let rows = vec![SigRow { sig: sig.into(), kind: "Cosmic Signature".into(), group: "Data Site".into(), name: "Unsecured Frontier Server".into(), added_at: 100, fresh_after: None }];
        s.share_apply_sigs(31_000_200, &rows, false, 100, "Pilot 2", group);
    }

    fn hole() -> Wormhole {
        Wormhole {
            system_id: 31_000_200,
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
    fn a_signature_keeps_when_it_was_first_seen_across_pastes_and_members() {
        use crate::wormholes::ScanSig;
        let sys = 31_000_200;
        let scan = [ScanSig { id: "ABC-123".into(), kind: "Cosmic Signature".into(), group: "Wormhole".into(), name: String::new() }];
        let a = Store::mem();
        joined(&a);
        a.merge_system_sigs(sys, &scan, "me", 1_000, false, None);
        a.merge_system_sigs(sys, &scan, "me", 5_000, false, None);
        assert_eq!(a.system_sigs(sys)[0].added_at, 1_000);
        let mut sent = outbox(&a).into_iter().filter_map(|(_, o)| match o {
            Outgoing::Sigs { rows, .. } => Some(rows),
            _ => None,
        });
        let rows = sent.next_back().expect("queued");
        assert_eq!(rows[0].added_at, 1_000, "a re-paste sends the first sighting, not its own time");

        let b = Store::mem();
        joined(&b);
        b.share_apply_sigs(sys, &rows, false, 5_000, "Pilot 1", "g");
        assert_eq!(b.system_sigs(sys)[0].added_at, 1_000);
        let later = vec![SigRow { added_at: 3_000, ..rows[0].clone() }];
        b.share_apply_sigs(sys, &later, false, 6_000, "Pilot 1", "g");
        assert_eq!(b.system_sigs(sys)[0].added_at, 1_000, "the earliest sighting stays");
        let bogus = vec![SigRow { added_at: 1, ..rows[0].clone() }];
        b.share_apply_sigs(sys, &bogus, false, 90 * 86_400, "Pilot 1", "g");
        assert_eq!(b.system_sigs(sys)[0].added_at, 1_000, "a clock a month off is ignored");
    }

    #[test]
    fn local_edits_are_queued_and_remote_ones_are_not() {
        let a = Store::mem();
        joined(&a);
        let id = a.upsert_wormhole(&hole());
        let uid = a.wormhole_by_id(id).unwrap().uid;
        assert_eq!(a.share_outbox(10).len(), 1);
        let (oid, _, _) = a.share_outbox(10)[0].clone();
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
    fn an_undone_sig_delete_is_shared_back_as_it_was() {
        let _guard = crate::disk::test_guard();
        let a = Store::mem();
        joined(&a);
        let scan = crate::wormholes::probe_scan("ABC-123\tCosmic Signature\tWormhole\tUnstable Wormhole\t100,0%\t8 AU");
        a.merge_system_sigs(31_000_200, &scan, "Scout", 100, false, None);
        let before = a.system_sigs(31_000_200);
        a.delete_system_sig(31_000_200, "ABC-123");
        assert!(a.system_sigs(31_000_200).is_empty());
        a.restore_system_sigs(31_000_200, &before);
        assert_eq!(a.system_sigs(31_000_200), before, "added, seen and who unchanged");
        let out: Vec<Outgoing> = a.share_outbox(10).into_iter().map(|(_, _, o)| o).collect();
        let Some(Outgoing::Sigs { system_id, rows, drop_missing, at }) = out.last() else { panic!("{out:?}") };
        assert!(matches!(out[out.len() - 2], Outgoing::SigDelete { .. }));
        assert_eq!((*system_id, rows.len(), *drop_missing, *at), (31_000_200, 1, false, 100));

        let b = Store::mem();
        joined(&b);
        b.share_apply_sigs(*system_id, rows, *drop_missing, *at, "Scout", "g");
        assert_eq!(b.system_sigs(31_000_200).len(), 1);
    }

        #[test]
    fn each_group_is_sent_what_it_was_set_to_get() {
        let _guard = crate::disk::test_guard();
        let a = Store::mem();
        join(&a, "holes", SharePrefs { send_sigs: false, ..Default::default() });
        join(&a, "scans", SharePrefs { send_holes: false, ..Default::default() });
        join(&a, "none", SharePrefs { send_holes: false, send_sigs: false, ..Default::default() });
        a.upsert_wormhole(&hole());
        let scan = crate::wormholes::probe_scan("ABC-123\tCosmic Signature\tWormhole\tUnstable Wormhole\t100,0%\t8 AU");
        a.merge_system_sigs(31_000_200, &scan, "Scout", 100, false, None);
        let out = outbox(&a);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(matches!(&out[0], (g, Outgoing::Hole(_)) if g == "holes"));
        assert!(matches!(&out[1], (g, Outgoing::Sigs { .. }) if g == "scans"));

        // Switched off: what was waiting for that group is dropped with it.
        a.share_prefs_save("holes", SharePrefs { send_holes: false, send_sigs: false, ..Default::default() });
        assert_eq!(outbox(&a).len(), 1);
    }

    #[test]
    fn what_came_from_one_group_goes_back_to_it_alone() {
        let _guard = crate::disk::test_guard();
        let a = Store::mem();
        join(&a, "g1", SharePrefs::default());
        join(&a, "g2", SharePrefs::default());
        let b = Store::mem();
        join(&b, "g1", SharePrefs::default());
        b.upsert_wormhole(&Wormhole { uid: "aaaa".into(), ..hole() });
        let from_g1 = b.share_hole_state("aaaa", 2).unwrap();
        assert!(a.share_apply_hole(&from_g1, "g1", "Pilot 2"));
        sigs_from(&a, "g1", "GIN-924");
        assert!(outbox(&a).is_empty(), "a remote change is not sent anywhere");

        let mut w = a.wormhole_where("uid=?1", params!["aaaa"]).unwrap();
        w.mass = Some(Mass::Critical);
        a.write_wormhole(&w);
        a.delete_system_sig(31_000_200, "GIN-924");
        let out = outbox(&a);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out.iter().all(|(g, _)| g == "g1"), "{out:?}");

        // A new member of g2 is not handed g1's hole or scans either.
        sigs_from(&a, "g1", "XYZ-999");
        let g2 = a.share_groups().into_iter().find(|g| g.id == "g2").unwrap();
        let (holes, _, sigs) = a.share_snapshot(&g2);
        assert!(holes.is_empty() && sigs.is_empty(), "{holes:?} {sigs:?}");
        let g1 = a.share_groups().into_iter().find(|g| g.id == "g1").unwrap();
        let (holes, _, sigs) = a.share_snapshot(&g1);
        assert_eq!((holes.len(), sigs.len()), (1, 1));
    }

    /// This install and another both saw one jump, one of them with the signature: one hole here,
    /// under the smaller uid, the signature joined in.
    #[test]
    fn one_jump_recorded_on_two_installs_is_one_hole() {
        let now = crate::clock::utc().timestamp();
        let jumped = |uid: &str, sig: Option<&str>| Wormhole {
            uid: uid.into(),
            signature: sig.map(str::to_owned),
            source: Source::Auto,
            reported_at: now,
            updated_at: now,
            ..hole()
        };
        let live = |s: &Store| s.wormholes().into_iter().filter(|w| w.dest_system_id == Some(30_000_142)).map(|w| (w.uid, w.signature)).collect::<Vec<_>>();
        let sent = |uid: &str| {
            let other = Store::mem();
            join(&other, "g1", SharePrefs::default());
            other.upsert_wormhole(&jumped(uid, Some("ABC-123")));
            other.share_hole_state(uid, 2).unwrap()
        };

        let a = Store::mem();
        join(&a, "g1", SharePrefs::default());
        a.upsert_wormhole(&jumped("mmmm", None));
        a.share_apply_hole(&sent("aaaa"), "g1", "Pilot 2");
        assert_eq!(live(&a), vec![("aaaa".to_owned(), Some("ABC-123".to_owned()))]);

        let b = Store::mem();
        join(&b, "g1", SharePrefs::default());
        b.upsert_wormhole(&jumped("mmmm", None));
        b.share_apply_hole(&sent("zzzz"), "g1", "Pilot 2");
        assert_eq!(live(&b), vec![("mmmm".to_owned(), Some("ABC-123".to_owned()))]);
        b.share_apply_dead("zzzz");
        assert!(live(&b).is_empty(), "closing it under the other uid closes it here");
    }

    #[test]
    fn a_hole_this_install_had_stays_its_own_when_a_group_updates_it() {
        let a = Store::mem();
        join(&a, "g1", SharePrefs::default());
        let id = a.upsert_wormhole(&hole());
        let uid = a.wormhole_by_id(id).unwrap().uid;
        let b = Store::mem();
        join(&b, "g1", SharePrefs::default());
        b.upsert_wormhole(&Wormhole { uid: "zzzz".into(), mass: Some(Mass::Reduced), ..hole() });
        b.upsert_wormhole(&Wormhole { uid: "bbbb".into(), system_id: 31_000_201, signature: Some("QQQ".into()), ..hole() });
        assert!(a.share_apply_hole(&b.share_hole_state("zzzz", 2).unwrap(), "g1", "Pilot 2"));
        assert_eq!(a.wormhole_by_id(id).unwrap().mass, Some(Mass::Reduced));
        assert_eq!(a.wormhole_group(&uid), None, "still this install's own");
        a.share_apply_hole(&b.share_hole_state("bbbb", 2).unwrap(), "g1", "Pilot 2");
        assert_eq!(a.wormhole_group("bbbb").as_deref(), Some("g1"));
    }

    #[test]
    fn a_hidden_groups_signatures_are_left_out_until_shown_again() {
        let _guard = crate::disk::test_guard();
        let a = Store::mem();
        join(&a, "g1", SharePrefs::default());
        sigs_from(&a, "g1", "GIN-924");
        let scan = crate::wormholes::probe_scan("ABC-123\tCosmic Signature\tWormhole\tUnstable Wormhole\t100,0%\t8 AU");
        a.merge_system_sigs(31_000_200, &scan, "Scout", 100, false, None);
        assert_eq!(a.system_sigs(31_000_200).len(), 2);
        assert_eq!(a.share_group_counts("g1"), (0, 1));

        a.share_prefs_save("g1", SharePrefs { hidden: true, ..Default::default() });
        assert_eq!(a.share_hidden_groups(), vec!["g1".to_owned()]);
        let shown: Vec<String> = a.system_sigs(31_000_200).into_iter().map(|s| s.sig).collect();
        assert_eq!(shown, vec!["ABC-123".to_owned()]);
        assert_eq!(a.all_system_sigs().len(), 1);
        assert_eq!(a.sig_pasters().len(), 1, "only the local paster");
        assert_eq!(a.sigs_in(31_000_200, false).len(), 2, "still stored");

        a.share_prefs_save("g1", SharePrefs::default());
        assert_eq!(a.system_sigs(31_000_200).len(), 2);
    }

    #[test]
    fn groups_from_before_the_choice_keep_sending_where_they_did() {
        let a = Store::mem();
        for id in ["old-target", "other"] {
            a.conn
                .execute("INSERT INTO share_groups (id, name, char_id, role, joined_at) VALUES (?1, ?1, 1, 'member', 0)", params![id])
                .unwrap();
        }
        let p = |id: &str| a.share_groups().into_iter().find(|g| g.id == id).unwrap().prefs;
        assert!(!p("old-target").send_holes, "unmigrated sends nothing");
        a.share_prefs_migrate(Some("old-target"));
        assert_eq!(p("old-target"), SharePrefs::default());
        assert_eq!(p("other"), SharePrefs { send_holes: false, send_sigs: false, ..Default::default() });
        a.share_prefs_save("other", SharePrefs::default());
        a.share_prefs_migrate(Some("old-target"));
        assert_eq!(p("other"), SharePrefs::default(), "a choice once made is kept");
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
