//! What the shared wormhole map sees of the web app: New Eden from the universe file, the group's
//! holes, and settings kept for this browser.

use std::collections::HashMap;
use std::sync::Arc;

use spai_core::geo::Systems;
use spai_core::wormholes::{Life, Mass, Source, SystemSig, Wormhole};
use spai_ui::wh_form::{self, SysHit, WhForm};
use spai_ui::wh_graph::WhGraphView;
use spai_ui::wh_tab::{WhHost, WhPrefs};

/// A change made on the map, for the app to store and send.
#[derive(Clone, Debug)]
pub enum Edit {
    Save(Wormhole),
    Dead(String),
    /// A probe scanner copy pasted for a system; `full` drops what it does not list.
    Sigs { system: i64, scan: Vec<spai_core::wormholes::ScanSig>, full: bool },
    /// Signatures deleted in the browser, and ones its undo put back.
    SigsGone(Vec<(i64, SystemSig)>),
    SigsBack(Vec<(i64, SystemSig)>),
}

/// How the Wormholes tab shows the holes, as the desktop's switch above it.
#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum WhView {
    #[default]
    Map,
    Table,
    Signatures,
}

pub struct WebHost {
    pub geo: Arc<Systems>,
    pub holes: Vec<Wormhole>,
    pub prefs: WhPrefs,
    pub layout: HashMap<i64, egui::Pos2>,
    pub sigs: HashMap<i64, Vec<SystemSig>>,
    /// Settings or the layout changed since they were last saved.
    pub dirty: bool,
    /// The character may share into a group, so holes can be added and edited.
    pub can_edit: bool,
    /// The add or edit window, the desktop's own form.
    pub form: Option<WhForm>,
    /// Which suggestion each system field has highlighted.
    sugg: HashMap<&'static str, usize>,
    /// Changes made since the app last took them.
    pub edits: Vec<Edit>,
    side: Side,
    pin_input: String,
    /// The system last opened in the side panel.
    last_sel: Option<i64>,
    /// The added characters where ESI says they are: name to (system, online).
    pub chars: HashMap<String, (i64, bool)>,
    /// Holes to fill in, one card at a time in the top right corner.
    pub prompts: std::collections::VecDeque<Prompt>,
    /// Pairs of systems the user said were joined by no hole, and when, by low id first.
    pub not_holes: HashMap<(i64, i64), i64>,
    /// How wide the side panel was drawn this frame, 0 when it was not: the card keeps clear of it.
    pub side_w: f32,
    /// Which holes the tab shows, as the desktop's Filter.
    pub filter: spai_core::wormholes::WhFilter,
    /// A paste keeps what it does not list, rather than taking it as gone.
    keep_missing: bool,
    /// How the last paste went.
    pub sig_note: Option<String>,
    /// Holes are recorded from the added characters' jumps; the app keeps it, this mirrors it for
    /// the settings menu.
    pub detect: bool,
    /// The settings menu asked for the Group tab.
    pub open_group: bool,
    /// Which holes routes may use: the map's route planner owns them, the app keeps this copy in
    /// step, and `plan_changed` says this side changed them.
    pub plan: crate::planner::PlanPrefs,
    pub plan_changed: bool,
    pub wh_view: WhView,
    pub sig_browser: spai_ui::sig_browser::SigBrowser,
    /// Group names by id, for the table's Source column.
    pub group_names: HashMap<String, String>,
    /// The top bar's system facts lookup, and the system it opened.
    pub facts_query: String,
    pub facts: Option<i64>,
    /// The group each shared hole came in, by uid.
    pub hole_group: HashMap<String, String>,
}

/// A jump an added character made that looks like a hole, asked about as the desktop asks.
pub struct Prompt {
    pub who: String,
    pub from: i64,
    pub to: i64,
    /// Certainly a hole, as opposed to maybe one.
    pub certain: bool,
    /// The hole types that could have joined the two systems.
    pub candidates: Vec<&'static str>,
    pub at: i64,
    /// The user said a possible hole was one.
    confirmed: bool,
    pub form: WhForm,
}

/// The side panel's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Side {
    #[default]
    Holes,
    Signatures,
    Routes,
}

impl WebHost {
    pub fn new(geo: Arc<Systems>) -> Self {
        let prefs = WhPrefs { pin_jumps: PIN_JUMPS, layout_style: "tree".into(), layout_pack: true, ..Default::default() };
        WebHost { geo, holes: Vec::new(), prefs, layout: HashMap::new(), sigs: HashMap::new(), dirty: false, can_edit: false, form: None, sugg: HashMap::new(), edits: Vec::new(), side: Side::default(), pin_input: String::new(), last_sel: None, chars: HashMap::new(), prompts: Default::default(), not_holes: HashMap::new(), side_w: 0.0, filter: Default::default(), keep_missing: false, sig_note: None, detect: true, open_group: false, plan: Default::default(), plan_changed: false, wh_view: WhView::Map, sig_browser: Default::default(), group_names: HashMap::new(), hole_group: HashMap::new(), facts_query: String::new(), facts: None }
    }
}

