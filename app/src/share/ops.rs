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
    Member,
    Admin,
    Owner,
}

impl Role {
    pub fn can_manage(self) -> bool {
        self >= Role::Admin
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Member => "Member",
            Role::Admin => "Admin",
            Role::Owner => "Owner",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub char_id: i64,
    pub name: String,
    pub keys: PublicKeys,
    pub role: Role,
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Op {
    /// The first entry: who owns the group.
    Genesis { name: String, owner: Member },
    MemberAdded { member: Member },
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
        matches!(self, Op::Genesis { .. } | Op::MemberAdded { .. } | Op::MemberRemoved { .. } | Op::RoleSet { .. })
    }
}

/// What the server stores and hands back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub op_id: String,
    pub group: String,
    pub epoch: u32,
    pub author: i64,
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
fn signed_bytes(group: &str, epoch: u32, op_id: &str, author: i64, sealed: &[u8]) -> Vec<u8> {
    let mut m = context(group, epoch, op_id);
    m.extend_from_slice(format!("|{author}|").as_bytes());
    m.extend_from_slice(sealed);
    m
}

pub fn new_op_id() -> String {
    crypto::b64(&crypto::random32()[..16])
}

pub fn seal_op(op: &Op, group: &str, epoch: u32, key: &Key, author: i64, device: &DeviceKeys) -> Envelope {
    let op_id = new_op_id();
    let plain = serde_json::to_vec(op).expect("ops serialize");
    let sealed = crypto::seal(key, &context(group, epoch, &op_id), &plain);
    let sig = device.sign(&signed_bytes(group, epoch, &op_id, author, &sealed));
    Envelope { op_id, group: group.to_owned(), epoch, author, sealed, sig }
}

/// Checks the author's signature against `author_keys`, then opens the payload.
pub fn open_op(env: &Envelope, key: &Key, author_keys: &PublicKeys) -> Result<Op> {
    let msg = signed_bytes(&env.group, env.epoch, &env.op_id, env.author, &env.sealed);
    if !crypto::verify(&author_keys.sign, &msg, &env.sig) {
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
                if !manager {
                    bail!("only an admin adds members");
                }
                if member.role == Role::Owner || (member.role == Role::Admin && self.role(author) != Some(Role::Owner)) {
                    bail!("role above what the author may give");
                }
                self.members.insert(member.char_id, member.clone());
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
        Member { char_id: id, name: format!("Pilot {id}"), keys: keys.public(), role }
    }

    #[test]
    fn an_op_opens_for_members_and_rejects_forgery() {
        let (alice, mallory) = (DeviceKeys::generate(), DeviceKeys::generate());
        let key = crypto::random32();
        let op = Op::HoleDead { uid: "u1".into(), at: 5 };
        let env = seal_op(&op, "g1", 0, &key, 1, &alice);
        assert_eq!(open_op(&env, &key, &alice.public()).unwrap(), op);
        assert!(open_op(&env, &key, &mallory.public()).is_err(), "claimed by someone else's key");
        let mut moved = env.clone();
        moved.group = "g2".into();
        assert!(open_op(&moved, &key, &alice.public()).is_err(), "replayed into another group");
        let mut reauthored = env.clone();
        reauthored.author = 2;
        assert!(open_op(&reauthored, &key, &alice.public()).is_err());
    }

    #[test]
    fn the_roster_only_takes_entries_their_author_may_make() {
        let (o, a, m) = (DeviceKeys::generate(), DeviceKeys::generate(), DeviceKeys::generate());
        let mut r = Roster::default();
        r.apply(1, &Op::Genesis { name: "Chain".into(), owner: member(1, &o, Role::Owner) }).unwrap();
        r.apply(1, &Op::MemberAdded { member: member(2, &a, Role::Admin) }).unwrap();
        r.apply(2, &Op::MemberAdded { member: member(3, &m, Role::Member) }).unwrap();
        assert!(r.apply(3, &Op::MemberAdded { member: member(4, &m, Role::Member) }).is_err(), "a member invites");
        assert!(r.apply(2, &Op::MemberAdded { member: member(5, &m, Role::Admin) }).is_err(), "an admin makes admins");
        assert!(r.apply(2, &Op::MemberRemoved { char_id: 1 }).is_err(), "the owner removed");
        assert!(r.apply(3, &Op::RoleSet { char_id: 3, role: Role::Admin }).is_err(), "self-promotion");
        r.apply(3, &Op::MemberRemoved { char_id: 3 }).unwrap();
        assert_eq!(r.role(3), None, "anyone may leave");
        assert!(r.apply(1, &Op::Genesis { name: "x".into(), owner: member(1, &o, Role::Owner) }).is_err());
    }
}
