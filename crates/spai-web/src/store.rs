//! What the web app keeps for sharing: the engine's storage, held in memory and saved to the
//! browser as one document after each change. Holes are kept as the group log carries them, each
//! field with its clock, so a reload rebuilds exactly what was merged.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use spai_core::wormholes::{SystemSig, Wormhole};
use spai_share::crypto::Key;
use spai_share::hole;
use spai_share::ops::{HoleState, Member, SigRow};
use spai_share::store::{Outgoing, ShareGroup, ShareStore, Snapshot};

/// A hole and the group it came from; `None` when made here.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Held {
    state: HoleState,
    group: Option<String>,
    dead: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Data {
    groups: Vec<ShareGroup>,
    keys: Vec<(String, u32, Key)>,
    members: HashMap<String, Vec<Member>>,
    applied: HashSet<String>,
    invites: HashMap<String, (String, Key, i64, String)>,
    holes: HashMap<String, Held>,
    sigs: HashMap<i64, Vec<SystemSig>>,
    outbox: Vec<(i64, Option<String>, Outgoing)>,
    next_out: i64,
}

#[derive(Default)]
pub struct WebStore {
    data: RefCell<Data>,
    /// Something changed since the last save.
    pub dirty: Cell<bool>,
    /// Bumped whenever holes or signatures changed, so the map redraws them.
    pub generation: Cell<u64>,
}

impl WebStore {
    pub fn from_json(text: &str) -> Self {
        WebStore { data: RefCell::new(serde_json::from_str(text).unwrap_or_default()), ..Default::default() }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&*self.data.borrow()).expect("plain data serializes")
    }

    fn touch(&self, data_changed: bool) {
        self.dirty.set(true);
        if data_changed {
            self.generation.set(self.generation.get() + 1);
        }
    }

    fn queue(&self, group: Option<String>, out: Outgoing) {
        let mut d = self.data.borrow_mut();
        d.next_out += 1;
        let id = d.next_out;
        d.outbox.push((id, group, out));
    }

    /// Stable within this browser: the map keys its lines and boxes by it.
    fn id_of(uid: &str) -> i64 {
        uid.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211)) as i64 & i64::MAX
    }

    pub fn share_groups_any(&self) -> bool {
        !self.data.borrow().groups.is_empty()
    }

    /// The live holes, for the map.
    pub fn wormholes(&self) -> Vec<Wormhole> {
        self.data
            .borrow()
            .holes
            .values()
            .filter(|h| !h.dead)
            .map(|h| Wormhole { id: Self::id_of(&h.state.uid), ..hole::fresh(&h.state) })
            .collect()
    }

    pub fn system_sigs(&self, system: i64) -> Vec<SystemSig> {
        self.data.borrow().sigs.get(&system).cloned().unwrap_or_default()
    }

    /// A change made here to `w`: the fields that differ from what is held take this moment's
    /// clock, and the hole goes out to the groups.
    pub fn edit_hole(&self, w: &Wormhole, now: i64) {
        let before = self.data.borrow().holes.get(&w.uid).map(|h| hole::fresh(&h.state));
        let fields = hole::changed(before.as_ref(), w);
        if fields.is_empty() {
            return;
        }
        {
            let mut d = self.data.borrow_mut();
            let held = d.holes.entry(w.uid.clone()).or_insert_with(|| Held {
                state: HoleState { uid: w.uid.clone(), system_id: w.system_id, source: w.source.code().to_owned(), reported_at: w.reported_at, fields: HashMap::new() },
                group: None,
                dead: false,
            });
            for f in fields {
                let at = held.state.fields.get(f).map_or(now, |c| now.max(c.at + 1));
                held.state.fields.insert(f.to_owned(), spai_share::ops::Field { v: hole::get(w, f), at, by: 0 });
            }
        }
        let group = self.wormhole_group(&w.uid);
        self.queue(group, Outgoing::Hole(w.uid.clone()));
        self.touch(true);
    }
}

impl ShareStore for WebStore {
    fn share_groups(&self) -> Vec<ShareGroup> {
        self.data.borrow().groups.clone()
    }

    fn share_group_save(&self, g: &ShareGroup) {
        let mut d = self.data.borrow_mut();
        match d.groups.iter_mut().find(|x| x.id == g.id) {
            Some(x) => *x = g.clone(),
            None => d.groups.push(g.clone()),
        }
        drop(d);
        self.touch(false);
    }

