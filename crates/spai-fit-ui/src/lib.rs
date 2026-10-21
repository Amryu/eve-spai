//! The fitting simulator's screens: a fit being edited, what it computes to, the pilot's skills
//! and the library of saved fits. No storage and no network of its own: the host (the desktop app
//! or the web page) answers [`HostRequest`]s with [`HostEvent`]s.

use std::sync::Arc;

use spai_fit::calc::{Calc, Env};
use spai_fit::fit::{Fit, Slot};
use spai_fit::history::{EditKind, History};
use spai_fit::ids::TypeId;
use spai_fit::skills::{MissingSkill, SkillSet};
use spai_fit::stats::Computed;
use spai_fit::warnings::Warning;
use spai_fit::Dogma;
use spai_ui::{tr, trf};

pub mod browser;
pub mod details;
pub mod dock;
pub mod editor;
pub mod fleet_panel;
pub mod graphs_panel;
pub mod icons;
pub mod library_panel;
pub mod picker;
pub mod plot;
pub mod rows;
pub mod stats_panel;
pub mod text;
pub mod variations_panel;
pub mod warnings_panel;

pub use browser::{drop_on, quick_add, DropTarget};
pub use dock::DockTab;
pub use fleet_panel::{FleetSession, FleetSummary};
pub use picker::{PickTarget, Picker};
pub use rows::DragItem;

/// A pilot's skills to compute with.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub skills: SkillSet,
}

impl Profile {
    pub fn all_v() -> Self {
        Profile { id: "all-v".into(), name: tr!("All skills V").to_owned(), skills: SkillSet::all_v() }
    }
}

/// A saved fit as the library lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct FitSummary {
    pub id: String,
    pub name: String,
    pub ship: TypeId,
    pub updated_at: i64,
}

/// Whose preset a doctrine is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctrineSide {
    Imperium,
    Enemy,
    Public,
}

impl DoctrineSide {
    pub const ALL: [DoctrineSide; 3] = [DoctrineSide::Imperium, DoctrineSide::Enemy, DoctrineSide::Public];

    pub fn name(self) -> &'static str {
        match self {
            DoctrineSide::Imperium => tr!("Imperium"),
            DoctrineSide::Enemy => tr!("Enemy"),
            DoctrineSide::Public => tr!("Public"),
        }
    }
}

/// A doctrine preset as the server lists it, its fit document included.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Doctrine {
    pub id: String,
    pub side: DoctrineSide,
    pub name: String,
    #[serde(rename = "ship_type_id")]
    pub ship: TypeId,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub refit_keys: Vec<String>,
    #[serde(default)]
    pub rev: i64,
    pub doc: serde_json::Value,
}

/// What the screens ask the host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum HostRequest {
    LoadDogma,
    ListFits,
    LoadFit(String),
    SaveFit { id: String, name: String, ship: TypeId, doc: String },
    DeleteFit(String),
    FetchSkills,
    CopyText(String),
    /// The clipboard's text, for the Import button; answered with `HostEvent::Clipboard`.
    ReadClipboard,
    /// The doctrine presets and what the signed-in character may do with them.
    FetchDoctrines,
    PublishDoctrine { id: String, side: DoctrineSide, name: String, tags: Vec<String>, refit_keys: Vec<String>, doc: String },
    DeleteDoctrine(String),
    /// Push the library's changes to the server and take its newer entries.
    SyncLibrary,
    /// The documents of these saved fits, for a fleet's numbers.
    FitDocs(Vec<String>),
    ListFleets,
    LoadFleet(String),
    /// A fleet's document without opening it: the one compared against.
    LoadFleetDoc(String),
    SaveFleet { id: String, name: String, doc: String },
    DeleteFleet(String),
}

/// What the host tells the screens.
#[derive(Clone, Debug)]
pub enum HostEvent {
    Dogma(Arc<Dogma>),
    DogmaFailed(String),
    Fits(Vec<FitSummary>),
    Fit { id: String, doc: String },
    Profiles(Vec<Profile>),
    Saved(String),
    Status(String),
    Clipboard(String),
    Doctrines(Vec<Doctrine>),
    /// Whether the character is signed in with the server, in the Imperium, and a doctrine admin.
    Access { signed_in: bool, imperium: bool, admin: bool },
    FitDocs(Vec<(String, String)>),
    Fleets(Vec<FleetSummary>),
    Fleet { id: String, doc: String },
    FleetDoc { id: String, doc: String },
    FleetSaved(String),
}

/// The fit on screen with its undo history and what it computes to.
pub struct Session {
    pub id: String,
    pub fit: Fit,
    pub history: History<Fit>,
    rev: u64,
    /// What `computed` was figured for: the fit's revision, the skills profile, the fits it is
    /// projected by.
    computed_for: Option<(u64, usize, u64)>,
    pub computed: Option<Arc<Computed>>,
    pub warnings: Vec<Warning>,
    pub missing: Vec<MissingSkill>,
    /// Each fitted module's own draw and damage, for the table's columns.
    pub module_stats: Vec<spai_fit::stats::ModuleStat>,
    pub dirty: bool,
    /// When the fit last changed, so the autosave waits for a pause in the editing.
    pub edited_at: u64,
    /// A save of the current change has gone to the host and has not come back yet.
    pub save_sent: bool,
}

/// How long a fit sits unchanged before it is saved.
pub const AUTOSAVE_MS: u64 = 1500;

