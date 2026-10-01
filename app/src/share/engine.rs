//! Runs the sharing engine (`spai_share::engine`) on a background thread against this install's
//! database, so the UI never waits on the network.

use anyhow::Result;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::client::Http;
use crate::store::Store;
pub use spai_share::engine::{parse_link, Cmd, Engine, Env, Status};

const POLL: Duration = Duration::from_secs(15);
/// What this install calls itself among a character's devices.
const DEVICE_LABEL: &str = "EVE Spai app";

/// Signed in per character from the EVE logins in the database; names looked up on ESI.
struct Desktop {
    path: std::path::PathBuf,
}

impl Env for Desktop {
    type T = Http;

    fn transport(&self, char_id: i64) -> Result<Http> {
        Http::for_character(&self.path, char_id)
    }

    async fn character(&self, name: &str) -> Result<Option<(i64, String)>> {
        Ok(crate::universe::character(&crate::http::client(20)?, name)?)
    }
}

pub struct Handle {
    pub tx: Sender<Cmd>,
    pub status: Arc<Mutex<Status>>,
}

pub fn spawn(ctx: egui::Context) -> Handle {
    let (tx, rx) = std::sync::mpsc::channel();
    let status = Arc::new(Mutex::new(Status::default()));
    let st = status.clone();
    std::thread::Builder::new().name("wh-share".into()).spawn(move || run(rx, st, ctx)).expect("spawning the share thread");
    Handle { tx, status }
}

fn run(rx: Receiver<Cmd>, status: Arc<Mutex<Status>>, ctx: egui::Context) {
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
        let env = Desktop { path: store.path().to_path_buf() };
        status.lock().unwrap().fingerprint = Some(device.public().fingerprint());
        let e = Engine { store: &store, env: &env, device, label: DEVICE_LABEL, status: &status };
        status.lock().unwrap().busy = true;
        ctx.request_repaint();
        let result = match cmd {
            Some(c) => pollster::block_on(e.command(c)),
            None => Ok(()),
        };
        let sync = pollster::block_on(e.sync_all());
        {
            let mut s = status.lock().unwrap();
            s.busy = false;
            s.error = result.err().or(sync.err()).map(|e| format!("{e:#}"));
        }
        ctx.request_repaint();
    }
}


/// Two installs sharing through a real server. Needs one running with a known session secret:
///
///   SPAI_SHARE_TEST_BASE=http://127.0.0.1:8099 SPAI_SHARE_TEST_SECRET=... cargo test ... -- --ignored share_end_to_end
#[cfg(test)]
mod end_to_end {
    use super::*;
    use crate::share::crypto::{self, DeviceKeys};
    use crate::share::ops::{self, Op, Role};
    use crate::store::SharePrefs;
    use crate::wormholes::{DestClass, Mass, Source, Wormhole};
    use spai_share::api::Client;
    use spai_share::engine::{join_msg, make_link, InviteInfo, JoinBody};

    /// Sessions minted with the test server's secret; no ESI.
    struct TestEnv {
        base: String,
        secret: String,
    }

    impl Env for TestEnv {
        type T = Http;

        fn transport(&self, c: i64) -> Result<Http> {
            Ok(Http::with_token(&self.base, &session(&self.secret, c, &format!("Pilot {c}"))))
        }

        async fn character(&self, _: &str) -> Result<Option<(i64, String)>> {
            unreachable!("the test makes invites itself")
        }
    }

    fn session(secret: &str, char_id: i64, name: &str) -> String {
        let enc = |v: serde_json::Value| crypto::b64(v.to_string().as_bytes());
        let now = crate::clock::utc().timestamp();
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
        env: TestEnv,
    }

    impl Install {
        fn new(base: &str, secret: &str) -> Self {
            let device: &'static DeviceKeys = Box::leak(Box::new(DeviceKeys::generate()));
            Install { store: Store::mem(), device, status: Default::default(), env: TestEnv { base: base.to_owned(), secret: secret.to_owned() } }
        }

        fn engine(&self) -> Engine<'_, Store, TestEnv> {
            Engine { store: &self.store, env: &self.env, device: self.device, label: DEVICE_LABEL, status: &self.status }
        }

