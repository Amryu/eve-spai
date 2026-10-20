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
pub struct Library {
    pub fits: BTreeMap<String, Entry>,
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

    pub fn remove(&mut self, id: &str) {
        self.fits.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
