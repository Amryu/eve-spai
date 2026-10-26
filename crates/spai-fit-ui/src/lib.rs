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
pub mod compare;
pub mod details;
pub mod mutation;
pub mod dock;
pub mod editor;

/// What a fit's base is called beside its variations.
pub fn base_label(fit: &Fit) -> String {
    fit.base_name.clone().unwrap_or_else(|| tr!("Base").to_owned())
}
pub mod fleet_graphs;
pub mod fleet_panel;
pub mod fleet_scene;
pub mod fleet_ships;
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
    /// Keep the layout (panel sizes, tabs shown) for the next start: a `SimLayout` as JSON.
    SaveLayout(String),
    /// The market's average prices, for what a fit costs; answered with `HostEvent::Prices`.
    FetchPrices,
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
    /// The layout kept from last time, as `SaveLayout` wrote it.
    Layout(String),
    /// Average market prices in ISK, by type.
    Prices(Vec<(TypeId, f64)>),
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
    /// The module the pilot last set: when a group limit has to put one offline, this one stays.
    touched: Option<(spai_fit::fit::Slot, u8)>,
}

/// How long a fit sits unchanged before it is saved.
pub const AUTOSAVE_MS: u64 = 1500;

impl Session {
    /// Bumped by every change, so caches keyed on it know when to rebuild.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn new(id: String, fit: Fit) -> Self {
        Session { id, fit, history: History::default(), rev: 0, computed_for: None, computed: None, warnings: Vec::new(), missing: Vec::new(), module_stats: Vec::new(), dirty: false, edited_at: 0, save_sent: false, touched: None }
    }

    fn refresh(&mut self, dogma: &Dogma, profile: usize, skills: &SkillSet, outside: &Outside, into_variation: bool) {
        let key = outside.key(&self.fit);
        if self.computed_for == Some((self.rev, profile, key)) {
            return;
        }
        let mut keep = self.touched.take();
        let (resolved, calc, computed) = loop {
            let (resolved, env) = outside.apply(dogma, &self.fit, skills);
            let calc = Calc::new(dogma, &resolved, skills, &env);
            let computed = spai_fit::stats::compute_with(&calc, &resolved);
            // One more of a group than it may online or run goes offline, as the game refuses
            // it, and the numbers are figured again without it.
            if self.yield_to_limits(dogma, &computed.capped, keep.take(), into_variation) {
                self.rev += 1;
                continue;
            }
            break (resolved, calc, computed);
        };
        self.missing = spai_fit::skills::missing(&resolved, dogma, skills);
        let mut warnings = spai_fit::warnings::check(dogma, &resolved, &computed, &self.missing);
        warnings.extend(spai_fit::warnings::check(dogma, &self.fit, &computed, &[]).into_iter().filter(|w| matches!(w.kind, spai_fit::warnings::WarningKind::VariationConflict { .. })));
        self.warnings = warnings;
        self.module_stats = spai_fit::stats::module_stats(&calc, &computed);
        self.computed = Some(Arc::new(computed));
        self.computed_for = Some((self.rev, profile, key));
    }

    /// Puts what a group limit holds back offline. With a variation on, the change goes through
    /// the last one switched on, as every edit does while it shows: the base fit may hold the
    /// module offline already, with the variation switching it on.
    fn yield_to_limits(&mut self, dogma: &Dogma, capped: &[spai_fit::calc::Capped], keep: Option<(spai_fit::fit::Slot, u8)>, into_variation: bool) -> bool {
        let shown = spai_fit::variations::resolve(dogma, &self.fit);
        match self.fit.active_variations.last().copied().filter(|_| into_variation) {
            Some(target) => {
                let mut edited = shown.clone();
                if !spai_fit::calc::yield_to_limits(dogma, &mut edited, &shown, capped, keep) {
                    return false;
                }
                let next = spai_fit::variations::edit_through(dogma, &self.fit, target, &shown, edited);
                let changed = next != self.fit;
                self.fit = next;
                changed
            }
            None => spai_fit::calc::yield_to_limits(dogma, &mut self.fit, &shown, capped, keep),
        }
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
    /// The fleets' 3D scene; the fit in fit mode.
    Scene,
    /// The fleet's ships one by one: ranges, and what they do to one target.
    Ships,
}

/// What of the screen's arrangement is kept between starts. Plain fields, each optional, so an
/// older or newer copy still reads.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SimLayout {
    pub left_w: Option<f32>,
    pub dock_h: Option<f32>,
    pub show_library: Option<bool>,
    /// 0 fits, 1 doctrines, 2 browse.
    pub left: Option<u8>,
    /// 0 fit, 1 graphs.
    pub center: Option<u8>,
    pub dock_first: Option<usize>,
    /// 0 hull, 1 tag, 2 A to Z.
    pub grouping: Option<u8>,
    /// The fits open in tabs, by library id, in tab order, and the one on screen.
    pub tabs: Option<Vec<String>>,
    pub active_tab: Option<String>,
    /// The graphs docked beside the fit, folded or not, and how wide.
    pub graph_docked: Option<bool>,
    pub graph_collapsed: Option<bool>,
    pub graph_dock_w: Option<f32>,
    /// The fleet graphs: open, where (0 window, 1 left, 2 bottom, 3 right) and how big docked.
    pub fleet_graph_open: Option<bool>,
    pub fleet_graph_place: Option<u8>,
    pub fleet_graph_size: Option<f32>,
}