/// A system name field with suggestions from New Eden, as the desktop's: arrows and Enter or a
/// click pick one, which writes its name into `q`.
fn system_input(ui: &mut egui::Ui, geo: &Systems, sel: &mut HashMap<&'static str, usize>, key: &'static str, q: &mut String, hint: &str, width: f32) -> Option<i64> {
    let hits: Vec<SysHit> = geo.search(q, 8).into_iter().map(|i| (i.id, i.name.clone(), i.security, i.constellation.clone(), i.region.clone())).collect();
    let pick = wh_form::system_field(ui, q, sel.entry(key).or_insert(0), hint, width, &hits);
    if let Some(i) = pick.and_then(|id| geo.info_of(id)) {
        *q = i.name.clone();
    }
    pick
}

/// A hole as the form saved it, kept the hole it was when editing: its id, origin and when it was
/// seen stay, and a reading not given keeps the old one, as the desktop merges.
fn merge(was: Option<&Wormhole>, fresh: Wormhole, now: i64) -> Wormhole {
    match was {
        Some(was) => Wormhole {
            id: was.id,
            uid: was.uid.clone(),
            source: was.source,
            seen_by: was.seen_by | Source::Manual.bit(),
            reported_at: was.reported_at,
            detected_by: was.detected_by.clone(),
            jumped_at: was.jumped_at,
            explicit_expiry: was.expiry_after_reading(fresh.life, now),
            mass: fresh.mass.or(was.mass),
            life: fresh.life.or(was.life),
            observed_at: fresh.observed_at.or(was.observed_at),
            ..fresh
        },
        None => Wormhole { uid: spai_share::crypto::random32()[..16].iter().map(|b| format!("{b:02x}")).collect(), ..fresh },
    }
}

impl WebHost {
    /// The add window, in `sel` or in a system still to be named.
    /// The desktop's Table view: every hole shown, with its buttons.
    pub fn table_view(&mut self, view: &mut WhGraphView, ui: &mut egui::Ui) {
        let now = spai_core::clock::utc().timestamp();
        let shown = self.holes(now);
        let list: Vec<&Wormhole> = shown.iter().collect();
        let geo = self.geo.clone();
        let (plan, names, of) = (&self.plan, &self.group_names, &self.hole_group);
        let mut act = spai_ui::wh_tab::holes_table(
            ui,
            &geo,
            &list,
            now,
            &|w| plan.wh_off_holes.contains(&w.uid),
            &|uid| of.get(uid).and_then(|g| names.get(g)).cloned(),
        );
        // Closing and editing change the group's map, which a viewer cannot.
        if !self.can_edit {
            (act.kill, act.edit) = (None, None);
        }
        let by_id = |id: i64| shown.iter().find(|w| w.id == id);
        if let Some(w) = act.kill.and_then(by_id) {
            self.edits.push(Edit::Dead(w.uid.clone()));
        }
        if let Some(w) = act.edit.and_then(by_id) {
            self.form = Some(WhForm::of(w, Some(&geo)));
        }
        if let Some(uid) = act.toggle {
            self.plan.toggle_off_hole(&uid);
            self.plan_changed = true;
        }
        if let Some(id) = act.open.or(act.info) {
            self.wh_view = WhView::Map;
            view.selected = Some(id);
            self.side = Side::Holes;
        }
    }

    /// The desktop's signature browser over what this browser holds.
    pub fn sig_view(&mut self, view: &mut WhGraphView, ui: &mut egui::Ui) {
        let now = spai_core::clock::utc().timestamp();
        let geo = self.geo.clone();
        let act = spai_ui::sig_browser::view(ui, &mut self.sig_browser, Some(&geo), &self.holes, now, false);
        if !act.deleted.is_empty() {
            self.edits.push(Edit::SigsGone(act.deleted));
        }
        if !act.restored.is_empty() {
            self.edits.push(Edit::SigsBack(act.restored));
        }
        if let Some(id) = act.open {
            self.wh_view = WhView::Map;
            view.selected = Some(id);
            self.side = Side::Signatures;
        }
        if self.can_edit {
            if let Some(w) = act.edit.and_then(|uid| self.holes.iter().find(|w| w.uid == uid)) {
                self.form = Some(WhForm::of(w, Some(&geo)));
            }
            if let Some(f) = act.new_hole {
                self.form = Some(f);
            }
        }
    }

