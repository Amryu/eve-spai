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

/// One line of the list: a group header, `depth` levels in, or a fit under it.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Group { key: String, title: String, n: usize, depth: u8 },
    Fit(Entry),
}

/// Where a hull sits in the Hull tree: its class (by size, then its name), its group, its name.
#[derive(Clone, Debug, PartialEq)]
pub struct HullPath {
    pub class: (u8, String),
    pub group: String,
    pub hull: String,
}

/// The class a hull's size puts it in, its group (Interceptor, Logistics) and its name.
pub fn hull_path(dogma: Option<&spai_fit::Dogma>, t: TypeId) -> HullPath {
    use spai_fit::fleet::HullClass as C;
    let Some(d) = dogma else { return HullPath { class: (u8::MAX, String::new()), group: String::new(), hull: trf!("Type {id}", id = t) } };
    let hull = d.type_(t).map_or_else(|| trf!("Type {id}", id = t), |x| x.name.clone());
    let group = d.type_(t).and_then(|x| d.group(x.group)).map_or_else(String::new, |g| g.name.clone());
    let class = spai_fit::fleet::hull_class(d, t);
    let name = match class {
        C::Frigate => tr!("Frigate"),
        C::Destroyer => tr!("Destroyer"),
        C::Cruiser => tr!("Cruiser"),
        C::Battlecruiser => tr!("Battlecruiser"),
        C::Battleship => tr!("Battleship"),
        C::Capital => tr!("Capital"),
        C::Industrial => tr!("Industrial and other"),
    };
    HullPath { class: (class as u8, name.to_owned()), group, hull }
}

/// The Hull tree: class, group, hull, then the fits, each level by name; a class's own group
/// (Frigate among the frigates) first. Folded headers hide what is under them.
fn hull_tree(list: &[&Entry], hull: &dyn Fn(TypeId) -> HullPath, folded: &BTreeSet<String>) -> Vec<Line> {
    let mut rows: Vec<(HullPath, &Entry)> = list.iter().map(|e| (hull(e.ship), *e)).collect();
    let group_key = |p: &HullPath| (p.group.to_lowercase() != p.class.1.to_lowercase(), p.group.to_lowercase());
    rows.sort_by(|(a, x), (b, y)| (a.class.0, group_key(a), a.hull.to_lowercase(), x.doctrine.is_some(), x.name.to_lowercase()).cmp(&(b.class.0, group_key(b), b.hull.to_lowercase(), y.doctrine.is_some(), y.name.to_lowercase())));
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let class = rows[i].0.class.clone();
        let in_class = rows[i..].iter().take_while(|(p, _)| p.class == class).count();
        let key = format!("class:{}", class.0);
        out.push(Line::Group { key: key.clone(), title: class.1.clone(), n: in_class, depth: 0 });
        let class_shut = folded.contains(&key);
        let end = i + in_class;
        while i < end {
            let group = rows[i].0.group.clone();
            let in_group = rows[i..end].iter().take_while(|(p, _)| p.group == group).count();
            let gkey = format!("group:{}:{}", class.0, group.to_lowercase());
            let group_end = i + in_group;
            if !class_shut {
                out.push(Line::Group { key: gkey.clone(), title: group.clone(), n: in_group, depth: 1 });
            }
            let group_shut = class_shut || folded.contains(&gkey);
            while i < group_end {
                let ship = rows[i].1.ship;
                let in_hull = rows[i..group_end].iter().take_while(|(_, e)| e.ship == ship).count();
                let hkey = format!("hull:{ship}");
                if !group_shut {
                    out.push(Line::Group { key: hkey.clone(), title: rows[i].0.hull.clone(), n: in_hull, depth: 2 });
                    if !folded.contains(&hkey) {
                        out.extend(rows[i..i + in_hull].iter().map(|(_, e)| Line::Fit((*e).clone())));
                    }
                }
                i += in_hull;
            }
        }
    }
    out
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

