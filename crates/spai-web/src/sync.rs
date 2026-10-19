//! Runs the shared sharing engine in the browser: calls go out with `fetch`, the store lives in
//! this tab and is saved to the browser after each round.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use spai_share::api::{Method, Transport};
use spai_share::crypto::{self, DeviceKeys, Key};
use spai_share::engine::{Cmd, Engine, Env, Status};
use spai_share::store::SharePrefs;

use crate::page;
use crate::sso::Session;
use crate::store::WebStore;

/// What this browser calls itself among a character's devices.
pub const DEVICE_LABEL: &str = "EVE Spai web";
const STORE: &str = "spai.store";
const DEVICE: &str = "spai.device";
/// The seed of this browser's keys, when they came from a recovery phrase.
const DEVICE_SEED: &str = "spai.device.seed";
/// The seed of the keys a move to a recovery phrase is under way to, so a move cut short resumes
/// with the same keys.
const DEVICE_NEXT: &str = "spai.device.next";
/// Seconds between rounds, as the desktop polls.
const POLL: i64 = 15;

pub struct Fetch {
    base: String,
    token: String,
}

impl Transport for Fetch {
    async fn call(&self, method: Method, path: &str, device: &str, body: Option<Value>) -> Result<Value> {
        let method = match method {
            Method::Get => ehttp::Method::GET,
            Method::Post => ehttp::Method::POST,
            Method::Delete => ehttp::Method::DELETE,
        };
        let mut req = ehttp::Request::new(method, format!("{}{path}", self.base), ehttp::Headers::new(&[("Accept", "application/json")]));
        req.headers.insert("Authorization", format!("Bearer {}", self.token));
        req.headers.insert("X-Spai-Device", device);
        if let Some(b) = body {
            req.headers.insert("Content-Type", "application/json");
            req.body = serde_json::to_vec(&b)?;
        }
        let r = ehttp::fetch_async(req).await.map_err(|e| anyhow!("the server did not answer: {e}"))?;
        if !r.ok {
            let text = String::from_utf8_lossy(&r.bytes).into_owned();
            let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"].as_str().map(str::to_owned)).unwrap_or(text);
            bail!("{} ({})", msg.trim(), r.status);
        }
        Ok(if r.bytes.is_empty() { Value::Null } else { serde_json::from_slice(&r.bytes)? })
    }
}

/// Signed in as one character, the only one a browser acts as.
pub struct WebEnv {
    session: Session,
}

impl Env for WebEnv {
    type T = Fetch;

    fn transport(&self, char_id: i64) -> Result<Fetch> {
        if char_id != self.session.character_id {
            bail!("this browser is signed in as {}", self.session.character_name);
        }
        Ok(Fetch { base: page::origin(), token: self.session.token.clone() })
    }

    async fn character(&self, name: &str) -> Result<Option<(i64, String)>> {
        let mut req = ehttp::Request::post("https://esi.evetech.net/latest/universe/ids/", serde_json::to_vec(&[name])?);
        req.headers = ehttp::Headers::new(&[("Content-Type", "application/json"), ("Accept", "application/json")]);
        let r = ehttp::fetch_async(req).await.map_err(|e| anyhow!("ESI: {e}"))?;
        let v: Value = serde_json::from_slice(&r.bytes)?;
        Ok(v["characters"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name))))
            .and_then(|c| Some((c["id"].as_i64()?, c["name"].as_str()?.to_owned()))))
    }
}

/// This browser's keys, and the seed they came from when they can be recovered. Browsers from
/// before recovery phrases have keys of their own until they move to a phrase.
#[derive(Clone)]
pub struct Keys {
    pub device: Rc<DeviceKeys>,
    pub seed: Option<Key>,
}

impl Keys {
    fn load() -> Self {
        if let Some(seed) = page::load::<String>(DEVICE_SEED).and_then(|t| crypto::unb64_32(&t).ok()) {
            return Keys::of_seed(seed);
        }
        if let Some(k) = page::load::<String>(DEVICE).and_then(|t| DeviceKeys::from_text(&t).ok()) {
            return Keys { device: Rc::new(k), seed: None };
        }
        let keys = Keys::of_seed(spai_share::recovery::new_seed());
        keys.save();
        keys
    }

    fn of_seed(seed: Key) -> Self {
        Keys { device: Rc::new(DeviceKeys::from_seed(&seed)), seed: Some(seed) }
    }

    fn save(&self) {
        page::save(DEVICE, &self.device.to_text());
        match &self.seed {
            Some(s) => page::save(DEVICE_SEED, &crypto::b64(s)),
            None => page::forget(DEVICE_SEED),
        }
    }

    /// The 24 words that bring these keys back, when they came from a phrase.
    pub fn phrase(&self) -> Option<String> {
        self.seed.as_ref().map(spai_share::recovery::phrase)
    }
}

/// Work on this browser's keys, done at the start of the next round.
#[derive(Clone)]
pub enum KeyJob {
    /// Moves this browser to keys from a recovery phrase, taking its groups along.
    MoveToPhrase,
    /// Becomes the device a phrase stands for, and takes up its groups.
    Restore(Key),
}

pub struct Sync {
    pub keys: Rc<RefCell<Keys>>,
    job: Rc<RefCell<Option<KeyJob>>>,
    pub store: Rc<WebStore>,
    pub status: Rc<Mutex<Status>>,
    commands: Rc<RefCell<VecDeque<Cmd>>>,
    running: Rc<RefCell<bool>>,
    last: i64,
}