/// ESI's market price list: every type's average price across the market, public, no sign-in.
pub const PRICES_URL: &str = "https://esi.evetech.net/latest/markets/prices/?datasource=tranquility";

/// The price list as ESI sends it; a type without an average falls back to its adjusted price.
pub fn parse_prices(bytes: &[u8]) -> Vec<(TypeId, f64)> {
    #[derive(serde::Deserialize)]
    struct Row {
        type_id: TypeId,
        average_price: Option<f64>,
        adjusted_price: Option<f64>,
    }
    serde_json::from_slice::<Vec<Row>>(bytes).unwrap_or_default().into_iter().filter_map(|r| r.average_price.or(r.adjusted_price).filter(|p| *p > 0.0).map(|p| (r.type_id, p))).collect()
}

/// A fit's price, by part, in ISK.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cost {
    pub hull: f64,
    pub modules: f64,
    pub charges: f64,
    pub drones: f64,
    /// Implants and boosters.
    pub worn: f64,
    pub cargo: f64,
    /// Items the price list has no price for.
    pub missing: usize,
}

impl Cost {
    pub fn total(&self) -> f64 {
        self.hull + self.modules + self.charges + self.drones + self.worn + self.cargo
    }

    /// The ship as it flies: what is lost with it, the cargo aside.
    pub fn fitted(&self) -> f64 {
        self.hull + self.modules + self.charges + self.drones
    }
}

/// What the middle shows in a wide window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Center {
    Fit,
    Graphs,
    /// The fleets' 3D scene; the fit in fit mode.
    Scene,
    /// The fleet's ships one by one.
    Ships,
}

