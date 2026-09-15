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
    Status = 5,
    Jabber = 6,
    Rescue = 7,
    Notes = 8,
}

const PANES: usize = 9;

pub struct WebState {
    pub gen: u64,
    pub seq: u64,
    intel: Option<IntelPane>,
    alerts: Option<AlertPane>,
    pings: Option<PingPane>,
    map: Option<MapLive>,
    meta: Option<Meta>,
    status: Option<StatusPane>,
    jabber: Option<crate::web::jabber::JabberPane>,
    rescue: Option<crate::web::rescue::RescuePane>,
    notes: Option<NotesPane>,
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
            status: None,
            jabber: None,
            rescue: None,
            notes: None,
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
    /// `Some(rev)`, after bumping the sequence, when the payload differs from the last publish.
    /// `None` means the caller should not rebuild the pane.
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
    pub fn put_status(&mut self, p: StatusPane) {
        self.status = Some(p);
    }
    /// Read live rather than from the server config, which changes only on a listener restart, and a
    /// restart can fail to rebind while the old socket closes.
    pub fn theme(&self) -> Option<crate::theme::Theme> {
        self.meta.as_ref().map(|m| m.theme.clone())
    }

    /// Per-system intel severity as last published, for the route warnings.
    pub fn intel_marks(&self) -> Vec<(i64, u8, i64)> {
        self.map.as_ref().map(|m| m.intel.clone()).unwrap_or_default()
    }

    /// Ship and pod kills per system in the last hour, as last published.
    pub fn kill_counts(&self) -> Vec<(i64, u32, u32)> {
        self.status
            .as_ref()
            .map(|s| s.systems.iter().map(|(id, i)| (*id, i.k, i.p)).collect())
            .unwrap_or_default()
    }

    pub fn put_jabber(&mut self, p: crate::web::jabber::JabberPane) {
        self.jabber = Some(p);
    }
    pub fn put_rescue(&mut self, p: crate::web::rescue::RescuePane) {
        self.rescue = Some(p);
    }
    pub fn put_notes(&mut self, p: NotesPane) {
        self.notes = Some(p);
    }

    /// Every pane that changed after `since`. `since == 0` gets everything.
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
            status: self.status.as_ref().filter(|p| keep(p.rev)).cloned(),
            jabber: self.jabber.as_ref().filter(|p| keep(p.rev)).cloned(),
            rescue: self.rescue.as_ref().filter(|p| keep(p.rev)).cloned(),
            notes: self.notes.as_ref().filter(|p| keep(p.rev)).cloned(),
        }
    }

    /// Serialized once and shared by `Arc` rather than per client.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `hash_of` serializes with serde_json, and each `HashMap` has its own seed, so identical maps
    /// built a tick apart would hash differently.
    #[test]
    fn a_map_hashes_the_same_however_it_was_built() {
        let build = || {
            let mut m = std::collections::BTreeMap::new();
            for (k, v) in
                [("alpha", 1), ("bravo", 2), ("charlie", 3), ("delta", 4), ("echo", 5)]
            {
                m.insert(k.to_owned(), v);
            }
            m
        };
        let mut reversed = std::collections::BTreeMap::new();
        for (k, v) in [("echo", 5), ("delta", 4), ("charlie", 3), ("bravo", 2), ("alpha", 1)] {
            reversed.insert(k.to_owned(), v);
        }
        assert_eq!(hash_of(&build()), hash_of(&reversed), "insertion order must not matter");

        // Two separately built instances.
        for _ in 0..20 {
            assert_eq!(hash_of(&build()), hash_of(&build()));
        }
    }

    /// Explains why the lookups must stay `BTreeMap`.
    #[test]
    fn a_hashmap_would_not_have_hashed_stably() {
        let build = || {
            let mut m = std::collections::HashMap::new();
            for (k, v) in [
                ("alpha", 1), ("bravo", 2), ("charlie", 3), ("delta", 4), ("echo", 5),
                ("foxtrot", 6), ("golf", 7), ("hotel", 8),
            ] {
                m.insert(k.to_owned(), v);
            }
            m
        };
        let first = hash_of(&build());
        let unstable = (0..50).any(|_| hash_of(&build()) != first);
        assert!(
            unstable,
            "a HashMap hashed identically 50 times; if this ever becomes true the BTreeMaps in \
             `snapshot` can go back to being HashMaps"
        );
    }

    #[test]
    fn changed_reports_only_real_changes() {
        let mut st = WebState::default();
        assert!(st.changed(Pane::Intel, 1).is_some(), "first publish");
        assert!(st.changed(Pane::Intel, 1).is_none(), "same hash");
        assert!(st.changed(Pane::Intel, 2).is_some(), "different hash");
        assert!(st.changed(Pane::Alerts, 1).is_some(), "panes are independent");
    }
}