impl Session {
    /// Bumped by every change, so caches keyed on it know when to rebuild.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn new(id: String, fit: Fit) -> Self {
        Session { id, fit, history: History::default(), rev: 0, computed_for: None, computed: None, warnings: Vec::new(), missing: Vec::new(), module_stats: Vec::new(), dirty: false, edited_at: 0, save_sent: false }
    }

    fn refresh(&mut self, dogma: &Dogma, profile: usize, skills: &SkillSet, outside: &Outside) {
        let key = outside.key(&self.fit);
        if self.computed_for == Some((self.rev, profile, key)) {
            return;
        }
        let (resolved, env) = outside.apply(dogma, &self.fit, skills);
        let calc = Calc::new(dogma, &resolved, skills, &env);
        let computed = spai_fit::stats::compute_with(&calc, &resolved);
        self.missing = spai_fit::skills::missing(&resolved, dogma, skills);
        let mut warnings = spai_fit::warnings::check(dogma, &resolved, &computed, &self.missing);
        warnings.extend(spai_fit::warnings::check(dogma, &self.fit, &computed, &[]).into_iter().filter(|w| matches!(w.kind, spai_fit::warnings::WarningKind::VariationConflict { .. })));
        self.warnings = warnings;
        self.module_stats = spai_fit::stats::module_stats(&calc, &computed);
        self.computed = Some(Arc::new(computed));
        self.computed_for = Some((self.rev, profile, key));
    }
}

/// The other fits a fit can be projected by, by library id: the saved ones and the open tabs.
pub struct Outside {
    fits: std::collections::HashMap<String, Fit>,
}

impl Outside {
    /// The fit with its variations applied and the fits projected onto it folded in, and the
    /// boosts those give.
    pub fn apply(&self, dogma: &Dogma, fit: &Fit, skills: &SkillSet) -> (Fit, Env) {
        let resolved = spai_fit::variations::resolve(dogma, fit);
        spai_fit::projection::expand(dogma, &resolved, &|id| self.fits.get(id).cloned(), skills)
    }

    /// A fit by library id.
    pub fn fit(&self, id: &str) -> Option<&Fit> {
        self.fits.get(id)
    }

    /// Changes when a fit projected onto `fit` changes, so its numbers are figured again.
    fn key(&self, fit: &Fit) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for pf in &fit.projected_fits {
            pf.fit_id.hash(&mut h);
            self.fits.get(&pf.fit_id).map(spai_fit::doc::to_json).hash(&mut h);
        }
        h.finish()
    }
}

/// Which column a narrow window shows; a wide one shows them side by side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Library,
    Browse,
    Fit,
    Stats,
    Graphs,
}

/// What the middle shows in a wide window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Center {
    Fit,
    Graphs,
}

/// What the left column shows: saved fits or the module browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeftTab {
    Fits,
    Doctrines,
    Browse,
}

/// Fits or fleets: what the screens are about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fits,
    Fleets,
}

/// Below this width the stats and warnings become tabs over the fit instead of a column beside it.
pub const NARROW: f32 = 1000.0;
const SIDE_W: f32 = 270.0;
const STATS_W: f32 = 300.0;

#[derive(Default)]
pub struct ImportDialog {
    pub text: String,
    pub error: Option<String>,
}

/// An imported fit close to one already here: the same hull with most of the rack in common.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportAsk {
    pub fit: Fit,
    pub base_id: String,
    pub base_name: String,
}

/// A fit is offered as a variation of another when this much of the rack is shared.
pub const VARIATION_LIKENESS: f64 = 0.6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Access {
    pub signed_in: bool,
    pub imperium: bool,
    pub admin: bool,
}

/// Publishing the open fit as a doctrine preset.
pub struct PublishDialog {
    pub id: String,
    pub name: String,
    pub side: DoctrineSide,
    pub tags: String,
}

pub struct SimUi {
    pub dogma: Option<Arc<Dogma>>,
    pub dogma_error: Option<String>,
    dogma_requested: bool,
    sessions: Vec<Session>,
    active: usize,
    pub library: Vec<FitSummary>,
    pub profiles: Vec<Profile>,
    pub profile: usize,
    pub picker: Option<Picker>,
    pub pane: Pane,
    pub center: Center,
    pub left: LeftTab,
    pub show_library: bool,
    pub browser: browser::Browser,
    pub library_state: library_panel::LibraryState,
    /// The window that picks a fit to project, with its own grouping and search.
    pub project_picker: Option<library_panel::LibraryState>,
    pub dock: dock::DockState,
    pub warnings_expanded: bool,
    /// The modules picked in the racks, for a menu that acts on all of them at once.
    pub selection: Vec<(Slot, u8)>,
    /// Where a shift-click's range starts.
    pub sel_anchor: Option<(Slot, u8)>,
    pub details: Vec<details::DetailsWindow>,
    /// The booster whose side effects are being picked.
    pub side_window: Option<TypeId>,
    /// The width the user dragged the left column to; read from egui's memory on first use.
    left_w: Option<f32>,
    pub skills_window: bool,
    pub traits_window: bool,
    pub graph_window: bool,
    pub import: Option<ImportDialog>,
    /// An imported fit that looks like a variation of a fit already here, waiting for the answer.
    pub import_ask: Option<ImportAsk>,
    /// Set by the Import button; the clipboard's text goes through the dialog when it is not a fit.
    import_from_button: bool,
    /// The clock of the last frame, for what the host answers between frames.
    now_ms: u64,
    pub graph: spai_fit::graphs::GraphKind,
    pub graph_drones: bool,
    /// Other open fits drawn beside the one on screen, by session id.
    pub graph_attackers: Vec<String>,
    /// Open fits taken as targets, by session id: their signature and speed stand in for the
    /// target profile. Empty is the profile alone.
    pub graph_targets: Vec<String>,
    /// The fit as it was when a variation recording started.
    pub recording: Option<Fit>,
    pub editing_variation: Option<u32>,
    pub doctrines: Vec<Doctrine>,
    pub access: Access,
    pub publish: Option<PublishDialog>,
    pub mode: Mode,
    pub fleets: Vec<FleetSummary>,
    pub fleet: Option<FleetSession>,
    /// Fit documents by library id, as fleets need them.
    pub fit_docs: std::collections::HashMap<String, Fit>,
    fit_docs_asked: std::collections::HashSet<String>,
    /// Fleets by library id, for the one compared against.
    pub fleet_docs: std::collections::HashMap<String, spai_fit::fleet::Fleet>,
    status: Option<(String, f64)>,
    requests: Vec<HostRequest>,
    next_id: u32,
}