/// The list as shown: filtered by the query (name, hull, its group or a tag), grouped, folded
/// groups shut.
pub fn lines(entries: &[Entry], hull: &dyn Fn(TypeId) -> HullPath, grouping: Grouping, query: &str, folded: &BTreeSet<String>) -> Vec<Line> {
    let q = query.trim().to_lowercase();
    let hit = |e: &Entry| {
        let p = hull(e.ship);
        q.is_empty() || e.name.to_lowercase().contains(&q) || p.hull.to_lowercase().contains(&q) || p.group.to_lowercase().contains(&q) || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
    };
    let mut list: Vec<&Entry> = entries.iter().filter(|e| hit(e)).collect();
    list.sort_by_key(|e| (e.doctrine.is_some(), e.name.to_lowercase()));
    match grouping {
        Grouping::Flat => return list.into_iter().cloned().map(Line::Fit).collect(),
        Grouping::Hull => return hull_tree(&list, hull, folded),
        Grouping::Tag => {}
    }
    let mut groups: Vec<(String, String, Vec<Entry>)> = Vec::new();
    let mut put = |key: String, title: String, e: &Entry| match groups.iter_mut().find(|(k, _, _)| *k == key) {
        Some((_, _, v)) => v.push(e.clone()),
        None => groups.push((key, title, vec![e.clone()])),
    };
    for e in &list {
        if e.tags.is_empty() {
            put("tag:".into(), tr!("No tag").to_owned(), e);
        }
        for t in &e.tags {
            put(format!("tag:{}", t.to_lowercase()), t.clone(), e);
        }
    }
    // Groups by title, the untagged last.
    groups.sort_by_key(|(k, t, _)| (k == "tag:", t.to_lowercase()));
    let mut out = Vec::new();
    for (key, title, fits) in groups {
        let shut = folded.contains(&key);
        out.push(Line::Group { key, title, n: fits.len(), depth: 0 });
        if !shut {
            out.extend(fits.into_iter().map(Line::Fit));
        }
    }
    out
}

/// How far in a fit's row starts under `grouping`: past its headers.
pub fn fit_indent(grouping: Grouping) -> f32 {
    match grouping {
        Grouping::Flat => 0.0,
        Grouping::Tag => GROUP_INDENT,
        Grouping::Hull => 3.0 * GROUP_INDENT,
    }
}

