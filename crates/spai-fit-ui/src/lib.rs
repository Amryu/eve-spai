//! The fitting simulator's screens: a fit being edited, what it computes to, the pilot's skills
//! and the library of saved fits. No storage and no network of its own: the host (the desktop app
//! or the web page) answers [`HostRequest`]s with [`HostEvent`]s.

use std::sync::Arc;

use spai_fit::calc::{Calc, Env};
use spai_fit::fit::Fit;
use spai_fit::history::{EditKind, History};
use spai_fit::ids::TypeId;
use spai_fit::skills::{MissingSkill, SkillSet};
use spai_fit::stats::Computed;
use spai_fit::warnings::Warning;
use spai_fit::Dogma;
use spai_ui::{tr, trf};

pub mod editor;
pub mod fleet_panel;
pub mod graphs_panel;
pub mod icons;
pub mod library_panel;
pub mod picker;
pub mod plot;
pub mod stats_panel;
pub mod text;
pub mod variations_panel;
pub mod warnings_panel;

pub use fleet_panel::{FleetSession, FleetSummary};
pub use picker::{PickTarget, Picker};

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
    computed_for: Option<(u64, usize)>,
    pub computed: Option<Arc<Computed>>,
    pub warnings: Vec<Warning>,
    pub missing: Vec<MissingSkill>,
    pub dirty: bool,
}

impl Session {
    pub fn new(id: String, fit: Fit) -> Self {
        Session { id, fit, history: History::default(), rev: 0, computed_for: None, computed: None, warnings: Vec::new(), missing: Vec::new(), dirty: false }
    }

    fn refresh(&mut self, dogma: &Dogma, profile: usize, skills: &SkillSet) {
        if self.computed_for == Some((self.rev, profile)) {
            return;
        }
        let resolved = spai_fit::variations::resolve(dogma, &self.fit);
        let calc = Calc::new(dogma, &resolved, skills, &Env::default());
        let computed = spai_fit::stats::compute_with(&calc, &resolved);
        self.missing = spai_fit::skills::missing(&resolved, dogma, skills);
        let mut warnings = spai_fit::warnings::check(dogma, &resolved, &computed, &self.missing);
        warnings.extend(spai_fit::warnings::check(dogma, &self.fit, &computed, &[]).into_iter().filter(|w| matches!(w.kind, spai_fit::warnings::WarningKind::VariationConflict { .. })));
        self.warnings = warnings;
        self.computed = Some(Arc::new(computed));
        self.computed_for = Some((self.rev, profile));
    }
}

/// Which column a narrow window shows; a wide one shows the fit beside stats or warnings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Library,
    Fit,
    Stats,
    Graphs,
    Warnings,
}

/// Fits or fleets: what the screens are about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fits,
    Fleets,
}

/// Below this width the stats and warnings become tabs over the fit instead of a column beside it.
pub const NARROW: f32 = 1000.0;
const SIDE_W: f32 = 240.0;
const STATS_W: f32 = 340.0;