impl Default for SimUi {
    fn default() -> Self {
        SimUi {
            dogma: None,
            dogma_error: None,
            dogma_requested: false,
            sessions: Vec::new(),
            active: 0,
            library: Vec::new(),
            profiles: vec![Profile::all_v()],
            profile: 0,
            picker: None,
            pane: Pane::Fit,
            center: Center::Fit,
            left: LeftTab::Browse,
            show_library: true,
            browser: browser::Browser::default(),
            library_state: Default::default(),
            project_picker: None,
            dock: dock::DockState::default(),
            warnings_expanded: false,
            selection: Vec::new(),
            sel_anchor: None,
            details: Vec::new(),
            side_window: None,
            left_w: None,
            skills_window: false,
            traits_window: false,
            graph_window: false,
            import: None,
            import_ask: None,
            import_from_button: false,
            now_ms: 0,
            graph: spai_fit::graphs::GraphKind::DamageVsDistance,
            graph_drones: true,
            graph_attackers: Vec::new(),
            graph_targets: Vec::new(),
            recording: None,
            editing_variation: None,
            doctrines: Vec::new(),
            access: Access::default(),
            publish: None,
            mode: Mode::Fits,
            fleets: Vec::new(),
            fleet: None,
            fit_docs: std::collections::HashMap::new(),
            fit_docs_asked: std::collections::HashSet::new(),
            fleet_docs: std::collections::HashMap::new(),
            status: None,
            requests: Vec::new(),
            next_id: 0,
        }
    }
}

impl SimUi {
    /// What the host should do now; emptied by the call.
    pub fn take_requests(&mut self) -> Vec<HostRequest> {
        std::mem::take(&mut self.requests)
    }