/// Each level of the tree's step in.
pub const GROUP_INDENT: f32 = 10.0;

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
            picker::open(sim, PickTarget::NewFit);
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
    let hull = |t: TypeId| hull_path(dogma.as_deref(), t);
    let hull_name = |t: TypeId| hull(t).hull;
    let shown = lines(&all, &hull, sim.library_state.grouping, &sim.library_state.query, &sim.library_state.folded);
    let mut load: Option<String> = None;
    let mut open_doctrine: Option<String> = None;
    let mut delete: Option<(String, String, bool)> = None;
    let mut toggle: Option<String> = None;
    let mut edit_tags: Option<(String, Vec<String>)> = None;
    let mut copy: Option<String> = None;
    let indent = fit_indent(sim.library_state.grouping);
    let folded = sim.library_state.folded.clone();
    // A search with no fit to show offers a new fit of each hull it names.
    let hulls: Vec<(TypeId, String)> = if shown.is_empty() && sim.library_state.grouping == Grouping::Hull { hulls_matching(sim, &sim.library_state.query) } else { Vec::new() };
    let mut new_hull: Option<TypeId> = None;
    egui::ScrollArea::vertical().id_salt("sim_library_list").auto_shrink([false, false]).show_rows(ui, ROW_H, shown.len().max(1 + hulls.len()), |ui, range| {
        if shown.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(rows::GAP);
                let note = if !hulls.is_empty() { tr!("No fit of these yet:") } else if all.is_empty() { tr!("No saved fits yet.") } else { tr!("Nothing matches.") };
                ui.add(egui::Label::new(egui::RichText::new(note).weak()).truncate());
            });
            for (t, name) in &hulls {
                let r = rows::row(ui, "fits", ("sim_lib_new", *t), ROW_H, false, |ui| {
                    crate::icons::type_icon(ui, *t, 18.0);
                    let w = ui.available_width();
                    rows::cell(ui, w, egui::RichText::new(trf!("{icon} New {hull} fit", icon = icon::PLUS, hull = name)), "");
                });
                if r.clicked() {
                    new_hull = Some(*t);
                }
            }
        }
        for line in shown.iter().skip(range.start).take(range.end - range.start) {
            match line {
                Line::Group { key, title, n, depth } => {
                    let shut = folded.contains(key);
                    let r = rows::row(ui, "fits", ("sim_lib_group", key), ROW_H, false, |ui| {
                        ui.add_space(f32::from(*depth) * GROUP_INDENT);
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
                        } else if icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete; Ctrl+click deletes without asking")).clicked() {
                            delete = Some((e.id.clone(), e.name.clone(), ui.input(|i| i.modifiers.command)));
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
                        if ui.button(trf!("{icon}  Copy, with its variations", icon = icon::COPY)).clicked() {
                            copy = Some(e.id.clone());
                            ui.close();
                        }
                        if ui.button(trf!("{icon}  Tags…", icon = icon::TAG)).clicked() {
                            edit_tags = Some((e.id.clone(), e.tags.clone()));
                            ui.close();
                        }
                        if ui.button(tr!("Delete")).clicked() {
                            delete = Some((e.id.clone(), e.name.clone(), ui.input(|i| i.modifiers.command)));
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
    if let Some(id) = copy {
        sim.copy_fit(&id, now_ms);
    }
    if let Some(t) = new_hull {
        sim.new_fit(t, now_ms);
        sim.library_state.query.clear();
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
    if let Some((id, name, ctrl)) = delete {
        sim.ask_delete(crate::Deleting::Fit, id, name, ctrl);
    }
}

/// The window that picks a fit to project onto the one on screen: only fits that act on others,
/// grouped and searchable like the Fits tab.
pub fn project_picker(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    if sim.project_picker.is_none() {
        return;
    }
    let Some(dogma) = sim.dogma.clone() else { return };
    let current = sim.session().map(|s| s.id.clone()).unwrap_or_default();
    let outside = sim.outside();
    let mut all: Vec<Entry> = entries(sim).into_iter().filter(|e| e.id != current).collect();
    for s in sim.sessions() {
        if s.id != current && s.fit.ship != 0 && !all.iter().any(|e| e.id == s.id) {
            all.push(Entry { id: s.id.clone(), name: s.fit.name.clone(), ship: s.fit.ship, tags: s.fit.tags.clone(), doctrine: None });
        }
    }
    all.retain(|e| outside.fit(&e.id).is_some_and(|f| spai_fit::projection::acts_on_others(&dogma, f)));
    let mut open = true;
    let mut chosen: Option<String> = None;
    let state = sim.project_picker.as_mut().expect("checked above");
    let size = egui::vec2(420.0, 480.0);
    egui::Window::new(tr!("Project a fit")).id(egui::Id::new("sim_project_picker")).open(&mut open).collapsible(false).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        ui.label(egui::RichText::new(tr!("Fits whose remote modules, drones, command bursts or phenomena generator act on others.")).weak());
        let empty = if all.is_empty() { tr!("No fit acts on others yet: give one a web, a rep, a burst or a generator and switch it on.") } else { tr!("Nothing matches.") };
        chosen = pick_list(ui, "project", state, &all, &[], Some(&dogma), empty);
    });
    if let Some(id) = chosen {
        // A booster covers all its charges from the start: one entry stands for every boost it runs.
        let charges = match (&sim.dogma, sim.sessions().iter().find(|s| s.id == id).map(|s| &s.fit).or_else(|| sim.fit_docs.get(&id))) {
            (Some(d), Some(fit)) => spai_fit::fleet::boostable_charges(d, fit),
            _ => Vec::new(),
        };
        sim.edit(spai_fit::history::EditKind::Other, now_ms, |f| match f.projected_fits.iter_mut().find(|p| p.fit_id == id) {
            Some(p) => p.count += 1,
            None => f.projected_fits.push(spai_fit::fit::ProjectedFit { fit_id: id, charges, ..Default::default() }),
        });
    }
    if !open {
        sim.project_picker = None;
    }
}

/// A picker's list: the grouping buttons, the search, then `first` under a heading of its own when
/// there are any, and the fits as the Fits tab groups them. The fit clicked.
pub fn pick_list(ui: &mut egui::Ui, salt: &str, state: &mut LibraryState, all: &[Entry], first: &[(String, Vec<Entry>)], dogma: Option<&spai_fit::Dogma>, empty: &str) -> Option<String> {
    use spai_ui::widgets::SteadySelect as _;
    let hull = |t: TypeId| hull_path(dogma, t);
    ui.horizontal(|ui| {
        let w = egui::vec2(((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(40.0), 22.0);
        for (g, label) in [(Grouping::Hull, tr!("Hull")), (Grouping::Tag, tr!("Tag")), (Grouping::Flat, tr!("A–Z"))] {
            if ui.menu_label_sized(w, state.grouping == g, label).clicked() {
                state.grouping = g;
            }
        }
    });
    ui.add_sized([ui.available_width(), 22.0], egui::TextEdit::singleline(&mut state.query).hint_text(trf!("{icon} Name, hull or tag", icon = icon::MAGNIFYING_GLASS)));
    ui.add_space(2.0);
    let mut shown: Vec<Line> = Vec::new();
    for (title, fits) in first {
        let found = lines(fits, &hull, Grouping::Flat, &state.query, &state.folded);
        if found.is_empty() {
            continue;
        }
        let key = format!("first:{title}");
        shown.push(Line::Group { key: key.clone(), title: title.clone(), n: found.len(), depth: 0 });
        if !state.folded.contains(&key) {
            shown.extend(found);
        }
    }
    let lead = shown.len();
    shown.extend(lines(all, &hull, state.grouping, &state.query, &state.folded));
    let mut chosen = None;
    let mut toggle: Option<String> = None;
    egui::ScrollArea::vertical().id_salt(("sim_pick_list", salt)).auto_shrink([false, false]).show_rows(ui, ROW_H, shown.len().max(1), |ui, range| {
        if shown.is_empty() {
            ui.label(egui::RichText::new(empty).weak());
        }
        for (k, line) in shown.iter().enumerate().skip(range.start).take(range.end - range.start) {
            match line {
                Line::Group { key, title, n, depth } => {
                    let shut = state.folded.contains(key);
                    let r = rows::row(ui, salt, ("sim_pick_group", salt, key), ROW_H, false, |ui| {
                        ui.add_space(f32::from(*depth) * GROUP_INDENT);
                        ui.add(egui::Label::new(if shut { icon::CARET_RIGHT } else { icon::CARET_DOWN }).selectable(false));
                        let w = ui.available_width();
                        rows::cell(ui, w, egui::RichText::new(format!("{title}  ({n})")).strong(), "");
                    });
                    if r.clicked() {
                        toggle = Some(key.clone());
                    }
                }
                Line::Fit(e) => {
                    let r = rows::row(ui, salt, ("sim_pick", salt, k, &e.id), ROW_H, false, |ui| {
                        ui.add_space(if k < lead { GROUP_INDENT } else { fit_indent(state.grouping) });
                        crate::icons::type_icon(ui, e.ship, 18.0);
                        let w = ui.available_width() - if e.doctrine.is_some() { 22.0 } else { 0.0 };
                        let p = hull(e.ship);
                        let label = if k < lead || state.grouping != Grouping::Hull { format!("{}: {}", p.hull, e.name) } else { e.name.clone() };
                        rows::cell(ui, w.max(40.0), egui::RichText::new(label), "").on_hover_ui(|ui| entry_tip(ui, e, &p.hull));
                        if e.doctrine.is_some() {
                            ui.add(egui::Label::new(egui::RichText::new(icon::SEAL_CHECK).color(ui.visuals().hyperlink_color)).selectable(false));
                        }
                    });
                    if r.clicked() {
                        chosen = Some(e.id.clone());
                    }
                }
            }
        }
    });
    if let Some(k) = toggle {
        if !state.folded.remove(&k) {
            state.folded.insert(k);
        }
    }
    chosen
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

/// Published hulls whose names hold every word of the search, for a fit started from it.
fn hulls_matching(sim: &SimUi, query: &str) -> Vec<(TypeId, String)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let Some(d) = sim.dogma.as_ref() else { return Vec::new() };
    if words.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(TypeId, String)> = d
        .types
        .iter()
        .filter(|t| t.published && t.category == spai_fit::ids::category::SHIP && t.market_group.is_some())
        .filter(|t| {
            let n = t.name.to_lowercase();
            words.iter().all(|w| n.contains(w.as_str()))
        })
        .map(|t| (t.id, t.name.clone()))
        .collect();
    out.sort_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| a.1.cmp(&b.1)));
    out.truncate(8);
    out
}

#[cfg(test)]
pub(crate) fn hulls_matching_for_test(sim: &SimUi, query: &str) -> Vec<(TypeId, String)> {
    hulls_matching(sim, query)
}
