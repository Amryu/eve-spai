//! What a group's log holds. Every entry is an [`Envelope`]: the payload sealed under the group's
//! key for its epoch, signed by the author's device. The server sees ids, the epoch and whether
//! the entry is kept past the data retention (membership is, data is not), nothing else.

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::crypto::{self, DeviceKeys, Key, PublicKeys};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Sees the group's data, shares none.
    Viewer,
    Member,
    Admin,
    Owner,
}

impl Role {
    pub fn can_manage(self) -> bool {
        self >= Role::Admin
    }

    pub fn can_write(self) -> bool {
        self >= Role::Member
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Viewer => "Viewer",
            Role::Member => "Member",
            Role::Admin => "Admin",
            Role::Owner => "Owner",
        }
    }

    pub fn from_code(code: &str) -> Option<Role> {
        [Role::Viewer, Role::Member, Role::Admin, Role::Owner].into_iter().find(|r| r.code() == code)
    }

    pub fn code(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Member => "member",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }
}

/// Most devices one character may hold in a group.
pub const MAX_DEVICES: usize = 16;

/// One install or browser of a member: its keys sign what it writes and receive the group key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub keys: PublicKeys,
    #[serde(default)]
    pub label: String,
}

impl Device {
    pub fn of(keys: PublicKeys, label: &str) -> Self {
        Device { id: keys.device_id(), keys, label: label.to_owned() }
    }
}

/// A character in a group. The role is the character's, whichever of its devices acts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "MemberWire")]
pub struct Member {
    pub char_id: i64,
    pub name: String,
    pub role: Role,
    pub devices: Vec<Device>,
}

/// A member as either version wrote it: before devices, one set of keys.
#[derive(Deserialize)]
struct MemberWire {
    char_id: i64,
    name: String,
    role: Role,
    #[serde(default)]
    devices: Vec<Device>,
    #[serde(default)]
    keys: Option<PublicKeys>,
}

impl From<MemberWire> for Member {
    fn from(w: MemberWire) -> Self {
        let devices = match (w.devices.is_empty(), w.keys) {
            (true, Some(k)) => vec![Device::of(k, "")],
            _ => w.devices,
        };
        Member { char_id: w.char_id, name: w.name, role: w.role, devices }
    }
}

impl Member {
    pub fn new(char_id: i64, name: &str, role: Role, device: Device) -> Self {
        Member { char_id, name: name.to_owned(), role, devices: vec![device] }
    }

    pub fn device(&self, id: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }
}

/// One field of a hole and when it was last set, for last-writer-wins per field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub v: serde_json::Value,
    pub at: i64,
    pub by: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoleState {
    pub uid: String,
    pub system_id: i64,
    /// Where it was first reported, which never changes.
    pub source: String,
    pub reported_at: i64,
    pub fields: HashMap<String, Field>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SigRow {
    pub sig: String,
    pub kind: String,
    pub group: String,
    pub name: String,
    pub added_at: i64,
    /// The paste before the one it first showed in; left out by clients that do not track it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_after: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Op {
    /// The first entry: who owns the group.
    Genesis { name: String, owner: Member },
    MemberAdded { member: Member },
    /// Another device of a member; the member's role covers it.
    DeviceAdded { char_id: i64, device: Device },
    DeviceRemoved { char_id: i64, device_id: String },
    MemberRemoved { char_id: i64 },
    RoleSet { char_id: i64, role: Role },
    Hole { hole: HoleState },
    HoleDead { uid: String, at: i64 },
    Sigs { system_id: i64, rows: Vec<SigRow>, drop_missing: bool, at: i64 },
    SigDelete { system_id: i64, sig: String },
    /// What the approving admin knew, for someone joining after older entries expired.
    Snapshot { holes: Vec<HoleState>, dead: Vec<String>, sigs: Vec<(i64, Vec<SigRow>)>, members: Vec<Member> },
}

impl Op {
    /// Membership is kept for as long as the group lives; data expires with the holes.
    pub fn keep(&self) -> bool {
        matches!(
            self,
            Op::Genesis { .. }
                | Op::MemberAdded { .. }
                | Op::DeviceAdded { .. }
                | Op::DeviceRemoved { .. }
                | Op::MemberRemoved { .. }
                | Op::RoleSet { .. }
        )
    }

    /// Group data, which only a member who may write can make.
    pub fn is_data(&self) -> bool {
        matches!(self, Op::Hole { .. } | Op::HoleDead { .. } | Op::Sigs { .. } | Op::SigDelete { .. })
    }
}

/// What the server stores and hands back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub op_id: String,
    pub group: String,
    pub epoch: u32,
    pub author: i64,
    /// Which of the author's devices signed. Absent in entries from before devices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(with = "b64v")]
    pub sealed: Vec<u8>,
    #[serde(with = "b64v")]
    pub sig: Vec<u8>,
}