        fn command(&self, cmd: Cmd) -> Result<()> {
            pollster::block_on(self.engine().command(cmd))
        }

        fn run(&self, cmd: Cmd) {
            self.command(cmd).unwrap();
            self.sync();
        }

        fn try_sync(&self) -> Result<()> {
            pollster::block_on(self.engine().sync_all())
        }

        fn sync(&self) {
            self.try_sync().unwrap();
        }

        fn client(&self, c: i64) -> Client<Http> {
            self.engine().client(c).unwrap()
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
            let id = pollster::block_on(e.client(group.char_id).unwrap().create_invite(g, &blob, 3600)).unwrap();
            self.store.share_invite_save(&id, g, &secret, char_id, name);
            make_link(&id, &secret)
        }

        /// An invite as `Cmd::Invite` makes one: it lets its character in as `role` when used.
        fn invite_as(&self, g: &str, char_id: i64, name: &str, role: Role) -> String {
            let link = self.invite_for(g, char_id, name);
            let (id, _) = parse_link(&link).unwrap();
            self.store.share_invite_role_save(&id, role);
            link
        }

        fn hole(&self, sig: &str) -> Option<Wormhole> {
            self.store.wormholes().into_iter().find(|w| w.signature.as_deref() == Some(sig))
        }
    }

    #[test]
    #[ignore = "needs a running server (SPAI_SHARE_TEST_BASE, SPAI_SHARE_TEST_SECRET)"]
    fn share_end_to_end() {
        let (Ok(base), Ok(secret)) = (std::env::var("SPAI_SHARE_TEST_BASE"), std::env::var("SPAI_SHARE_TEST_SECRET")) else { return };
        let owner_id = 91_000_000 + (crate::clock::utc().timestamp() % 100_000) * 3;
        let joiner_id = owner_id + 1;
        let a = Install::new(&base, &secret);
        let b = Install::new(&base, &secret);
        let hole = |sig: &str| Wormhole {
            system_id: 31_000_200,
            signature: Some(sig.into()),
            dest: DestClass::Highsec,
            dest_system_id: Some(30_000_142),
            source: Source::Manual,
            reported_at: crate::clock::utc().timestamp(),
            updated_at: crate::clock::utc().timestamp(),
            ..Default::default()
        };
        a.store.upsert_wormhole(&hole("ABC"));

        a.run(Cmd::Create { name: "Chain".into(), char_id: owner_id, char_name: "Owner".into(), prefs: SharePrefs::default() });
        let g = a.store.share_groups()[0].id.clone();
        let link = a.invite_for(&g, joiner_id, "Pilot J");
        let stolen = a.invite_for(&g, joiner_id, "Pilot J");
        let thief = Install::new(&base, &secret);
        let err = thief.command(Cmd::Join { link: stolen.clone(), char_id: joiner_id + 1, prefs: SharePrefs::default() }).unwrap_err();
        assert!(err.to_string().contains("is for Pilot J"), "{err}");
        // A modified app skips that check and asks anyway, with a MAC that is valid for its own
        // character: the owner must still see it is not who the invite was for.
        let (inv_id, s) = parse_link(&stolen).unwrap();
        let keys = thief.device.public();
        let mac = crypto::invite_mac(&s, &join_msg(joiner_id + 1, &g, &keys));
        pollster::block_on(thief.client(joiner_id + 1).join(&inv_id, &serde_json::to_string(&JoinBody { keys, mac: crypto::b64(&mac), mac2: None }).unwrap(), "thief")).unwrap();
        a.sync();
        let reqs = a.status.lock().unwrap().requests.get(&g).cloned().unwrap_or_default();
        let bad = reqs.iter().find(|r| r.row.char_id == joiner_id + 1).expect("the thief's request");
        assert!(!bad.verified && bad.meant_for.as_deref() == Some("Pilot J"), "{bad:?}");
        let thief_dev = thief.device.public().device_id();
        assert!(a.command(Cmd::Approve { group: g.clone(), char_id: joiner_id + 1, device_id: thief_dev, role: Role::Member }).is_err());

        b.run(Cmd::Join { link: link.clone(), char_id: joiner_id, prefs: SharePrefs::default() });
        assert!(b.hole("ABC").is_none(), "nothing readable before approval");
        a.sync();
        let reqs = a.status.lock().unwrap().requests.get(&g).cloned().unwrap_or_default();
        assert!(reqs.iter().any(|r| r.row.char_id == joiner_id && r.verified), "the request proves the invite: {reqs:?}");
        let b_dev = b.device.public().device_id();
        a.run(Cmd::Approve { group: g.clone(), char_id: joiner_id, device_id: b_dev.clone(), role: Role::Member });

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

        // Made admin and back: B took itself for an admin, was refused the join requests, and must
        // still read the entry that demoted it.
        a.run(Cmd::SetRole { group: g.clone(), char_id: joiner_id, role: Role::Admin });
        b.sync();
        assert_eq!(b.store.share_groups()[0].role, Role::Admin);
        a.run(Cmd::SetRole { group: g.clone(), char_id: joiner_id, role: Role::Member });
        b.try_sync().expect("a demoted member still syncs");
        assert_eq!(b.store.share_groups()[0].role, Role::Member);

        // B, a member, invites a guest. The invite is the approval: B's next sync lets them in, as a
        // viewer, and the owner's install takes B's entry for them.
        let guest_id = joiner_id + 400_000;
        let w = Install::new(&base, &secret);
        let link_w = b.invite_as(&g, guest_id, "Pilot W", Role::Viewer);
        w.run(Cmd::Join { link: link_w, char_id: guest_id, prefs: SharePrefs::default() });
        b.sync();
        let reqs = b.status.lock().unwrap().requests.get(&g).cloned().unwrap_or_default();
        assert!(!reqs.iter().any(|r| r.row.char_id == guest_id), "let in at once, nothing left to approve: {reqs:?}");
        w.sync();
        assert!(w.hole("ABC").is_some(), "the guest reads the group");
        assert_eq!(w.store.share_groups()[0].role, Role::Viewer);
        a.sync();
        let guest = a.store.share_members(&g).into_iter().find(|m| m.char_id == guest_id).expect("the owner takes the member's entry");
        assert_eq!(guest.role, Role::Viewer);

        // B stops taking holes: A's next one waits in the log until B takes them again.
        let off = SharePrefs { recv_holes: false, ..SharePrefs::default() };
        b.store.share_prefs_save(&g, off);
        a.store.upsert_wormhole(&hole("RCV"));
        a.sync();
        b.sync();
        assert!(b.hole("RCV").is_none(), "not taken while switched off");
        b.store.share_prefs_save(&g, SharePrefs::default());
        b.run(Cmd::Rescan { group: g.clone() });
        assert!(b.hole("RCV").is_some(), "read again once taken");

        // A second device of B's character: it joins with an invite of its own and takes B's role
        // whatever the approval says.
        let b2 = Install::new(&base, &secret);
        let link2 = a.invite_for(&g, joiner_id, "Pilot J");
        b2.run(Cmd::Join { link: link2, char_id: joiner_id, prefs: SharePrefs::default() });
        a.sync();
        let b2_dev = b2.device.public().device_id();
        a.run(Cmd::Approve { group: g.clone(), char_id: joiner_id, device_id: b2_dev.clone(), role: Role::Viewer });
        b2.sync();
        assert!(b2.hole("ABC").is_some(), "the second device reads the group");
        let roster = a.store.share_members(&g);
        let joiner = roster.iter().find(|m| m.char_id == joiner_id).unwrap();
        assert_eq!((joiner.devices.len(), joiner.role), (2, Role::Member), "one member, two devices, the member's role");

        // A viewer reads but cannot share: the server refuses their data.
        let viewer_id = owner_id + 2;
        let v = Install::new(&base, &secret);
        let link3 = a.invite_for(&g, viewer_id, "Pilot V");
        v.run(Cmd::Join { link: link3, char_id: viewer_id, prefs: SharePrefs::default() });
        a.sync();
        a.run(Cmd::Approve { group: g.clone(), char_id: viewer_id, device_id: v.device.public().device_id(), role: Role::Viewer });
        v.sync();
        assert!(v.hole("ABC").is_some(), "a viewer reads");
        let vg = v.store.share_groups().into_iter().find(|x| x.id == g).unwrap();
        assert_eq!(vg.role, Role::Viewer);
        let key = v.store.share_key(&g, vg.epoch).unwrap();
        let op = Op::HoleDead { uid: "nope".into(), at: crate::clock::utc().timestamp() };
        let env = ops::seal_op(&op, &g, vg.epoch, &key, viewer_id, v.device);
        let refused = pollster::block_on(v.client(viewer_id).post_op(&g, &env.op_id, vg.epoch, false, &serde_json::to_string(&env).unwrap()));
        assert!(refused.is_err(), "the server takes no data from a viewer");

        // The viewer invites their own other device: their own sync lets it in, as a viewer.
        let v2 = Install::new(&base, &secret);
        let link_v2 = v.invite_as(&g, viewer_id, "Pilot V", Role::Viewer);
        v2.run(Cmd::Join { link: link_v2, char_id: viewer_id, prefs: SharePrefs::default() });
        v.sync();
        v2.sync();
        assert!(v2.hole("ABC").is_some(), "the viewer's second device reads the group");
        a.sync();
        let viewer = a.store.share_members(&g).into_iter().find(|m| m.char_id == viewer_id).unwrap();
        assert_eq!((viewer.devices.len(), viewer.role), (2, Role::Viewer), "one viewer, two devices, still a viewer");

        // B's second device is removed: the new key reaches every other device and not it.
        a.run(Cmd::RemoveDevice { group: g.clone(), char_id: joiner_id, device_id: b2_dev });
        a.store.upsert_wormhole(&hole("DEV"));
        a.sync();
        b.sync();
        v.sync();
        let _ = b2.try_sync();
        assert!(b.hole("DEV").is_some() && v.hole("DEV").is_some(), "the devices that stay read on");
        assert!(b2.hole("DEV").is_none(), "the removed device reads nothing new");

        // Removed, B gets nothing new, and A carries on under a new key.
        a.run(Cmd::Remove { group: g.clone(), char_id: joiner_id });
        assert_eq!(a.store.share_groups()[0].epoch, 2);
        a.store.upsert_wormhole(&hole("XYZ"));
        a.sync();
        let _ = b.try_sync();
        assert!(b.hole("XYZ").is_none(), "a removed member reads nothing new");
    }

    /// The web app joins through an invite, is approved by the owner here, and syncs the owner's
    /// hole. Needs the server as for `share_end_to_end`, a trunk build served with the API by
    /// `crates/spai-web/e2e/serve.mjs` at SPAI_WEB_SITE, and Playwright where node finds it
    /// (NODE_PATH):
    ///
    ///   SPAI_WEB_SITE=http://127.0.0.1:8188 ... cargo test ... -- --ignored web_joins_through_an_invite
    ///
    /// With `SPAI_E2E_CLICKS='[[1140,135],[1130,296],[1130,429],[993,361]]'` the browser joins as a
    /// member and marks the hole critical through the side panel (pencil, Mass, Less than 10%,
    /// Save, at 1280x800), and the owner here must receive it.
    #[test]
    #[ignore = "needs a running server, a served web build and Playwright"]
    fn web_joins_through_an_invite() {
        use std::io::{BufRead as _, Write as _};
        let (Ok(base), Ok(secret), Ok(site)) =
            (std::env::var("SPAI_SHARE_TEST_BASE"), std::env::var("SPAI_SHARE_TEST_SECRET"), std::env::var("SPAI_WEB_SITE"))
        else {
            return;
        };
        // A member edits in the browser (SPAI_E2E_CLICKS: where to click); a viewer only reads.
        let member = std::env::var("SPAI_E2E_CLICKS").is_ok();
        let now = crate::clock::utc().timestamp();
        let owner_id = 92_000_000 + (now % 100_000) * 2;
        let web_id = owner_id + 1;
        let a = Install::new(&base, &secret);
        a.store.upsert_wormhole(&Wormhole {
            system_id: 31_000_200,
            signature: Some("WEB-123".into()),
            dest: DestClass::Highsec,
            dest_system_id: Some(30_000_142),
            source: Source::Manual,
            reported_at: now,
            updated_at: now,
            ..Default::default()
        });
        // SPAI_E2E_DRIFTER: a hole as the desktop records an auto-detected one, from a drifter
        // system to k-space and read half a day ago.
        if std::env::var("SPAI_E2E_DRIFTER").is_ok() {
            let at = now - 13 * 3600;
            a.store.upsert_wormhole(&Wormhole {
                system_id: 31_000_006,
                signature: Some("FIJ".into()),
                wh_type: Some("R259".into()),
                dest: DestClass::Nullsec,
                dest_system_id: Some(30_002_901),
                size: Some(crate::wormholes::ShipSize::Large),
                reported_at: at,
                explicit_expiry: Some(at + 86_400),
                source: Source::Auto,
                updated_at: at,
                detected_by: Some("Owner".into()),
                jumped_at: Some(at),
                mass: Some(Mass::Fresh),
                life: Some(crate::wormholes::Life::UnderDay),
                observed_at: Some(at),
                ..Default::default()
            });
        }
        let scan = [crate::wormholes::ScanSig { id: "WEB-123".into(), kind: "Cosmic Signature".into(), group: "Wormhole".into(), name: "Unstable Wormhole".into() }];
        a.store.merge_system_sigs(31_000_200, &scan, "Owner", now - 3600, false, None);
        a.run(Cmd::Create { name: "Web chain".into(), char_id: owner_id, char_name: "Owner".into(), prefs: SharePrefs::default() });
        let g = a.store.share_groups()[0].id.clone();
        let link = a.invite_for(&g, web_id, "Pilot W");
        let invite = format!("/wh/{}", &link[link.find("join/").unwrap()..]);
        let web_session = serde_json::json!({
            "token": session(&secret, web_id, "Pilot W"), "expires_at": now + 3600, "character_id": web_id, "character_name": "Pilot W",
        });
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/spai-web/e2e/join.mjs");
        let shot = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/webshots/web-joined.png");
        let _ = std::fs::create_dir_all(std::path::Path::new(shot).parent().unwrap());
        let mut node = std::process::Command::new("node")
            .args([script, &site, &invite, &web_session.to_string(), shot, if member { "edit" } else { "view" }])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("node runs");
        let mut out = std::io::BufReader::new(node.stdout.take().unwrap()).lines();
        let mut seen = Vec::new();
        for line in out.by_ref() {
            let line = line.unwrap();
            seen.push(line.clone());
            if line == "requested" {
                break;
            }
        }
        assert_eq!(seen.last().map(String::as_str), Some("requested"), "{seen:?}");
        a.sync();
        let reqs = a.status.lock().unwrap().requests.get(&g).cloned().unwrap_or_default();
        let req = reqs.iter().find(|r| r.row.char_id == web_id).unwrap_or_else(|| panic!("the browser's request: {reqs:?}"));
        assert!(req.verified, "the browser proves the invite: {req:?}");
        assert_eq!(req.row.label, "EVE Spai web");
        let role = if member { Role::Member } else { Role::Viewer };
        a.run(Cmd::Approve { group: g.clone(), char_id: web_id, device_id: req.row.device_id.clone(), role });
        writeln!(node.stdin.as_mut().unwrap(), "approved").unwrap();
        drop(node.stdin.take());
        let rest: Vec<String> = out.map_while(Result::ok).collect();
        let _ = node.wait();
        let holds = rest.iter().find(|l| l.starts_with("holds ")).unwrap_or_else(|| panic!("{rest:?}"));
        assert!(holds.contains("\"holes\":[\"WEB-123\"]") && holds.contains(role.code()), "{holds}");
        if member {
            assert!(rest.iter().any(|l| l == "edited"), "{rest:?}");
            a.sync();
            assert_eq!(a.hole("WEB-123").map(|w| w.mass), Some(Some(Mass::Critical)), "the owner took the browser's edit");
        }
        assert!(holds.contains("\"sigs\":[\"WEB-123\"]"), "the probe scan came with the snapshot: {holds}");
    }


    /// The web app as an admin lets a new member in: the owner here promotes the browser, a third
    /// install asks to join, the browser approves it from its Group tab, and the newcomer reads
    /// the group. Run as `web_joins_through_an_invite`, with `SPAI_E2E_ADMIN_CLICKS` (the Group
    /// tab, then the request's Approve, at 1280x800).
    #[test]
    #[ignore = "needs a running server, a served web build and Playwright"]
    fn web_admin_lets_a_member_in() {
        use std::io::{BufRead as _, Write as _};
        let (Ok(base), Ok(secret), Ok(site), Ok(clicks)) = (
            std::env::var("SPAI_SHARE_TEST_BASE"),
            std::env::var("SPAI_SHARE_TEST_SECRET"),
            std::env::var("SPAI_WEB_SITE"),
            std::env::var("SPAI_E2E_ADMIN_CLICKS"),
        ) else {
            return;
        };
        let now = crate::clock::utc().timestamp();
        let owner_id = 93_000_000 + (now % 100_000) * 3;
        let (web_id, new_id) = (owner_id + 1, owner_id + 2);
        let a = Install::new(&base, &secret);
        a.store.upsert_wormhole(&Wormhole {
            system_id: 31_000_200,
            signature: Some("ADM-123".into()),
            dest: DestClass::Highsec,
            dest_system_id: Some(30_000_142),
            source: Source::Manual,
            reported_at: now,
            updated_at: now,
            ..Default::default()
        });
        a.run(Cmd::Create { name: "Admin chain".into(), char_id: owner_id, char_name: "Owner".into(), prefs: SharePrefs::default() });
        let g = a.store.share_groups()[0].id.clone();
        let link = a.invite_for(&g, web_id, "Pilot W");
        let invite = format!("/wh/{}", &link[link.find("join/").unwrap()..]);
        let web_session = serde_json::json!({
            "token": session(&secret, web_id, "Pilot W"), "expires_at": now + 3600, "character_id": web_id, "character_name": "Pilot W",
        });
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/spai-web/e2e/join.mjs");
        let shot = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/webshots/web-admin.png");
        let _ = std::fs::create_dir_all(std::path::Path::new(shot).parent().unwrap());
        let mut node = std::process::Command::new("node")
            .args([script, &site, &invite, &web_session.to_string(), shot, "admin"])
            .env("SPAI_E2E_CLICKS", &clicks)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("node runs");
        let mut out = std::io::BufReader::new(node.stdout.take().unwrap()).lines();
        for line in out.by_ref() {
            if line.unwrap() == "requested" {
                break;
            }
        }
        a.sync();
        let reqs = a.status.lock().unwrap().requests.get(&g).cloned().unwrap_or_default();
        let req = reqs.iter().find(|r| r.row.char_id == web_id).expect("the browser's request");
        a.run(Cmd::Approve { group: g.clone(), char_id: web_id, device_id: req.row.device_id.clone(), role: Role::Admin });
        // Someone new asks while the browser is an admin.
        let newcomer = Install::new(&base, &secret);
        let link2 = a.invite_for(&g, new_id, "Pilot N");
        newcomer.run(Cmd::Join { link: link2, char_id: new_id, prefs: SharePrefs::default() });
        writeln!(node.stdin.as_mut().unwrap(), "approved").unwrap();
        drop(node.stdin.take());
        let rest: Vec<String> = out.map_while(Result::ok).collect();
        let _ = node.wait();
        assert!(rest.iter().any(|l| l == "approved-there"), "{rest:?}");
        newcomer.sync();
        assert!(newcomer.hole("ADM-123").is_some(), "the browser let the newcomer in and the snapshot reached them");
        let roster = a.store.share_members(&g);
        a.sync();
        assert!(a.store.share_members(&g).len() > roster.len() || a.store.share_members(&g).iter().any(|m| m.char_id == new_id));
    }

}