    fn share_group_forget(&self, id: &str) {
        let mut d = self.data.borrow_mut();
        d.groups.retain(|g| g.id != id);
        d.keys.retain(|(g, _, _)| g != id);
        d.members.remove(id);
        d.holes.retain(|_, h| h.group.as_deref() != Some(id));
        drop(d);
        self.touch(true);
    }

    fn share_key(&self, group: &str, epoch: u32) -> Option<Key> {
        self.data.borrow().keys.iter().find(|(g, e, _)| g == group && *e == epoch).map(|(_, _, k)| *k)
    }

    fn share_key_save(&self, group: &str, epoch: u32, key: &Key) {
        if self.share_key(group, epoch).is_none() {
            self.data.borrow_mut().keys.push((group.to_owned(), epoch, *key));
            self.touch(false);
        }
    }

    fn share_members(&self, group: &str) -> Vec<Member> {
        self.data.borrow().members.get(group).cloned().unwrap_or_default()
    }

    fn share_members_save(&self, group: &str, members: &[Member]) {
        self.data.borrow_mut().members.insert(group.to_owned(), members.to_vec());
        self.touch(false);
    }

    fn share_cursor_save(&self, group: &str, cursor: i64) {
        if let Some(g) = self.data.borrow_mut().groups.iter_mut().find(|g| g.id == group) {
            g.cursor = cursor;
        }
        self.touch(false);
    }

    fn share_mark_applied(&self, op_id: &str, _group: &str) -> bool {
        let fresh = self.data.borrow_mut().applied.insert(op_id.to_owned());
        self.touch(false);
        fresh
    }

    fn share_unmark_applied(&self, op_id: &str) -> bool {
        self.data.borrow_mut().applied.remove(op_id)
    }

    fn share_invite_save(&self, id: &str, group: &str, secret: &Key, for_char: i64, for_name: &str) {
        self.data.borrow_mut().invites.insert(id.to_owned(), (group.to_owned(), *secret, for_char, for_name.to_owned()));
        self.touch(false);
    }

    fn share_invite(&self, id: &str) -> Option<(Key, i64, String)> {
        self.data.borrow().invites.get(id).map(|(_, k, c, n)| (*k, *c, n.clone()))
    }

    fn share_queue_group(&self, group: &str, holes: bool, sigs: bool) {
        let (uids, systems): (Vec<String>, Vec<i64>) = {
            let d = self.data.borrow();
            let uids = if holes { d.holes.iter().filter(|(_, h)| h.group.is_none() && !h.dead).map(|(u, _)| u.clone()).collect() } else { Vec::new() };
            let systems = if sigs { d.sigs.iter().filter(|(_, v)| v.iter().any(|s| s.origin.is_none())).map(|(s, _)| *s).collect() } else { Vec::new() };
            (uids, systems)
        };
        for uid in uids {
            self.queue(Some(group.to_owned()), Outgoing::Hole(uid));
        }
        for system_id in systems {
            let rows: Vec<SigRow> = self.system_sigs(system_id).iter().filter(|s| s.origin.is_none()).map(sig_row).collect();
            let at = rows.iter().map(|r| r.added_at).max().unwrap_or(0);
            self.queue(Some(group.to_owned()), Outgoing::Sigs { system_id, rows, drop_missing: false, at });
        }
        self.touch(false);
    }

    fn share_snapshot(&self, g: &ShareGroup) -> Snapshot {
        let d = self.data.borrow();
        let ours = |h: &&Held| h.group.as_deref().is_none_or(|x| x == g.id);
        let (mut holes, mut dead, mut sigs) = (Vec::new(), Vec::new(), Vec::new());
        if g.prefs.send_holes {
            holes = d.holes.values().filter(ours).filter(|h| !h.dead).map(|h| settled(&h.state, g.char_id)).collect();
            dead = d.holes.values().filter(ours).filter(|h| h.dead).map(|h| h.state.uid.clone()).collect();
        }
        if g.prefs.send_sigs {
            sigs = d
                .sigs
                .iter()
                .map(|(sys, v)| (*sys, v.iter().filter(|s| s.origin.as_deref().is_none_or(|o| o == g.id)).map(sig_row).collect::<Vec<_>>()))
                .filter(|(_, rows)| !rows.is_empty())
                .collect();
        }
        (holes, dead, sigs)
    }