    /// The desktop's "System facts" box: a system's class, effect, statics and celestials.
    pub fn facts_search(&mut self, ui: &mut egui::Ui) {
        let geo = self.geo.clone();
        let picked = system_input(ui, &geo, &mut self.sugg, "web_facts", &mut self.facts_query, "System facts: J-name or system", 200.0);
        if picked.is_some() {
            self.facts = picked;
        }
    }

    pub fn facts_window(&mut self, ctx: &egui::Context, top: f32) {
        let Some(sys) = self.facts else { return };
        let geo = self.geo.clone();
        let Some(info) = geo.info_of(sys).cloned() else {
            self.facts = None;
            return;
        };
        let mut open = true;
        egui::Window::new(&info.name)
            .id(egui::Id::new("web_facts"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-(self.side_w + 12.0), top + 8.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.label(format!("{} \u{b7} {}", spai_core::whdata::class_of(sys, info.security, &info.region).label(), info.region));
                spai_ui::wh_tab::wh_system_facts(ui, sys, &info, true);
                ui.add_space(8.0);
                ui.label(egui::RichText::new(spai_core::whdata::ATTRIBUTION).weak());
            });
        if !open {
            self.facts = None;
        }
    }

    pub fn add_form(&mut self, sel: Option<i64>) {
        let system = sel.and_then(|id| self.geo.info_of(id)).map(|i| i.name.clone()).unwrap_or_default();
        self.form = Some(WhForm { system, ..WhForm::fresh() });
    }

    /// The signatures saved in a system that a hole there could still take, as the desktop offers
    /// them: wormholes first, then what is not scanned yet. `except` is the hole being edited.
    fn offered(&self, name: &str, except: Option<&str>) -> Vec<(String, String)> {
        let Some(id) = self.geo.lookup(name.trim()).map(|i| i.id) else { return Vec::new() };
        let letters = |x: &str| x.trim().chars().take(3).collect::<String>().to_uppercase();
        let taken: Vec<String> = self
            .holes
            .iter()
            .filter(|w| Some(w.uid.as_str()) != except)
            .filter_map(|w| if w.system_id == id { w.signature.clone() } else if w.dest_system_id == Some(id) { w.dest_signature.clone() } else { None })
            .map(|s| letters(&s))
            .collect();
        let mut list: Vec<SystemSig> = self.sigs.get(&id).cloned().unwrap_or_default();
        list.retain(|s| {
            let group = s.group.to_lowercase();
            !s.kind.to_lowercase().contains("anomal") && (group.is_empty() || group.contains("wormhole")) && !taken.contains(&letters(&s.sig))
        });
        let rank = |g: &str| if g.to_lowercase().contains("wormhole") { 0 } else if g.is_empty() { 1 } else { 2 };
        list.sort_by(|a, b| rank(&a.group).cmp(&rank(&b.group)).then(a.sig.cmp(&b.sig)));
        list.into_iter()
            .map(|s| {
                let what = if s.name.is_empty() { if s.group.is_empty() { "not scanned yet".to_owned() } else { s.group } } else { s.name };
                (s.sig, what)
            })
            .collect()
    }

    /// A typed signature in `system` made whole from those known there, or which ones it could be.
    fn complete(&self, system: Option<i64>, typed: &str, except: Option<&str>) -> Result<Option<String>, String> {
        let Some(system) = system else { return Ok(None) };
        let mut known: Vec<String> = self.sigs.get(&system).map(|v| v.iter().map(|s| s.sig.clone()).collect()).unwrap_or_default();
        for w in self.holes.iter().filter(|w| Some(w.uid.as_str()) != except) {
            if w.system_id == system {
                known.extend(w.signature.clone());
            }
            if w.dest_system_id == Some(system) {
                known.extend(w.dest_signature.clone());
            }
        }
        spai_core::wormholes::complete_sig(typed, &known).map_err(|ids| {
            let name = self.geo.info_of(system).map(|i| i.name.clone()).unwrap_or_default();
            format!("{} in {name} could be {}. Type more of it.", typed.trim().to_uppercase(), ids.join(" or "))
        })
    }

    /// The desktop's Add and Edit wormhole window.
    pub fn form_window(&mut self, ctx: &egui::Context, top: f32) {
        let Some(mut form) = self.form.take() else { return };
        let geo = self.geo.clone();
        let except = form.uid.clone();
        let (here_sigs, there_sigs) = (self.offered(&form.system, except.as_deref()), self.offered(&form.dest, except.as_deref()));
        let (type_was, dest_was) = (form.wh_type.clone(), form.dest.clone());
        let (mut open, mut save) = (true, false);
        let sugg = &mut self.sugg;
        egui::Window::new(if form.uid.is_some() { "Edit wormhole" } else { "Add wormhole" })
            .id(egui::Id::new("web_wh_form"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-(self.side_w + 12.0), top + 8.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                save = wh_form::form_ui(ui, &mut form, &mut |ui, key, q, hint, w| system_input(ui, &geo, sugg, key, q, hint, w), &here_sigs, &there_sigs);
            });
        let (t, d) = (form.wh_type != type_was, form.dest != dest_was);
        wh_form::drifter_autofill(&geo, &mut form.wh_type, &mut form.dest, t, d);
        if save {
            let now = spai_core::clock::utc().timestamp();
            let complete = |sys: Option<i64>, typed: &str| self.complete(sys, typed, except.as_deref());
            match wh_form::build(&mut form, &geo, &complete, now) {
                Ok((fresh, _)) => {
                    let was = except.as_ref().and_then(|u| self.holes.iter().find(|w| &w.uid == u));
                    let w = merge(was, fresh, now);
                    self.edits.push(Edit::Save(w));
                    return;
                }
                Err(e) => form.error = Some(e),
            }
        }
        if open {
            self.form = Some(form);
        }
    }