/// Where the fleet graphs show: a window of their own, or docked on a side of the fleet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphPlace {
    Window,
    Left,
    #[default]
    Bottom,
    Right,
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
    /// The dock's height, once dragged or read back.
    pub dock_h: Option<f32>,
    /// The layout last handed to the host, so it is sent again only when it changed.
    layout_sent: Option<SimLayout>,
    /// The tab to end on while last time's tabs are loading back.
    restore_active: Option<String>,
    restore_pending: Vec<String>,
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
    /// Items side by side, once something was sent to compare.
    pub compare: Option<compare::CompareWindow>,
    /// The booster whose side effects are being picked.
    pub side_window: Option<TypeId>,
    /// The item whose abyssal roll is being set.
    pub mutation_window: Option<mutation::MutTarget>,
    /// The selection as it was before the last click on a module row, for a double click to act on.
    pub sel_before_click: Option<Vec<(Slot, u8)>>,
    /// The fitted values behind the tooltips, for the revision and profile they were figured for.
    tip_cache: Option<((u64, usize), std::sync::Arc<details::TipValues>)>,
    /// Average market prices in ISK, by type, once the host has them.
    pub prices: std::collections::HashMap<TypeId, f64>,
    prices_asked: bool,
    /// The width the user dragged the left column to; read from egui's memory on first use.
    left_w: Option<f32>,
    pub skills_window: bool,
    pub traits_window: bool,
    pub graph_window: bool,
    /// The graphs docked beside the fit, above the bottom dock, rather than in a window.
    pub graph_docked: bool,
    /// Docked but folded to a strip.
    pub graph_collapsed: bool,
    /// Where the graphs open again once closed: docked if they were last.
    pub graph_prefer_dock: bool,
    pub graph_dock_w: Option<f32>,
    /// Set while the graphs draw in their window, which offers to dock them.
    pub(crate) graph_in_window: bool,
    pub fleet_graph: fleet_graphs::FleetGraph,
    pub fleet_graph_open: bool,
    pub fleet_graph_place: GraphPlace,
    pub fleet_graph_size: Option<f32>,
    pub fleet_ships: fleet_ships::ShipsState,
    /// Which way the fleet results look: 0 the open fleet shooting, 1 its opponent.
    pub fleet_view: usize,
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
    /// Whether the fit on screen is among the graph's shooters, whichever tab that is.
    pub graph_current: bool,
    /// Whether the target profile is among the graph's targets.
    pub graph_ideal: bool,
    /// How the shooter and the target move, for the damage graphs' transversal.
    pub graph_motion: graphs_panel::Motion,
    /// The page address a fit link starts with, where the host can open one (the web).
    pub link_base: Option<String>,
    /// A fit handed over before the data is in, imported once it is.
    pending_import: Option<String>,
    /// Where the bottom and the side of the graph end, per kind of axis; automatic without one.
    pub graph_x_to: std::collections::HashMap<spai_fit::graphs::GraphX, f64>,
    pub graph_y_to: std::collections::HashMap<spai_fit::graphs::GraphY, f64>,
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
    /// The engagement analysis of the open fleet against its opponent, with what it was for.
    pub(crate) fleet_analysis: Option<(u64, std::sync::Arc<fleet_panel::Analysis>)>,
    pub(crate) fleet_cache: std::cell::RefCell<fleet_panel::FleetCache>,
    /// The open fleet's content as last edited and since when, and the content last sent to be
    /// saved: it autosaves once edits rest, like a fit.
    pub(crate) fleet_edit: Option<(u64, u64)>,
    pub(crate) fleet_sent: Option<u64>,
    /// The save under way was an autosave, which says nothing.
    pub(crate) fleet_quiet: bool,
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
            compare: None,
            pane: Pane::Fit,
            center: Center::Fit,
            left: LeftTab::Browse,
            show_library: true,
            dock_h: None,
            layout_sent: None,
            restore_active: None,
            restore_pending: Vec::new(),
            browser: browser::Browser::default(),
            library_state: Default::default(),
            project_picker: None,
            dock: dock::DockState::default(),
            warnings_expanded: false,
            selection: Vec::new(),
            sel_anchor: None,
            details: Vec::new(),
            side_window: None,
            mutation_window: None,
            sel_before_click: None,
            tip_cache: None,
            prices: Default::default(),
            prices_asked: false,
            left_w: None,
            skills_window: false,
            traits_window: false,
            graph_window: false,
            graph_docked: false,
            graph_collapsed: false,
            graph_prefer_dock: false,
            graph_dock_w: None,
            graph_in_window: false,
            fleet_graph: fleet_graphs::FleetGraph::default(),
            fleet_graph_open: false,
            fleet_graph_place: GraphPlace::default(),
            fleet_graph_size: None,
            fleet_ships: fleet_ships::ShipsState::default(),
            fleet_view: 0,
            import: None,
            import_ask: None,
            import_from_button: false,
            now_ms: 0,
            graph: spai_fit::graphs::GraphKind::DamageVsDistance,
            graph_drones: true,
            graph_attackers: Vec::new(),
            graph_targets: Vec::new(),
            graph_current: true,
            graph_ideal: true,
            graph_motion: graphs_panel::Motion::default(),
            link_base: None,
            pending_import: None,
            graph_x_to: Default::default(),
            graph_y_to: Default::default(),
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
            fleet_analysis: None,
            fleet_cache: Default::default(),
            fleet_edit: None,
            fleet_sent: None,
            fleet_quiet: false,
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
                if let Some(text) = self.pending_import.take() {
                    if !self.import_text(&text, self.now_ms) {
                        self.status(tr!("The link holds no fit this could read").to_owned());
                    }
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
                Ok(fit) => {
                    let had = self.active;
                    let restoring = self.restore_pending.contains(&id);
                    self.restore_pending.retain(|x| *x != id);
                    self.open_session(Session::new(id.clone(), fit));
                    // Tabs coming back from last time open in their order, and the one that was on
                    // screen stays on screen however they arrive.
                    if restoring {
                        self.active = self.restore_active.as_ref().and_then(|a| self.sessions.iter().position(|s| &s.id == a)).unwrap_or(had.min(self.sessions.len() - 1));
                        if self.restore_pending.is_empty() {
                            self.restore_active = None;
                        }
                    }
                }
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
            HostEvent::Layout(json) => self.apply_layout(&json),
            HostEvent::Prices(list) => {
                self.prices = list.into_iter().collect();
                self.tip_cache = None;
            }
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
                    // The opponent saved with the fleet, else the one already picked.
                    let enemy = fleet.enemy.clone().or_else(|| self.fleet.as_ref().and_then(|f| f.enemy.clone()));
                    self.fleet = Some(FleetSession { id, fleet, dirty: false, enemy });
                }
                Err(e) => self.status(trf!("Could not read the fleet: {e}", e = e)),
            },
            HostEvent::FleetDoc { id, doc } => {
                if let Ok(fleet) = serde_json::from_str::<spai_fit::fleet::Fleet>(&doc) {
                    self.fleet_docs.insert(id, fleet);
                }
            }
            HostEvent::FleetSaved(id) => {
                let now_hash = self.fleet.as_ref().map(|f| fleet_panel::content_hash(&f.fleet));
                if let Some(f) = &mut self.fleet {
                    if f.id == id {
                        // Edited again since it was sent: still to save.
                        if now_hash == self.fleet_sent {
                            f.dirty = false;
                        }
                        self.fleet_docs.insert(id, f.fleet.clone());
                    }
                }
                self.requests.push(HostRequest::ListFleets);
                if !std::mem::take(&mut self.fleet_quiet) {
                    self.status(tr!("Saved").to_owned());
                }
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

    /// A new library fit from a saved one, its variations and all, opened in a tab.
    pub fn copy_fit(&mut self, id: &str, now_ms: u64) {
        let source = self.sessions.iter().find(|s| s.id == id).map(|s| s.fit.clone()).or_else(|| self.fit_docs.get(id).cloned());
        let Some(mut fit) = source else {
            self.status(tr!("That fit is still loading; try again in a moment").to_owned());
            return;
        };
        fit.name = trf!("{name} (copy)", name = fit.name);
        self.open_fit(fit, now_ms);
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
    pub(crate) fn autosave_for_test(&mut self, now_ms: u64) {
        self.autosave(now_ms);
    }

    #[cfg(test)]
    pub(crate) fn import_from_button_for_test(&mut self) {
        self.import_from_button = true;
    }

    /// A fit from outside, a link's, imported now or as soon as the data is in.
    pub fn import_when_ready(&mut self, text: String) {
        if self.dogma.is_some() {
            if !self.import_text(&text, self.now_ms) {
                self.status(tr!("The link holds no fit this could read").to_owned());
            }
        } else {
            self.pending_import = Some(text);
        }
    }

    /// Imports any fit format; `false` when it holds none. Unknown names are noted, not refused.
    pub fn import_text(&mut self, text: &str, now_ms: u64) -> bool {
        let Some(dogma) = self.dogma.clone() else { return false };
        let found = spai_fit::formats::import_any(&dogma, text);
        if found.is_empty() {
            return false;
        }
        let unknown: Vec<String> = found.iter().flat_map(|i| i.unknown.iter().cloned()).collect();
        if !unknown.is_empty() {
            self.status(trf!("Unknown: {names}", names = unknown.join(", ")));
        }
        // One fit goes where it belongs, asking about near copies; several open side by side.
        if found.len() == 1 {
            self.place_import(found.into_iter().next().expect("one").fit, now_ms);
        } else {
            for imp in found {
                self.open_imported(imp.fit, now_ms);
            }
        }
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
            let error = if text.trim().is_empty() { None } else { Some(tr!("No fit found: paste EFT text, ESI's JSON, the game's fitting XML or a DNA string.").to_owned()) };
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

    /// The arrangement as it stands, for the host to keep.
    pub fn layout(&self) -> SimLayout {
        SimLayout {
            left_w: self.left_w,
            dock_h: self.dock_h,
            show_library: Some(self.show_library),
            left: Some(match self.left {
                LeftTab::Fits => 0,
                LeftTab::Doctrines => 1,
                LeftTab::Browse => 2,
            }),
            center: Some(match self.center {
                Center::Fit => 0,
                Center::Graphs => 1,
                Center::Scene => 2,
                Center::Ships => 3,
            }),
            dock_first: Some(self.dock.first),
            // Only fits the library holds: a tab not saved yet could not be opened again.
            tabs: Some(self.sessions.iter().filter(|s| s.fit.ship != 0 && self.library.iter().any(|e| e.id == s.id)).map(|s| s.id.clone()).collect()),
            active_tab: self.session().filter(|s| s.fit.ship != 0).map(|s| s.id.clone()),
            graph_docked: Some(self.graph_docked),
            graph_collapsed: Some(self.graph_collapsed),
            graph_dock_w: self.graph_dock_w,
            fleet_graph_open: Some(self.fleet_graph_open),
            fleet_graph_place: Some(match self.fleet_graph_place {
                GraphPlace::Window => 0,
                GraphPlace::Left => 1,
                GraphPlace::Bottom => 2,
                GraphPlace::Right => 3,
            }),
            fleet_graph_size: self.fleet_graph_size,
            grouping: Some(match self.library_state.grouping {
                library_panel::Grouping::Hull => 0,
                library_panel::Grouping::Tag => 1,
                library_panel::Grouping::Flat => 2,
            }),
        }
    }

    /// Takes back a kept arrangement; what it does not say stays as it is.
    pub fn apply_layout(&mut self, json: &str) {
        let Ok(l) = serde_json::from_str::<SimLayout>(json) else { return };
        if let Some(w) = l.left_w {
            self.left_w = Some(w);
        }
        if let Some(h) = l.dock_h {
            self.dock_h = Some(h);
        }
        if let Some(b) = l.show_library {
            self.show_library = b;
        }
        if let Some(t) = l.left {
            self.left = match t {
                0 => LeftTab::Fits,
                1 => LeftTab::Doctrines,
                _ => LeftTab::Browse,
            };
        }
        if let Some(c) = l.center {
            self.center = match c {
                1 => Center::Graphs,
                2 => Center::Scene,
                3 => Center::Ships,
                _ => Center::Fit,
            };
        }
        if let Some(f) = l.dock_first {
            self.dock.first = f;
        }
        if let Some(d) = l.graph_docked {
            self.graph_docked = d;
            self.graph_prefer_dock = d;
        }
        if let Some(c) = l.graph_collapsed {
            self.graph_collapsed = c;
        }
        if l.graph_dock_w.is_some() {
            self.graph_dock_w = l.graph_dock_w;
        }
        if let Some(o) = l.fleet_graph_open {
            self.fleet_graph_open = o;
        }
        if let Some(p) = l.fleet_graph_place {
            self.fleet_graph_place = match p {
                0 => GraphPlace::Window,
                1 => GraphPlace::Left,
                3 => GraphPlace::Right,
                _ => GraphPlace::Bottom,
            };
        }
        if l.fleet_graph_size.is_some() {
            self.fleet_graph_size = l.fleet_graph_size;
        }
        if let Some(tabs) = l.tabs.filter(|t| !t.is_empty()) {
            for id in &tabs {
                self.requests.push(HostRequest::LoadFit(id.clone()));
            }
            self.restore_pending = tabs.clone();
            self.restore_active = l.active_tab.or_else(|| tabs.last().cloned());
        }
        if let Some(g) = l.grouping {
            self.library_state.grouping = match g {
                1 => library_panel::Grouping::Tag,
                2 => library_panel::Grouping::Flat,
                _ => library_panel::Grouping::Hull,
            };
        }
        self.layout_sent = Some(self.layout());
    }

    /// Hands the arrangement to the host once it settles: never mid-drag, and only when changed.
    fn keep_layout(&mut self, ctx: &egui::Context) {
        if let Some(st) = egui::PanelState::load(ctx, egui::Id::new("sim_dock")) {
            self.dock_h = Some(st.rect.height());
        }
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let now = self.layout();
        if self.layout_sent.as_ref() != Some(&now) {
            if let Ok(json) = serde_json::to_string(&now) {
                self.requests.push(HostRequest::SaveLayout(json));
            }
            self.layout_sent = Some(now);
        }
    }

    /// What the fit on screen costs at the market's average prices, by part.
    pub fn cost(&self) -> Option<Cost> {
        let (dogma, s) = (self.dogma.as_ref()?, self.session()?);
        let price = |t: TypeId| self.prices.get(&t).copied();
        let mut c = Cost::default();
        let mut add = |part: &mut f64, t: TypeId, n: f64| match price(t) {
            Some(p) => *part += p * n,
            None => c.missing += 1,
        };
        let mut hull = 0.0;
        add(&mut hull, s.fit.ship, 1.0);
        let (mut modules, mut charges, mut drones, mut worn, mut cargo) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for m in &s.fit.modules {
            add(&mut modules, m.type_id, 1.0);
            if let Some(ch) = m.charge {
                // A full load: what the module holds of the charge.
                let cap = dogma.type_(m.type_id).and_then(|t| t.attr(spai_fit::dogma::attr::CAPACITY)).map_or(0.0, f64::from);
                let vol = dogma.type_(ch).and_then(|t| t.attr(spai_fit::dogma::attr::VOLUME)).map_or(0.0, f64::from);
                let n = if cap > 0.0 && vol > 0.0 { (cap / vol + 1e-6).floor().max(1.0) } else { 1.0 };
                add(&mut charges, ch, n);
            }
        }
        for d in &s.fit.drones {
            add(&mut drones, d.type_id, f64::from(d.count));
        }
        for f in &s.fit.fighters {
            add(&mut drones, f.type_id, f64::from(f.count));
        }
        for i in &s.fit.implants {
            add(&mut worn, *i, 1.0);
        }
        for b in &s.fit.boosters {
            add(&mut worn, b.type_id, 1.0);
        }
        for k in &s.fit.cargo {
            add(&mut cargo, k.type_id, f64::from(k.quantity));
        }
        c.hull = hull;
        c.modules = modules;
        c.charges = charges;
        c.drones = drones;
        c.worn = worn;
        c.cargo = cargo;
        Some(c)
    }

    /// The damage the fit's effective HP is figured against.
    pub fn set_damage_profile(&mut self, p: spai_fit::fit::DamageProfile) {
        self.edit(EditKind::Target, self.now_ms, move |f| f.damage_profile = p);
    }

    /// What an open fit computes to, by session id, refreshed when stale.
    pub fn computed_of(&mut self, id: &str) -> Option<Arc<Computed>> {
        let dogma = self.dogma.clone()?;
        let profile = self.profile;
        let skills = self.skills().clone();
        let outside = self.outside();
        // A recording goes on in the tab on screen alone.
        let into_variation = self.recording.is_none() || self.session().is_some_and(|a| a.id != id);
        let s = self.sessions.iter_mut().find(|s| s.id == id)?;
        s.refresh(&dogma, profile, &skills, &outside, into_variation);
        s.computed.clone()
    }

    /// The fit on screen.
    pub fn session(&self) -> Option<&Session> {
        self.sessions.get(self.active)
    }

    /// The module the pilot just set, so a group limit that has to put one offline spares it.
    pub fn touch(&mut self, slot: spai_fit::fit::Slot, index: u8) {
        if let Some(s) = self.session_mut() {
            s.touched = Some((slot, index));
        }
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
    pub fn open_session(&mut self, mut s: Session) {
        if let Some(d) = &self.dogma {
            spai_fit::formats::clamp_states(d, &mut s.fit);
            spai_fit::formats::sync_boosters(d, &mut s.fit, None);
        }
        // Five drones out at most, as a pilot with the skill at V; edits cap it to the profile's.
        s.fit.cap_drones_out(5);
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
        // The open fleet the same way, once its edits rest.
        if let Some(f) = self.fleet.as_ref().filter(|f| f.dirty) {
            let h = fleet_panel::content_hash(&f.fleet);
            if self.fleet_edit.is_none_or(|(e, _)| e != h) {
                self.fleet_edit = Some((h, now_ms));
            }
            let since = self.fleet_edit.map_or(now_ms, |(_, t)| t);
            if self.fleet_sent != Some(h) && now_ms >= since + AUTOSAVE_MS {
                self.fleet_quiet = true;
                self.save_fleet();
            }
        }
    }

    fn save_session(&mut self, i: usize) {
        let s = &mut self.sessions[i];
        s.save_sent = true;
        self.requests.push(HostRequest::SaveFit { id: s.id.clone(), name: s.fit.name.clone(), ship: s.fit.ship, doc: spai_fit::doc::to_json(&s.fit) });
    }

    /// Changes the fit on screen, recording the state before for undo.
    /// How many drones the pilot can have in space at once.
    pub fn drone_limit(&mut self) -> u32 {
        self.computed().map_or(5, |c| c.drones.max_active)
    }

    pub fn edit(&mut self, kind: EditKind, now_ms: u64, f: impl FnOnce(&mut Fit)) {
        let limit = if self.has_hull() { self.drone_limit() } else { 5 };
        let dogma = self.dogma.clone();
        // With a variation on, what is shown is what changes: the edit goes into the last one
        // switched on. Switching and naming variations, and recording one, edit the base.
        let into_variation = self.recording.is_none() && !matches!(kind, EditKind::Variation | EditKind::Rename);
        let Some(s) = self.session_mut() else { return };
        let before = s.fit.clone();
        match (dogma.as_deref(), s.fit.active_variations.last().copied().filter(|_| into_variation)) {
            (Some(d), Some(target)) => {
                let shown = spai_fit::variations::resolve(d, &before);
                let mut edited = shown.clone();
                f(&mut edited);
                edited.cap_drones_out(limit);
                spai_fit::formats::sync_boosters(d, &mut edited, Some(&shown));
                spai_fit::formats::drop_foreign_mode(d, &mut edited);
                s.fit = spai_fit::variations::edit_through(d, &before, target, &shown, edited);
            }
            _ => {
                f(&mut s.fit);
                s.fit.cap_drones_out(limit);
                if let Some(d) = &dogma {
                    spai_fit::formats::sync_boosters(d, &mut s.fit, Some(&before));
                    spai_fit::formats::drop_foreign_mode(d, &mut s.fit);
                }
            }
        }
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
        let into_variation = self.recording.is_none();
        let s = self.session_mut()?;
        s.refresh(&dogma, profile, &skills, &outside, into_variation);
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

    /// The whole simulator, in the space given.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if self.dogma.is_none() {
            self.loading(ui);
            return;
        }
        let now_ms = (ui.input(|i| i.time) * 1000.0) as u64;
        self.now_ms = now_ms;
        // An engagement waiting for the knobs to rest needs a frame once they have.
        if self.fleet_analysis_pending() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(fleet_panel::SETTLE_MS));
        }
        if !self.prices_asked {
            self.prices_asked = true;
            self.requests.push(HostRequest::FetchPrices);
        }
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
                egui::Panel::right("sim_fleet_results").resizable(true).default_size(STATS_W + 40.0).size_range(320.0..=720.0).show_inside(ui, |ui| {
                    fleet_panel::results(self, ui);
                });
            }
            if !narrow && self.fleet_graph_open {
                fleet_graphs::docked(self, ui);
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
                if narrow {
                    ui.horizontal(|ui| {
                        use spai_ui::widgets::SteadySelect as _;
                        let tabs = [(Pane::Library, tr!("Fleets")), (Pane::Fit, tr!("Fleet")), (Pane::Ships, tr!("Ships")), (Pane::Stats, tr!("Results")), (Pane::Graphs, tr!("Graphs")), (Pane::Scene, tr!("Scene"))];
                        let w = egui::vec2(((ui.available_width() - 5.0 * ui.spacing().item_spacing.x) / 6.0).max(40.0), 24.0);
                        for (pane, label) in tabs {
                            if ui.menu_label_sized(w, self.pane == pane, label).clicked() {
                                self.pane = pane;
                            }
                        }
                    });
                    ui.separator();
                }
                match (narrow, self.pane, self.center) {
                    (true, Pane::Library, _) => fleet_panel::library(self, ui, now_ms),
                    (true, Pane::Stats | Pane::Browse, _) => fleet_panel::results(self, ui),
                    (true, Pane::Graphs, _) => fleet_graphs::show(self, ui),
                    (true, Pane::Ships, _) | (false, _, Center::Ships) => fleet_ships::show(self, ui),
                    (true, Pane::Scene, _) | (false, _, Center::Scene) => fleet_scene::show(self, ui),
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
        if !narrow {
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
                (true, Pane::Graphs, _) => graphs_panel::show(self, ui, now_ms),
                _ => editor::show(self, ui, now_ms),
            }
        });
        self.show_dialogs(ui.ctx(), now_ms);
    }

    /// Whether the graphs are on screen, in their window or docked.
    pub fn graphs_shown(&self) -> bool {
        self.graph_window || self.graph_docked
    }

    /// Closes the graphs, or opens them where they were last: docked or in a window.
    pub fn toggle_graphs(&mut self) {
        if self.graphs_shown() {
            self.graph_prefer_dock = self.graph_docked;
            self.graph_window = false;
            self.graph_docked = false;
        } else if self.graph_prefer_dock {
            self.graph_docked = true;
            self.graph_collapsed = false;
        } else {
            self.graph_window = true;
        }
    }

    /// The graphs into the dock beside the fit, out of their window.
    pub fn dock_graphs(&mut self, docked: bool) {
        self.graph_docked = docked;
        self.graph_window = !docked;
        self.graph_collapsed = false;
        self.graph_prefer_dock = docked;
    }

    /// The picker and the dialogs, floating over whatever is below.
    pub fn show_dialogs(&mut self, ctx: &egui::Context, now_ms: u64) {
        picker::show_window(self, ctx, now_ms);
        self.import_window(ctx, now_ms);
        self.publish_window(ctx);
        warnings_panel::skills_window(self, ctx);
        graphs_panel::window(self, ctx, now_ms);
        if self.mode == Mode::Fleets {
            fleet_graphs::window(self, ctx);
        }
        details::windows(self, ctx);
        compare::window(self, ctx);
        details::side_effects_window(self, ctx, now_ms);
        mutation::window(self, ctx, now_ms);
        editor::traits_window(self, ctx);
        library_panel::project_picker(self, ctx, now_ms);
        self.import_ask_window(ctx, now_ms);
        rows::drag_ghost(ctx);
        self.keep_layout(ctx);
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
        egui::Panel::left(id).resizable(true).show_separator_line(false).default_size(w).min_size(MIN_W).max_size(MAX_W).show_inside(ui, |ui| body(self, ui));
        // The resize grip reaches a few points past the edge, and whatever is laid out after it wins
        // the pointer: a gap keeps the central view's drag areas off the grip. The column's edge is
        // drawn past the gap, against the central view.
        let fill = ui.visuals().panel_fill;
        egui::Panel::left(id.with("gap")).resizable(false).show_separator_line(false).exact_size(4.0).frame(egui::Frame::NONE.fill(fill)).show_inside(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().vline(r.right() - 0.5, r.y_range(), ui.visuals().widgets.noninteractive.bg_stroke);
        });
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
                if ui.button(labelled(icon::CLIPBOARD_TEXT, tr!("Import"))).on_hover_text(tr!("Import the fit on the clipboard, or paste one: EFT, ESI JSON, fitting XML or DNA")).clicked() {
                    self.import_from_button = true;
                    self.requests.push(HostRequest::ReadClipboard);
                }
                let mut copy: Option<(String, &'static str)> = None;
                ui.add_enabled_ui(has_fit && self.mode != Mode::Fleets, |ui| {
                    ui.menu_button(labelled(icon::COPY, tr!("Copy")), |ui| {
                        ui.set_min_width(200.0);
                        let (Some(d), Some(s)) = (self.dogma.clone(), self.session()) else { return };
                        let fit = spai_fit::variations::resolve(&d, &s.fit);
                        let mut item = |ui: &mut egui::Ui, label: &'static str, make: &dyn Fn() -> String| {
                            if ui.button(label).clicked() {
                                copy = Some((make(), label));
                                ui.close();
                            }
                        };
                        item(ui, tr!("EFT"), &|| spai_fit::formats::eft::export(&d, &fit));
                        item(ui, tr!("ESI JSON"), &|| serde_json::to_string_pretty(&spai_fit::formats::esi_json::export(&fit, "")).unwrap_or_default());
                        item(ui, tr!("Fitting XML"), &|| spai_fit::formats::xml::export(&d, &[&fit]));
                        item(ui, tr!("DNA"), &|| spai_fit::formats::dna::export(&fit));
                        item(ui, tr!("Multibuy"), &|| spai_fit::formats::multibuy::export(&d, &fit));

                    })
                    .response
                    .on_hover_text(tr!("Copy the fit on screen, with its active variations, in a format of your choice"));
                });
                // The fit on screen as a link to the web simulator, the fit whole: every variation, the
                // ones switched on still on.
                if let (Some(base), true) = (self.link_base.clone(), self.mode != Mode::Fleets) {
                    if ui.add_enabled(has_fit, egui::Button::new(labelled(icon::LINK, tr!("Copy link")))).on_hover_text(tr!("A link that opens this fit, with all its variations, in the web simulator")).clicked() {
                        if let Some(s) = self.session() {
                            copy = Some((format!("{base}#fit={}", spai_fit::formats::link::encode(&spai_fit::doc::to_json(&s.fit))), tr!("Link")));
                        }
                    }
                }
                if let Some((text, what)) = copy {
                    self.requests.push(HostRequest::CopyText(text));
                    self.status(trf!("{what} copied", what = what));
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
                if !narrow {
                    ui.separator();
                    use spai_ui::widgets::SteadySelect as _;
                    let w = egui::vec2(64.0, 24.0);
                    let fleets = self.mode == Mode::Fleets;
                    if fleets {
                        let on_fleet = !matches!(self.center, Center::Ships | Center::Scene);
                        if ui.menu_label_sized(w, on_fleet, tr!("Fleet")).clicked() {
                            self.center = Center::Fit;
                        }
                        if ui.menu_label_sized(w, self.center == Center::Ships, tr!("Ships")).on_hover_text(tr!("Every ship's reach with each charge, and what each does to one target")).clicked() {
                            self.center = Center::Ships;
                        }
                        if ui.menu_label_sized(w, self.center == Center::Scene, tr!("Scene")).clicked() {
                            self.center = Center::Scene;
                        }
                        ui.separator();
                        if ui.menu_label_sized(w, self.fleet_graph_open, tr!("Graphs")).on_hover_text(tr!("The fleet graphs, in a window or docked left, below or right")).clicked() {
                            self.fleet_graph_open = !self.fleet_graph_open;
                        }
                    } else if ui.menu_label_sized(w, self.graphs_shown(), tr!("Graphs")).on_hover_text(tr!("Opens the graphs in a window, which can be docked beside the fit")).clicked() {
                        self.toggle_graphs();
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

/// Which edge a dock sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DockSide {
    Left,
    Right,
    Bottom,
}

/// A resizable dock as big as it was dragged and no bigger: egui widens a panel to its content
/// and loses the drag to what is laid out after the grip, so the size is kept here, put back
/// while nobody drags, and a strip after the panel keeps the next view's widgets off the grip.
/// The same margin on every side.
pub(crate) fn dock_panel(ui: &mut egui::Ui, id: egui::Id, side: DockSide, size: &mut Option<f32>, default: f32, range: std::ops::RangeInclusive<f32>, body: impl FnOnce(&mut egui::Ui)) {
    let ctx = ui.ctx().clone();
    let (lo, hi) = (*range.start(), (*range.end()).max(*range.start()));
    let want = size.unwrap_or(default).clamp(lo, hi);
    let dragging = ctx.is_being_dragged(id.with("__resize"));
    let along = |r: egui::Rect| if side == DockSide::Bottom { r.height() } else { r.width() };
    if !dragging {
        if let Some(mut st) = egui::PanelState::load(&ctx, id) {
            if (along(st.rect) - want).abs() > 0.5 {
                // The edge that moves is the one facing the content; the other stays on the side.
                match side {
                    DockSide::Left => st.rect.max.x = st.rect.min.x + want,
                    DockSide::Right => st.rect.min.x = st.rect.max.x - want,
                    DockSide::Bottom => st.rect.min.y = st.rect.max.y - want,
                }
                ctx.data_mut(|d| d.insert_persisted(id, st));
            }
        }
    }
    let panel = match side {
        DockSide::Left => egui::Panel::left(id),
        DockSide::Right => egui::Panel::right(id),
        DockSide::Bottom => egui::Panel::bottom(id),
    };
    let frame = egui::Frame::new().inner_margin(8).fill(ui.visuals().panel_fill);
    panel.resizable(true).frame(frame).default_size(want).size_range(lo..=hi).show_inside(ui, |ui| {
        let r = ui.max_rect();
        ui.set_clip_rect(ui.clip_rect().intersect(r));
        // As big as it was dragged, whatever it shows: egui keeps a panel's size from its content.
        ui.set_min_size(ui.available_size());
        body(ui);
    });
    if dragging {
        if let Some(st) = egui::PanelState::load(&ctx, id) {
            *size = Some(along(st.rect).clamp(lo, hi));
        }
    } else if size.is_none() {
        *size = Some(want);
    }
    let gap = id.with("gap");
    let strip = match side {
        DockSide::Left => egui::Panel::left(gap),
        DockSide::Right => egui::Panel::right(gap),
        DockSide::Bottom => egui::Panel::bottom(gap),
    };
    strip.resizable(false).show_separator_line(false).exact_size(4.0).frame(egui::Frame::NONE).show_inside(ui, |_| {});
}

#[cfg(test)]
mod tests;
