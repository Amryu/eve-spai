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
    /// The alerts the app raised: when and what.
    pub alerts: Arc<Mutex<Vec<(i64, String)>>>,
    /// The Battles page's history, clustered from the stored kills, and whether it is being built.
    pub battle_history: crate::zkill::SharedBattles,
    pub battle_history_loading: Arc<std::sync::atomic::AtomicBool>,
    /// Set by the assistant when it needs the history and the page has not built it: the app
    /// builds it the way the page does.
    pub want_battle_history: Arc<std::sync::atomic::AtomicBool>,
    /// Jump Drive Calibration and Jump Fuel Conservation of the active character, when read.
    pub jump_skills: crate::esi::SharedJumpSkills,
    /// Standings from the user's contacts, by character, corporation or alliance id.
    pub standings: Arc<Mutex<std::collections::HashMap<i64, f32>>>,
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
    pub alert_rules: Vec<crate::settings::AlertRule>,
    pub setup: Setup,
    pub doctrines: Vec<DoctrineFacts>,
    pub route_anchors: Vec<i64>,
    pub route_destination: Option<i64>,
    /// The comms channels the app knows: (name, mumble:// link when known, gnf.lt link when known).
    pub comms: Vec<(String, Option<String>, Option<String>)>,
    /// The d-scan last opened: its link and (ship, count).
    pub last_dscan: Option<(String, Vec<(String, u32)>)>,
    pub coalitions: Vec<crate::settings::Coalition>,
    /// The names in the local being looked up now.
    pub lookup_current: Vec<String>,
    /// Popped-out chat windows by id, with the conversations in each.
    pub chat_windows: Vec<(u64, Vec<String>)>,
    /// The server of the user's own Jabber address, for addressing someone not met yet.
    pub jabber_domain: String,
    /// The conversation has read opsec data (see [`AiDeps::opsec`]); set by the session per request.
    pub opsec: bool,
}

/// A doctrine as the user set it up for fleets.
#[derive(Clone, Default)]
pub struct DoctrineFacts {
    pub name: String,
    pub main: Vec<String>,
    pub support: Vec<String>,
    pub tank: Option<String>,
    pub url: Option<String>,
    pub line: Option<String>,
    /// Only its own hulls belong in it.
    pub strict: bool,
    /// Boosts it wants: (charge, priority).
    pub boosts: Vec<(String, String)>,
}

/// The parts of the user's setup the assistant reads.
#[derive(Clone, Default)]
pub struct Setup {
    pub staging: String,
    pub capital: String,
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    pub avoid_sov: Vec<String>,
    pub sec: [bool; 3],
    pub routes: Vec<crate::settings::SavedMapRoute>,
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
            alerts: Default::default(),
            standings: Default::default(),
            jump_skills: Default::default(),
            battle_history: Default::default(),
            battle_history_loading: Default::default(),
            want_battle_history: Default::default(),
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