    fn share_apply_hole(&self, remote: &HoleState, group: &str, _who: &str) -> bool {
        if !hole::system_id_ok(remote.system_id) || remote.uid.is_empty() || remote.uid.len() > 64 {
            return false;
        }
        let changed = {
            let mut d = self.data.borrow_mut();
            match d.holes.get_mut(&remote.uid) {
                Some(held) => {
                    let mut w = hole::fresh(&held.state);
                    let mut clocks: HashMap<String, hole::Clock> = held.state.fields.iter().map(|(k, f)| (k.clone(), (f.at, f.by))).collect();
                    let taken = hole::merge(&mut w, &mut clocks, remote);
                    for (name, (at, by)) in &taken {
                        held.state.fields.insert(name.clone(), spai_share::ops::Field { v: hole::get(&w, name), at: *at, by: *by });
                    }
                    held.state.reported_at = w.reported_at;
                    !taken.is_empty()
                }
                None => {
                    // Only what `fresh` believes is kept, as the desktop does.
                    let w = hole::fresh(remote);
                    let mut state = remote.clone();
                    state.fields.retain(|name, f| hole::get(&w, name) == f.v);
                    d.holes.insert(remote.uid.clone(), Held { state, group: Some(group.to_owned()), dead: false });
                    true
                }
            }
        };
        if changed {
            self.touch(true);
        }
        changed
    }

    fn share_apply_dead(&self, uid: &str) {
        if let Some(h) = self.data.borrow_mut().holes.get_mut(uid) {
            h.dead = true;
        }
        self.touch(true);
    }

    fn share_apply_sigs(&self, system_id: i64, rows: &[SigRow], drop_missing: bool, at: i64, who: &str, group: &str) {
        if !hole::system_id_ok(system_id) || rows.len() > 2000 {
            return;
        }
        let fits = |r: &&SigRow| r.sig.chars().count() <= 16 && [&r.kind, &r.group, &r.name].iter().all(|s| s.chars().count() <= 100);
        {
            let mut d = self.data.borrow_mut();
            let list = d.sigs.entry(system_id).or_default();
            for r in rows.iter().filter(fits) {
                let found = if r.added_at > at - 30 * 86_400 && r.added_at > 0 { r.added_at.min(at) } else { at };
                match list.iter_mut().find(|s| s.sig == r.sig) {
                    Some(s) => {
                        s.kind = r.kind.clone();
                        if !r.group.is_empty() {
                            s.group = r.group.clone();
                        }
                        if !r.name.is_empty() {
                            s.name = r.name.clone();
                        }
                        s.updated_at = at;
                        s.added_at = s.added_at.min(found);
                        s.who = who.to_owned();
                        s.origin = Some(group.to_owned());
                    }
                    None => list.push(SystemSig {
                        sig: r.sig.clone(),
                        kind: r.kind.clone(),
                        group: r.group.clone(),
                        name: r.name.clone(),
                        added_at: found,
                        updated_at: at,
                        who: who.to_owned(),
                        origin: Some(group.to_owned()),
                    }),
                }
            }
            if drop_missing {
                // Only kinds the paste covers: a copy without anomalies says nothing about them.
                let kinds: HashSet<String> = rows.iter().map(|r| r.kind.to_lowercase()).collect();
                let listed: HashSet<&str> = rows.iter().map(|r| r.sig.as_str()).collect();
                list.retain(|s| !kinds.contains(&s.kind.to_lowercase()) || listed.contains(s.sig.as_str()));
            }
        }
        self.touch(true);
    }

    fn share_apply_sig_delete(&self, system_id: i64, sig: &str) {
        if let Some(list) = self.data.borrow_mut().sigs.get_mut(&system_id) {
            list.retain(|s| s.sig != sig);
        }
        self.touch(true);
    }

    fn share_outbox(&self, limit: usize) -> Vec<(i64, Option<String>, Outgoing)> {
        self.data.borrow().outbox.iter().take(limit).cloned().collect()
    }

    fn share_outbox_done(&self, id: i64) {
        self.data.borrow_mut().outbox.retain(|(i, _, _)| *i != id);
        self.touch(false);
    }

    fn share_hole_state(&self, uid: &str, me: i64) -> Option<HoleState> {
        self.data.borrow().holes.get(uid).map(|h| settled(&h.state, me))
    }

