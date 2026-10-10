//! What the assistant's tools read from, off the UI thread: the app's shared state, cloned at spawn,
//! plus [`AiFacts`], the part that lives only on the UI thread and is pushed down when it changes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::perms::{self, Unlocked};

#[derive(Clone)]
pub struct AiDeps {
    pub intel_state: Arc<Mutex<crate::intel::IntelState>>,
    pub player: crate::esi::SharedPlayer,
    pub system_status: crate::systemstatus::SharedStatus,
    pub jabber: crate::jabber::SharedJabber,
    pub killfeed: crate::zkill::SharedKillFeed,
    pub battles: crate::zkill::SharedBattles,
    pub camps: crate::camp::SharedCamps,
    pub rescue: Arc<Mutex<crate::rescue::RescueState>>,
    pub fleet: Arc<Mutex<crate::fleets::FleetState>>,
    pub lookup_table: crate::localscan::SharedTable,
    pub facts: Arc<Mutex<AiFacts>>,
    pub memories: crate::ai::memory::SharedMemories,
    pub watches: crate::ai::watch::SharedWatches,
    pub feeds: crate::ai::feeds::SharedFeeds,
    /// Set once the conversation has read Jabber, rescue or feed messages, which must not leave
    /// the app: from then on nothing may reach the web. A new chat clears it.
    pub opsec: Arc<std::sync::atomic::AtomicBool>,
    /// The user's latest question, for safeguards that depend on what was actually asked.
    pub last_question: Arc<Mutex<String>>,
    /// Whether tools may reach ESI and the web. Off in tests.
    pub online: bool,
}

/// UI-thread state the tools need, pushed whenever it changes.
#[derive(Clone, Default)]
pub struct AiFacts {
    pub systems: Option<Arc<crate::geo::Systems>>,
    pub perms: BTreeMap<String, bool>,
    pub unlocked: Unlocked,
    pub chat_dir: Option<std::path::PathBuf>,
    pub severity: crate::settings::SeverityRules,
    pub cyno_generators: Vec<i64>,
    pub jump_bridges: Vec<crate::settings::JumpBridge>,
    pub sov_upgrades: Vec<crate::settings::SovUpgrade>,
    pub fleet_presets: Vec<crate::settings::FleetPreset>,
    /// The fleet dashboard, while it is unlocked. Tools only ever read through it.
    pub fleet_backend: Option<Arc<dyn crate::fleets::backend::FleetBackend>>,
    pub notes_view: Option<Arc<crate::notes::NotesView>>,
    /// The assistant's own settings: provider, model, caps.
    pub ai: crate::ai::config::AiSettings,
    /// Popped-out chat windows by id, with the conversations in each.
    pub chat_windows: Vec<(u64, Vec<String>)>,
    /// The server of the user's own Jabber address, for addressing someone not met yet.
    pub jabber_domain: String,
    /// The conversation has read opsec data (see [`AiDeps::opsec`]); set by the session per request.
    pub opsec: bool,
}

impl AiFacts {
    pub fn allowed(&self, key: &str) -> bool {
        perms::allowed(&self.perms, key, self.unlocked)
    }
}

impl AiDeps {
    pub fn facts(&self) -> AiFacts {
        self.facts.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    #[cfg(test)]
    pub fn for_tests(facts: AiFacts) -> Self {
        Self {
            intel_state: Default::default(),
            player: Default::default(),
            system_status: Default::default(),
            jabber: Default::default(),
            killfeed: Default::default(),
            battles: Default::default(),
            camps: Default::default(),
            rescue: Default::default(),
            fleet: Default::default(),
            lookup_table: Default::default(),
            facts: Arc::new(Mutex::new(facts)),
            memories: Default::default(),
            watches: Default::default(),
            feeds: Default::default(),
            opsec: Default::default(),
            last_question: Default::default(),
            online: false,
        }
    }
}

/// A system by name, case-insensitively, or by a unique prefix ("1dq" for 1DQ1-A).
pub fn resolve_system(geo: &crate::geo::Systems, name: &str) -> Option<i64> {
    let n = name.trim();
    if n.is_empty() {
        return None;
    }
    if let Some(i) = geo.lookup(n) {
        return Some(i.id);
    }
    let hits = geo.search(n, 2);
    (hits.len() == 1).then(|| hits[0].id)
}