mod b64v {
    pub fn serialize<S: serde::Serializer>(k: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::crypto::b64(k))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = <String as serde::Deserialize>::deserialize(d)?;
        super::crypto::unb64(&s).map_err(serde::de::Error::custom)
    }
}

fn context(group: &str, epoch: u32, op_id: &str) -> Vec<u8> {
    format!("eve-spai wh op|{group}|{epoch}|{op_id}").into_bytes()
}

/// The bytes a signature covers: everything the envelope says, so none of it can be moved.
fn signed_bytes(group: &str, epoch: u32, op_id: &str, author: i64, device: Option<&str>, sealed: &[u8]) -> Vec<u8> {
    let mut m = context(group, epoch, op_id);
    match device {
        Some(d) => m.extend_from_slice(format!("|{author}|{d}|").as_bytes()),
        None => m.extend_from_slice(format!("|{author}|").as_bytes()),
    }
    m.extend_from_slice(sealed);
    m
}

/// The payload format. A payload without `v` is version 1.
pub const VERSION: u32 = 1;

pub fn new_op_id() -> String {
    crypto::b64(&crypto::random32()[..16])
}

pub fn seal_op(op: &Op, group: &str, epoch: u32, key: &Key, author: i64, device: &DeviceKeys) -> Envelope {
    let op_id = new_op_id();
    // The format version rides inside the sealed payload, where nobody can change it. Readers
    // ignore fields they do not know, so older versions open this the same.
    let mut value = serde_json::to_value(op).expect("ops serialize");
    value["v"] = serde_json::Value::from(VERSION);
    let plain = serde_json::to_vec(&value).expect("ops serialize");
    let sealed = crypto::seal(key, &context(group, epoch, &op_id), &plain);
    let id = device.public().device_id();
    let sig = device.sign(&signed_bytes(group, epoch, &op_id, author, Some(&id), &sealed));
    Envelope { op_id, group: group.to_owned(), epoch, author, device: Some(id), sealed, sig }
}

/// Checks the signature against the author's device that made it, then opens the payload. An entry
/// from before devices names none, and any of the author's devices may have signed it.
pub fn open_op(env: &Envelope, key: &Key, author: &Member) -> Result<Op> {
    let signed_by = |d: &Device| {
        let msg = signed_bytes(&env.group, env.epoch, &env.op_id, env.author, env.device.as_deref(), &env.sealed);
        crypto::verify(&d.keys.sign, &msg, &env.sig)
    };
    let ok = match &env.device {
        Some(id) => author.device(id).is_some_and(|d| d.keys.device_id() == *id && signed_by(d)),
        None => author.devices.iter().any(signed_by),
    };
    if !ok {
        bail!("signature does not match the author");
    }
    let plain = crypto::open(key, &context(&env.group, env.epoch, &env.op_id), &env.sealed)?;
    Ok(serde_json::from_slice(&plain)?)
}

/// Who is in a group, built only from signed membership entries. An entry counts only when its
/// author could make it at the time: the owner creates, admins add and remove, the owner sets roles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Roster {
    pub name: String,
    pub members: HashMap<i64, Member>,
}

impl Roster {
    pub fn role(&self, char_id: i64) -> Option<Role> {
        self.members.get(&char_id).map(|m| m.role)
    }

    /// Whether `author` may share data now.
    pub fn can_write(&self, author: i64) -> bool {
        self.role(author).is_some_and(Role::can_write)
    }