    fn share_settle(&self, uid: &str, me: i64) {
        if let Some(h) = self.data.borrow_mut().holes.get_mut(uid) {
            for f in h.state.fields.values_mut().filter(|f| f.by == 0) {
                f.by = me;
            }
        }
        self.touch(false);
    }

    fn wormhole_group(&self, uid: &str) -> Option<String> {
        self.data.borrow().holes.get(uid).and_then(|h| h.group.clone())
    }
}

/// `s` with this install's unsent changes signed by `me`, as it goes out.
fn settled(s: &HoleState, me: i64) -> HoleState {
    let mut s = s.clone();
    for f in s.fields.values_mut().filter(|f| f.by == 0) {
        f.by = me;
    }
    s
}

fn sig_row(s: &SystemSig) -> SigRow {
    SigRow { sig: s.sig.clone(), kind: s.kind.clone(), group: s.group.clone(), name: s.name.clone(), added_at: s.added_at }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spai_share::ops::Field;

    fn remote(uid: &str, sig: &str, at: i64, by: i64) -> HoleState {
        HoleState {
            uid: uid.into(),
            system_id: 31_000_200,
            source: "manual".into(),
            reported_at: 100,
            fields: HashMap::from([
                ("signature".to_owned(), Field { v: serde_json::json!(sig), at, by }),
                ("dest_system_id".to_owned(), Field { v: serde_json::json!(30_000_142), at, by }),
            ]),
        }
    }

    #[test]
    fn holes_merge_by_field_and_survive_a_reload() {
        let s = WebStore::default();
        assert!(s.share_apply_hole(&remote("u1", "ABC-123", 1_000, 7), "g", "Pilot"));
        assert!(!s.share_apply_hole(&remote("u1", "OLD-000", 900, 7), "g", "Pilot"), "an older change loses");
        assert!(s.share_apply_hole(&remote("u1", "NEW-999", 1_100, 8), "g", "Pilot"));
        let back = WebStore::from_json(&s.to_json());
        let w = &back.wormholes()[0];
        assert_eq!((w.signature.as_deref(), w.dest_system_id), (Some("NEW-999"), Some(30_000_142)));
        assert_eq!(back.wormhole_group("u1").as_deref(), Some("g"));
        back.share_apply_dead("u1");
        assert!(back.wormholes().is_empty());
    }

    #[test]
    fn a_local_edit_goes_out_signed_by_me() {
        let s = WebStore::default();
        s.share_apply_hole(&remote("u1", "ABC-123", 1_000, 7), "g", "Pilot");
        let mut w = s.wormholes()[0].clone();
        w.signature = Some("ABC-124".into());
        s.edit_hole(&w, 2_000);
        let out = s.share_outbox(10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1.as_deref(), Some("g"), "back to the group it came from");
        let st = s.share_hole_state("u1", 42).unwrap();
        assert_eq!(st.fields["signature"].v, serde_json::json!("ABC-124"));
        assert_eq!((st.fields["signature"].at, st.fields["signature"].by), (2_000, 42));
        s.share_outbox_done(out[0].0);
        assert!(s.share_outbox(10).is_empty());
    }

    #[test]
    fn signatures_keep_the_first_sighting_and_drop_what_a_full_paste_lacks() {
        let s = WebStore::default();
        let row = |sig: &str, kind: &str, added_at| SigRow { sig: sig.into(), kind: kind.into(), group: String::new(), name: String::new(), added_at };
        s.share_apply_sigs(31_000_200, &[row("ABC-123", "Cosmic Signature", 1_000), row("ANO-001", "Cosmic Anomaly", 1_000)], false, 1_000, "P", "g");
        s.share_apply_sigs(31_000_200, &[row("ABC-123", "Cosmic Signature", 900)], true, 5_000, "P", "g");
        let sigs = s.system_sigs(31_000_200);
        let abc = sigs.iter().find(|x| x.sig == "ABC-123").unwrap();
        assert_eq!((abc.added_at, abc.updated_at), (900, 5_000));
        assert!(sigs.iter().any(|x| x.sig == "ANO-001"), "a paste of signatures says nothing of anomalies");
        s.share_apply_sigs(31_000_200, &[row("XYZ-000", "Cosmic Signature", 5_000)], true, 6_000, "P", "g");
        assert!(!s.system_sigs(31_000_200).iter().any(|x| x.sig == "ABC-123"));
    }
}
