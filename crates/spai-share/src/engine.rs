//! Keeps this install's groups in step with the server: fetches the keys it was given, applies new
//! log entries in order (checking each author's signature and right to make it), and sends local
//! changes. Group management (create, invite, join, approve, remove) runs here too. The platform
//! supplies storage ([`ShareStore`]), the way to the server ([`Env`]) and the loop that drives it.

use anyhow::{anyhow, bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::api::{Client, RequestRow, Transport};
use crate::crypto::{self, DeviceKeys, Key, PublicKeys, Wrapped};
use crate::ops::{self, Device, Envelope, Member, Op, Role, Roster};
use crate::store::{Outgoing, ShareGroup, SharePrefs, ShareStore};

const INVITE_TTL_SECS: i64 = 2 * 86_400;

pub enum Cmd {
    Create { name: String, char_id: i64, char_name: String, prefs: SharePrefs },
    /// An invite only `for_name` can use.
    Invite { group: String, for_name: String },
    Join { link: String, char_id: i64, prefs: SharePrefs },
    /// Lets a device in: a new character with `role`, or another device of a member, which keeps
    /// the member's role.
    Approve { group: String, char_id: i64, device_id: String, role: Role },
    Reject { group: String, char_id: i64, device_id: String },
    Remove { group: String, char_id: i64 },
    RemoveDevice { group: String, char_id: i64, device_id: String },
    Leave { group: String },
    SetRole { group: String, char_id: i64, role: Role },
    /// Reads the group's whole log again, for what was passed over while it was not taken.
    Rescan { group: String },
    SyncNow,
}

/// A join request and whether it proves it came through our invite with these keys.
#[derive(Clone, Debug)]
pub struct Request {
    pub row: RequestRow,
    pub keys: Option<PublicKeys>,
    /// It proves our invite link, from the character the invite was for.
    pub verified: bool,
    /// Who the invite was for, when someone else answered it.
    pub meant_for: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub requests: HashMap<String, Vec<Request>>,
    pub error: Option<String>,
    /// The last invite link made: group, link, and the character it is for.
    pub invite: Option<(String, String, String)>,
    /// This install's key fingerprint, for a joiner to read out to whoever approves them.
    pub fingerprint: Option<String>,
    pub busy: bool,
    pub synced_at: HashMap<String, i64>,
    /// Bumped whenever holes or signatures changed here, so the UI reloads them.
    pub generation: u64,
    /// Groups whose keys from before devices this device has claimed, this run.
    pub claimed: std::collections::HashSet<String>,
}

/// What the engine needs from the platform besides storage.
#[allow(async_fn_in_trait)]
pub trait Env {
    type T: Transport;
    /// The way to the server as `char_id`, signed in.
    fn transport(&self, char_id: i64) -> Result<Self::T>;
    /// A character by exact name, from ESI.
    async fn character(&self, name: &str) -> Result<Option<(i64, String)>>;
}

/// What an invite link opens: sealed under a key only the link carries.
#[derive(Serialize, Deserialize)]
pub struct InviteInfo {
    pub group_id: String,
    pub name: String,
    pub members: Vec<Member>,
    /// The only character that may use it.
    pub for_char: i64,
    pub for_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct JoinBody {
    pub keys: PublicKeys,
    /// Over the keys' JSON text. Every version checks this one, so joiners keep sending it.
    #[serde(default)]
    pub mac: String,
    /// Over the raw key bytes, which another client can reproduce without matching our JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac2: Option<String>,
}

pub fn join_msg2(char_id: i64, group: &str, keys: &PublicKeys) -> Vec<u8> {
    let mut m = format!("{char_id}|{group}|").into_bytes();
    m.extend_from_slice(&keys.sign);
    m.extend_from_slice(&keys.enc);
    m
}

/// Whether `body` proves the invite `secret` for `char_id`, by either MAC.
fn join_proven(secret: &Key, char_id: i64, group: &str, body: &JoinBody) -> bool {
    let ok = |mac: &str, msg: Vec<u8>| crypto::unb64(mac).is_ok_and(|m| crypto::invite_mac_ok(secret, &msg, &m));
    body.mac2.as_deref().is_some_and(|m| ok(m, join_msg2(char_id, group, &body.keys)))
        || (!body.mac.is_empty() && ok(&body.mac, join_msg(char_id, group, &body.keys)))
}

fn wrap_ctx(epoch: u32) -> Vec<u8> {
    format!("eve-spai group key|{epoch}").into_bytes()
}

pub fn join_msg(char_id: i64, group: &str, keys: &PublicKeys) -> Vec<u8> {
    format!("{char_id}|{group}|{}", serde_json::to_string(keys).unwrap_or_default()).into_bytes()
}

/// `eve-spai://join/<id>#<secret>`, or the same pasted with anything around it.
pub fn parse_link(link: &str) -> Option<(String, Key)> {
    let rest = link.trim().split("join/").nth(1)?;
    let (id, secret) = rest.split_once('#')?;
    let secret = secret.split_whitespace().next()?;
    Some((id.trim().to_owned(), crypto::unb64_32(secret).ok()?))
}

pub fn make_link(id: &str, secret: &Key) -> String {
    format!("eve-spai://join/{id}#{}", crypto::b64(secret))
}

pub struct Engine<'a, S: ShareStore, E: Env> {
    pub store: &'a S,
    pub env: &'a E,
    pub device: &'a DeviceKeys,
    /// What this install calls itself among a character's devices.
    pub label: &'a str,
    pub status: &'a Mutex<Status>,
}

impl<S: ShareStore, E: Env> Engine<'_, S, E> {
    pub fn client(&self, char_id: i64) -> Result<Client<E::T>> {
        Ok(Client::new(self.env.transport(char_id)?, &self.device.public().device_id()))
    }

    pub fn group(&self, id: &str) -> Result<ShareGroup> {
        self.store.share_groups().into_iter().find(|g| g.id == id).ok_or_else(|| anyhow!("not in that group"))
    }

    fn roster(&self, group: &str) -> Roster {
        let members = self.store.share_members(group);
        Roster { name: String::new(), members: members.into_iter().map(|m| (m.char_id, m)).collect() }
    }

    async fn post(&self, c: &Client<E::T>, g: &ShareGroup, op: &Op) -> Result<()> {
        let key = self.store.share_key(&g.id, g.epoch).ok_or_else(|| anyhow!("no key for the group yet"))?;
        let env = ops::seal_op(op, &g.id, g.epoch, &key, g.char_id, self.device);
        c.post_op(&g.id, &env.op_id, g.epoch, op.keep(), &serde_json::to_string(&env)?).await?;
        // Our own entry comes back from the server like anyone's; it is already applied here.
        self.store.share_mark_applied(&env.op_id, &g.id);
        Ok(())
    }

    pub async fn command(&self, cmd: Cmd) -> Result<()> {
        match cmd {
            Cmd::Create { name, char_id, char_name, prefs } => {
                let c = self.client(char_id)?;
                let key = crypto::random32();
                let wrapped = crypto::wrap_key(&self.device.public().enc, &key, &wrap_ctx(0));
                let id = c.create_group(&serde_json::to_string(&wrapped)?, self.label).await?;
                let g = ShareGroup { id: id.clone(), name: name.clone(), char_id, role: Role::Owner, epoch: 0, cursor: 0, prefs };
                self.store.share_group_save(&g);
                self.store.share_key_save(&id, 0, &key);
                let owner = Member::new(char_id, &char_name, Role::Owner, Device::of(self.device.public(), self.label));
                self.store.share_members_save(&id, std::slice::from_ref(&owner));
                self.post(&c, &g, &Op::Genesis { name, owner }).await?;
                self.store.share_queue_group(&id, true, true);
            }
            Cmd::Invite { group, for_name } => {
                let g = self.group(&group)?;
                let (for_char, for_name) = self.character(&for_name).await?;
                let c = self.client(g.char_id)?;
                let secret = crypto::random32();
                let info = InviteInfo {
                    group_id: g.id.clone(),
                    name: g.name.clone(),
                    members: self.store.share_members(&g.id),
                    for_char,
                    for_name: for_name.clone(),
                };
                let blob = crypto::b64(&crypto::seal(&crypto::invite_key(&secret), b"eve-spai invite", &serde_json::to_vec(&info)?));
                let id = c.create_invite(&g.id, &blob, INVITE_TTL_SECS).await?;
                self.store.share_invite_save(&id, &g.id, &secret, for_char, &for_name);
                self.status.lock().unwrap().invite = Some((g.id, make_link(&id, &secret), for_name));
            }
            Cmd::Join { link, char_id, prefs } => {
                let (id, secret) = parse_link(&link).ok_or_else(|| anyhow!("that is not an invite link"))?;
                let c = self.client(char_id)?;
                let (group_id, blob) = c.fetch_invite(&id).await.context("the invite is unknown, used or expired")?;
                let plain = crypto::open(&crypto::invite_key(&secret), b"eve-spai invite", &crypto::unb64(&blob)?)
                    .context("the link does not open its invite")?;
                let info: InviteInfo = serde_json::from_slice(&plain)?;
                if info.group_id != group_id {
                    bail!("the invite names another group than the server says");
                }
                if info.for_char != char_id {
                    bail!("this invite is for {}; join as that character", info.for_name);
                }
                let keys = self.device.public();
                let mac = crypto::b64(&crypto::invite_mac(&secret, &join_msg(char_id, &group_id, &keys)));
                let mac2 = Some(crypto::b64(&crypto::invite_mac(&secret, &join_msg2(char_id, &group_id, &keys))));
                c.join(&id, &serde_json::to_string(&JoinBody { keys, mac, mac2 })?, self.label).await?;
                // Until an admin approves there is no key; the members the invite named are who
                // this install trusts to sign the group's entries.
                self.store.share_group_save(&ShareGroup { id: group_id.clone(), name: info.name, char_id, role: Role::Member, epoch: 0, cursor: 0, prefs });
                self.store.share_members_save(&group_id, &info.members);
            }
            Cmd::Approve { group, char_id, device_id, role } => {
                let g = self.group(&group)?;
                let c = self.client(g.char_id)?;
                let req = self
                    .fetch_requests(&c, &g).await?
                    .into_iter()
                    .find(|r| r.row.char_id == char_id && r.row.device_id == device_id)
                    .ok_or_else(|| anyhow!("no such request"))?;
                let keys = match (req.verified, req.keys) {
                    (true, Some(k)) => k,
                    _ if req.meant_for.is_some() => bail!("this invite was for {}, not {}", req.meant_for.unwrap_or_default(), req.row.name),
                    _ => bail!("this request does not prove it came through our invite; not approving it"),
                };
                // Every epoch held, so the log's older membership entries open for them too.
                let wrapped: Vec<(u32, String)> = (0..=g.epoch)
                    .filter_map(|e| Some((e, self.store.share_key(&g.id, e)?)))
                    .map(|(e, k)| Ok((e, serde_json::to_string(&crypto::wrap_key(&keys.enc, &k, &wrap_ctx(e)))?)))
                    .collect::<Result<_>>()?;
                if !wrapped.iter().any(|(e, _)| *e == g.epoch) {
                    bail!("no key for the group");
                }
                let mut roster = self.roster(&g.id);
                let device = Device::of(keys, &req.row.label);
                // A character already in has its role; only a new one gets `role`, and only the
                // owner makes admins.
                let op = if roster.members.contains_key(&char_id) {
                    Op::DeviceAdded { char_id, device }
                } else {
                    let role = if role == Role::Admin && g.role != Role::Owner { Role::Member } else { role.min(Role::Admin) };
                    Op::MemberAdded { member: Member::new(char_id, &req.row.name, role, device) }
                };
                let code = roster.role(char_id).unwrap_or(role).code();
                c.approve(&g.id, char_id, &device_id, code, &wrapped).await?;
                self.post(&c, &g, &op).await?;
                roster.apply(g.char_id, &op)?;
                self.store.share_members_save(&g.id, &roster.members.values().cloned().collect::<Vec<_>>());
                let (holes, dead, sigs) = self.store.share_snapshot(&g);
                self.post(&c, &g, &Op::Snapshot { holes, dead, sigs, members: roster.members.into_values().collect() }).await?;
            }
            Cmd::Reject { group, char_id, device_id } => {
                let g = self.group(&group)?;
                self.client(g.char_id)?.reject(&g.id, char_id, &device_id).await?;
            }
            Cmd::Remove { group, char_id } => self.remove(&group, char_id, None).await?,
            Cmd::RemoveDevice { group, char_id, device_id } => self.remove(&group, char_id, Some(&device_id)).await?,
            Cmd::Leave { group } => {
                let g = self.group(&group)?;
                let me = self.device.public().device_id();
                let others = self
                    .store
                    .share_members(&g.id)
                    .into_iter()
                    .find(|m| m.char_id == g.char_id)
                    .is_some_and(|m| m.devices.iter().any(|d| d.id != me));
                // With other devices, only this one leaves and the character stays in the group.
                if g.role == Role::Owner && !others {
                    bail!("the owner cannot leave; remove the others or keep the group");
                }
                if self.store.share_key(&g.id, g.epoch).is_some() {
                    self.remove(&group, g.char_id, others.then_some(me.as_str())).await?;
                }
                self.store.share_group_forget(&g.id);
            }
            Cmd::SetRole { group, char_id, role } => {
                let g = self.group(&group)?;
                let c = self.client(g.char_id)?;
                c.set_role(&g.id, char_id, role.code()).await?;
                self.post(&c, &g, &Op::RoleSet { char_id, role }).await?;
                let mut roster = self.roster(&g.id);
                if let Some(m) = roster.members.get_mut(&char_id) {
                    m.role = role;
                }
                self.store.share_members_save(&g.id, &roster.members.into_values().collect::<Vec<_>>());
            }
            Cmd::Rescan { group } => self.store.share_cursor_save(&group, 0),
            Cmd::SyncNow => {}
        }
        Ok(())
    }

    /// Takes `char_id` out, or only its `device`, and moves every device that stays to a new key
    /// only they can open.
    async fn remove(&self, group: &str, char_id: i64, device: Option<&str>) -> Result<()> {
        let g = self.group(group)?;
        let c = self.client(g.char_id)?;
        let next = g.epoch + 1;
        let key = crypto::random32();
        let mut stay: Vec<Member> = self.store.share_members(&g.id);
        match device {
            Some(d) => {
                for m in stay.iter_mut().filter(|m| m.char_id == char_id) {
                    m.devices.retain(|x| x.id != d);
                }
                stay.retain(|m| !m.devices.is_empty());
            }
            None => stay.retain(|m| m.char_id != char_id),
        }
        let wrapped: Vec<(i64, String, String)> = stay
            .iter()
            .flat_map(|m| m.devices.iter().map(move |d| (m.char_id, d)))
            .map(|(c, d)| Ok((c, d.id.clone(), serde_json::to_string(&crypto::wrap_key(&d.keys.enc, &key, &wrap_ctx(next)))?)))
            .collect::<Result<_>>()?;
        match device {
            Some(d) => c.remove_device(&g.id, char_id, d, next, &wrapped).await?,
            None => c.remove(&g.id, char_id, next, &wrapped).await?,
        }
        let me = self.device.public().device_id();
        if char_id == g.char_id && device.is_none_or(|d| d == me) {
            return Ok(());
        }
        let g = ShareGroup { epoch: next, ..g };
        self.store.share_key_save(&g.id, next, &key);
        self.store.share_group_save(&g);
        self.store.share_members_save(&g.id, &stay);
        match device {
            Some(d) => self.post(&c, &g, &Op::DeviceRemoved { char_id, device_id: d.to_owned() }).await,
            None => self.post(&c, &g, &Op::MemberRemoved { char_id }).await,
        }
    }

    async fn character(&self, name: &str) -> Result<(i64, String)> {
        self.env.character(name.trim()).await?.ok_or_else(|| anyhow!("no character named {}", name.trim()))
    }

    async fn fetch_requests(&self, c: &Client<E::T>, g: &ShareGroup) -> Result<Vec<Request>> {
        Ok(c.requests(&g.id).await?
            .into_iter()
            .map(|row| {
                let body: Option<JoinBody> = serde_json::from_str(&row.body).ok();
                let invite = self.store.share_invite(&row.invite_id);
                // The MAC covers the character id, so the server cannot pass one character's
                // request off as another's; checking it against the invite's character closes a
                // leaked link.
                let proven = match (&body, &invite) {
                    (Some(b), Some((s, _, _))) => join_proven(s, row.char_id, &g.id, b),
                    _ => false,
                };
                let meant_for = invite.filter(|(_, c, _)| *c != row.char_id).map(|(_, _, n)| n);
                // The device the server says asked must be the one whose keys came with the proof.
                let same_device = body.as_ref().is_some_and(|b| b.keys.device_id() == row.device_id);
                Request { keys: body.map(|b| b.keys), verified: proven && same_device && meant_for.is_none(), meant_for, row }
            })
            .collect())
    }

    pub async fn sync_all(&self) -> Result<()> {
        let mut first_err = None;
        for g in self.store.share_groups() {
            if let Err(e) = self.sync_group(&g).await {
                first_err.get_or_insert(e.context(format!("group {}", g.name)));
            }
        }
        if let Err(e) = self.send_outbox().await {
            first_err.get_or_insert(e);
        }
        first_err.map_or(Ok(()), Err)
    }

    async fn sync_group(&self, g: &ShareGroup) -> Result<()> {
        let c = self.client(g.char_id)?;
        let mut g = g.clone();
        // Once a run: the keys the server held for this character from before devices become this
        // device's. A no-op once done, or for a device that joined as one.
        let claimed = self.status.lock().unwrap().claimed.contains(&g.id);
        if !claimed && c.claim(&g.id).await.is_ok() {
            self.status.lock().unwrap().claimed.insert(g.id.clone());
        }
        // Keys this install was given and does not hold yet.
        let had_key = self.store.share_key(&g.id, g.epoch).is_some();
        let keys = match c.keys(&g.id).await {
            Ok(k) => k,
            // Asked to join and not approved yet: nothing to fetch, nothing wrong.
            Err(_) if !had_key => return Ok(()),
            Err(e) => return Err(e),
        };
        for k in keys {
            if self.store.share_key(&g.id, k.epoch).is_none() {
                let w: Wrapped = serde_json::from_str(&k.wrapped)?;
                let key = self.device.unwrap_key(&w, &wrap_ctx(k.epoch)).context("a key wrapped to us does not open")?;
                self.store.share_key_save(&g.id, k.epoch, &key);
            }
            g.epoch = g.epoch.max(k.epoch);
        }
        self.store.share_group_save(&g);
        if !had_key && self.store.share_key(&g.id, g.epoch).is_some() {
            self.store.share_queue_group(&g.id, true, true);
        }
        let mut roster = self.roster(&g.id);
        let mut changed = false;
        loop {
            let page = c.ops(&g.id, g.cursor).await?;
            if page.is_empty() {
                break;
            }
            for row in page {
                match self.apply(&g, &mut roster, row.author, &row.blob) {
                    Ok(c) => changed |= c,
                    // A key for a later epoch is on its way: stop and pick this entry up next round.
                    // One for an epoch before ours will never come, so that entry is passed over.
                    Err(e) if e.to_string().starts_with("no key") && !self.epoch_passed(&g, &row.blob) => {
                        self.finish(&c, &g, &roster, changed).await;
                        return Ok(());
                    }
                    Err(e) => eprintln!("[share] skipped an entry in {}: {e:#}", g.name),
                }
                g.cursor = row.seq;
                self.store.share_cursor_save(&g.id, g.cursor);
            }
        }
        self.finish(&c, &g, &roster, changed).await;
        Ok(())
    }

    fn epoch_passed(&self, g: &ShareGroup, blob: &str) -> bool {
        serde_json::from_str::<Envelope>(blob).is_ok_and(|e| e.epoch < g.epoch)
    }

    async fn finish(&self, c: &Client<E::T>, g: &ShareGroup, roster: &Roster, changed: bool) {
        self.store.share_members_save(&g.id, &roster.members.values().cloned().collect::<Vec<_>>());
        let role = roster.members.get(&g.char_id).map_or(g.role, |me| me.role);
        if role != g.role {
            self.store.share_group_save(&ShareGroup { role, ..g.clone() });
        }
        // After the log, whose role changes may have just demoted us, and never fatal: a member
        // who still took itself for an admin was refused here every round and so never read the
        // entry that says otherwise.
        let reqs = if role.can_manage() {
            self.fetch_requests(c, g).await.inspect_err(|e| eprintln!("[share] join requests for {}: {e:#}", g.name)).ok()
        } else {
            None
        };
        let mut s = self.status.lock().unwrap();
        match reqs {
            Some(r) => {
                s.requests.insert(g.id.clone(), r);
            }
            None => {
                s.requests.remove(&g.id);
            }
        }
        s.synced_at.insert(g.id.clone(), spai_core::clock::utc().timestamp());
        if changed {
            s.generation += 1;
        }
    }

    /// One log entry. Returns whether holes or signatures changed.
    fn apply(&self, g: &ShareGroup, roster: &mut Roster, author: i64, blob: &str) -> Result<bool> {
        let env: Envelope = serde_json::from_str(blob)?;
        if env.group != g.id || env.author != author {
            bail!("the entry claims another group or author than the server recorded");
        }
        if !self.store.share_mark_applied(&env.op_id, &g.id) {
            return Ok(false);
        }
        let result = self.apply_new(g, roster, &env);
        if result.is_err() {
            // Not applied after all; a later round may manage (a key arriving, say).
            let _ = self.store.share_unmark_applied(&env.op_id);
        }
        result
    }

    fn apply_new(&self, g: &ShareGroup, roster: &mut Roster, env: &Envelope) -> Result<bool> {
        let key = self.store.share_key(&g.id, env.epoch).ok_or_else(|| anyhow!("no key for epoch {}", env.epoch))?;
        let author = roster.members.get(&env.author).ok_or_else(|| anyhow!("author {} is not a member", env.author))?;
        let op = ops::open_op(env, &key, author)?;
        if op.is_data() && !roster.can_write(env.author) {
            bail!("a viewer shares nothing");
        }
        let who = roster.members.get(&env.author).map(|m| m.name.clone()).unwrap_or_default();
        let (holes_in, sigs_in) = (g.prefs.recv_holes, g.prefs.recv_sigs);
        // Not taken now, so left unapplied: a Rescan after taking it again picks it up.
        let pass = || {
            let _ = self.store.share_unmark_applied(&env.op_id);
            Ok(false)
        };
        match &op {
            Op::Genesis { owner, .. } if roster.members.get(&owner.char_id) == Some(owner) => {}
            Op::Genesis { .. }
            | Op::MemberAdded { .. }
            | Op::DeviceAdded { .. }
            | Op::DeviceRemoved { .. }
            | Op::MemberRemoved { .. }
            | Op::RoleSet { .. } => roster.apply(env.author, &op)?,
            Op::Hole { .. } | Op::HoleDead { .. } if !holes_in => return pass(),
            Op::Sigs { .. } | Op::SigDelete { .. } if !sigs_in => return pass(),
            Op::Hole { hole } => return Ok(self.store.share_apply_hole(hole, &g.id, &who)),
            Op::HoleDead { at, .. } | Op::Sigs { at, .. } if *at > crate::hole::latest_believable() => {
                bail!("dated {at}, too far ahead of this clock")
            }
            Op::HoleDead { uid, .. } => self.store.share_apply_dead(uid),
            Op::Sigs { system_id, rows, drop_missing, at } => self.store.share_apply_sigs(*system_id, rows, *drop_missing, *at, &who, &g.id),
            Op::SigDelete { system_id, sig } => self.store.share_apply_sig_delete(*system_id, sig),
            Op::Snapshot { holes, dead, sigs, members } => {
                if !roster.role(env.author).is_some_and(Role::can_manage) {
                    bail!("a snapshot from someone who is not an admin");
                }
                for m in members {
                    let known = roster.members.entry(m.char_id).or_insert_with(|| m.clone());
                    for d in &m.devices {
                        if known.device(&d.id).is_none() && d.keys.device_id() == d.id && known.devices.len() < ops::MAX_DEVICES {
                            known.devices.push(d.clone());
                        }
                    }
                }
                if holes_in {
                    for h in holes {
                        self.store.share_apply_hole(h, &g.id, &who);
                    }
                    for uid in dead {
                        self.store.share_apply_dead(uid);
                    }
                }
                if sigs_in {
                    for (sys, rows) in sigs {
                        let at = rows.iter().map(|r| r.added_at).max().unwrap_or(0);
                        self.store.share_apply_sigs(*sys, rows, false, at, &who, &g.id);
                    }
                }
                if !(holes_in && sigs_in) {
                    // Applied in part; the rest waits for a Rescan. Applying it twice is harmless.
                    let _ = self.store.share_unmark_applied(&env.op_id);
                }
            }
        }
        Ok(!matches!(op, Op::Genesis { .. } | Op::MemberAdded { .. } | Op::MemberRemoved { .. } | Op::RoleSet { .. }))
    }

    async fn send_outbox(&self) -> Result<()> {
        let groups: Vec<ShareGroup> =
            self.store.share_groups().into_iter().filter(|g| self.store.share_key(&g.id, g.epoch).is_some()).collect();
        for (id, group, out) in self.store.share_outbox(200) {
            let (kind, origin) = match &out {
                Outgoing::Hole(uid) => ("hole", self.store.wormhole_group(uid)),
                Outgoing::Dead(uid) => ("dead", self.store.wormhole_group(uid)),
                Outgoing::Sigs { .. } => ("sigs", None),
                Outgoing::SigDelete { .. } => ("sigdel", None),
            };
            // Addressed rows go to their group; older ones to every group that takes them.
            let to = groups.iter().filter(|g| group.as_ref().is_none_or(|t| *t == g.id) && g.takes(kind, origin.as_deref()));
            for g in to {
                let c = self.client(g.char_id)?;
                let op = match &out {
                    Outgoing::Hole(uid) => match self.store.share_hole_state(uid, g.char_id) {
                        Some(hole) => Op::Hole { hole },
                        None => continue,
                    },
                    Outgoing::Dead(uid) => Op::HoleDead { uid: uid.clone(), at: spai_core::clock::utc().timestamp() },
                    Outgoing::Sigs { system_id, rows, drop_missing, at } => {
                        Op::Sigs { system_id: *system_id, rows: rows.clone(), drop_missing: *drop_missing, at: *at }
                    }
                    Outgoing::SigDelete { system_id, sig } => Op::SigDelete { system_id: *system_id, sig: sig.clone() },
                };
                self.post(&c, g, &op).await?;
                if let Outgoing::Hole(uid) = &out {
                    self.store.share_settle(uid, g.char_id);
                }
            }
            // Sent, or nowhere to send it: done either way.
            self.store.share_outbox_done(id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_join_request_proves_the_invite_by_either_mac() {
        let (secret, keys) = (crypto::random32(), DeviceKeys::generate().public());
        let mac = crypto::b64(&crypto::invite_mac(&secret, &join_msg(7, "g", &keys)));
        let mac2 = crypto::b64(&crypto::invite_mac(&secret, &join_msg2(7, "g", &keys)));
        let body = |mac: &str, mac2: Option<&str>| JoinBody { keys, mac: mac.into(), mac2: mac2.map(Into::into) };
        assert!(join_proven(&secret, 7, "g", &body(&mac, None)), "as every released version sends it");
        assert!(join_proven(&secret, 7, "g", &body("", Some(&mac2))), "raw bytes only");
        assert!(join_proven(&secret, 7, "g", &body(&mac, Some(&mac2))));
        assert!(!join_proven(&secret, 8, "g", &body(&mac, Some(&mac2))), "another character");
        assert!(!join_proven(&crypto::random32(), 7, "g", &body(&mac, Some(&mac2))), "without the link");
        assert!(!join_proven(&secret, 7, "g", &body("", None)));
        // An old admin reads the new body: `mac2` is a field it does not know, and ignores.
        #[derive(Deserialize)]
        struct Old {
            mac: String,
        }
        let old: Old = serde_json::from_str(&serde_json::to_string(&body(&mac, Some(&mac2))).unwrap()).unwrap();
        assert_eq!(old.mac, mac);
    }

    #[test]
    fn an_invite_link_round_trips_and_tolerates_what_chat_adds() {
        let secret = crypto::random32();
        let link = make_link("abc123", &secret);
        assert_eq!(parse_link(&link), Some(("abc123".to_owned(), secret)));
        assert_eq!(parse_link(&format!("join us: {link} fly safe")), Some(("abc123".to_owned(), secret)));
        assert_eq!(parse_link("eve-spai://join/abc123"), None, "no secret, no invite");
    }
}