    /// Applies a membership entry by `author`. Data entries are none of its business.
    pub fn apply(&mut self, author: i64, op: &Op) -> Result<()> {
        let manager = self.role(author).is_some_and(Role::can_manage);
        match op {
            Op::Genesis { name, owner } => {
                if !self.members.is_empty() {
                    bail!("a second genesis");
                }
                if owner.char_id != author || owner.role != Role::Owner {
                    bail!("genesis not by its owner");
                }
                self.name = name.clone();
                self.members.insert(owner.char_id, owner.clone());
            }
            Op::MemberAdded { member } => {
                // A member may let in viewers, through invites of their own.
                let member_adds_viewer = self.role(author) == Some(Role::Member) && member.role == Role::Viewer;
                if !manager && !member_adds_viewer {
                    bail!("only an admin adds members, and a member only viewers");
                }
                if member.role == Role::Owner || (member.role == Role::Admin && self.role(author) != Some(Role::Owner)) {
                    bail!("role above what the author may give");
                }
                if self.members.contains_key(&member.char_id) {
                    bail!("already a member; another device is added as a device");
                }
                if member.devices.is_empty() || member.devices.len() > MAX_DEVICES || member.devices.iter().any(|d| d.keys.device_id() != d.id) {
                    bail!("a member's devices do not add up");
                }
                self.members.insert(member.char_id, member.clone());
            }
            Op::DeviceAdded { char_id, device } => {
                if !manager && *char_id != author {
                    bail!("only an admin, or the member themselves, adds a device");
                }
                if device.keys.device_id() != device.id {
                    bail!("a device named for other keys");
                }
                let m = self.members.get_mut(char_id).ok_or_else(|| anyhow!("not a member"))?;
                if m.device(&device.id).is_none() {
                    if m.devices.len() >= MAX_DEVICES {
                        bail!("too many devices");
                    }
                    m.devices.push(device.clone());
                }
            }
            Op::DeviceRemoved { char_id, device_id } => {
                let own = *char_id == author;
                let target = self.role(*char_id).ok_or_else(|| anyhow!("not a member"))?;
                if !own && (!manager || (target >= Role::Admin && self.role(author) != Some(Role::Owner))) {
                    bail!("not allowed to remove that device");
                }
                let m = self.members.get_mut(char_id).expect("checked above");
                let last = m.devices.iter().all(|d| d.id == *device_id);
                if last && target == Role::Owner {
                    bail!("the owner keeps at least one device");
                }
                m.devices.retain(|d| d.id != *device_id);
                if last {
                    self.members.remove(char_id);
                }
            }
            Op::MemberRemoved { char_id } => {
                let leaving = *char_id == author;
                if !leaving && !manager {
                    bail!("only an admin removes members");
                }
                if self.role(*char_id) == Some(Role::Owner) {
                    bail!("the owner cannot be removed");
                }
                if !leaving && self.role(*char_id) == Some(Role::Admin) && self.role(author) != Some(Role::Owner) {
                    bail!("only the owner removes an admin");
                }
                self.members.remove(char_id);
            }
            Op::RoleSet { char_id, role } => {
                if self.role(author) != Some(Role::Owner) || *role == Role::Owner {
                    bail!("only the owner sets roles");
                }
                let m = self.members.get_mut(char_id).ok_or_else(|| anyhow!("not a member"))?;
                m.role = *role;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: i64, keys: &DeviceKeys, role: Role) -> Member {
        Member::new(id, &format!("Pilot {id}"), role, Device::of(keys.public(), ""))
    }

    #[test]
    fn an_op_opens_for_members_and_rejects_forgery() {
        let (alice, mallory) = (DeviceKeys::generate(), DeviceKeys::generate());
        let key = crypto::random32();
        let op = Op::HoleDead { uid: "u1".into(), at: 5 };
        let env = seal_op(&op, "g1", 0, &key, 1, &alice);
        let (a, m) = (member(1, &alice, Role::Member), member(1, &mallory, Role::Member));
        assert_eq!(open_op(&env, &key, &a).unwrap(), op);
        assert!(open_op(&env, &key, &m).is_err(), "claimed by someone else's key");
        let mut moved = env.clone();
        moved.group = "g2".into();
        assert!(open_op(&moved, &key, &a).is_err(), "replayed into another group");
        let mut reauthored = env.clone();
        reauthored.author = 2;
        assert!(open_op(&reauthored, &key, &a).is_err());
        // Another device of the same member signed it: the named device must be the one.
        let mut both = a.clone();
        both.devices.push(Device::of(mallory.public(), "second"));
        let mut renamed = env.clone();
        renamed.device = Some(mallory.public().device_id());
        assert!(open_op(&renamed, &key, &both).is_err(), "a signature passed off as another device's");
        let by_second = seal_op(&op, "g1", 0, &key, 1, &mallory);
        assert_eq!(open_op(&by_second, &key, &both).unwrap(), op, "any of the member's devices writes");
    }

    #[test]
    fn a_payload_opens_with_or_without_its_version() {
        let (alice, key) = (DeviceKeys::generate(), crypto::random32());
        let op = Op::SigDelete { system_id: 31_000_004, sig: "ABC-123".into() };
        let env = seal_op(&op, "g1", 0, &key, 1, &alice);
        let plain = crypto::open(&key, &context("g1", 0, &env.op_id), &env.sealed).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&plain).unwrap();
        assert_eq!(v["v"], 1);
        let a = member(1, &alice, Role::Member);
        assert_eq!(open_op(&env, &key, &a).unwrap(), op);
        // As released versions seal it, without `v` and without a device.
        let op_id = new_op_id();
        let sealed = crypto::seal(&key, &context("g1", 0, &op_id), &serde_json::to_vec(&op).unwrap());
        let sig = alice.sign(&signed_bytes("g1", 0, &op_id, 1, None, &sealed));
        let old = Envelope { op_id, group: "g1".into(), epoch: 0, author: 1, device: None, sealed, sig };
        assert_eq!(open_op(&old, &key, &a).unwrap(), op);
    }

    #[test]
    fn the_roster_only_takes_entries_their_author_may_make() {
        let (o, a, m) = (DeviceKeys::generate(), DeviceKeys::generate(), DeviceKeys::generate());
        let mut r = Roster::default();
        r.apply(1, &Op::Genesis { name: "Chain".into(), owner: member(1, &o, Role::Owner) }).unwrap();
        r.apply(1, &Op::MemberAdded { member: member(2, &a, Role::Admin) }).unwrap();
        r.apply(2, &Op::MemberAdded { member: member(3, &m, Role::Member) }).unwrap();
        assert!(r.apply(3, &Op::MemberAdded { member: member(4, &m, Role::Member) }).is_err(), "a member invites a member");
        r.apply(3, &Op::MemberAdded { member: member(6, &m, Role::Viewer) }).unwrap();
        assert!(r.apply(6, &Op::MemberAdded { member: member(7, &m, Role::Viewer) }).is_err(), "a viewer invites");
        assert!(r.apply(2, &Op::MemberAdded { member: member(5, &m, Role::Admin) }).is_err(), "an admin makes admins");
        assert!(r.apply(2, &Op::MemberRemoved { char_id: 1 }).is_err(), "the owner removed");
        assert!(r.apply(3, &Op::RoleSet { char_id: 3, role: Role::Admin }).is_err(), "self-promotion");
        r.apply(3, &Op::MemberRemoved { char_id: 3 }).unwrap();
        assert_eq!(r.role(3), None, "anyone may leave");
        assert!(r.apply(1, &Op::Genesis { name: "x".into(), owner: member(1, &o, Role::Owner) }).is_err());
    }

    /// A character's devices share its role; each is added by an admin or by the character itself,
    /// and a viewer is a member that shares nothing.
    #[test]
    fn devices_share_their_character_and_viewers_do_not_write() {
        let (o, v, v2, x) = (DeviceKeys::generate(), DeviceKeys::generate(), DeviceKeys::generate(), DeviceKeys::generate());
        let mut r = Roster::default();
        r.apply(1, &Op::Genesis { name: "Chain".into(), owner: member(1, &o, Role::Owner) }).unwrap();
        r.apply(1, &Op::MemberAdded { member: member(2, &v, Role::Viewer) }).unwrap();
        assert!(!r.can_write(2) && r.can_write(1));
        assert!(r.apply(1, &Op::MemberAdded { member: member(2, &v2, Role::Member) }).is_err(), "a second device is not a second member");
        r.apply(2, &Op::DeviceAdded { char_id: 2, device: Device::of(v2.public(), "browser") }).unwrap();
        assert_eq!(r.members[&2].devices.len(), 2);
        assert_eq!(r.role(2), Some(Role::Viewer), "the new device keeps the character's role");
        let forged = Device { id: v.public().device_id(), keys: x.public(), label: String::new() };
        assert!(r.apply(1, &Op::DeviceAdded { char_id: 2, device: forged }).is_err(), "an id that is not its keys'");
        assert!(r.apply(2, &Op::DeviceAdded { char_id: 1, device: Device::of(x.public(), "") }).is_err(), "adding to someone else");
        r.apply(1, &Op::RoleSet { char_id: 2, role: Role::Member }).unwrap();
        assert!(r.can_write(2));
        r.apply(2, &Op::DeviceRemoved { char_id: 2, device_id: v.public().device_id() }).unwrap();
        r.apply(1, &Op::DeviceRemoved { char_id: 2, device_id: v2.public().device_id() }).unwrap();
        assert_eq!(r.role(2), None, "no devices left, no member");
        assert!(r.apply(1, &Op::DeviceRemoved { char_id: 1, device_id: o.public().device_id() }).is_err(), "the owner's last device");
    }

    /// A member stored or logged before devices had one set of keys: it reads as one device.
    #[test]
    fn a_member_from_before_devices_reads_as_one_device() {
        let k = DeviceKeys::generate().public();
        let old = serde_json::json!({ "char_id": 7, "name": "Old Pilot", "keys": k, "role": "admin" });
        let m: Member = serde_json::from_value(old).unwrap();
        assert_eq!(m.devices, vec![Device::of(k, "")]);
        assert_eq!(m.role, Role::Admin);
        let back: Member = serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }
}
