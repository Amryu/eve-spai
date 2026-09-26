//! Keeps this install's groups in step with the server on a background thread: fetches the keys it
//! was given, applies new log entries in order (checking each author's signature and right to make
//! it), and sends local changes. Group management (create, invite, join, approve, remove) runs here
//! too, so the UI never waits on the network.

use anyhow::{anyhow, bail, Context as _, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::client::{Client, RequestRow};
use super::crypto::{self, DeviceKeys, Key, PublicKeys, Wrapped};
use super::ops::{self, Envelope, Member, Op, Role, Roster};
use crate::store::{Outgoing, ShareGroup, Store};

const POLL: Duration = Duration::from_secs(15);
const INVITE_TTL_SECS: i64 = 2 * 86_400;

pub enum Cmd {
    Create { name: String, char_id: i64, char_name: String },
    /// An invite only `for_name` can use.
    Invite { group: String, for_name: String },
    Join { link: String, char_id: i64 },
    Approve { group: String, char_id: i64 },
    Reject { group: String, char_id: i64 },
    Remove { group: String, char_id: i64 },
    Leave { group: String },
    SetRole { group: String, char_id: i64, role: Role },
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
}

pub struct Handle {
    pub tx: Sender<Cmd>,
    pub status: Arc<Mutex<Status>>,
    /// The group local changes go to; `None` keeps them local.
    pub target: Arc<Mutex<Option<String>>>,
}

pub fn spawn(ctx: egui::Context, target: Option<String>) -> Handle {
    let (tx, rx) = std::sync::mpsc::channel();
    let status = Arc::new(Mutex::new(Status::default()));
    let target = Arc::new(Mutex::new(target));
    let (st, tg) = (status.clone(), target.clone());
    std::thread::Builder::new()
        .name("wh-share".into())
        .spawn(move || run(rx, st, tg, ctx))
        .expect("spawning the share thread");
    Handle { tx, status, target }
}

fn run(rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, target: Arc<Mutex<Option<String>>>, ctx: egui::Context) {
    let store = match Store::open() {
        Ok(s) => s,
        Err(e) => {
            status.lock().unwrap().error = Some(format!("sharing is off: the database did not open: {e}"));
            return;
        }
    };
    loop {
        let cmd = match rx.recv_timeout(POLL) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if store.share_groups().is_empty() && cmd.is_none() {
            continue;
        }
        let device = match super::keys::device() {
            Ok(d) => d,
            Err(e) => {
                status.lock().unwrap().error = Some(format!("{e:#}"));
                continue;
            }
        };
        let path = store.path().to_path_buf();
        let clients = move |char_id: i64| Client::for_character(&path, char_id);
        status.lock().unwrap().fingerprint = Some(device.public().fingerprint());
        let e = Engine { store: &store, device, status: &status, target: &target, clients: &clients };
        status.lock().unwrap().busy = true;
        ctx.request_repaint();
        let result = match cmd {
            Some(c) => e.command(c),
            None => Ok(()),
        };
        let target = target.lock().unwrap().clone();
        let sync = e.sync_all(target.as_deref());
        {
            let mut s = status.lock().unwrap();
            s.busy = false;
            s.error = result.err().or(sync.err()).map(|e| format!("{e:#}"));
        }
        ctx.request_repaint();
    }
}

#[derive(Serialize, Deserialize)]
struct InviteInfo {
    group_id: String,
    name: String,
    members: Vec<Member>,
    /// The only character that may use it.
    for_char: i64,
    for_name: String,
}

#[derive(Serialize, Deserialize)]
struct JoinBody {
    keys: PublicKeys,
    mac: String,
}

fn wrap_ctx(epoch: u32) -> Vec<u8> {
    format!("eve-spai group key|{epoch}").into_bytes()
}

fn join_msg(char_id: i64, group: &str, keys: &PublicKeys) -> Vec<u8> {
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

struct Engine<'a> {
    store: &'a Store,
    device: &'static DeviceKeys,
    status: &'a Arc<Mutex<Status>>,
    target: &'a Arc<Mutex<Option<String>>>,
    clients: &'a dyn Fn(i64) -> Result<Client>,
}

impl Engine<'_> {
    /// A first group becomes where local changes go, so what is already known gets shared.
    fn default_target(&self, group: &str) {
        let mut t = self.target.lock().unwrap();
        if t.is_none() {
            *t = Some(group.to_owned());
        }
    }

    fn client(&self, char_id: i64) -> Result<Client> {
        (self.clients)(char_id)
    }

    fn group(&self, id: &str) -> Result<ShareGroup> {
        self.store.share_groups().into_iter().find(|g| g.id == id).ok_or_else(|| anyhow!("not in that group"))
    }

    fn roster(&self, group: &str) -> Roster {
        let members = self.store.share_members(group);
        Roster { name: String::new(), members: members.into_iter().map(|m| (m.char_id, m)).collect() }
    }

    fn post(&self, c: &Client, g: &ShareGroup, op: &Op) -> Result<()> {
        let key = self.store.share_key(&g.id, g.epoch).ok_or_else(|| anyhow!("no key for the group yet"))?;
        let env = ops::seal_op(op, &g.id, g.epoch, &key, g.char_id, self.device);
        c.post_op(&g.id, &env.op_id, g.epoch, op.keep(), &serde_json::to_string(&env)?)?;
        // Our own entry comes back from the server like anyone's; it is already applied here.
        self.store.share_mark_applied(&env.op_id, &g.id);
        Ok(())
    }

    fn command(&self, cmd: Cmd) -> Result<()> {
        match cmd {
            Cmd::Create { name, char_id, char_name } => {
                let c = self.client(char_id)?;
                let key = crypto::random32();
                let wrapped = crypto::wrap_key(&self.device.public().enc, &key, &wrap_ctx(0));
                let id = c.create_group(&serde_json::to_string(&wrapped)?)?;
                let g = ShareGroup { id: id.clone(), name: name.clone(), char_id, role: Role::Owner, epoch: 0, cursor: 0 };
                self.store.share_group_save(&g);
                self.store.share_key_save(&id, 0, &key);
                let owner = Member { char_id, name: char_name, keys: self.device.public(), role: Role::Owner };
                self.store.share_members_save(&id, std::slice::from_ref(&owner));
                self.post(&c, &g, &Op::Genesis { name, owner })?;
                self.default_target(&id);
                self.store.share_queue_all();
            }
            Cmd::Invite { group, for_name } => {
                let g = self.group(&group)?;
                let (for_char, for_name) = self.character(&for_name)?;
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
                let id = c.create_invite(&g.id, &blob, INVITE_TTL_SECS)?;
                self.store.share_invite_save(&id, &g.id, &secret, for_char, &for_name);
                self.status.lock().unwrap().invite = Some((g.id, make_link(&id, &secret), for_name));
            }
            Cmd::Join { link, char_id } => {
                let (id, secret) = parse_link(&link).ok_or_else(|| anyhow!("that is not an invite link"))?;
                let c = self.client(char_id)?;
                let (group_id, blob) = c.fetch_invite(&id).context("the invite is unknown, used or expired")?;
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
                let mac = crypto::invite_mac(&secret, &join_msg(char_id, &group_id, &keys));
                c.join(&id, &serde_json::to_string(&JoinBody { keys, mac: crypto::b64(&mac) })?)?;
                // Until an admin approves there is no key; the members the invite named are who
                // this install trusts to sign the group's entries.
                self.store.share_group_save(&ShareGroup { id: group_id.clone(), name: info.name, char_id, role: Role::Member, epoch: 0, cursor: 0 });
                self.store.share_members_save(&group_id, &info.members);
            }
            Cmd::Approve { group, char_id } => {
                let g = self.group(&group)?;
                let c = self.client(g.char_id)?;
                let req = self.fetch_requests(&c, &g)?.into_iter().find(|r| r.row.char_id == char_id).ok_or_else(|| anyhow!("no such request"))?;
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
                c.approve(&g.id, char_id, &wrapped)?;
                let member = Member { char_id, name: req.row.name.clone(), keys, role: Role::Member };
                self.post(&c, &g, &Op::MemberAdded { member: member.clone() })?;
                let mut roster = self.roster(&g.id);
                roster.members.insert(char_id, member);
                self.store.share_members_save(&g.id, &roster.members.values().cloned().collect::<Vec<_>>());
                let (holes, dead, sigs) = self.store.share_snapshot(g.char_id);
                self.post(&c, &g, &Op::Snapshot { holes, dead, sigs, members: roster.members.into_values().collect() })?;
            }
            Cmd::Reject { group, char_id } => {
                let g = self.group(&group)?;
                self.client(g.char_id)?.reject(&g.id, char_id)?;
            }
            Cmd::Remove { group, char_id } => self.remove(&group, char_id)?,
            Cmd::Leave { group } => {
                let g = self.group(&group)?;
                if g.role == Role::Owner {
                    bail!("the owner cannot leave; remove the others or keep the group");
                }
                if self.store.share_key(&g.id, g.epoch).is_some() {
                    self.remove(&group, g.char_id)?;
                }
                self.store.share_group_forget(&g.id);
            }
            Cmd::SetRole { group, char_id, role } => {
                let g = self.group(&group)?;
                let c = self.client(g.char_id)?;
                c.set_role(&g.id, char_id, if role == Role::Admin { "admin" } else { "member" })?;
                self.post(&c, &g, &Op::RoleSet { char_id, role })?;
                let mut roster = self.roster(&g.id);
                if let Some(m) = roster.members.get_mut(&char_id) {
                    m.role = role;
                }
                self.store.share_members_save(&g.id, &roster.members.into_values().collect::<Vec<_>>());
            }
            Cmd::SyncNow => {}
        }
        Ok(())
    }

    /// Takes `char_id` out and moves everyone who stays to a new key they alone can open.
    fn remove(&self, group: &str, char_id: i64) -> Result<()> {
        let g = self.group(group)?;
        let c = self.client(g.char_id)?;
        let next = g.epoch + 1;
        let key = crypto::random32();
        let stay: Vec<Member> = self.store.share_members(&g.id).into_iter().filter(|m| m.char_id != char_id).collect();
        let wrapped: Vec<(i64, String)> = stay
            .iter()
            .map(|m| Ok((m.char_id, serde_json::to_string(&crypto::wrap_key(&m.keys.enc, &key, &wrap_ctx(next)))?)))
            .collect::<Result<_>>()?;
        c.remove(&g.id, char_id, next, &wrapped)?;
        if char_id == g.char_id {
            return Ok(());
        }
        let g = ShareGroup { epoch: next, ..g };
        self.store.share_key_save(&g.id, next, &key);
        self.store.share_group_save(&g);
        self.store.share_members_save(&g.id, &stay);
        self.post(&c, &g, &Op::MemberRemoved { char_id })
    }

    fn character(&self, name: &str) -> Result<(i64, String)> {
        let http = crate::http::client(20)?;
        crate::universe::character(&http, name.trim())?.ok_or_else(|| anyhow!("no character named {}", name.trim()))
    }

    fn fetch_requests(&self, c: &Client, g: &ShareGroup) -> Result<Vec<Request>> {
        Ok(c.requests(&g.id)?
            .into_iter()
            .map(|row| {
                let body: Option<JoinBody> = serde_json::from_str(&row.body).ok();
                let invite = self.store.share_invite(&row.invite_id);
                // The MAC covers the character id, so the server cannot pass one character's
                // request off as another's; checking it against the invite's character closes a
                // leaked link.
                let proven = match (&body, &invite) {
                    (Some(b), Some((s, _, _))) => crypto::unb64(&b.mac)
                        .is_ok_and(|mac| crypto::invite_mac_ok(s, &join_msg(row.char_id, &g.id, &b.keys), &mac)),
                    _ => false,
                };
                let meant_for = invite.filter(|(_, c, _)| *c != row.char_id).map(|(_, _, n)| n);
                Request { keys: body.map(|b| b.keys), verified: proven && meant_for.is_none(), meant_for, row }
            })
            .collect())
    }

    fn sync_all(&self, target: Option<&str>) -> Result<()> {
        let mut first_err = None;
        for g in self.store.share_groups() {
            if let Err(e) = self.sync_group(&g) {
                first_err.get_or_insert(e.context(format!("group {}", g.name)));
            }
        }
        if let Err(e) = self.send_outbox(target) {
            first_err.get_or_insert(e);
        }
        first_err.map_or(Ok(()), Err)
    }

    fn sync_group(&self, g: &ShareGroup) -> Result<()> {
        let c = self.client(g.char_id)?;
        let mut g = g.clone();
        // Keys this install was given and does not hold yet.
        let had_key = self.store.share_key(&g.id, g.epoch).is_some();
        let keys = match c.keys(&g.id) {
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
            self.default_target(&g.id);
            self.store.share_queue_all();
        }
        if g.role.can_manage() {
            let reqs = self.fetch_requests(&c, &g)?;
            self.status.lock().unwrap().requests.insert(g.id.clone(), reqs);
        }
        let mut roster = self.roster(&g.id);
        let mut changed = false;
        loop {
            let page = c.ops(&g.id, g.cursor)?;
            if page.is_empty() {
                break;
            }
            for row in page {
                match self.apply(&g, &mut roster, row.author, &row.blob) {
                    Ok(c) => changed |= c,
                    // A key for a later epoch is on its way: stop and pick this entry up next round.
                    // One for an epoch before ours will never come, so that entry is passed over.
                    Err(e) if e.to_string().starts_with("no key") && !self.epoch_passed(&g, &row.blob) => {
                        self.finish(&g, &roster, changed);
                        return Ok(());
                    }
                    Err(e) => eprintln!("[share] skipped an entry in {}: {e:#}", g.name),
                }
                g.cursor = row.seq;
                self.store.share_cursor_save(&g.id, g.cursor);
            }
        }
        self.finish(&g, &roster, changed);
        Ok(())
    }

    fn epoch_passed(&self, g: &ShareGroup, blob: &str) -> bool {
        serde_json::from_str::<Envelope>(blob).is_ok_and(|e| e.epoch < g.epoch)
    }

    fn finish(&self, g: &ShareGroup, roster: &Roster, changed: bool) {
        self.store.share_members_save(&g.id, &roster.members.values().cloned().collect::<Vec<_>>());
        if let Some(me) = roster.members.get(&g.char_id) {
            if me.role != g.role {
                self.store.share_group_save(&ShareGroup { role: me.role, ..g.clone() });
            }
        }
        let mut s = self.status.lock().unwrap();
        s.synced_at.insert(g.id.clone(), chrono::Utc::now().timestamp());
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
        let keys = roster.members.get(&env.author).map(|m| m.keys).ok_or_else(|| anyhow!("author {} is not a member", env.author))?;
        let op = ops::open_op(env, &key, &keys)?;
        let who = roster.members.get(&env.author).map(|m| m.name.clone()).unwrap_or_default();
        match &op {
            Op::Genesis { owner, .. } if roster.members.get(&owner.char_id) == Some(owner) => {}
            Op::Genesis { .. } | Op::MemberAdded { .. } | Op::MemberRemoved { .. } | Op::RoleSet { .. } => roster.apply(env.author, &op)?,
            Op::Hole { hole } => return Ok(self.store.share_apply_hole(hole, &g.id, &who)),
            Op::HoleDead { uid, .. } => self.store.share_apply_dead(uid),
            Op::Sigs { system_id, rows, drop_missing, at } => self.store.share_apply_sigs(*system_id, rows, *drop_missing, *at, &who),
            Op::SigDelete { system_id, sig } => self.store.share_apply_sig_delete(*system_id, sig),
            Op::Snapshot { holes, dead, sigs, members } => {
                if !roster.role(env.author).is_some_and(Role::can_manage) {
                    bail!("a snapshot from someone who is not an admin");
                }
                for m in members {
                    roster.members.entry(m.char_id).or_insert_with(|| m.clone());
                }
                for h in holes {
                    self.store.share_apply_hole(h, &g.id, &who);
                }
                for uid in dead {
                    self.store.share_apply_dead(uid);
                }
                for (sys, rows) in sigs {
                    let at = rows.iter().map(|r| r.added_at).max().unwrap_or(0);
                    self.store.share_apply_sigs(*sys, rows, false, at, &who);
                }
            }
        }
        Ok(!matches!(op, Op::Genesis { .. } | Op::MemberAdded { .. } | Op::MemberRemoved { .. } | Op::RoleSet { .. }))
    }

    fn send_outbox(&self, target: Option<&str>) -> Result<()> {
        let groups = self.store.share_groups();
        let find = |id: &str| groups.iter().find(|g| g.id == id && self.store.share_key(&g.id, g.epoch).is_some());
        let default = target.and_then(find);
        for (id, out) in self.store.share_outbox(200) {
            let uid_group = match &out {
                Outgoing::Hole(uid) | Outgoing::Dead(uid) => self.store.wormhole_group(uid),
                _ => None,
            };
            let Some(g) = uid_group.as_deref().and_then(find).or(default) else {
                // Nowhere to send it: local only.
                self.store.share_outbox_done(id);
                continue;
            };
            let c = self.client(g.char_id)?;
            let op = match &out {
                Outgoing::Hole(uid) => match self.store.share_hole_state(uid, g.char_id) {
                    Some(hole) => Op::Hole { hole },
                    None => {
                        self.store.share_outbox_done(id);
                        continue;
                    }
                },
                Outgoing::Dead(uid) => Op::HoleDead { uid: uid.clone(), at: chrono::Utc::now().timestamp() },
                Outgoing::Sigs { system_id, rows, drop_missing, at } => {
                    Op::Sigs { system_id: *system_id, rows: rows.clone(), drop_missing: *drop_missing, at: *at }
                }
                Outgoing::SigDelete { system_id, sig } => Op::SigDelete { system_id: *system_id, sig: sig.clone() },
            };
            self.post(&c, g, &op)?;
            if let Outgoing::Hole(uid) = &out {
                self.store.share_settle(uid, g.char_id);
                self.store.share_set_group(uid, &g.id);
            }
            self.store.share_outbox_done(id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invite_link_round_trips_and_tolerates_what_chat_adds() {
        let secret = crypto::random32();
        let link = make_link("abc123", &secret);
        assert_eq!(parse_link(&link), Some(("abc123".to_owned(), secret)));
        assert_eq!(parse_link(&format!("join us: {link} fly safe")), Some(("abc123".to_owned(), secret)));
        assert_eq!(parse_link("eve-spai://join/abc123"), None, "no secret, no invite");
    }
}

/// Two installs sharing through a real server. Needs one running with a known session secret:
///
///   SPAI_SHARE_TEST_BASE=http://127.0.0.1:8099 SPAI_SHARE_TEST_SECRET=... cargo test ... -- --ignored share_end_to_end
#[cfg(test)]
mod end_to_end {
    use super::*;
    use crate::wormholes::{DestClass, Mass, Source, Wormhole};

    fn session(secret: &str, char_id: i64, name: &str) -> String {
        let enc = |v: serde_json::Value| crypto::b64(v.to_string().as_bytes());
        let now = chrono::Utc::now().timestamp();
        let head = enc(serde_json::json!({ "alg": "HS256", "typ": "JWT" }));
        let body = enc(serde_json::json!({
            "iss": "eve-spai.com", "aud": "eve-spai.com", "sub": char_id.to_string(), "name": name, "iat": now, "exp": now + 3600,
        }));
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
        let sig = ring::hmac::sign(&key, format!("{head}.{body}").as_bytes());
        format!("{head}.{body}.{}", crypto::b64(sig.as_ref()))
    }

    struct Install {
        store: Store,
        device: &'static DeviceKeys,
        status: Arc<Mutex<Status>>,
        target: Arc<Mutex<Option<String>>>,
        clients: Box<dyn Fn(i64) -> Result<Client>>,
    }

    impl Install {
        fn new(base: &str, secret: &str) -> Self {
            let (base, secret) = (base.to_owned(), secret.to_owned());
            Install {
                store: Store::mem(),
                device: Box::leak(Box::new(DeviceKeys::generate())),
                status: Default::default(),
                target: Default::default(),
                clients: Box::new(move |c| Ok(Client::with_token(&base, &session(&secret, c, &format!("Pilot {c}"))))),
            }
        }

        fn engine(&self) -> Engine<'_> {
            Engine { store: &self.store, device: self.device, status: &self.status, target: &self.target, clients: &*self.clients }
        }

        fn run(&self, cmd: Cmd) {
            let e = self.engine();
            e.command(cmd).unwrap();
            self.sync();
        }

        fn sync(&self) {
            let target = self.target.lock().unwrap().clone();
            self.engine().sync_all(target.as_deref()).unwrap();
        }

        /// An invite for `char_id`, made without the ESI name lookup `Cmd::Invite` does.
        fn invite_for(&self, g: &str, char_id: i64, name: &str) -> String {
            let e = self.engine();
            let group = e.group(g).unwrap();
            let secret = crypto::random32();
            let info = InviteInfo {
                group_id: group.id.clone(),
                name: group.name.clone(),
                members: self.store.share_members(g),
                for_char: char_id,
                for_name: name.into(),
            };
            let blob = crypto::b64(&crypto::seal(&crypto::invite_key(&secret), b"eve-spai invite", &serde_json::to_vec(&info).unwrap()));
            let id = e.client(group.char_id).unwrap().create_invite(g, &blob, 3600).unwrap();
            self.store.share_invite_save(&id, g, &secret, char_id, name);
            make_link(&id, &secret)
        }

        fn hole(&self, sig: &str) -> Option<Wormhole> {
            self.store.wormholes().into_iter().find(|w| w.signature.as_deref() == Some(sig))
        }
    }

    #[test]
    #[ignore = "needs a running server (SPAI_SHARE_TEST_BASE, SPAI_SHARE_TEST_SECRET)"]
    fn share_end_to_end() {
        let (Ok(base), Ok(secret)) = (std::env::var("SPAI_SHARE_TEST_BASE"), std::env::var("SPAI_SHARE_TEST_SECRET")) else { return };
        let owner_id = 91_000_000 + (chrono::Utc::now().timestamp() % 100_000) * 3;
        let joiner_id = owner_id + 1;
        let a = Install::new(&base, &secret);
        let b = Install::new(&base, &secret);
        let hole = |sig: &str| Wormhole {
            system_id: 31_000_001,
            signature: Some(sig.into()),
            dest: DestClass::Highsec,
            dest_system_id: Some(30_000_142),
            source: Source::Manual,
            reported_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            ..Default::default()
        };
        a.store.upsert_wormhole(&hole("ABC"));

        a.run(Cmd::Create { name: "Chain".into(), char_id: owner_id, char_name: "Owner".into() });
        let g = a.store.share_groups()[0].id.clone();
        let link = a.invite_for(&g, joiner_id, "Pilot J");
        let stolen = a.invite_for(&g, joiner_id, "Pilot J");
        let thief = Install::new(&base, &secret);
        let err = thief.engine().command(Cmd::Join { link: stolen.clone(), char_id: joiner_id + 1 }).unwrap_err();
        assert!(err.to_string().contains("is for Pilot J"), "{err}");
        // A modified app skips that check and asks anyway, with a MAC that is valid for its own
        // character: the owner must still see it is not who the invite was for.
        let (inv_id, s) = parse_link(&stolen).unwrap();
        let keys = thief.device.public();
        let mac = crypto::invite_mac(&s, &join_msg(joiner_id + 1, &g, &keys));
        (thief.clients)(joiner_id + 1).unwrap().join(&inv_id, &serde_json::to_string(&JoinBody { keys, mac: crypto::b64(&mac) }).unwrap()).unwrap();
        a.sync();
        let reqs = a.status.lock().unwrap().requests.get(&g).cloned().unwrap_or_default();
        let bad = reqs.iter().find(|r| r.row.char_id == joiner_id + 1).expect("the thief's request");
        assert!(!bad.verified && bad.meant_for.as_deref() == Some("Pilot J"), "{bad:?}");
        assert!(a.engine().command(Cmd::Approve { group: g.clone(), char_id: joiner_id + 1 }).is_err());

        b.run(Cmd::Join { link: link.clone(), char_id: joiner_id });
        assert!(b.hole("ABC").is_none(), "nothing readable before approval");
        a.sync();
        let reqs = a.status.lock().unwrap().requests.get(&g).cloned().unwrap_or_default();
        assert!(reqs.iter().any(|r| r.row.char_id == joiner_id && r.verified), "the request proves the invite: {reqs:?}");
        a.run(Cmd::Approve { group: g.clone(), char_id: joiner_id });

        b.sync();
        let seen = b.hole("ABC").expect("the snapshot brought the owner's hole");
        assert_eq!(seen.dest_system_id, Some(30_000_142));
        assert_eq!(b.store.share_members(&g).len(), 2);

        // B degrades it; A takes it, and nothing bounces back to B.
        let mut w = b.hole("ABC").unwrap();
        w.mass = Some(Mass::Critical);
        b.store.write_wormhole(&w);
        b.sync();
        a.sync();
        assert_eq!(a.hole("ABC").unwrap().mass, Some(Mass::Critical));
        assert!(a.store.share_outbox(10).is_empty(), "applied changes are not re-sent");
        b.sync();
        assert!(b.store.share_outbox(10).is_empty());

        // Removed, B gets nothing new, and A carries on under a new key.
        a.run(Cmd::Remove { group: g.clone(), char_id: joiner_id });
        assert_eq!(a.store.share_groups()[0].epoch, 1);
        a.store.upsert_wormhole(&hole("XYZ"));
        a.sync();
        let _ = b.engine().sync_all(None);
        assert!(b.hole("XYZ").is_none(), "a removed member reads nothing new");
    }
}