impl Sync {
    pub fn new() -> Self {
        let store = page::load_raw(STORE).map(|t| WebStore::from_json(&t)).unwrap_or_default();
        let mut keys = Keys::load();
        // Nothing to take along: a browser in no group moves to a phrase on the spot.
        if keys.seed.is_none() && !store.share_groups_any() {
            keys = Keys::of_seed(spai_share::recovery::new_seed());
            keys.save();
        }
        let status = Status { fingerprint: Some(keys.device.public().fingerprint()), ..Default::default() };
        // Any group the server has keys for this device in comes back, should this browser have
        // lost its store but kept its keys.
        let commands: VecDeque<Cmd> = VecDeque::new();
        Sync {
            keys: Rc::new(RefCell::new(keys)),
            job: Default::default(),
            store: Rc::new(store),
            status: Rc::new(Mutex::new(status)),
            commands: Rc::new(RefCell::new(commands)),
            running: Default::default(),
            last: 0,
        }
    }

    /// Asks for `job` at the start of the next round.
    pub fn key_job(&mut self, job: KeyJob) {
        *self.job.borrow_mut() = Some(job);
        self.last = 0;
    }

    /// Whether a key job is waiting or running.
    pub fn key_job_pending(&self) -> bool {
        self.job.borrow().is_some()
    }

    /// Something changed here: sync now rather than at the next round.
    pub fn poke(&mut self) {
        self.last = 0;
    }

    pub fn send(&mut self, cmd: Cmd) {
        self.commands.borrow_mut().push_back(cmd);
        self.last = 0;
    }

    /// Starts a round when one is due and none is running.
    pub fn tick(&mut self, session: &Session, ctx: &egui::Context) {
        let now = spai_core::clock::utc().timestamp();
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        if *self.running.borrow() || now - self.last < POLL {
            return;
        }
        self.last = now;
        *self.running.borrow_mut() = true;
        let (store, status, commands, running, ctx) = (self.store.clone(), self.status.clone(), self.commands.clone(), self.running.clone(), ctx.clone());
        let env = WebEnv { session: session.clone() };
        let (keys, job) = (self.keys.clone(), self.job.clone());
        wasm_bindgen_futures::spawn_local(async move {
            status.lock().unwrap().busy = true;
            let pending = job.borrow().clone();
            if let Some(j) = pending {
                let done = key_job(j, &keys, &store, &env, &status).await;
                if let Err(err) = &done {
                    status.lock().unwrap().error = Some(format!("{err:#}"));
                }
                *job.borrow_mut() = None;
            }
            let device = keys.borrow().device.clone();
            let e = Engine { store: &*store, env: &env, device: &device, label: DEVICE_LABEL, status: &status };
            // Everything asked since the last round, in order; the first failure is the one shown.
            let mut result = Ok(());
            loop {
                let Some(c) = commands.borrow_mut().pop_front() else { break };
                if let Err(err) = e.command(c).await {
                    if result.is_ok() {
                        result = Err(err);
                    }
                }
            }
            let sync = if store.share_groups_any() { e.sync_all().await } else { Ok(()) };
            {
                let mut s = status.lock().unwrap();
                s.busy = false;
                s.error = result.err().or(sync.err()).map(|e| format!("{e:#}"));
            }
            store.prune(spai_core::clock::utc().timestamp());
            if store.dirty.replace(false) {
                page::save_raw(STORE, &store.to_json());
            }
            *running.borrow_mut() = false;
            ctx.request_repaint();
        });
    }
}

/// Carries out a [`KeyJob`]. The keys this browser uses change only once everything they hold has
/// moved, so a job cut short leaves the browser as it was, and running it again picks up.
async fn key_job(job: KeyJob, keys: &Rc<RefCell<Keys>>, store: &Rc<WebStore>, env: &WebEnv, status: &Rc<Mutex<Status>>) -> Result<()> {
    let switch = |next: Keys| {
        next.save();
        page::forget(DEVICE_NEXT);
        status.lock().unwrap().fingerprint = Some(next.device.public().fingerprint());
        *keys.borrow_mut() = next;
    };
    match job {
        KeyJob::Restore(seed) => {
            store.reset();
            switch(Keys::of_seed(seed));
            let device = keys.borrow().device.clone();
            let e = Engine { store: &**store, env, device: &device, label: DEVICE_LABEL, status };
            e.command(Cmd::Recover { char_id: env.session.character_id, prefs: SharePrefs::default() }).await?;
        }
        KeyJob::MoveToPhrase => {
            if keys.borrow().seed.is_some() {
                return Ok(());
            }
            let seed = page::load::<String>(DEVICE_NEXT).and_then(|t| crypto::unb64_32(&t).ok()).unwrap_or_else(|| {
                let s = spai_share::recovery::new_seed();
                page::save(DEVICE_NEXT, &crypto::b64(&s));
                s
            });
            let next = Keys::of_seed(seed);
            let old = keys.borrow().device.clone();
            let e_old = Engine { store: &**store, env, device: &old, label: DEVICE_LABEL, status };
            e_old.sync_all().await?;
            e_old.command(Cmd::MoveTo { new: next.device.public() }).await?;
            // The new keys read what the old ones were let in to, then take the old ones out:
            // only a device still in may sign the entry that removes the other.
            let e_new = Engine { store: &**store, env, device: &next.device, label: DEVICE_LABEL, status };
            e_new.sync_all().await?;
            e_new.command(Cmd::DropDevice { device_id: old.public().device_id() }).await?;
            switch(next);
        }
    }
    if store.dirty.replace(false) {
        page::save_raw(STORE, &store.to_json());
    }
    Ok(())
}

/// The link a stashed invite came from, in the form the engine reads.
pub fn invite_link(id: &str, secret: &str) -> String {
    format!("eve-spai://join/{id}#{secret}")
}
