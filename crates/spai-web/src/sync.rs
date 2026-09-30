//! Runs the shared sharing engine in the browser: calls go out with `fetch`, the store lives in
//! this tab and is saved to the browser after each round.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use spai_share::api::{Method, Transport};
use spai_share::crypto::DeviceKeys;
use spai_share::engine::{Cmd, Engine, Env, Status};

use crate::page;
use crate::sso::Session;
use crate::store::WebStore;

/// What this browser calls itself among a character's devices.
pub const DEVICE_LABEL: &str = "EVE Spai web";
const STORE: &str = "spai.store";
const DEVICE: &str = "spai.device";
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

/// This browser's keys, made on first use. Lost with the browser's data: then the group re-invites
/// it, keeping the character's role.
fn device() -> &'static DeviceKeys {
    thread_local! {
        static KEYS: &'static DeviceKeys = Box::leak(Box::new(
            page::load::<String>(DEVICE).and_then(|t| DeviceKeys::from_text(&t).ok()).unwrap_or_else(|| {
                let k = DeviceKeys::generate();
                page::save(DEVICE, &k.to_text());
                k
            }),
        ));
    }
    KEYS.with(|k| *k)
}

pub struct Sync {
    pub store: Rc<WebStore>,
    pub status: Rc<Mutex<Status>>,
    commands: Rc<RefCell<VecDeque<Cmd>>>,
    running: Rc<RefCell<bool>>,
    last: i64,
}

impl Sync {
    pub fn new() -> Self {
        let store = page::load_raw(STORE).map(|t| WebStore::from_json(&t)).unwrap_or_default();
        let status = Status { fingerprint: Some(device().public().fingerprint()), ..Default::default() };
        Sync { store: Rc::new(store), status: Rc::new(Mutex::new(status)), commands: Default::default(), running: Default::default(), last: 0 }
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
        wasm_bindgen_futures::spawn_local(async move {
            let e = Engine { store: &*store, env: &env, device: device(), label: DEVICE_LABEL, status: &status };
            status.lock().unwrap().busy = true;
            let cmd = commands.borrow_mut().pop_front();
            let result = match cmd {
                Some(c) => e.command(c).await,
                None => Ok(()),
            };
            let sync = if store.share_groups_any() { e.sync_all().await } else { Ok(()) };
            {
                let mut s = status.lock().unwrap();
                s.busy = false;
                s.error = result.err().or(sync.err()).map(|e| format!("{e:#}"));
            }
            if store.dirty.replace(false) {
                page::save_raw(STORE, &store.to_json());
            }
            *running.borrow_mut() = false;
            ctx.request_repaint();
        });
    }
}

/// The link a stashed invite came from, in the form the engine reads.
pub fn invite_link(id: &str, secret: &str) -> String {
    format!("eve-spai://join/{id}#{secret}")
}
