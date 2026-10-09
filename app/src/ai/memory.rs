//! What the assistant remembers between conversations: facts about the user's setup, their
//! preferences, pilots and places worth knowing. The model adds and changes them with tools; the
//! user sees, edits and deletes them in the Assistant tab. Kept in the database, not in settings.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

const KV_KEY: &str = "ai_memories";
/// More than this and the oldest the user did not write go first.
pub const MAX: usize = 200;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemKind {
    /// How the user plays: home, corp, ships, routes, what they care about.
    #[default]
    About,
    /// How they want answers.
    Preference,
    /// A hostile or friendly pilot or group.
    Pilot,
    /// A system, structure or route.
    Place,
    Other,
    #[serde(other)]
    Unknown,
}

impl MemKind {
    pub const CHOICES: [MemKind; 5] = [MemKind::About, MemKind::Preference, MemKind::Pilot, MemKind::Place, MemKind::Other];

    pub fn label(self) -> &'static str {
        match self {
            MemKind::About => "About you",
            MemKind::Preference => "Preferences",
            MemKind::Pilot => "Pilots and groups",
            MemKind::Place => "Places",
            MemKind::Other | MemKind::Unknown => "Other",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            MemKind::About => "about",
            MemKind::Preference => "preference",
            MemKind::Pilot => "pilot",
            MemKind::Place => "place",
            MemKind::Other | MemKind::Unknown => "other",
        }
    }

    pub fn from_code(s: &str) -> Self {
        match s {
            "about" => MemKind::About,
            "preference" => MemKind::Preference,
            "pilot" => MemKind::Pilot,
            "place" => MemKind::Place,
            _ => MemKind::Other,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    pub id: u64,
    pub kind: MemKind,
    pub text: String,
    pub created: i64,
    pub updated: i64,
    /// Written or last changed by the user rather than the model.
    pub by_user: bool,
}

#[derive(Default)]
pub struct Memories {
    pub list: Vec<Memory>,
    next: u64,
}

pub type SharedMemories = Arc<Mutex<Memories>>;

impl Memories {
    pub fn load(store: Option<&crate::store::Store>) -> Self {
        let list: Vec<Memory> = store.and_then(|s| s.kv_get(KV_KEY)).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        let next = list.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        Self { list, next }
    }

    pub fn save(&self, store: Option<&crate::store::Store>) {
        if let (Some(s), Ok(j)) = (store, serde_json::to_string(&self.list)) {
            s.kv_set(KV_KEY, &j);
        }
    }

    pub fn add(&mut self, kind: MemKind, text: &str, by_user: bool, now: i64) -> u64 {
        let id = self.next;
        self.next += 1;
        self.list.push(Memory { id, kind, text: text.trim().to_owned(), created: now, updated: now, by_user });
        if self.list.len() > MAX {
            if let Some(i) = self.list.iter().position(|m| !m.by_user) {
                self.list.remove(i);
            }
        }
        id
    }

    pub fn update(&mut self, id: u64, kind: Option<MemKind>, text: &str, by_user: bool, now: i64) -> bool {
        match self.list.iter_mut().find(|m| m.id == id) {
            Some(m) => {
                m.text = text.trim().to_owned();
                if let Some(k) = kind {
                    m.kind = k;
                }
                m.updated = now;
                m.by_user |= by_user;
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.list.len();
        self.list.retain(|m| m.id != id);
        self.list.len() != before
    }

    /// The memories as the model reads them, grouped by kind, each with its id for changing it.
    pub fn prompt(&self) -> String {
        if self.list.is_empty() {
            return "You have no memories saved yet.".into();
        }
        let mut s = String::from("What you remember from earlier conversations (id: text):\n");
        for k in MemKind::CHOICES {
            let of: Vec<&Memory> = self.list.iter().filter(|m| m.kind == k || (k == MemKind::Other && m.kind == MemKind::Unknown)).collect();
            if of.is_empty() {
                continue;
            }
            s.push_str(&format!("{}:\n", k.label()));
            for m in of {
                s.push_str(&format!("- {}: {}\n", m.id, m.text));
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memories_add_change_and_go() {
        let mut m = Memories::default();
        let a = m.add(MemKind::About, " Lives in 1DQ1-A ", false, 1);
        let b = m.add(MemKind::Preference, "Short answers", true, 2);
        assert!(m.update(a, Some(MemKind::Place), "Stages in 1DQ1-A", false, 3));
        assert!(m.prompt().contains(&format!("- {a}: Stages in 1DQ1-A")));
        assert!(m.remove(b));
        assert!(!m.remove(b));
        assert_eq!(m.list.len(), 1);
    }

    #[test]
    fn the_cap_drops_the_oldest_model_memory_first() {
        let mut m = Memories::default();
        m.add(MemKind::About, "mine", true, 0);
        for i in 0..MAX {
            m.add(MemKind::Other, &format!("note {i}"), false, i as i64);
        }
        assert_eq!(m.list.len(), MAX);
        assert!(m.list.iter().any(|x| x.text == "mine"), "the user's own stays");
        assert!(!m.list.iter().any(|x| x.text == "note 0"));
    }
}