    /// A card for a jump an added character made that looks like a hole.
    pub fn detected(&mut self, who: &str, from: i64, to: i64, at: i64, certain: bool, candidates: Vec<&'static str>) {
        if self.prompts.iter().any(|p| p.from == from && p.to == to) {
            return;
        }
        let name = |id: i64| self.geo.info_of(id).map(|i| i.name.clone()).unwrap_or_default();
        let wh_type = if candidates.len() == 1 { candidates[0].to_owned() } else { String::new() };
        let known: Vec<&str> = if wh_type.is_empty() { candidates.clone() } else { vec![wh_type.as_str()] };
        let sizes = spai_core::wormholes::sizes_for(&known);
        let form = WhForm { system: name(from), dest: name(to), size: (sizes.len() == 1).then(|| sizes[0]), wh_type, ..WhForm::fresh() };
        self.prompts.push_back(Prompt { who: who.to_owned(), from, to, certain, candidates, at, confirmed: false, form });
    }

    /// The card for the front of the queue, in the top right corner and clear of the side panel: the
    /// desktop's questions, on the desktop's widgets.
    pub fn corner(&mut self, ctx: &egui::Context, top: f32) {
        use egui_phosphor::regular as icon;
        if self.prompts.is_empty() {
            return;
        }
        let geo = self.geo.clone();
        let name = |id: i64| geo.info_of(id).map_or_else(|| id.to_string(), |i| i.name.clone());
        let count = self.prompts.len();
        let (here_opts, there_opts) = {
            let p = &self.prompts[0];
            (self.offered(&p.form.system, None), self.offered(&p.form.dest, None))
        };
        // Clear of the add and edit window, which takes the corner while it is open.
        let below = if self.form.is_some() { 470.0 } else { 0.0 };
        let p = self.prompts.front_mut().expect("checked");
        let mut act: Option<&'static str> = None;
        egui::Window::new("Wormhole?")
            .id(egui::Id::new("web_corner_card"))
            .title_bar(false)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-(self.side_w + 12.0), top + 8.0 + below))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                let n = if count > 1 { format!(" (1 of {count})") } else { String::new() };
                ui.label(egui::RichText::new(format!("{}  Wormhole?{n}", icon::SPIRAL)).strong());
                // EVE time, which is UTC.
                let when = format!("{:02}:{:02}", p.at.rem_euclid(86_400) / 3600, p.at.rem_euclid(3600) / 60);
                ui.label(format!("{}: {} {} {} at {when}", p.who, name(p.from), icon::ARROW_RIGHT, name(p.to)));
                if !(p.certain || p.confirmed) {
                    ui.label(egui::RichText::new("That jump fits a wormhole, a filament, a clone or a capital jump.").weak());
                    ui.horizontal(|ui| {
                        if ui.button(format!("{}  Wormhole", icon::SPIRAL)).clicked() {
                            p.confirmed = true;
                        }
                        if ui.button("Not a hole").on_hover_text("A filament, a clone or a jump. Not asked again for this pair for an hour.").clicked() {
                            act = Some("not");
                        }
                    });
                    return;
                }
                egui::Grid::new("web_prompt_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(format!("Sig in {}", name(p.from)));
                    wh_form::sig_field(ui, "web_prompt_sig_here", &mut p.form.sig, "ABC-123", &here_opts);
                    ui.end_row();
                    ui.label("Type");
                    // The likely types first, then every other one: the guess can be wrong.
                    let mut candidates: Vec<&str> = p.candidates.clone();
                    for t in spai_core::whdata::types() {
                        if !candidates.contains(&t.code.as_str()) {
                            candidates.push(t.code.as_str());
                        }
                    }
                    if wh_form::wh_type_picker(ui, "web_prompt_type", 170.0, &mut p.form.wh_type, &candidates) {
                        let sizes = spai_core::wormholes::sizes_for(&[p.form.wh_type.as_str()]);
                        p.form.size = (sizes.len() == 1).then(|| sizes[0]);
                    }
                    ui.end_row();
                    ui.label(format!("Sig in {}", name(p.to)));
                    wh_form::sig_field(ui, "web_prompt_sig_there", &mut p.form.dest_sig, "ABC-123", &there_opts);
                    ui.end_row();
                    ui.label("Size");
                    let known: Vec<&str> = if p.form.wh_type.is_empty() { p.candidates.clone() } else { vec![p.form.wh_type.as_str()] };
                    let sizes: Vec<_> = spai_core::wormholes::sizes_for(&known).into_iter().map(|s| (s, s.short(), s.label())).collect();
                    wh_form::choice_row(ui, &mut p.form.size, &sizes);
                    ui.end_row();
                    ui.label("Time left");
                    let lives: Vec<_> = Life::ALL.into_iter().map(|l| (l, l.short(), l.label())).collect();
                    wh_form::choice_row(ui, &mut p.form.life, &lives);
                    ui.end_row();
                    ui.label("Mass left");
                    let masses: Vec<_> = Mass::ALL.into_iter().map(|m| (m, m.short(), m.label())).collect();
                    wh_form::choice_row(ui, &mut p.form.mass, &masses);
                    ui.end_row();
                    if let Some(e) = &p.form.error {
                        ui.label("");
                        ui.label(egui::RichText::new(e).color(spai_ui::theme::standing::HOSTILE));
                        ui.end_row();
                    }
                    ui.label("Note");
                    ui.add(egui::TextEdit::singleline(&mut p.form.note).desired_width(170.0));
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("{}  Save", icon::FLOPPY_DISK)).clicked() {
                        act = Some("save");
                    }
                    if ui.button("Skip").on_hover_text("Move on without saving it").clicked() {
                        act = Some("skip");
                    }
                });
            });
        let now = spai_core::clock::utc().timestamp();
        match act {
            Some("save") => {
                let Some(mut p) = self.prompts.pop_front() else { return };
                let complete = |sys: Option<i64>, typed: &str| self.complete(sys, typed, None);
                match wh_form::build(&mut p.form, &geo, &complete, now) {
                    Ok((mut w, _)) => {
                        w.source = Source::Auto;
                        w.detected_by = Some(p.who.clone());
                        w.jumped_at = Some(p.at);
                        w.reported_at = p.at;
                        // One hole whichever way it is taken: one already joining the two is filled in.
                        let was = self.holes.iter().find(|h| (h.system_id == p.from && h.dest_system_id == Some(p.to)) || (h.system_id == p.to && h.dest_system_id == Some(p.from)));
                        let w = merge(was, w, now);
                        self.edits.push(Edit::Save(w));
                    }
                    Err(e) => {
                        p.form.error = Some(e);
                        self.prompts.push_front(p);
                    }
                }
            }
            Some("not") => {
                if let Some(p) = self.prompts.pop_front() {
                    self.not_holes.insert((p.from.min(p.to), p.from.max(p.to)), now);
                }
            }
            Some(_) => {
                self.prompts.pop_front();
            }
            None => {}
        }
    }
}

