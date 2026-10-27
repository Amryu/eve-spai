//! The fit library in the browser: every saved fit as its document, under one key.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use spai_fit_ui::FitSummary;

pub const KEY: &str = "spai.sim.fits";
pub const VIEW_KEY: &str = "spai.sim.view";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub ship: u32,
    pub updated_at: i64,
    pub doc: String,
    /// Counted up on each save here; `server_rev` is what the server last confirmed.
    #[serde(default)]
    pub rev: i64,
    #[serde(default)]
    pub server_rev: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FleetEntry {
    pub name: String,
    pub updated_at: i64,
    pub doc: String,
    #[serde(default)]
    pub rev: i64,
    #[serde(default)]
    pub server_rev: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub fits: BTreeMap<String, Entry>,
    pub fleets: BTreeMap<String, FleetEntry>,
    /// The pilot's own doctrines, the same shape as fleets.
    pub doctrines: BTreeMap<String, FleetEntry>,
    /// Deleted here, by API path (`/fits/<id>`), until the server has deleted them too: without
    /// it the next sync brought them back.
    pub deleted: std::collections::BTreeSet<String>,
}

impl Library {
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn summaries(&self) -> Vec<FitSummary> {
        let mut v: Vec<FitSummary> = self.fits.iter().map(|(id, e)| FitSummary { id: id.clone(), name: e.name.clone(), ship: e.ship, updated_at: e.updated_at }).collect();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.name.cmp(&b.name)));
        v
    }

    pub fn put(&mut self, id: &str, name: &str, ship: u32, doc: &str, now: i64) {
        let (rev, server_rev) = self.fits.get(id).map_or((1, 0), |e| (e.rev + 1, e.server_rev));
        self.fits.insert(id.to_owned(), Entry { name: name.to_owned(), ship, updated_at: now, doc: doc.to_owned(), rev, server_rev });
    }

    pub fn put_fleet(&mut self, id: &str, name: &str, doc: &str, now: i64) {
        let (rev, server_rev) = self.fleets.get(id).map_or((1, 0), |e| (e.rev + 1, e.server_rev));
        self.fleets.insert(id.to_owned(), FleetEntry { name: name.to_owned(), updated_at: now, doc: doc.to_owned(), rev, server_rev });
    }

    pub fn put_doctrine(&mut self, id: &str, name: &str, doc: &str, now: i64) {
        let (rev, server_rev) = self.doctrines.get(id).map_or((1, 0), |e| (e.rev + 1, e.server_rev));
        self.doctrines.insert(id.to_owned(), FleetEntry { name: name.to_owned(), updated_at: now, doc: doc.to_owned(), rev, server_rev });
    }

    pub fn doctrine_docs(&self) -> Vec<String> {
        self.doctrines.values().map(|e| e.doc.clone()).collect()
    }

    pub fn remove(&mut self, id: &str) {
        self.fits.remove(id);
        self.deleted.insert(format!("/fits/{id}"));
    }

    pub fn remove_fleet(&mut self, id: &str) {
        self.fleets.remove(id);
        self.deleted.insert(format!("/fleets/{id}"));
    }

    pub fn remove_doctrine(&mut self, id: &str) {
        self.doctrines.remove(id);
        self.deleted.insert(format!("/own-doctrines/{id}"));
    }

    /// Whether the server's copy at `path` is one deleted here.
    pub fn is_deleted(&self, path: &str) -> bool {
        self.deleted.contains(path)
    }

    /// After a sync that started from `self`: deletes made here meanwhile hold, those the server
    /// confirmed are forgotten.
    pub fn merge_deletes(&mut self, stored: &Library, confirmed: &[String]) {
        self.deleted.extend(stored.deleted.iter().cloned());
        for p in confirmed {
            self.deleted.remove(p);
        }
        let gone: Vec<String> = self.deleted.iter().cloned().collect();
        for p in gone {
            if let Some(id) = p.strip_prefix("/fits/") {
                self.fits.remove(id);
            } else if let Some(id) = p.strip_prefix("/fleets/") {
                self.fleets.remove(id);
            } else if let Some(id) = p.strip_prefix("/own-doctrines/") {
                self.doctrines.remove(id);
            }
        }
    }

    pub fn fleet_summaries(&self) -> Vec<spai_fit_ui::FleetSummary> {
        let mut v: Vec<spai_fit_ui::FleetSummary> = self.fleets.iter().map(|(id, e)| spai_fit_ui::FleetSummary { id: id.clone(), name: e.name.clone(), updated_at: e.updated_at }).collect();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.name.cmp(&b.name)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A delete holds through a sync that started before it, and is forgotten once the server
    /// has it.
    #[test]
    fn a_delete_outlasts_the_sync_and_the_server_copy() {
        let mut lib = Library::default();
        lib.put("a", "A", 1, "{}", 1);
        lib.put_fleet("f", "F", "{}", 1);
        let synced = lib.clone();
        lib.remove_fleet("f");
        assert!(lib.is_deleted("/fleets/f"));
        let mut after = synced.clone();
        after.merge_deletes(&lib, &[]);
        assert!(!after.fleets.contains_key("f"), "deleted while the sync ran");
        assert!(after.is_deleted("/fleets/f"));
        after.merge_deletes(&after.clone(), &["/fleets/f".to_owned()]);
        assert!(!after.is_deleted("/fleets/f"), "the server has it");
        assert!(after.fits.contains_key("a"));
        let back = Library::from_json(&lib.to_json());
        assert!(back.is_deleted("/fleets/f"), "kept between visits");
    }

    #[test]
    fn the_library_lists_newest_first_and_round_trips() {
        let mut l = Library::default();
        l.put("a", "Old", 587, "{}", 10);
        l.put("b", "New", 587, "{}", 20);
        assert_eq!(l.summaries().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["New", "Old"]);
        let back = Library::from_json(&l.to_json());
        assert_eq!(back, l);
        l.put("a", "Old again", 587, "{}", 30);
        assert_eq!((l.fits["a"].rev, l.fits["a"].server_rev), (2, 0), "saves count up until the server confirms one");
        l.remove("a");
        assert_eq!(l.summaries().len(), 1);
        assert_eq!(Library::from_json("garbage"), Library::default());
    }

    #[test]
    fn own_doctrines_keep_with_the_library_and_older_saves_still_read() {
        let mut l = Library::default();
        l.put_doctrine("d", "Mine", "{\"id\":\"d\"}", 5);
        l.put_doctrine("d", "Mine", "{\"id\":\"d\",\"name\":\"x\"}", 6);
        assert_eq!(l.doctrines["d"].rev, 2);
        assert_eq!(Library::from_json(&l.to_json()).doctrine_docs(), vec!["{\"id\":\"d\",\"name\":\"x\"}".to_owned()]);
        assert!(Library::from_json("{\"fits\":{},\"fleets\":{}}").doctrines.is_empty());
    }
}