    pub fn handle(&mut self, event: HostEvent) {
        match event {
            HostEvent::Dogma(d) => {
                self.dogma = Some(d);
                self.dogma_error = None;
                for s in &mut self.sessions {
                    s.computed_for = None;
                }
            }
            HostEvent::DogmaFailed(e) => {
                self.dogma_error = Some(e);
                self.dogma_requested = false;
            }
            HostEvent::Fits(list) => {
                self.library = list;
                let ids: Vec<String> = self.library.iter().map(|e| e.id.clone()).collect();
                self.want_fit_docs(ids.into_iter());
            }
            HostEvent::Fit { id, doc } => match spai_fit::doc::from_json(&doc) {
                Ok(fit) => self.open_session(Session::new(id, fit)),
                Err(e) => self.status(trf!("Could not read the fit: {e}", e = e)),
            },
            HostEvent::Profiles(p) => {
                let current = self.profiles.get(self.profile).map(|p| p.id.clone());
                self.profiles = std::iter::once(Profile::all_v()).chain(p).collect();
                self.profile = current.and_then(|id| self.profiles.iter().position(|p| p.id == id)).unwrap_or(0);
                for s in &mut self.sessions {
                    s.computed_for = None;
                }
            }
            HostEvent::Saved(id) => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
                    s.dirty = false;
                    s.save_sent = false;
                    self.fit_docs.insert(id.clone(), s.fit.clone());
                }
                self.requests.push(HostRequest::ListFits);
                self.status(tr!("Saved").to_owned());
            }
            HostEvent::Status(s) => self.status(s),
            HostEvent::Clipboard(text) => self.import_clipboard(&text),
            HostEvent::Doctrines(d) => {
                for doc in &d {
                    if let Ok(fit) = spai_fit::doc::from_json(&doc.doc.to_string()) {
                        self.fit_docs.insert(doc.id.clone(), fit);
                    }
                }
                self.doctrines = d;
            }
            HostEvent::Access { signed_in, imperium, admin } => self.access = Access { signed_in, imperium, admin },
            HostEvent::FitDocs(docs) => {
                for (id, doc) in docs {
                    if let Ok(fit) = spai_fit::doc::from_json(&doc) {
                        self.fit_docs.insert(id, fit);
                    }
                }
            }
            HostEvent::Fleets(list) => self.fleets = list,
            HostEvent::Fleet { id, doc } => match serde_json::from_str::<spai_fit::fleet::Fleet>(&doc) {
                Ok(fleet) => {
                    self.fleet_docs.insert(id.clone(), fleet.clone());
                    self.fleet = Some(FleetSession { id, fleet, dirty: false, enemy: self.fleet.as_ref().and_then(|f| f.enemy.clone()) });
                }
                Err(e) => self.status(trf!("Could not read the fleet: {e}", e = e)),
            },
            HostEvent::FleetDoc { id, doc } => {
                if let Ok(fleet) = serde_json::from_str::<spai_fit::fleet::Fleet>(&doc) {
                    self.fleet_docs.insert(id, fleet);
                }
            }
            HostEvent::FleetSaved(id) => {
                if let Some(f) = &mut self.fleet {
                    if f.id == id {
                        f.dirty = false;
                        self.fleet_docs.insert(id, f.fleet.clone());
                    }
                }
                self.requests.push(HostRequest::ListFleets);
                self.status(tr!("Saved").to_owned());
            }
        }
    }

    /// A saved fit changed: its document here is stale.
    pub fn forget_fit_doc(&mut self, id: &str) {
        self.fit_docs.remove(id);
        self.fit_docs_asked.remove(id);
    }

    /// Opens a copy of the doctrine as a new fit of the library.
    pub fn open_doctrine(&mut self, id: &str, now_ms: u64) {
        let Some(d) = self.doctrines.iter().find(|d| d.id == id) else { return };
        match spai_fit::doc::from_json(&d.doc.to_string()) {
            Ok(mut fit) => {
                fit.name = d.name.clone();
                self.open_fit(fit, now_ms);
                // A copy to look at is not a library entry; the first edit makes it one.
                if let Some(s) = self.session_mut() {
                    s.dirty = false;
                }
            }
            Err(e) => self.status(trf!("Could not read the fit: {e}", e = e)),
        }
    }

    /// Starts publishing the open fit; the dialog takes the name, side and tags.
    pub fn start_publish(&mut self) {
        let Some(s) = self.session() else { return };
        let existing = self.doctrines.iter().find(|d| d.id == s.id);
        self.publish = Some(PublishDialog {
            id: s.id.clone(),
            name: s.fit.name.clone(),
            side: existing.map_or(DoctrineSide::Imperium, |d| d.side),
            tags: existing.map(|d| d.tags.join(", ")).unwrap_or_default(),
        });
    }

    fn publish_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.publish else { return };
        let mut open = true;
        let mut go = false;
        let size = egui::vec2(420.0, 220.0);
        egui::Window::new(tr!("Publish as doctrine")).open(&mut open).collapsible(false).resizable(false).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
            use spai_ui::widgets::SteadySelect as _;
            egui::Grid::new("sim_publish").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label(tr!("Name"));
                ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(280.0));
                ui.end_row();
                ui.label(tr!("Side"));
                egui::ComboBox::from_id_salt("sim_publish_side").selected_text(dialog.side.name()).width(160.0).show_ui(ui, |ui| {
                    for side in DoctrineSide::ALL {
                        ui.menu_value(&mut dialog.side, side, side.name());
                    }
                });
                ui.end_row();
                ui.label(tr!("Tags"));
                ui.add(egui::TextEdit::singleline(&mut dialog.tags).desired_width(280.0).hint_text(tr!("comma separated")));
                ui.end_row();
            });
            ui.add(egui::Label::new(egui::RichText::new(tr!("Refit keys come from the fit's variations. Publishing again replaces the preset with the same id.")).weak()).wrap());
            ui.add_space(6.0);
            if ui.add_enabled(!dialog.name.trim().is_empty(), egui::Button::new(tr!("Publish"))).clicked() {
                go = true;
            }
        });
        if go {
            if let (Some(d), Some(s)) = (self.publish.take(), self.session()) {
                let refit_keys: Vec<String> = s.fit.variations.iter().filter_map(|v| v.refit_key.clone()).collect();
                let tags: Vec<String> = d.tags.split(',').map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect();
                let mut fit = s.fit.clone();
                fit.name = d.name.trim().to_owned();
                self.requests.push(HostRequest::PublishDoctrine { id: d.id, side: d.side, name: fit.name.clone(), tags, refit_keys, doc: spai_fit::doc::to_json(&fit) });
            }
        } else if !open {
            self.publish = None;
        }
    }

    fn status(&mut self, text: String) {
        self.status = Some((text, 0.0));
    }

    /// A short note in the top bar.
    pub fn note(&mut self, text: String) {
        self.status(text);
    }

    pub fn skills(&self) -> &SkillSet {
        static ALL_V: std::sync::OnceLock<SkillSet> = std::sync::OnceLock::new();
        self.profiles.get(self.profile).map_or_else(|| ALL_V.get_or_init(SkillSet::all_v), |p| &p.skills)
    }

    fn new_id(&mut self, now_ms: u64) -> String {
        self.next_id += 1;
        format!("{now_ms:x}-{:x}", self.next_id)
    }

    /// Starts a fresh fit of the hull.
    pub fn new_fit(&mut self, ship: TypeId, now_ms: u64) {
        let name = self.dogma.as_ref().and_then(|d| d.type_(ship)).map_or_else(|| tr!("New fit").to_owned(), |t| t.name.clone());
        let id = self.new_id(now_ms);
        self.open_session(Session::new(id, Fit::new(ship, name)));
    }

    /// Opens an imported fit as a new library entry.
    pub fn open_fit(&mut self, fit: Fit, now_ms: u64) {
        let id = self.new_id(now_ms);
        let mut s = Session::new(id, fit);
        s.dirty = true;
        s.edited_at = now_ms;
        self.open_session(s);
    }

    #[cfg(test)]
    pub(crate) fn import_from_button_for_test(&mut self) {
        self.import_from_button = true;
    }

    /// Imports EFT text; `false` when it is not a fit. Unknown names are noted, not refused.
    pub fn import_text(&mut self, text: &str, now_ms: u64) -> bool {
        let Some(dogma) = self.dogma.clone() else { return false };
        let Some(imp) = spai_fit::formats::eft::import(&dogma, text).filter(|i| i.fit.ship != 0) else { return false };
        if !imp.unknown.is_empty() {
            self.status(trf!("Unknown: {names}", names = imp.unknown.join(", ")));
        }
        self.place_import(imp.fit, now_ms);
        true
    }

    /// What the host read off the clipboard for the Import button: a fit is imported, anything
    /// else goes into the paste dialog, which also says what was wrong with it.
    fn import_clipboard(&mut self, text: &str) {
        let from_button = std::mem::take(&mut self.import_from_button);
        if self.import_text(text, self.now_ms) {
            return;
        }
        if from_button {
            let error = if text.trim().is_empty() { None } else { Some(tr!("That is not an EFT fit: the first line must be [Ship, Name].").to_owned()) };
            self.import = Some(ImportDialog { text: text.to_owned(), error });
        }
    }

    /// Where an imported fit goes: to the tab or library fit that already holds the same fitting,
    /// to a question when it is a near copy of one, into an untouched tab, else into a new one.
    pub fn place_import(&mut self, fit: Fit, now_ms: u64) {
        if let Some(i) = self.sessions.iter().position(|s| s.fit.same_fitting(&fit)) {
            self.activate(i);
            self.status(tr!("Already open: the same fit").to_owned());
            return;
        }
        let in_library: Option<(String, String)> = self.library.iter().find(|e| self.fit_docs.get(&e.id).is_some_and(|d| d.same_fitting(&fit))).map(|e| (e.id.clone(), e.name.clone()));
        if let Some((id, name)) = in_library {
            self.requests.push(HostRequest::LoadFit(id));
            self.status(trf!("Already in the library as {name}", name = name));
            return;
        }
        let likeness = |other: &Fit| other.rack_similarity(&fit);
        let open_like = self.sessions.iter().filter(|s| likeness(&s.fit) >= VARIATION_LIKENESS).map(|s| (likeness(&s.fit), s.id.clone(), s.fit.name.clone()));
        let saved_like = self.library.iter().filter_map(|e| self.fit_docs.get(&e.id).map(|d| (likeness(d), e.id.clone(), e.name.clone()))).filter(|(l, _, _)| *l >= VARIATION_LIKENESS);
        if let Some((_, base_id, base_name)) = open_like.chain(saved_like).max_by(|a, b| a.0.total_cmp(&b.0)) {
            self.import_ask = Some(ImportAsk { fit, base_id, base_name });
            return;
        }
        self.open_imported(fit, now_ms);
    }

    /// Into the tab on screen when nothing has been put in it yet, else into a new tab.
    fn open_imported(&mut self, fit: Fit, now_ms: u64) {
        let untouched = self.session().is_some_and(|s| s.fit.ship == 0 || !s.dirty && s.fit.modules.is_empty() && s.fit.drones.is_empty() && s.fit.fighters.is_empty() && s.fit.implants.is_empty() && s.fit.boosters.is_empty() && s.fit.cargo.is_empty());
        if untouched {
            if let Some(s) = self.session_mut() {
                s.fit = fit;
                s.rev += 1;
                s.dirty = true;
                s.save_sent = false;
                s.edited_at = now_ms;
                s.computed_for = None;
            }
            self.selection.clear();
            self.pane = Pane::Fit;
        } else {
            self.open_fit(fit, now_ms);
        }
    }

    /// Adds the fit waiting in `import_ask` to its base fit as a variation holding the difference.
    pub fn import_as_variation(&mut self, now_ms: u64) {
        let Some(ask) = self.import_ask.take() else { return };
        if let Some(i) = self.sessions.iter().position(|s| s.id == ask.base_id) {
            self.activate(i);
        } else if let Some(doc) = self.fit_docs.get(&ask.base_id).cloned() {
            self.open_session(Session::new(ask.base_id.clone(), doc));
        } else {
            return;
        }
        let Some(dogma) = self.dogma.clone() else { return };
        let Some(s) = self.session() else { return };
        let mut plain = s.fit.clone();
        plain.active_variations.clear();
        let imported = ask.fit;
        let ops = spai_fit::variations::diff(&spai_fit::variations::resolve(&dogma, &plain), &imported);
        let id = spai_fit::variations::next_id(&s.fit);
        let kind = variations_panel::guess_kind(&ops);
        let name = if imported.name.trim().is_empty() || imported.name == plain.name { trf!("{kind} {n}", kind = variations_panel::kind_name(kind), n = id) } else { imported.name.clone() };
        self.edit(EditKind::Variation, now_ms, move |f| {
            f.variations.push(spai_fit::variations::Variation { id, name, kind, refit_key: None, ops });
            spai_fit::variations::activate(f, id);
        });
    }

    fn import_ask_window(&mut self, ctx: &egui::Context, now_ms: u64) {
        let Some(ask) = self.import_ask.clone() else { return };
        let mut decided: Option<u8> = None;
        egui::Modal::new(egui::Id::new("sim_import_ask")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(trf!("{name} looks like a variation of {base}", name = ask.fit.name, base = ask.base_name));
            ui.label(egui::RichText::new(tr!("Same hull, most of the rack in common. Keep it as a variation of that fit, or open it as a fit of its own.")).weak());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("Cancel")).clicked() {
                    decided = Some(0);
                }
                if ui.button(tr!("Own fit")).clicked() {
                    decided = Some(1);
                }
                if ui.button(tr!("Add as a variation")).clicked() {
                    decided = Some(2);
                }
            });
        });
        match decided {
            Some(0) => self.import_ask = None,
            Some(1) => {
                self.import_ask = None;
                self.open_imported(ask.fit, now_ms);
            }
            Some(2) => self.import_as_variation(now_ms),
            _ => {}
        }
    }

    /// What an open fit computes to, by session id, refreshed when stale.
    pub fn computed_of(&mut self, id: &str) -> Option<Arc<Computed>> {
        let dogma = self.dogma.clone()?;
        let profile = self.profile;
        let skills = self.skills().clone();
        let outside = self.outside();
        let s = self.sessions.iter_mut().find(|s| s.id == id)?;
        s.refresh(&dogma, profile, &skills, &outside);
        s.computed.clone()
    }

    /// The fit on screen.
    pub fn session(&self) -> Option<&Session> {
        self.sessions.get(self.active)
    }

    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.sessions.get_mut(self.active)
    }

    /// Every open fit, in tab order.
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn active_tab(&self) -> usize {
        self.active
    }

    pub fn activate(&mut self, i: usize) {
        if i < self.sessions.len() {
            self.active = i;
            self.pane = Pane::Fit;
            self.selection.clear();
            self.sel_anchor = None;
        }
    }

    /// Shows the fit in a tab: the one already holding this fit, else a new one at the end.
    pub fn open_session(&mut self, s: Session) {
        match self.sessions.iter().position(|x| x.id == s.id) {
            Some(i) => {
                self.sessions[i] = s;
                self.active = i;
            }
            None => {
                self.sessions.push(s);
                self.active = self.sessions.len() - 1;
            }
        }
        self.selection.clear();
        self.sel_anchor = None;
        self.pane = Pane::Fit;
    }

    /// Closes a tab; what it changed is saved first.
    pub fn close_tab(&mut self, i: usize) {
        if i >= self.sessions.len() {
            return;
        }
        if self.sessions[i].dirty {
            self.save_session(i);
        }
        self.sessions.remove(i);
        if self.active >= self.sessions.len() {
            self.active = self.sessions.len().saturating_sub(1);
        } else if i < self.active {
            self.active -= 1;
        }
    }

    /// Drops the tab holding this fit without saving: the fit is gone from the library.
    pub fn close_id(&mut self, id: &str) {
        if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
            self.sessions.remove(i);
            if self.active >= self.sessions.len() {
                self.active = self.sessions.len().saturating_sub(1);
            } else if i < self.active {
                self.active -= 1;
            }
        }
    }

    /// Hands every fit that has sat unchanged for a moment to the host, once per change.
    pub fn autosave(&mut self, now_ms: u64) {
        let due: Vec<usize> = (0..self.sessions.len()).filter(|&i| { let s = &self.sessions[i]; s.dirty && !s.save_sent && now_ms >= s.edited_at + AUTOSAVE_MS }).collect();
        for i in due {
            self.save_session(i);
        }
    }

    fn save_session(&mut self, i: usize) {
        let s = &mut self.sessions[i];
        s.save_sent = true;
        self.requests.push(HostRequest::SaveFit { id: s.id.clone(), name: s.fit.name.clone(), ship: s.fit.ship, doc: spai_fit::doc::to_json(&s.fit) });
    }

    /// Changes the fit on screen, recording the state before for undo.
    pub fn edit(&mut self, kind: EditKind, now_ms: u64, f: impl FnOnce(&mut Fit)) {
        let Some(s) = self.session_mut() else { return };
        let before = s.fit.clone();
        f(&mut s.fit);
        if s.fit != before {
            s.history.record(kind, &before, now_ms);
            s.rev += 1;
            s.dirty = true;
            s.save_sent = false;
            s.edited_at = now_ms;
        }
    }

    pub fn undo(&mut self) {
        if let Some(s) = self.session_mut() {
            if let Some(fit) = s.history.undo(&s.fit) {
                s.fit = fit;
                s.rev += 1;
                s.dirty = true;
                s.save_sent = false;
            }
        }
    }

    pub fn redo(&mut self) {
        if let Some(s) = self.session_mut() {
            if let Some(fit) = s.history.redo(&s.fit) {
                s.fit = fit;
                s.rev += 1;
                s.dirty = true;
                s.save_sent = false;
            }
        }
    }

    pub fn save(&mut self) {
        if self.session().is_some() {
            self.save_session(self.active);
        }
    }

    pub fn request(&mut self, r: HostRequest) {
        self.requests.push(r);
    }

    /// A tab with no hull yet, waiting for one.
    pub fn new_empty_tab(&mut self, now_ms: u64) {
        let id = self.new_id(now_ms);
        self.open_session(Session::new(id, Fit::new(0, tr!("New fit"))));
    }

    /// The tab on screen has a hull to show.
    pub fn has_hull(&self) -> bool {
        self.session().is_some_and(|s| s.fit.ship != 0)
    }

    /// Recomputes what is stale and returns the current numbers.
    pub fn computed(&mut self) -> Option<Arc<Computed>> {
        if !self.has_hull() {
            return None;
        }
        let dogma = self.dogma.clone()?;
        let profile = self.profile;
        let skills = self.skills().clone();
        let outside = self.outside();
        let s = self.session_mut()?;
        s.refresh(&dogma, profile, &skills, &outside);
        s.computed.clone()
    }

    /// Every fit that can be projected onto another: the library's, then the open tabs over them.
    pub fn outside(&self) -> Outside {
        let mut fits = self.fit_docs.clone();
        for s in &self.sessions {
            fits.insert(s.id.clone(), s.fit.clone());
        }
        Outside { fits }
    }

    /// What the capacitor graph needs beyond the computed numbers.
    pub fn cap_inputs(&self) -> Option<spai_fit::graphs::CapInputs> {
        let (dogma, s) = (self.dogma.as_ref()?, self.session()?);
        let (resolved, env) = self.outside().apply(dogma, &s.fit, self.skills());
        let calc = Calc::new(dogma, &resolved, self.skills(), &env);
        Some(spai_fit::graphs::CapInputs { users: spai_fit::stats::cap_users(&calc) })
    }

    /// The whole simulator, in the space given.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if self.dogma.is_none() {
            self.loading(ui);
            return;
        }
        let now_ms = (ui.input(|i| i.time) * 1000.0) as u64;
        self.now_ms = now_ms;
        rows::clear_log(ui.ctx());
        // Ctrl+V with no text box focused imports what was pasted.
        if self.mode == Mode::Fits && ui.ctx().memory(|m| m.focused().is_none()) {
            let pasted: Option<String> = ui.input(|i| i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None }));
            if let Some(text) = pasted {
                if !self.import_text(&text, now_ms) {
                    self.status(tr!("The clipboard holds no EFT fit").to_owned());
                }
            }
        }
        self.autosave(now_ms);
        self.computed();
        let narrow = ui.available_width() < NARROW;
        self.top_bar(ui, now_ms, narrow);
        if self.mode == Mode::Fleets {
            if !narrow {
                self.left_panel(ui, |sim, ui| fleet_panel::library(sim, ui, now_ms));
                egui::Panel::right("sim_fleet_results").resizable(false).exact_size(STATS_W + 40.0).show_inside(ui, |ui| {
                    fleet_panel::results(self, ui);
                });
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
                if narrow {
                    ui.horizontal(|ui| {
                        use spai_ui::widgets::SteadySelect as _;
                        let tabs = [(Pane::Library, tr!("Fleets")), (Pane::Fit, tr!("Fleet")), (Pane::Stats, tr!("Results"))];
                        let w = egui::vec2(((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(40.0), 24.0);
                        for (pane, label) in tabs {
                            if ui.menu_label_sized(w, self.pane == pane, label).clicked() {
                                self.pane = pane;
                            }
                        }
                    });
                    ui.separator();
                }
                match (narrow, self.pane) {
                    (true, Pane::Library) => fleet_panel::library(self, ui, now_ms),
                    (true, Pane::Stats | Pane::Graphs | Pane::Browse) => fleet_panel::results(self, ui),
                    _ => fleet_panel::editor(self, ui, now_ms),
                }
            });
            self.show_dialogs(ui.ctx(), now_ms);
            return;
        }
        if self.show_library && !narrow {
            self.left_panel(ui, |sim, ui| {
                sim.left_tabs(ui);
                match sim.left {
                    LeftTab::Fits => library_panel::show(sim, ui, now_ms),
                    LeftTab::Doctrines => library_panel::doctrines_tab(sim, ui, now_ms),
                    LeftTab::Browse => browser::show(sim, ui, now_ms),
                }
            });
        }
        if !narrow && self.center == Center::Fit {
            egui::Panel::right("sim_stats").resizable(false).exact_size(STATS_W).show_inside(ui, |ui| {
                stats_panel::show(self, ui);
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            if narrow {
                // Everything becomes a tab, so nothing is ever wider than the window.
                ui.horizontal(|ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    let tabs = [(Pane::Library, tr!("Fits")), (Pane::Browse, tr!("Browse")), (Pane::Fit, tr!("Fit")), (Pane::Stats, tr!("Stats")), (Pane::Graphs, tr!("Graphs"))];
                    let w = egui::vec2(((ui.available_width() - 4.0 * ui.spacing().item_spacing.x) / 5.0).max(40.0), 24.0);
                    for (pane, label) in tabs {
                        if ui.menu_label_sized(w, self.pane == pane, label).clicked() {
                            self.pane = pane;
                        }
                    }
                });
                ui.separator();
            }
            match (narrow, self.pane, self.center) {
                (true, Pane::Library, _) => library_panel::show(self, ui, now_ms),
                (true, Pane::Browse, _) => browser::show(self, ui, now_ms),
                (true, Pane::Stats, _) => stats_panel::show(self, ui),
                (true, Pane::Graphs, _) | (false, _, Center::Graphs) => graphs_panel::show(self, ui, now_ms),
                _ => editor::show(self, ui, now_ms),
            }
        });
        self.show_dialogs(ui.ctx(), now_ms);
    }

    /// The picker and the dialogs, floating over whatever is below.
    pub fn show_dialogs(&mut self, ctx: &egui::Context, now_ms: u64) {
        picker::show_window(self, ctx, now_ms);
        self.import_window(ctx, now_ms);
        self.publish_window(ctx);
        warnings_panel::skills_window(self, ctx);
        graphs_panel::window(self, ctx, now_ms);
        details::windows(self, ctx);
        details::side_effects_window(self, ctx, now_ms);
        library_panel::project_picker(self, ctx, now_ms);
        self.import_ask_window(ctx, now_ms);
        rows::drag_ghost(ctx);
    }

    /// The left column, as wide as the user dragged it and no wider: egui lets content that
    /// overflows widen a panel, and a column that moves on its own is worse than one that clips.
    fn left_panel(&mut self, ui: &mut egui::Ui, body: impl FnOnce(&mut Self, &mut egui::Ui)) {
        const MIN_W: f32 = 200.0;
        const MAX_W: f32 = 600.0;
        let id = egui::Id::new("sim_left");
        let ctx = ui.ctx().clone();
        let stored = egui::PanelState::load(&ctx, id).map(|st| st.rect.width());
        let w = *self.left_w.get_or_insert_with(|| stored.unwrap_or(SIDE_W).clamp(MIN_W, MAX_W));
        let dragging = ctx.is_being_dragged(id.with("__resize"));
        if !dragging && stored.is_some_and(|s| (s - w).abs() > 0.5) {
            if let Some(mut st) = egui::PanelState::load(&ctx, id) {
                st.rect.set_width(w);
                ctx.data_mut(|d| d.insert_persisted(id, st));
            }
        }
        egui::Panel::left(id).resizable(true).default_size(w).min_size(MIN_W).max_size(MAX_W).show_inside(ui, |ui| body(self, ui));
        if dragging {
            if let Some(st) = egui::PanelState::load(&ctx, id) {
                self.left_w = Some(st.rect.width().clamp(MIN_W, MAX_W));
            }
        }
    }

    fn left_tabs(&mut self, ui: &mut egui::Ui) {
        use spai_ui::widgets::SteadySelect as _;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(rows::GAP);
            // Three tabs share what the column has, no wider than their words need.
            let w = egui::vec2(((ui.available_width() - rows::GAP - 2.0 * ui.spacing().item_spacing.x) / 3.0).min(90.0), 22.0);
            for (tab, label) in [(LeftTab::Fits, tr!("Fits")), (LeftTab::Doctrines, tr!("Doctrines")), (LeftTab::Browse, tr!("Browse"))] {
                if ui.menu_label_sized(w, self.left == tab, label).clicked() {
                    self.left = tab;
                }
            }
        });
        ui.separator();
    }

    fn loading(&mut self, ui: &mut egui::Ui) {
        if !self.dogma_requested && self.dogma_error.is_none() {
            self.dogma_requested = true;
            self.requests.push(HostRequest::LoadDogma);
        }
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            match &self.dogma_error {
                Some(e) => {
                    ui.label(trf!("The fitting data could not be loaded: {e}", e = e));
                    if ui.button(tr!("Try again")).clicked() {
                        self.dogma_error = None;
                    }
                }
                None => {
                    ui.spinner();
                    ui.label(tr!("Loading fitting data…"));
                }
            }
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, _now_ms: u64, narrow: bool) {
        use egui_phosphor::regular as icon;
        use spai_ui::widgets::icon_button;
        let frame = egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin { left: 8, right: 8, top: 6, bottom: 6 });
        egui::Panel::top("sim_top").resizable(false).frame(frame).show_inside(ui, |ui| {
            // Words on the buttons only when the bar has room for all of them.
            let narrow = narrow || ui.available_width() < 1200.0;
            ui.horizontal(|ui| {
                if icon_button(ui, icon::SIDEBAR_SIMPLE).on_hover_text(tr!("Fits and browser column")).clicked() {
                    self.show_library = !self.show_library;
                }
                {
                    use spai_ui::widgets::SteadySelect as _;
                    let w = egui::vec2(if narrow { 52.0 } else { 64.0 }, 24.0);
                    if ui.menu_label_sized(w, self.mode == Mode::Fits, tr!("Fits")).clicked() {
                        self.mode = Mode::Fits;
                    }
                    if ui.menu_label_sized(w, self.mode == Mode::Fleets, tr!("Fleets")).clicked() {
                        self.mode = Mode::Fleets;
                        self.requests.push(HostRequest::ListFleets);
                    }
                }
                ui.separator();
                let (can_undo, can_redo) = self.session().map_or((false, false), |s| (s.history.can_undo(), s.history.can_redo()));
                if ui.add_enabled(can_undo, egui::Button::new(icon::ARROW_U_UP_LEFT)).on_hover_text(tr!("Undo")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(can_redo, egui::Button::new(icon::ARROW_U_UP_RIGHT)).on_hover_text(tr!("Redo")).clicked() {
                    self.redo();
                }
                ui.separator();
                let has_fit = if self.mode == Mode::Fleets { self.fleet.is_some() } else { self.session().is_some() };
                // Icons alone when the window is narrow; the words are on hover either way.
                let labelled = |glyph: &str, word: &str| if narrow { glyph.to_owned() } else { format!("{glyph} {word}") };
                // Fits save themselves; fleets still wait for the button.
                if self.mode == Mode::Fleets && ui.add_enabled(has_fit, egui::Button::new(labelled(icon::FLOPPY_DISK, tr!("Save")))).on_hover_text(tr!("Save")).clicked() {
                    self.save_fleet();
                }
                if ui.button(labelled(icon::CLIPBOARD_TEXT, tr!("Import EFT"))).on_hover_text(tr!("Import the fit on the clipboard, or paste one")).clicked() {
                    self.import_from_button = true;
                    self.requests.push(HostRequest::ReadClipboard);
                }
                if ui.add_enabled(has_fit, egui::Button::new(labelled(icon::COPY, tr!("Copy EFT")))).on_hover_text(tr!("Copy EFT")).clicked() {
                    if let (Some(d), Some(s)) = (&self.dogma, self.session()) {
                        let text = spai_fit::formats::eft::export(d, &s.fit);
                        self.requests.push(HostRequest::CopyText(text));
                        self.status(tr!("EFT copied").to_owned());
                    }
                }
                if self.access.admin && ui.add_enabled(has_fit, egui::Button::new(labelled(icon::UPLOAD_SIMPLE, tr!("Publish")))).on_hover_text(tr!("Publish as doctrine")).clicked() {
                    self.start_publish();
                }
                ui.separator();
                if !narrow {
                    ui.label(tr!("Skills"));
                }
                let name = self.profiles.get(self.profile).map_or(String::new(), |p| p.name.clone());
                let width = if narrow { 110.0 } else { 180.0 };
                egui::ComboBox::from_id_salt("sim_profile").selected_text(text::clip(&name, if narrow { 12 } else { 22 })).width(width).show_ui(ui, |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    for i in 0..self.profiles.len() {
                        let label = self.profiles[i].name.clone();
                        ui.menu_value(&mut self.profile, i, label);
                    }
                });
                if icon_button(ui, icon::ARROWS_CLOCKWISE).on_hover_text(tr!("Fetch skills from EVE")).clicked() {
                    self.requests.push(HostRequest::FetchSkills);
                }
                if self.mode == Mode::Fits && !narrow {
                    ui.separator();
                    use spai_ui::widgets::SteadySelect as _;
                    let w = egui::vec2(64.0, 24.0);
                    if ui.menu_label_sized(w, self.center == Center::Fit, tr!("Fit")).clicked() {
                        self.center = Center::Fit;
                    }
                    if ui.menu_label_sized(w, self.center == Center::Graphs, tr!("Graphs")).clicked() {
                        self.center = Center::Graphs;
                    }
                    if icon_button(ui, icon::ARROW_SQUARE_OUT).on_hover_text(tr!("Graphs in their own window")).clicked() {
                        self.graph_window = !self.graph_window;
                    }
                }
                // A fixed slot for notes, so one appearing never moves the buttons; it takes what
                // is left and nothing more.
                let room = ui.available_width() - ui.spacing().item_spacing.x;
                if room > 40.0 {
                    let text = match &mut self.status {
                        Some((t, shown)) => {
                            *shown += ui.input(|i| i.stable_dt as f64);
                            (*shown <= 4.0).then(|| t.clone())
                        }
                        None => None,
                    };
                    if text.is_none() {
                        self.status = None;
                    }
                    ui.add_sized([room, 20.0], egui::Label::new(egui::RichText::new(text.unwrap_or_default()).weak()).truncate());
                }
            });
        });
    }

    fn import_window(&mut self, ctx: &egui::Context, now_ms: u64) {
        let Some(dialog) = &mut self.import else { return };
        let mut open = true;
        let mut done: Option<Result<Fit, String>> = None;
        let size = egui::vec2(480.0, 360.0);
        egui::Window::new(tr!("Import EFT")).open(&mut open).collapsible(false).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
            ui.label(tr!("Paste a fit in EFT format (as the game's fitting window copies it)."));
            // A fixed box: the text scrolls inside, the Import button stays where it is.
            egui::ScrollArea::vertical().max_height(260.0).min_scrolled_height(260.0).show(ui, |ui| {
                ui.set_min_height(260.0);
                ui.add(egui::TextEdit::multiline(&mut dialog.text).desired_width(f32::INFINITY).desired_rows(14).font(egui::TextStyle::Monospace));
            });
            let slot = egui::vec2(ui.available_width(), 20.0);
            ui.add_sized(slot, egui::Label::new(egui::RichText::new(dialog.error.clone().unwrap_or_default()).color(ui.visuals().error_fg_color)).truncate());
            if ui.button(tr!("Import")).clicked() {
                done = Some(match self.dogma.as_ref().map(|d| spai_fit::formats::eft::import(d, &dialog.text)) {
                    Some(Some(imp)) if imp.fit.ship != 0 => {
                        if imp.unknown.is_empty() {
                            Ok(imp.fit)
                        } else {
                            dialog.error = Some(trf!("Unknown: {names}", names = imp.unknown.join(", ")));
                            Ok(imp.fit)
                        }
                    }
                    Some(Some(imp)) => Err(trf!("No such ship: {names}", names = imp.unknown.first().cloned().unwrap_or_default())),
                    _ => Err(tr!("That is not an EFT fit: the first line must be [Ship, Name].").to_owned()),
                });
            }
        });
        match done {
            Some(Ok(fit)) => {
                let note = self.import.as_ref().and_then(|d| d.error.clone());
                self.import = None;
                self.place_import(fit, now_ms);
                if let Some(n) = note {
                    self.status(n);
                }
            }
            Some(Err(e)) => {
                if let Some(d) = &mut self.import {
                    d.error = Some(e);
                }
            }
            None => {}
        }
        if !open {
            self.import = None;
        }
    }
}

#[cfg(test)]
mod tests;
