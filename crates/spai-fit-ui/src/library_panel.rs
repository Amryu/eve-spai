//! Saved fits, grouped by hull or by tag with the doctrine presets among them, and the doctrine
//! presets on a tab of their own.

use std::collections::BTreeSet;

use egui_phosphor::regular as icon;
use spai_fit::ids::TypeId;
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::picker::{self, PickTarget};
use crate::rows::{self, ROW_H};
use crate::{DoctrineSide, HostRequest, SimUi};

/// How the Fits tab groups its list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    #[default]
    Hull,
    Tag,
    Flat,
}

/// The Fits tab's own state.
#[derive(Default)]
pub struct LibraryState {
    pub grouping: Grouping,
    pub query: String,
    /// Group headers folded shut, by key.
    pub folded: BTreeSet<String>,
    /// The fit whose tags are being typed, and the text so far.
    pub tag_edit: Option<(String, String)>,
}

/// A fit of the list: one of the library's, or a doctrine preset among them.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub ship: TypeId,
    pub tags: Vec<String>,
    pub doctrine: Option<DoctrineSide>,
}

/// One line of the list: a group header or a fit under it.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Group { key: String, title: String, n: usize },
    Fit(Entry),
}

/// The library's fits and the doctrine presets as one list. A preset carries its own tags and
/// its side, so it sorts in with the fits it belongs to.
pub fn entries(sim: &SimUi) -> Vec<Entry> {
    let mut out: Vec<Entry> = sim
        .library
        .iter()
        .map(|e| Entry { id: e.id.clone(), name: e.name.clone(), ship: e.ship, tags: sim.fit_docs.get(&e.id).map(|f| f.tags.clone()).unwrap_or_default(), doctrine: None })
        .collect();
    out.extend(sim.doctrines.iter().map(|d| {
        let mut tags = d.tags.clone();
        let side = trf!("{side} doctrine", side = d.side.name());
        if !tags.contains(&side) {
            tags.push(side);
        }
        Entry { id: d.id.clone(), name: d.name.clone(), ship: d.ship, tags, doctrine: Some(d.side) }
    }));
    out
}

/// The list as shown: filtered by the query (name, hull or tag), grouped, folded groups shut.
pub fn lines(entries: &[Entry], hull_name: &dyn Fn(TypeId) -> String, grouping: Grouping, query: &str, folded: &BTreeSet<String>) -> Vec<Line> {
    let q = query.trim().to_lowercase();
    let hit = |e: &Entry| q.is_empty() || e.name.to_lowercase().contains(&q) || hull_name(e.ship).to_lowercase().contains(&q) || e.tags.iter().any(|t| t.to_lowercase().contains(&q));
    let mut list: Vec<&Entry> = entries.iter().filter(|e| hit(e)).collect();
    list.sort_by_key(|e| (e.doctrine.is_some(), e.name.to_lowercase()));
    if grouping == Grouping::Flat {
        return list.into_iter().cloned().map(Line::Fit).collect();
    }
    let mut groups: Vec<(String, String, Vec<Entry>)> = Vec::new();
    let mut put = |key: String, title: String, e: &Entry| match groups.iter_mut().find(|(k, _, _)| *k == key) {
        Some((_, _, v)) => v.push(e.clone()),
        None => groups.push((key, title, vec![e.clone()])),
    };
    for e in &list {
        match grouping {
            Grouping::Hull => put(format!("hull:{}", e.ship), hull_name(e.ship), e),
            _ => {
                if e.tags.is_empty() {
                    put("tag:".into(), tr!("No tag").to_owned(), e);
                }
                for t in &e.tags {
                    put(format!("tag:{}", t.to_lowercase()), t.clone(), e);
                }
            }
        }
    }
    // Groups by title, the untagged last.
    groups.sort_by_key(|(k, t, _)| (k == "tag:", t.to_lowercase()));
    let mut out = Vec::new();
    for (key, title, fits) in groups {
        let shut = folded.contains(&key);
        out.push(Line::Group { key, title, n: fits.len() });
        if !shut {
            out.extend(fits.into_iter().map(Line::Fit));
        }
    }
    out
}

