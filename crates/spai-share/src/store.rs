//! What sharing keeps locally, as the engine sees it. The desktop keeps it in SQLite, the web app
//! in the browser; each implements [`ShareStore`].

use crate::crypto::Key;
use crate::ops::{HoleState, Member, Role, SigRow};

/// Live holes, recently collapsed ones, and the signatures of each system.
pub type Snapshot = (Vec<HoleState>, Vec<String>, Vec<(i64, Vec<SigRow>)>);

/// What this install sends a group and takes from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharePrefs {
    pub send_holes: bool,
    pub send_sigs: bool,
    pub recv_holes: bool,
    pub recv_sigs: bool,
    /// What the group sent stays stored and keeps syncing, but is left off the map and lists.
    pub hidden: bool,
}

impl Default for SharePrefs {
    fn default() -> Self {
        SharePrefs { send_holes: true, send_sigs: true, recv_holes: true, recv_sigs: true, hidden: false }
    }
}

impl SharePrefs {
    fn sends(&self, kind: &str) -> bool {
        if matches!(kind, "hole" | "dead") { self.send_holes } else { self.send_sigs }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShareGroup {
    pub id: String,
    pub name: String,
    /// Which of this install's characters is the member.
    pub char_id: i64,
    pub role: Role,
    pub epoch: u32,
    pub cursor: i64,
    pub prefs: SharePrefs,
}

impl ShareGroup {
    /// Whether a change of `kind` goes to this group. Something that came from one group goes
    /// back to that group only, never on to the others.
    pub fn takes(&self, kind: &str, origin: Option<&str>) -> bool {
        self.role.can_write() && self.prefs.sends(kind) && origin.is_none_or(|o| o == self.id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outgoing {
    Hole(String),
    Dead(String),
    Sigs { system_id: i64, rows: Vec<SigRow>, drop_missing: bool, at: i64 },
    SigDelete { system_id: i64, sig: String },
}

/// The engine's view of local storage. Writes cannot fail from the engine's side: a store that
/// loses one catches up on the next sync.
pub trait ShareStore {
    fn share_groups(&self) -> Vec<ShareGroup>;
    fn share_group_save(&self, g: &ShareGroup);
    fn share_group_forget(&self, id: &str);
    fn share_key(&self, group: &str, epoch: u32) -> Option<Key>;
    fn share_key_save(&self, group: &str, epoch: u32, key: &Key);
    fn share_members(&self, group: &str) -> Vec<Member>;
    fn share_members_save(&self, group: &str, members: &[Member]);
    fn share_cursor_save(&self, group: &str, cursor: i64);
    /// False when `op_id` was applied already.
    fn share_mark_applied(&self, op_id: &str, group: &str) -> bool;
    fn share_unmark_applied(&self, op_id: &str) -> bool;
    fn share_invite_save(&self, id: &str, group: &str, secret: &Key, for_char: i64, for_name: &str);
    /// An invite made here: its secret, and the character it is for.
    fn share_invite(&self, id: &str) -> Option<(Key, i64, String)>;
    /// Queues everything held for a group that has just been joined or made.
    fn share_queue_group(&self, group: &str, holes: bool, sigs: bool);
    fn share_snapshot(&self, g: &ShareGroup) -> Snapshot;
    /// Returns whether the hole changed here.
    fn share_apply_hole(&self, remote: &HoleState, group: &str, who: &str) -> bool;
    fn share_apply_dead(&self, uid: &str);
    fn share_apply_sigs(&self, system_id: i64, rows: &[SigRow], drop_missing: bool, at: i64, who: &str, group: &str);
    fn share_apply_sig_delete(&self, system_id: i64, sig: &str);
    /// Local changes waiting to go out: id, the group they are for if any, and the change.
    fn share_outbox(&self, limit: usize) -> Vec<(i64, Option<String>, Outgoing)>;
    fn share_outbox_done(&self, id: i64);
    fn share_hole_state(&self, uid: &str, me: i64) -> Option<HoleState>;
    fn share_settle(&self, uid: &str, me: i64);
    /// The group a hole came from, if it came from one.
    fn wormhole_group(&self, uid: &str) -> Option<String>;
}