/// Gate jumps under which a pinned system joins a chain, the desktop's default.
pub const PIN_JUMPS: u32 = 30;

/// Pinned on first use: the chains hang from these and routes are measured to them.
pub const DEFAULT_PINS: [&str; 3] = ["C-J6MT", "C-N4OD", "4-HWWF"];

impl WebHost {
    /// The Routes tab: how far each pinned system is from the selected one, and the pins
    /// themselves to add and remove.
    fn routes(&mut self, ui: &mut egui::Ui, geo: &Systems, sel: i64, sel_name: &str) -> Option<i64> {
        let pinned = self.prefs.route_pins.iter().any(|p| p.eq_ignore_ascii_case(sel_name));
        let label = if pinned { format!("Unpin {sel_name}") } else { format!("Pin {sel_name}") };
        let mut change: Option<(String, bool)> = None;
        if ui.button(format!("{}  {label}", egui_phosphor::regular::PUSH_PIN)).clicked() {
            change = Some((sel_name.to_owned(), !pinned));
        }
        ui.add_space(4.0);
        if self.prefs.route_pins.is_empty() {
            ui.label(egui::RichText::new("No pinned systems. Pin the systems you stage in: chains hang from them and routes are measured to them.").weak());
        }
        // Pinned systems, then where the added characters are, by gates, bridges and every live hole.
        let now = spai_core::clock::utc().timestamp();
        let adj = self.plan.hole_graph(geo, &self.holes, now);
        let mut targets: Vec<(String, i64, bool)> = self.prefs.route_pins.iter().filter_map(|p| geo.lookup(p).map(|i| (i.name.clone(), i.id, true))).collect();
        let mut chars: Vec<(&String, &(i64, bool))> = self.chars.iter().collect();
        chars.sort();
        targets.extend(chars.into_iter().map(|(n, (sys, _))| (n.clone(), *sys, false)));
        let (unpin, select) = spai_ui::wh_tab::route_rows(ui, geo, sel, &targets, &adj);
        ui.horizontal(|ui| {
            ui.label("Through");
            if self.plan.kinds_button(ui) {
                self.plan_changed = true;
            }
        });
        if let Some(name) = unpin {
            change = Some((name, false));
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // Picking a suggestion pins it at once; Pin takes what was typed.
            if let Some(i) = system_input(ui, geo, &mut self.sugg, "web_pin", &mut self.pin_input, "System to pin", 160.0).and_then(|id| geo.info_of(id)) {
                change = Some((i.name.clone(), true));
                self.pin_input.clear();
            }
            if ui.button("Pin").clicked() {
                match geo.lookup(self.pin_input.trim()).or_else(|| geo.lookup_prefix(self.pin_input.trim())) {
                    Some(i) => {
                        change = Some((i.name.clone(), true));
                        self.pin_input.clear();
                    }
                    None => self.pin_input = format!("{} (no such system)", self.pin_input.trim()),
                }
            }
        });
        if let Some((name, on)) = change {
            self.prefs.route_pins.retain(|p| !p.eq_ignore_ascii_case(&name));
            if on {
                self.prefs.route_pins.push(name);
            }
            self.dirty = true;
        }
        select
    }
}

