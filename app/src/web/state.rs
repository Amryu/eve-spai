//! The published state, and the rule for when a pane counts as changed.

use std::sync::{Arc, Mutex};

use super::snapshot::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Intel = 0,
    Alerts = 1,
    Pings = 2,
    Map = 3,
    Meta = 4,
}

const PANES: usize = 5;

pub struct WebState {
    pub gen: u64,
    pub seq: u64,
    intel: Option<IntelPane>,
    alerts: Option<AlertPane>,
    pings: Option<PingPane>,
    map: Option<MapLive>,
    meta: Option<Meta>,
    hashes: [Option<u64>; PANES],
    full: Option<Arc<str>>,
}

impl Default for WebState {
    fn default() -> Self {
        Self {
            gen: new_gen(),
            seq: 0,
            intel: None,
            alerts: None,
            pings: None,
            map: None,
            meta: None,
            hashes: [None; PANES],
            full: None,
        }
    }
}

pub type SharedWeb = Arc<Mutex<WebState>>;

pub fn shared() -> SharedWeb {
    Arc::new(Mutex::new(WebState::default()))
}

fn new_gen() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}

pub fn hash_of<T: serde::Serialize>(v: &T) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    h.finish()
}

impl WebState {
    /// `Some(rev)` when this pane's payload differs from what was published last, after bumping the
    /// sequence. `None` means nothing changed and the caller should not rebuild the pane, which is
    /// what keeps an idle app from pushing a frame every tick.
    pub fn changed(&mut self, pane: Pane, hash: u64) -> Option<u64> {
        let slot = &mut self.hashes[pane as usize];
        if *slot == Some(hash) {
            return None;
        }
        *slot = Some(hash);
        self.seq += 1;
        self.full = None;
        Some(self.seq)
    }

    pub fn put_intel(&mut self, p: IntelPane) {
        self.intel = Some(p);
    }
    pub fn put_alerts(&mut self, p: AlertPane) {
        self.alerts = Some(p);
    }
    pub fn put_pings(&mut self, p: PingPane) {
        self.pings = Some(p);
    }
    pub fn put_map(&mut self, p: MapLive) {
        self.map = Some(p);
    }
    pub fn put_meta(&mut self, p: Meta) {
        self.meta = Some(p);
    }

    /// Every pane that changed after `since`. `since == 0` is a client that has nothing, so it gets
    /// everything.
    pub fn snapshot_since(&self, since: u64) -> Snapshot {
        let keep = |rev: u64| rev > since;
        Snapshot {
            seq: self.seq,
            gen: self.gen,
            intel: self.intel.as_ref().filter(|p| keep(p.rev)).cloned(),
            alerts: self.alerts.as_ref().filter(|p| keep(p.rev)).cloned(),
            pings: self.pings.as_ref().filter(|p| keep(p.rev)).cloned(),
            map: self.map.as_ref().filter(|p| keep(p.rev)).cloned(),
            meta: self.meta.as_ref().filter(|p| keep(p.rev)).cloned(),
        }
    }

    /// The whole state, serialized once and handed out by `Arc`. Per-client serialization is how
    /// eight phones come to cost eight times one phone.
    pub fn full_json(&mut self) -> Arc<str> {
        if let Some(j) = &self.full {
            return j.clone();
        }
        let json: Arc<str> =
            serde_json::to_string(&self.snapshot_since(0)).unwrap_or_else(|_| "{}".into()).into();
        self.full = Some(json.clone());
        json
    }
}