/// Tags as typed: split at commas, trimmed, empty and repeated ones dropped.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(t)) {
            out.push(t.to_owned());
        }
    }
    out
}

impl SimUi {
    /// Gives a library fit new tags: through the editor when it is open, else straight to the host.
    pub fn set_fit_tags(&mut self, id: &str, tags: Vec<String>, now_ms: u64) {
        if let Some(i) = self.sessions().iter().position(|s| s.id == id) {
            let back = self.active_tab();
            self.activate(i);
            let t = tags.clone();
            self.edit(spai_fit::history::EditKind::Other, now_ms, move |f| f.tags = t);
            self.activate(back);
        } else if let Some(doc) = self.fit_docs.get_mut(id) {
            doc.tags = tags;
            let (name, ship, json) = (doc.name.clone(), doc.ship, spai_fit::doc::to_json(doc));
            self.request(HostRequest::SaveFit { id: id.to_owned(), name, ship, doc: json });
        }
        if let Some(s) = self.sessions().iter().find(|s| s.id == id) {
            let fit = s.fit.clone();
            self.fit_docs.insert(id.to_owned(), fit);
        }
    }
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    use spai_ui::widgets::SteadySelect as _;
    rows::heading(ui, tr!("Fits"), None, |ui| {
        if ui.small_button(trf!("{icon} New", icon = icon::PLUS)).on_hover_text(tr!("Start a fit from a hull")).clicked() {
            picker::open(sim, PickTarget::Ship);
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        let w = egui::vec2(((ui.available_width() - rows::GAP - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(40.0), 22.0);
        for (g, label) in [(Grouping::Hull, tr!("Hull")), (Grouping::Tag, tr!("Tag")), (Grouping::Flat, tr!("A–Z"))] {
            if ui.menu_label_sized(w, sim.library_state.grouping == g, label).clicked() {
                sim.library_state.grouping = g;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        let w = ui.available_width() - rows::GAP;
        ui.add_sized([w, 22.0], egui::TextEdit::singleline(&mut sim.library_state.query).hint_text(trf!("{icon} Name, hull or tag", icon = icon::MAGNIFYING_GLASS)));
    });
    ui.add_space(2.0);
    let current = sim.session().map(|s| s.id.clone());
    let dirty = sim.session().is_some_and(|s| s.dirty);
    let all = entries(sim);
    let dogma = sim.dogma.clone();
    let hull_name = |t: TypeId| dogma.as_ref().and_then(|d| d.type_(t)).map_or_else(|| trf!("Type {id}", id = t), |d| d.name.clone());
    let shown = lines(&all, &hull_name, sim.library_state.grouping, &sim.library_state.query, &sim.library_state.folded);
    let mut load: Option<String> = None;
    let mut open_doctrine: Option<String> = None;
    let mut delete: Option<String> = None;
    let mut toggle: Option<String> = None;
    let mut edit_tags: Option<(String, Vec<String>)> = None;
    let indent = if sim.library_state.grouping == Grouping::Flat { 0.0 } else { 12.0 };
    let folded = sim.library_state.folded.clone();
    egui::ScrollArea::vertical().id_salt("sim_library_list").auto_shrink([false, false]).show_rows(ui, ROW_H, shown.len().max(1), |ui, range| {
        if shown.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(rows::GAP);
                let note = if all.is_empty() { tr!("No saved fits yet.") } else { tr!("Nothing matches.") };
                ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
            });
        }
        for line in shown.iter().skip(range.start).take(range.end - range.start) {
            match line {
                Line::Group { key, title, n } => {
                    let shut = folded.contains(key);
                    let r = rows::row(ui, "fits", ("sim_lib_group", key), ROW_H, false, |ui| {
                        ui.add(egui::Label::new(if shut { icon::CARET_RIGHT } else { icon::CARET_DOWN }).selectable(false));
                        let w = ui.available_width();
                        rows::cell(ui, w, egui::RichText::new(format!("{title}  ({n})")).strong(), "");
                    });
                    if r.clicked() {
                        toggle = Some(key.clone());
                    }
                }
                Line::Fit(e) => {
                    let selected = current.as_deref() == Some(e.id.as_str());
                    let r = rows::row(ui, "fits", ("sim_lib", &e.id, e.doctrine.is_some()), ROW_H, selected, |ui| {
                        ui.add_space(indent);
                        crate::icons::type_icon(ui, e.ship, 18.0);
                        let side = if e.doctrine.is_some() { 22.0 } else { 26.0 + rows::GAP };
                        let w = ui.available_width() - side;
                        let label = if selected && dirty { format!("{}*", e.name) } else { e.name.clone() };
                        let label = if selected { egui::RichText::new(label).strong() } else { egui::RichText::new(label) };
                        rows::cell(ui, w.max(40.0), label, "").on_hover_ui(|ui| entry_tip(ui, e, &hull_name(e.ship)));
                        if e.doctrine.is_some() {
                            ui.add(egui::Label::new(egui::RichText::new(icon::SEAL_CHECK).color(ui.visuals().hyperlink_color)).selectable(false)).on_hover_text(tr!("A doctrine fit: opens as a copy"));
                        } else if icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete")).clicked() {
                            delete = Some(e.id.clone());
                        }
                    });
                    if r.clicked() {
                        if e.doctrine.is_some() {
                            open_doctrine = Some(e.id.clone());
                        } else {
                            load = Some(e.id.clone());
                        }
                    }
                    r.context_menu(|ui| {
                        ui.set_min_width(220.0);
                        if e.doctrine.is_some() {
                            if ui.button(tr!("Open a copy")).clicked() {
                                open_doctrine = Some(e.id.clone());
                                ui.close();
                            }
                            return;
                        }
                        if ui.button(tr!("Open")).clicked() {
                            load = Some(e.id.clone());
                            ui.close();
                        }
                        if ui.button(trf!("{icon}  Tags…", icon = icon::TAG)).clicked() {
                            edit_tags = Some((e.id.clone(), e.tags.clone()));
                            ui.close();
                        }
                        if ui.button(tr!("Delete")).clicked() {
                            delete = Some(e.id.clone());
                            ui.close();
                        }
                    });
                }
            }
        }
    });
    if let Some((id, tags)) = edit_tags {
        sim.library_state.tag_edit = Some((id, tags.join(", ")));
    }
    if let Some((id, tags)) = tag_window(sim, ui.ctx()) {
        sim.set_fit_tags(&id, tags, now_ms);
    }
    if let Some(k) = toggle {
        if !sim.library_state.folded.remove(&k) {
            sim.library_state.folded.insert(k);
        }
    }
    if let Some(id) = open_doctrine {
        sim.open_doctrine(&id, now_ms);
    }
    if let Some(id) = load {
        if current.as_deref() != Some(id.as_str()) {
            sim.request(HostRequest::LoadFit(id));
        }
    }
    if let Some(id) = delete {
        if current.as_deref() == Some(id.as_str()) {
            sim.close_id(&id);
        }
        sim.library.retain(|e| e.id != id);
        sim.forget_fit_doc(&id);
        sim.request(HostRequest::DeleteFit(id));
    }
}

/// A fit's tooltip in the list: its name, hull and tags.
fn entry_tip(ui: &mut egui::Ui, e: &Entry, hull: &str) {
    ui.set_max_width(320.0);
    ui.label(egui::RichText::new(&e.name).strong());
    ui.label(egui::RichText::new(hull).weak());
    if let Some(side) = e.doctrine {
        ui.label(trf!("{icon} {side} doctrine, opens as a copy", icon = icon::SEAL_CHECK, side = side.name()));
    }
    if !e.tags.is_empty() {
        ui.label(trf!("{icon} {tags}", icon = icon::TAG, tags = e.tags.join(", ")));
    }
}

/// The small window that takes a fit's tags, comma separated; the tags once saved.
fn tag_window(sim: &mut SimUi, ctx: &egui::Context) -> Option<(String, Vec<String>)> {
    let (id, text) = sim.library_state.tag_edit.as_mut()?;
    let mut open = true;
    let mut done = false;
    egui::Window::new(tr!("Tags")).id(egui::Id::new("sim_tags")).open(&mut open).collapsible(false).resizable(false).default_width(320.0).show(ctx, |ui| {
        ui.label(egui::RichText::new(tr!("Comma separated, for grouping the list by tag.")).weak());
        let r = ui.add_sized([300.0, 22.0], egui::TextEdit::singleline(text).hint_text(tr!("e.g. kiting, Alpha, home defence")));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(4.0);
        if ui.button(tr!("Save")).clicked() || enter {
            done = true;
        }
    });
    let out = done.then(|| (id.clone(), parse_tags(text)));
    if done || !open {
        sim.library_state.tag_edit = None;
    }
    out
}

/// The doctrine presets the server lists, by side; a click opens a copy.
pub fn doctrines_tab(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    rows::heading(ui, tr!("Doctrines"), None, |ui| {
        if icon_button(ui, icon::ARROWS_CLOCKWISE).on_hover_text(tr!("Fetch the presets again")).clicked() {
            sim.request(HostRequest::FetchDoctrines);
        }
    });
    if sim.doctrines.is_empty() {
        let note = if sim.access.signed_in { tr!("No presets published yet.") } else { tr!("Sign in with an Imperium character to see the coalition's presets.") };
        ui.horizontal(|ui| {
            ui.add_space(rows::GAP);
            ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
        });
        return;
    }
    let mut open: Option<String> = None;
    let mut delete: Option<String> = None;
    egui::ScrollArea::vertical().id_salt("sim_doctrine_list").auto_shrink([false, false]).show(ui, |ui| {
        for side in DoctrineSide::ALL {
            let list: Vec<(String, String, u32, Vec<String>)> = sim.doctrines.iter().filter(|d| d.side == side).map(|d| (d.id.clone(), d.name.clone(), d.ship, d.tags.clone())).collect();
            if list.is_empty() {
                continue;
            }
            rows::row(ui, "doctrines", ("sim_doc_side", side.name()), ROW_H, false, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(side.name()).strong()).selectable(false));
            });
            for (id, name, ship, tags) in list {
                let ship_name = sim.dogma.as_ref().and_then(|d| d.type_(ship)).map_or(String::new(), |t| t.name.clone());
                let admin = sim.access.admin;
                let e = Entry { id: id.clone(), name: name.clone(), ship, tags, doctrine: Some(side) };
                let r = rows::row(ui, "doctrines", ("sim_doc", &id), ROW_H, false, |ui| {
                    ui.add_space(12.0);
                    crate::icons::type_icon(ui, ship, 18.0);
                    let trash = if admin { 26.0 + rows::GAP } else { 0.0 };
                    let w = ui.available_width() - trash;
                    rows::cell(ui, w.max(40.0), egui::RichText::new(&name), "").on_hover_ui(|ui| entry_tip(ui, &e, &ship_name));
                    if admin && icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete the preset")).clicked() {
                        delete = Some(id.clone());
                    }
                });
                if r.clicked() {
                    open = Some(id.clone());
                }
            }
        }
    });
    if let Some(id) = open {
        sim.open_doctrine(&id, now_ms);
    }
    if let Some(id) = delete {
        sim.doctrines.retain(|d| d.id != id);
        sim.request(HostRequest::DeleteDoctrine(id));
    }
}