impl WhHost for WebHost {
    fn systems(&self) -> Option<Arc<Systems>> {
        Some(self.geo.clone())
    }

    fn holes(&self, now: i64) -> Vec<Wormhole> {
        let touches = |w: &Wormhole, d: spai_core::wormholes::DestClass| w.dest == d || spai_core::wormholes::dest_class(&self.geo, w.system_id) == d;
        self.holes.iter().filter(|w| !w.is_expired(now) && self.filter.matches(w, now, |d| touches(w, d))).cloned().collect()
    }

    fn characters(&self) -> HashMap<String, (i64, bool)> {
        self.chars.clone()
    }

    fn prefs(&self) -> WhPrefs {
        self.prefs.clone()
    }

    fn set_prefs(&mut self, prefs: WhPrefs) {
        self.prefs = prefs;
        self.dirty = true;
    }

    fn saved_layout(&self) -> HashMap<i64, egui::Pos2> {
        self.layout.clone()
    }

    fn save_layout(&mut self, id: i64, at: egui::Pos2) {
        self.layout.insert(id, at);
        self.dirty = true;
    }

    fn clear_layout(&mut self) {
        self.layout.clear();
        self.dirty = true;
    }

    fn blocked(&self, w: &Wormhole, _: i64) -> bool {
        self.disabled(w)
    }

    fn disabled(&self, w: &Wormhole) -> bool {
        self.plan.wh_off_holes.contains(&w.uid) || self.plan.wh_off_systems.contains(&w.system_id) || w.dest_system_id.is_some_and(|b| self.plan.wh_off_systems.contains(&b))
    }

    fn disabled_count(&self) -> usize {
        self.holes.iter().filter(|w| self.plan.wh_off_holes.contains(&w.uid)).count() + self.plan.wh_off_systems.len()
    }

    fn disabled_systems(&self) -> Vec<i64> {
        self.plan.wh_off_systems.clone()
    }

    fn clear_disabled(&mut self) {
        self.plan.wh_off_holes.clear();
        self.plan.wh_off_systems.clear();
        self.plan_changed = true;
    }

    fn group_name(&self, _: &str) -> Option<String> {
        None
    }

    fn open_system(&mut self, _: i64) {}

    /// The desktop's: the system's holes, each to switch on or off for routes.
    fn system_menu(&mut self, ui: &mut egui::Ui, id: i64) {
        let (geo, holes) = (self.geo.clone(), self.holes.clone());
        if self.plan.hole_switches(ui, id, &geo, &holes) {
            self.plan_changed = true;
        }
    }