#[derive(Default)]
pub struct ImportDialog {
    pub text: String,
    pub error: Option<String>,
}

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
    pub session: Option<Session>,
    pub library: Vec<FitSummary>,
    pub profiles: Vec<Profile>,
    pub profile: usize,
    pub picker: Option<Picker>,
    pub pane: Pane,
    pub show_library: bool,
    pub import: Option<ImportDialog>,
    pub graph: spai_fit::graphs::GraphKind,
    pub graph_drones: bool,
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
            session: None,
            library: Vec::new(),
            profiles: vec![Profile::all_v()],
            profile: 0,
            picker: None,
            pane: Pane::Fit,
            show_library: true,
            import: None,
            graph: spai_fit::graphs::GraphKind::DamageVsDistance,
            graph_drones: true,
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
                if let Some(s) = &mut self.session {
                    s.computed_for = None;
                }
            }
            HostEvent::DogmaFailed(e) => {
                self.dogma_error = Some(e);
                self.dogma_requested = false;
            }
            HostEvent::Fits(list) => self.library = list,
            HostEvent::Fit { id, doc } => match spai_fit::doc::from_json(&doc) {
                Ok(fit) => self.session = Some(Session::new(id, fit)),
                Err(e) => self.status(trf!("Could not read the fit: {e}", e = e)),
            },
            HostEvent::Profiles(p) => {
                let current = self.profiles.get(self.profile).map(|p| p.id.clone());
                self.profiles = std::iter::once(Profile::all_v()).chain(p).collect();
                self.profile = current.and_then(|id| self.profiles.iter().position(|p| p.id == id)).unwrap_or(0);
                if let Some(s) = &mut self.session {
                    s.computed_for = None;
                }
            }
            HostEvent::Saved(id) => {
                if let Some(s) = &mut self.session {
                    if s.id == id {
                        s.dirty = false;
                        self.fit_docs.insert(id.clone(), s.fit.clone());
                    }
                }
                self.requests.push(HostRequest::ListFits);
                self.status(tr!("Saved").to_owned());
            }
            HostEvent::Status(s) => self.status(s),
            HostEvent::Doctrines(d) => self.doctrines = d,
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
            }
            Err(e) => self.status(trf!("Could not read the fit: {e}", e = e)),
        }
    }

    /// Starts publishing the open fit; the dialog takes the name, side and tags.
    pub fn start_publish(&mut self) {
        let Some(s) = &self.session else { return };
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
            if let (Some(d), Some(s)) = (self.publish.take(), self.session.as_ref()) {
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
        let mut s = Session::new(id, Fit::new(ship, name));
        s.dirty = true;
        self.session = Some(s);
        self.pane = Pane::Fit;
    }

    /// Opens an imported fit as a new library entry.
    pub fn open_fit(&mut self, fit: Fit, now_ms: u64) {
        let id = self.new_id(now_ms);
        let mut s = Session::new(id, fit);
        s.dirty = true;
        self.session = Some(s);
        self.pane = Pane::Fit;
    }

    /// Changes the fit on screen, recording the state before for undo.
    pub fn edit(&mut self, kind: EditKind, now_ms: u64, f: impl FnOnce(&mut Fit)) {
        let Some(s) = &mut self.session else { return };
        let before = s.fit.clone();
        f(&mut s.fit);
        if s.fit != before {
            s.history.record(kind, &before, now_ms);
            s.rev += 1;
            s.dirty = true;
        }
    }

    pub fn undo(&mut self) {
        if let Some(s) = &mut self.session {
            if let Some(fit) = s.history.undo(&s.fit) {
                s.fit = fit;
                s.rev += 1;
                s.dirty = true;
            }
        }
    }

    pub fn redo(&mut self) {
        if let Some(s) = &mut self.session {
            if let Some(fit) = s.history.redo(&s.fit) {
                s.fit = fit;
                s.rev += 1;
                s.dirty = true;
            }
        }
    }

    pub fn save(&mut self) {
        if let Some(s) = &self.session {
            self.requests.push(HostRequest::SaveFit { id: s.id.clone(), name: s.fit.name.clone(), ship: s.fit.ship, doc: spai_fit::doc::to_json(&s.fit) });
        }
    }

    pub fn request(&mut self, r: HostRequest) {
        self.requests.push(r);
    }

    /// Recomputes what is stale and returns the current numbers.
    pub fn computed(&mut self) -> Option<Arc<Computed>> {
        let dogma = self.dogma.clone()?;
        let profile = self.profile;
        let skills = self.skills().clone();
        let s = self.session.as_mut()?;
        s.refresh(&dogma, profile, &skills);
        s.computed.clone()
    }

    /// What the capacitor graph needs beyond the computed numbers.
    pub fn cap_inputs(&self) -> Option<spai_fit::graphs::CapInputs> {
        let (dogma, s) = (self.dogma.as_ref()?, self.session.as_ref()?);
        let resolved = spai_fit::variations::resolve(dogma, &s.fit);
        let calc = Calc::new(dogma, &resolved, self.skills(), &Env::default());
        Some(spai_fit::graphs::CapInputs { users: spai_fit::stats::cap_users(&calc) })
    }

    /// The whole simulator, in the space given.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if self.dogma.is_none() {
            self.loading(ui);
            return;
        }
        let now_ms = (ui.input(|i| i.time) * 1000.0) as u64;
        self.computed();
        let narrow = ui.available_width() < NARROW;
        self.top_bar(ui, now_ms, narrow);
        if self.mode == Mode::Fleets {
            if !narrow {
                egui::Panel::left("sim_fleet_library").resizable(false).exact_size(SIDE_W).show_inside(ui, |ui| {
                    fleet_panel::library(self, ui, now_ms);
                });
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
                    (true, Pane::Stats | Pane::Graphs | Pane::Warnings) => fleet_panel::results(self, ui),
                    _ => fleet_panel::editor(self, ui, now_ms),
                }
            });
            self.show_dialogs(ui.ctx(), now_ms);
            return;
        }
        if self.show_library && !narrow {
            egui::Panel::left("sim_library").resizable(false).exact_size(SIDE_W).show_inside(ui, |ui| {
                library_panel::show(self, ui, now_ms);
            });
        }
        if !narrow {
            egui::Panel::right("sim_stats").resizable(false).exact_size(STATS_W).show_inside(ui, |ui| {
                self.side_tabs(ui);
                match self.pane {
                    Pane::Warnings => warnings_panel::show(self, ui),
                    Pane::Graphs => graphs_panel::show(self, ui, now_ms),
                    _ => stats_panel::show(self, ui),
                }
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            if narrow {
                // Everything becomes a tab, so nothing is ever wider than the window.
                ui.horizontal(|ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    let tabs = [(Pane::Library, tr!("Fits")), (Pane::Fit, tr!("Fit")), (Pane::Stats, tr!("Stats")), (Pane::Graphs, tr!("Graphs")), (Pane::Warnings, tr!("Warnings"))];
                    let w = egui::vec2(((ui.available_width() - 4.0 * ui.spacing().item_spacing.x) / 5.0).max(40.0), 24.0);
                    for (pane, label) in tabs {
                        if ui.menu_label_sized(w, self.pane == pane, label).clicked() {
                            self.pane = pane;
                        }
                    }
                });
                ui.separator();
            }
            match (narrow, self.pane) {
                (true, Pane::Library) => library_panel::show(self, ui, now_ms),
                (true, Pane::Stats) => stats_panel::show(self, ui),
                (true, Pane::Graphs) => graphs_panel::show(self, ui, now_ms),
                (true, Pane::Warnings) => warnings_panel::show(self, ui),
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

    fn side_tabs(&mut self, ui: &mut egui::Ui) {
        use spai_ui::widgets::SteadySelect as _;
        ui.horizontal(|ui| {
            let w = egui::vec2((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0, 24.0);
            let warnings = self.session.as_ref().map_or(0, |s| s.warnings.len());
            let label = if warnings > 0 { trf!("Warnings ({n})", n = warnings) } else { tr!("Warnings").to_owned() };
            let stats = !matches!(self.pane, Pane::Warnings | Pane::Graphs);
            if ui.menu_label_sized(w, stats, tr!("Stats")).clicked() {
                self.pane = Pane::Stats;
            }
            if ui.menu_label_sized(w, self.pane == Pane::Graphs, tr!("Graphs")).clicked() {
                self.pane = Pane::Graphs;
            }
            if ui.menu_label_sized(w, self.pane == Pane::Warnings, label).clicked() {
                self.pane = Pane::Warnings;
            }
        });
        ui.separator();
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, _now_ms: u64, narrow: bool) {
        use egui_phosphor::regular as icon;
        use spai_ui::widgets::icon_button;
        egui::Panel::top("sim_top").resizable(false).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if icon_button(ui, icon::LIST).on_hover_text(tr!("Fit library")).clicked() {
                    self.show_library = !self.show_library;
                }
                {
                    use spai_ui::widgets::SteadySelect as _;
                    let w = egui::vec2(if narrow { 52.0 } else { 64.0 }, 22.0);
                    if ui.menu_label_sized(w, self.mode == Mode::Fits, tr!("Fits")).clicked() {
                        self.mode = Mode::Fits;
                    }
                    if ui.menu_label_sized(w, self.mode == Mode::Fleets, tr!("Fleets")).clicked() {
                        self.mode = Mode::Fleets;
                        self.requests.push(HostRequest::ListFleets);
                    }
                }
                ui.separator();
                let (can_undo, can_redo) = self.session.as_ref().map_or((false, false), |s| (s.history.can_undo(), s.history.can_redo()));
                if ui.add_enabled(can_undo, egui::Button::new(icon::ARROW_U_UP_LEFT)).on_hover_text(tr!("Undo")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(can_redo, egui::Button::new(icon::ARROW_U_UP_RIGHT)).on_hover_text(tr!("Redo")).clicked() {
                    self.redo();
                }
                ui.separator();
                let has_fit = if self.mode == Mode::Fleets { self.fleet.is_some() } else { self.session.is_some() };
                // Icons alone when the window is narrow; the words are on hover either way.
                let labelled = |glyph: &str, word: &str| if narrow { glyph.to_owned() } else { format!("{glyph} {word}") };
                if ui.add_enabled(has_fit, egui::Button::new(labelled(icon::FLOPPY_DISK, tr!("Save")))).on_hover_text(tr!("Save")).clicked() {
                    if self.mode == Mode::Fleets {
                        self.save_fleet();
                    } else {
                        self.save();
                    }
                }
                if ui.button(labelled(icon::CLIPBOARD_TEXT, tr!("Import EFT"))).on_hover_text(tr!("Import EFT")).clicked() {
                    self.import = Some(ImportDialog::default());
                }
                if ui.add_enabled(has_fit, egui::Button::new(labelled(icon::COPY, tr!("Copy EFT")))).on_hover_text(tr!("Copy EFT")).clicked() {
                    if let (Some(d), Some(s)) = (&self.dogma, &self.session) {
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
                self.open_fit(fit, now_ms);
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