    /// The desktop's Filter, beside Legend: which holes the tab shows.
    fn toolbar(&mut self, _: &mut WhGraphView, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let active = self.filter.active();
        let label = if active == 0 { format!("{}  Filter", icon::FUNNEL) } else { format!("{}  Filter ({active})", icon::FUNNEL) };
        let btn = ui.button(label).on_hover_text("Which holes the map shows");
        egui::Popup::from_toggle_button_response(&btn).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            self.dirty |= spai_ui::wh_tab::wh_filter_ui(ui, &mut self.filter);
        });
        if ui.add_enabled(active > 0, egui::Button::new(icon::FUNNEL_X)).on_hover_text("Clear the filter").on_disabled_hover_text("No filter set").clicked() {
            self.filter = Default::default();
            self.dirty = true;
        }
        ui.menu_button(icon::GEAR_SIX, |ui| {
            ui.checkbox(&mut self.detect, "Record holes my characters go through")
                .on_hover_text("Watches where your added characters are. A jump the gates cannot explain opens a card to fill the hole in.");
            ui.horizontal(|ui| {
                ui.label("Pinned systems join clusters within");
                self.dirty |= ui
                    .add(egui::DragValue::new(&mut self.prefs.pin_jumps).range(1..=spai_ui::wh_graph::WH_PIN_JUMPS_MAX))
                    .on_hover_text("Gate jumps from a cluster's nearest exit. Further out, a pinned system shows on its own.")
                    .changed();
                ui.label("jumps");
            });
            ui.separator();
            if ui.button(format!("{}  Sharing\u{2026}", icon::USERS_THREE)).on_hover_text("The group: members, invites and roles").clicked() {
                self.open_group = true;
                ui.close();
            }
        });
    }

    /// The pinned systems are in the side panel's Routes tab, which only shows with a system open:
    /// this opens it, on the last system opened, else the first pinned one.
    fn after_tidy(&mut self, view: &mut WhGraphView, ui: &mut egui::Ui) {
        let open = view.selected.is_some() && self.side == Side::Routes;
        if ui
            .add(egui::Button::new(format!("{}  Pinned", egui_phosphor::regular::PUSH_PIN)).selected(open))
            .on_hover_text("Pinned systems, and how far each is from the open system")
            .clicked()
        {
            let staging = self.prefs.route_pins.first().and_then(|p| self.geo.lookup(p)).map(|i| i.id);
            view.selected = view.selected.or(self.last_sel).or(staging);
            self.side = Side::Routes;
        }
    }

    fn side_panel(
        &mut self,
        view: &mut WhGraphView,
        ui: &mut egui::Ui,
        geo: &Arc<Systems>,
        holes: &[Wormhole],
        _: &HashMap<String, (i64, bool)>,
        _: i64,
    ) {
        let Some(sel) = view.selected else { return };
        self.last_sel = Some(sel);
        let Some(info) = geo.info_of(sel) else { return };
        let now = spai_core::clock::utc().timestamp();
        let sigs = self.sigs.get(&sel).cloned().unwrap_or_default();
        let mut pick: Option<i64> = None;
        let shown = egui::Panel::right("wh_web_side").resizable(true).default_size(380.0).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading(&info.name);
                ui.label(egui::RichText::new(format!("{} \u{b7} {:.1}", info.region, info.security)).weak());
                ui.separator();
                let here_n = holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).count();
                ui.horizontal(|ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    // The desktop's tabs, in its order.
                    ui.menu_value(&mut self.side, Side::Holes, if here_n == 0 { "Info".to_owned() } else { format!("Info ({here_n})") });
                    ui.menu_value(&mut self.side, Side::Routes, "Routes");
                    ui.menu_value(&mut self.side, Side::Signatures, if sigs.is_empty() { "Signatures".to_owned() } else { format!("Signatures ({})", sigs.len()) });
                });
                ui.separator();
                match self.side {
                    Side::Holes => {
                ui.horizontal(|ui| {
                    ui.strong("Connections");
                    if self.can_edit && ui.small_button(format!("{}  Add", egui_phosphor::regular::PLUS)).clicked() {
                        self.add_form(Some(sel));
                    }
                });
                let here: Vec<&Wormhole> = holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).collect();
                if here.is_empty() {
                    ui.label(egui::RichText::new("None known here").weak());
                }
                let mut edit: Option<WhForm> = None;
                let mut dead: Option<String> = None;
                let mut flip_route: Option<String> = None;
                let plan = &self.plan;
                let conn = spai_ui::side_lists::connections(ui, sel, &here, now, geo, self.can_edit, &|w| plan.wh_off_holes.contains(&w.uid));
                if let Some(w) = conn.edit.and_then(|uid| here.iter().find(|w| w.uid == uid)) {
                    edit = Some(WhForm::of(w, Some(geo)));
                }
                dead = conn.kill.or(dead);
                flip_route = conn.toggle.or(flip_route);
                if conn.select.is_some() {
                    pick = conn.select;
                }
                if edit.is_some() {
                    self.form = edit;
                }
                if let Some(uid) = dead {
                    self.edits.push(Edit::Dead(uid));
                }
                if let Some(uid) = flip_route {
                    self.plan.toggle_off_hole(&uid);
                    self.plan_changed = true;
                }
                // What a wormhole system is like: its class, effect, statics and celestials.
                if !spai_ui::star_map::is_kspace(sel) {
                    ui.add_space(10.0);
                    spai_ui::wh_tab::wh_system_facts(ui, sel, &info, false);
                }
                    }
                    Side::Signatures => {
                // A probe scanner copy, pasted with Ctrl+V: a browser page cannot read the
                // clipboard by itself the way the app does.
                if self.can_edit {
                    let pasted = ui.input(|i| {
                        i.events.iter().find_map(|e| match e {
                            egui::Event::Paste(t) => Some(t.clone()),
                            _ => None,
                        })
                    });
                    let typing = ui.ctx().memory(|m| m.focused().is_some());
                    if let Some(text) = pasted.filter(|_| !typing) {
                        let scan = spai_core::wormholes::probe_scan(&text);
                        if scan.is_empty() {
                            self.sig_note = Some("That paste holds no probe scanner rows.".into());
                        } else {
                            self.edits.push(Edit::Sigs { system: sel, scan, full: !self.keep_missing });
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{}  In EVE, select all in the probe scanner, copy, then Ctrl+V here", egui_phosphor::regular::CLIPBOARD_TEXT));
                        ui.checkbox(&mut self.keep_missing, "Keep missing")
                            .on_hover_text("Keep signatures the paste does not list. Off, a full paste replaces the list: what is missing is gone from space.");
                        let back = self.sig_browser.undo_button(ui, egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE, Some(geo));
                        if !back.is_empty() {
                            self.edits.push(Edit::SigsBack(back));
                        }
                    });
                    if let Some(n) = &self.sig_note {
                        ui.label(egui::RichText::new(n).weak());
                    }
                    ui.add_space(4.0);
                }
                if sigs.is_empty() {
                    ui.label(egui::RichText::new("No probe scan for this system yet").weak());
                }
                let mut sigs = sigs.clone();
                sigs.sort_by(|a, b| a.sig.cmp(&b.sig));
                let every = self.holes.clone();
                let act = spai_ui::side_lists::sig_table(ui, sel, &info.name, &sigs, &every, now, true, geo, self.can_edit);
                if act.select.is_some() {
                    pick = act.select;
                }
                if let Some(sg) = act.delete {
                    // Through the browser's list, so its undo puts it back.
                    self.sig_browser.forget(vec![(sel, sg.clone())]);
                    self.edits.push(Edit::SigsGone(vec![(sel, sg)]));
                }
                if let Some(w) = act.edit.and_then(|uid| every.iter().find(|w| w.uid == uid)) {
                    self.form = Some(WhForm::of(w, Some(geo)));
                }
                if act.new_hole.is_some() {
                    self.form = act.new_hole;
                }
                    }
                    Side::Routes => {
                        if let Some(id) = self.routes(ui, geo, sel, &info.name) {
                            view.selected = Some(id);
                        }
                    }
                }
            });
        });
        self.side_w = shown.response.rect.width();
        if pick.is_some() {
            view.selected = pick;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo() -> Systems {
        spai_core::test_support::small_universe(&[(31_000_200, "J100200".into(), -1.0, "A-R00001".into())])
    }

    #[test]
    fn a_new_hole_from_the_shared_form_gets_a_fresh_id() {
        let g = geo();
        let mut f = WhForm { system: "J100200".into(), sig: "abc-123".into(), dest: "Jita".into(), life: Some(Life::UnderDay), ..Default::default() };
        let none = |_: Option<i64>, _: &str| Ok(None);
        let (fresh, _) = wh_form::build(&mut f, &g, &none, 1_000).unwrap();
        let w = merge(None, fresh, 1_000);
        assert_eq!((w.signature.as_deref(), w.dest_system_id), (Some("ABC-123"), Some(30_000_142)));
        assert_eq!(w.uid.len(), 32);
        assert_eq!(w.explicit_expiry, Some(1_000 + 86_400));
        let mut bad = WhForm { system: "J100200".into(), dest: "Nowhere".into(), ..Default::default() };
        assert!(wh_form::build(&mut bad, &g, &none, 1_000).is_err());
    }

    #[test]
    fn an_edit_keeps_the_hole_and_what_was_not_read_again() {
        let g = geo();
        let was = Wormhole { uid: "u".into(), system_id: 31_000_200, reported_at: 10, signature: Some("AAA-111".into()), dest_system_id: Some(30_000_142), life: Some(Life::UnderDay), ..Default::default() };
        let mut f = WhForm::of(&was, Some(&g));
        assert_eq!((f.system.as_str(), f.dest.as_str(), f.uid.as_deref()), ("J100200", "Jita", Some("u")));
        f.mass = Some(Mass::Critical);
        f.life = None;
        let none = |_: Option<i64>, _: &str| Ok(None);
        let (fresh, _) = wh_form::build(&mut f, &g, &none, 5_000).unwrap();
        let w = merge(Some(&was), fresh, 5_000);
        assert_eq!((w.uid.as_str(), w.reported_at, w.mass, w.life), ("u", 10, Some(Mass::Critical), Some(Life::UnderDay)));
    }
}
