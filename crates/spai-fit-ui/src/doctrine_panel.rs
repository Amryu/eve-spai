//! The Doctrines tab: supplied doctrines, the server's single-fit presets and the pilot's own
//! doctrines in one tree of alliance, kind, doctrine and fits, with a filter by where they come
//! from.

use std::collections::BTreeSet;

use egui_phosphor::regular as icon;
use spai_fit::doctrine::{category_rank, Doctrine, DoctrineFit, FitRole};
use spai_fit::fit::Fit;
use spai_fit::ids::TypeId;
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::rows::{self, ROW_H};
use crate::{DoctrineSide, HostRequest, SimUi};

/// Which doctrines the tab lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    All,
    Supplied,
    Mine,
}

/// Where a listed doctrine comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Supplied(DoctrineSide),
    /// A single-fit preset, grouped by its tag.
    Preset(DoctrineSide),
    Own,
    /// The pilot's link to a supplied doctrine; `false` while that one is not here (signed out).
    Link(bool),
}

impl Kind {
    pub fn supplied(self) -> bool {
        matches!(self, Kind::Supplied(_) | Kind::Preset(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub kind: Kind,
    /// As listed: for a link, the supplied doctrine under the link's id and placement.
    pub doctrine: Doctrine,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Alliance { key: String, title: String, n: usize },
    Category { key: String, title: String, n: usize },
    Doctrine { at: usize, key: String, open: bool },
    /// Under an open doctrine with a source: when it last changed.
    Info { at: usize },
    Fit { at: usize, fit: usize },
}

/// A pilot's own doctrine being edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edit {
    pub doctrine: Doctrine,
    pub tags: String,
    pub query: String,
}

/// Doctrines read from a dropped file, waiting for an admin to publish them.
#[derive(Clone, Debug, PartialEq)]
pub struct Publish {
    pub doctrines: Vec<Doctrine>,
    pub side: DoctrineSide,
}

#[derive(Default)]
pub struct TabState {
    pub source: Source,
    pub query: String,
    /// Alliances and kinds folded shut.
    pub folded: BTreeSet<String>,
    /// Doctrines opened to their fits.
    pub opened: BTreeSet<String>,
    pub edit: Option<Edit>,
    pub publish: Option<Publish>,
}

/// The heading an own doctrine with no alliance sorts under.
pub fn mine_title() -> &'static str {
    tr!("My doctrines")
}

/// Everything the tab can list: supplied doctrines, presets grouped by tag, own doctrines with
/// links resolved.
pub fn shown(sim: &SimUi) -> Vec<Shown> {
    let mut out: Vec<Shown> = sim.supplied.iter().map(|s| Shown { kind: Kind::Supplied(s.side), doctrine: s.doctrine.clone() }).collect();
    let mut presets: Vec<(DoctrineSide, String, Doctrine)> = Vec::new();
    for d in &sim.doctrines {
        let Some(fit) = sim.fit_docs.get(&d.id) else { continue };
        let group = d.tags.first().cloned().unwrap_or_else(|| d.name.clone());
        let role = sim.dogma.as_deref().map_or(FitRole::Line, |dg| FitRole::of_hull(dg, d.ship));
        let entry = DoctrineFit { fit_id: d.id.clone(), fit: Some(fit.clone()), role, main: false, note: String::new() };
        match presets.iter_mut().find(|(s, g, _)| *s == d.side && *g == group) {
            Some((_, _, doc)) => doc.fits.push(entry),
            None => presets.push((
                d.side,
                group.clone(),
                Doctrine { id: format!("preset:{}:{group}", d.side.name()), alliance: d.side.name().to_owned(), category: tr!("Presets").to_owned(), name: group, tags: d.tags.clone(), fits: vec![entry], ..Doctrine::default() },
            )),
        }
    }
    out.extend(presets.into_iter().map(|(side, _, doctrine)| Shown { kind: Kind::Preset(side), doctrine }));
    for own in &sim.own_doctrines {
        if own.link {
            let target = own.origin.as_deref().and_then(|o| sim.supplied.iter().find(|s| s.doctrine.id == o));
            let doctrine = match target {
                Some(s) => Doctrine { id: own.id.clone(), alliance: own.alliance.clone(), category: own.category.clone(), name: own.name.clone(), tags: own.tags.clone(), origin: own.origin.clone(), link: true, ..s.doctrine.clone() },
                None => own.clone(),
            };
            out.push(Shown { kind: Kind::Link(target.is_some()), doctrine });
        } else {
            out.push(Shown { kind: Kind::Own, doctrine: own.clone() });
        }
    }
    out
}

fn wanted(kind: Kind, source: Source) -> bool {
    match source {
        Source::All => true,
        Source::Supplied => kind.supplied(),
        Source::Mine => !kind.supplied(),
    }
}

/// A fit of a doctrine, from the doctrine or the library.
pub fn fit_of<'a>(sim_docs: &'a std::collections::HashMap<String, Fit>, f: &'a DoctrineFit) -> Option<&'a Fit> {
    f.fit.as_ref().or_else(|| sim_docs.get(&f.fit_id))
}

/// The tree as listed: filtered by source and search, alliances and kinds folded as the pilot
/// left them, doctrines open where opened (all of them while searching). A search that names a
/// doctrine shows all its fits, one that names a fit shows that fit.
pub fn lines(all: &[Shown], source: Source, query: &str, folded: &BTreeSet<String>, opened: &BTreeSet<String>, fit_label: &dyn Fn(&DoctrineFit) -> String) -> Vec<Line> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let hits = |hay: &str| words.iter().all(|w| hay.contains(w.as_str()));
    let mut picked: Vec<(usize, Vec<usize>)> = Vec::new();
    for (at, s) in all.iter().enumerate() {
        if !wanted(s.kind, source) {
            continue;
        }
        let d = &s.doctrine;
        let mut order: Vec<usize> = (0..d.fits.len()).collect();
        order.sort_by_key(|i| (!d.fits[*i].main, d.fits[*i].role));
        if words.is_empty() {
            picked.push((at, order));
            continue;
        }
        let head = format!("{} {} {} {}", d.name, d.alliance, d.category, d.tags.join(" ")).to_lowercase();
        if hits(&head) {
            picked.push((at, order));
            continue;
        }
        let fits: Vec<usize> = order.into_iter().filter(|i| hits(&format!("{head} {}", fit_label(&d.fits[*i])).to_lowercase())).collect();
        if !fits.is_empty() {
            picked.push((at, fits));
        }
    }
    let alliance_of = |d: &Doctrine| if d.alliance.trim().is_empty() { mine_title().to_owned() } else { d.alliance.trim().to_owned() };
    let category_of = |d: &Doctrine| if d.category.trim().is_empty() { tr!("Other").to_owned() } else { d.category.trim().to_owned() };
    picked.sort_by(|(a, _), (b, _)| {
        let (a, b) = (&all[*a].doctrine, &all[*b].doctrine);
        let mine = |d: &Doctrine| d.alliance.trim().is_empty();
        (!mine(a), alliance_of(a).to_lowercase(), category_rank(&category_of(a)), category_of(a).to_lowercase(), a.name.to_lowercase()).cmp(&(!mine(b), alliance_of(b).to_lowercase(), category_rank(&category_of(b)), category_of(b).to_lowercase(), b.name.to_lowercase()))
    });
    let searching = !words.is_empty();
    let mut out = Vec::new();
    let mut i = 0;
    while i < picked.len() {
        let alliance = alliance_of(&all[picked[i].0].doctrine);
        let a_key = format!("a:{}", alliance.to_lowercase());
        let a_end = picked[i..].iter().position(|(at, _)| alliance_of(&all[*at].doctrine) != alliance).map_or(picked.len(), |n| i + n);
        out.push(Line::Alliance { key: a_key.clone(), title: alliance.clone(), n: a_end - i });
        if folded.contains(&a_key) && !searching {
            i = a_end;
            continue;
        }
        let mut j = i;
        while j < a_end {
            let category = category_of(&all[picked[j].0].doctrine);
            let c_key = format!("c:{}\u{1}{}", alliance.to_lowercase(), category.to_lowercase());
            let c_end = picked[j..a_end].iter().position(|(at, _)| category_of(&all[*at].doctrine).to_lowercase() != category.to_lowercase()).map_or(a_end, |n| j + n);
            out.push(Line::Category { key: c_key.clone(), title: category, n: c_end - j });
            if !folded.contains(&c_key) || searching {
                for (at, fits) in &picked[j..c_end] {
                    let key = format!("d:{}", all[*at].doctrine.id);
                    let open = searching || opened.contains(&key);
                    out.push(Line::Doctrine { at: *at, key, open });
                    if open && all[*at].doctrine.updated.is_some() {
                        out.push(Line::Info { at: *at });
                    }
                    if open {
                        out.extend(fits.iter().map(|f| Line::Fit { at: *at, fit: *f }));
                    }
                }
            }
            j = c_end;
        }
        i = a_end;
    }
    out
}

/// Year, month and day of a Unix time.
pub fn civil(t: i64) -> (i64, u32, u32) {
    let z = t.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `2025-10-04`, the same in every language.
pub fn date(t: i64) -> String {
    let (y, m, d) = civil(t);
    format!("{y}-{m:02}-{d:02}")
}

/// How long ago, in the largest whole unit.
pub fn age(then: i64, now: i64) -> String {
    let days = (now - then).max(0) / 86_400;
    match days {
        0 => tr!("today").to_owned(),
        1 => tr!("yesterday").to_owned(),
        d if d < 60 => trf!("{n} days ago", n = d),
        d if d < 730 => trf!("{n} months ago", n = d / 30),
        d => trf!("{n} years ago", n = d / 365),
    }
}

pub fn role_name(role: FitRole) -> &'static str {
    match role {
        FitRole::Line => tr!("Line"),
        FitRole::Logi => tr!("Logistics"),
        FitRole::Command => tr!("Boosts"),
        FitRole::Tackle => tr!("Tackle"),
        FitRole::Ewar => tr!("Ewar"),
        FitRole::Support => tr!("Support"),
    }
}

fn hull_name(sim: &SimUi, ship: TypeId) -> String {
    sim.dogma.as_ref().and_then(|d| d.type_(ship)).map_or_else(|| trf!("Type {id}", id = ship), |t| t.name.clone())
}

/// `Hull: fit name`, the way a fit reads in the tree and in searches.
pub fn fit_label(sim: &SimUi, f: &DoctrineFit) -> String {
    match fit_of(&sim.fit_docs, f) {
        Some(fit) => format!("{}: {}", hull_name(sim, fit.ship), fit.name),
        None => sim.library.iter().find(|e| e.id == f.fit_id).map_or_else(|| tr!("A fit not here").to_owned(), |e| format!("{}: {}", hull_name(sim, e.ship), e.name)),
    }
}

enum Action {
    Toggle(String),
    ToggleOpen(String),
    OpenFit(usize, usize),
    Fleet(usize),
    Copy(usize),
    Link(usize),
    Edit(usize),
    Delete(usize),
    Publish(usize),
    DeleteSupplied(String),
    Url(String),
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    use spai_ui::widgets::SteadySelect as _;
    rows::heading(ui, tr!("Doctrines"), None, |ui| {
        if icon_button(ui, icon::ARROWS_CLOCKWISE).on_hover_text(tr!("Fetch the doctrines again")).clicked() {
            sim.request(HostRequest::FetchDoctrines);
        }
        if ui.small_button(trf!("{icon} New", icon = icon::PLUS)).on_hover_text(tr!("Start a doctrine of your own")).clicked() {
            sim.new_own_doctrine(now_ms);
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        let w = egui::vec2(((ui.available_width() - rows::GAP - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(40.0), 22.0);
        for (s, label) in [(Source::All, tr!("All")), (Source::Supplied, tr!("Supplied")), (Source::Mine, tr!("Mine"))] {
            if ui.menu_label_sized(w, sim.doctrine_tab.source == s, label).clicked() {
                sim.doctrine_tab.source = s;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        let w = ui.available_width() - rows::GAP;
        ui.add_sized([w, 22.0], egui::TextEdit::singleline(&mut sim.doctrine_tab.query).hint_text(trf!("{icon} Doctrine, hull, fit or tag", icon = icon::MAGNIFYING_GLASS)));
    });
    if sim.access.admin {
        take_dropped(sim, ui.ctx());
    }
    ui.add_space(2.0);
    let all = shown(sim);
    let label = |f: &DoctrineFit| fit_label(sim, f);
    let st = &sim.doctrine_tab;
    let list = lines(&all, st.source, &st.query, &st.folded, &st.opened, &label);
    let labels: Vec<String> = list.iter().map(|l| if let Line::Fit { at, fit } = l { label(&all[*at].doctrine.fits[*fit]) } else { String::new() }).collect();
    let mut action: Option<Action> = None;
    let now_s = (now_ms / 1000) as i64;
    let admin = sim.access.admin;
    let current = sim.session().map(|s| s.id.clone());
    egui::ScrollArea::vertical().id_salt("sim_doctrine_tree").auto_shrink([false, false]).show_rows(ui, ROW_H, list.len().max(1), |ui, range| {
        if list.is_empty() {
            let note = if !sim.doctrine_tab.query.trim().is_empty() {
                tr!("Nothing matches.")
            } else if sim.doctrine_tab.source == Source::Mine {
                tr!("No doctrines of your own yet: start one with New, or copy or link a supplied one.")
            } else if sim.access.signed_in {
                tr!("No doctrines published yet.")
            } else {
                tr!("Sign in with an Imperium character to see the coalition's doctrines.")
            };
            ui.horizontal(|ui| {
                ui.add_space(rows::GAP);
                ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
            });
        }
        for (n, line) in list.iter().enumerate().skip(range.start).take(range.end - range.start) {
            match line {
                Line::Alliance { key, title, n } | Line::Category { key, title, n } => {
                    let category = matches!(line, Line::Category { .. });
                    let shut = sim.doctrine_tab.folded.contains(key);
                    let r = rows::row(ui, "doctrines", ("sim_doc_group", key), ROW_H, false, |ui| {
                        if category {
                            ui.add_space(12.0);
                        }
                        ui.add(egui::Label::new(if shut { icon::CARET_RIGHT } else { icon::CARET_DOWN }).selectable(false));
                        let w = ui.available_width();
                        let title = if category { category_name(title) } else { title.as_str() };
                        let text = egui::RichText::new(format!("{title}  ({n})"));
                        rows::cell(ui, w, if category { text } else { text.strong() }, "");
                    });
                    if r.clicked() {
                        action = Some(Action::Toggle(key.clone()));
                    }
                }
                Line::Doctrine { at, key, open } => {
                    let s = &all[*at];
                    let d = &s.doctrine;
                    let r = rows::row(ui, "doctrines", ("sim_doc", key), ROW_H, false, |ui| {
                        ui.add_space(24.0);
                        ui.add(egui::Label::new(if *open { icon::CARET_DOWN } else { icon::CARET_RIGHT }).selectable(false));
                        let (glyph, tip) = match s.kind {
                            Kind::Supplied(_) | Kind::Preset(_) => (icon::SEAL_CHECK, tr!("Supplied: opens as a copy, the original stays as it is")),
                            Kind::Link(true) => (icon::LINK, tr!("A link to a supplied doctrine: always the current one")),
                            Kind::Link(false) => (icon::LINK_SIMPLE, tr!("A link to a supplied doctrine that is not here: sign in with an Imperium character")),
                            Kind::Own => (icon::USER, tr!("Your own doctrine")),
                        };
                        let color = if s.kind.supplied() || s.kind == Kind::Link(true) { ui.visuals().hyperlink_color } else { ui.visuals().weak_text_color() };
                        ui.add(egui::Label::new(egui::RichText::new(glyph).color(color)).selectable(false)).on_hover_text(tip);
                        let wide = ui.available_width() > 240.0;
                        let date_w = if wide && d.updated.is_some() { 84.0 } else { 0.0 };
                        let link_w = if d.url.is_some() { 26.0 } else { 0.0 };
                        let w = (ui.available_width() - date_w - link_w).max(40.0);
                        rows::cell(ui, w, egui::RichText::new(&d.name).strong(), "").on_hover_ui(|ui| doctrine_tip(ui, s, now_s));
                        if date_w > 0.0 {
                            if let Some(t) = d.updated {
                                ui.add_sized([date_w - 6.0, ROW_H], egui::Label::new(egui::RichText::new(date(t)).weak()).selectable(false)).on_hover_text(trf!("Updated {date}, {age}", date = date(t), age = age(t, now_s)));
                            }
                        }
                        if let Some(url) = &d.url {
                            if icon_button(ui, icon::ARROW_SQUARE_OUT).on_hover_text(tr!("Open the forum topic")).clicked() {
                                action = Some(Action::Url(url.clone()));
                            }
                        }
                    });
                    if r.clicked() && action.is_none() {
                        action = Some(Action::ToggleOpen(key.clone()));
                    }
                    r.context_menu(|ui| {
                        ui.set_min_width(240.0);
                        let mut pick = |ui: &mut egui::Ui, text: String, a: Action| {
                            if ui.button(text).clicked() {
                                action = Some(a);
                                ui.close();
                            }
                        };
                        let has_fits = !d.fits.is_empty();
                        if has_fits {
                            pick(ui, trf!("{icon}  New fleet of this doctrine", icon = icon::USERS_THREE), Action::Fleet(*at));
                        }
                        if let Some(url) = &d.url {
                            pick(ui, trf!("{icon}  Open the forum topic", icon = icon::ARROW_SQUARE_OUT), Action::Url(url.clone()));
                        }
                        ui.separator();
                        match s.kind {
                            Kind::Supplied(_) => {
                                pick(ui, trf!("{icon}  Copy to my doctrines", icon = icon::COPY), Action::Copy(*at));
                                pick(ui, trf!("{icon}  Link in my doctrines", icon = icon::LINK), Action::Link(*at));
                                if admin {
                                    ui.separator();
                                    pick(ui, trf!("{icon}  Delete the supplied doctrine", icon = icon::TRASH), Action::DeleteSupplied(d.id.clone()));
                                }
                            }
                            Kind::Preset(_) => pick(ui, trf!("{icon}  Copy to my doctrines", icon = icon::COPY), Action::Copy(*at)),
                            Kind::Link(found) => {
                                if found {
                                    pick(ui, trf!("{icon}  Copy to my doctrines", icon = icon::COPY), Action::Copy(*at));
                                }
                                pick(ui, trf!("{icon}  Edit…", icon = icon::PENCIL_SIMPLE), Action::Edit(*at));
                                pick(ui, trf!("{icon}  Remove the link", icon = icon::TRASH), Action::Delete(*at));
                            }
                            Kind::Own => {
                                pick(ui, trf!("{icon}  Edit…", icon = icon::PENCIL_SIMPLE), Action::Edit(*at));
                                if admin && has_fits {
                                    let text = if d.origin.is_some() { tr!("Publish over the supplied one…") } else { tr!("Publish as supplied…") };
                                    pick(ui, format!("{}  {text}", icon::UPLOAD_SIMPLE), Action::Publish(*at));
                                }
                                pick(ui, trf!("{icon}  Delete", icon = icon::TRASH), Action::Delete(*at));
                            }
                        }
                    });
                }
                Line::Info { at } => {
                    let d = &all[*at].doctrine;
                    rows::row(ui, "doctrines", ("sim_doc_info", &d.id), ROW_H, false, |ui| {
                        ui.add_space(44.0);
                        let t = d.updated.unwrap_or(0);
                        let w = ui.available_width();
                        rows::cell(ui, w, egui::RichText::new(trf!("{icon} Updated {date}, {age}", icon = icon::CLOCK, date = date(t), age = age(t, now_s))).weak(), "");
                    });
                }
                Line::Fit { at, fit } => {
                    let s = &all[*at];
                    let f = &s.doctrine.fits[*fit];
                    let ship = fit_of(&sim.fit_docs, f).map(|x| x.ship).or_else(|| sim.library.iter().find(|e| e.id == f.fit_id).map(|e| e.ship)).unwrap_or(0);
                    let selected = current.as_deref() == Some(f.fit_id.as_str());
                    let r = rows::row(ui, "doctrines", ("sim_doc_fit", &s.doctrine.id, *fit), ROW_H, selected, |ui| {
                        ui.add_space(44.0);
                        crate::icons::type_icon(ui, ship, 18.0);
                        let main_w = if f.main { 22.0 } else { 0.0 };
                        let w = (ui.available_width() - main_w).max(40.0);
                        let text = &labels[n];
                        let (hull, name) = text.split_once(": ").unwrap_or((text.as_str(), ""));
                        let mut job = egui::text::LayoutJob::default();
                        let style = ui.style();
                        egui::RichText::new(hull).append_to(&mut job, style, egui::FontSelection::Default, egui::Align::Center);
                        egui::RichText::new(format!("  {name}")).weak().append_to(&mut job, style, egui::FontSelection::Default, egui::Align::Center);
                        ui.allocate_ui_with_layout(egui::vec2(w, ROW_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_width(w);
                            ui.add(egui::Label::new(job).truncate().selectable(false).sense(egui::Sense::hover())).on_hover_ui(|ui| fit_tip(ui, f, text));
                        });
                        if f.main {
                            ui.add(egui::Label::new(egui::RichText::new(icon::CROWN_SIMPLE).color(ui.visuals().warn_fg_color)).selectable(false)).on_hover_text(tr!("Mainline damage"));
                        }
                    });
                    if r.clicked() {
                        action = Some(Action::OpenFit(*at, *fit));
                    }
                }
            }
        }
    });
    let Some(action) = action else { return };
    match action {
        Action::Toggle(k) => {
            if !sim.doctrine_tab.folded.remove(&k) {
                sim.doctrine_tab.folded.insert(k);
            }
        }
        Action::ToggleOpen(k) => {
            if !sim.doctrine_tab.opened.remove(&k) {
                sim.doctrine_tab.opened.insert(k);
            }
        }
        Action::Url(url) => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
        Action::OpenFit(at, fit) => {
            let f = all[at].doctrine.fits[fit].clone();
            match &f.fit {
                Some(inline) => sim.open_doctrine_fit(inline.clone(), now_ms),
                None if sim.session().is_some_and(|s| s.id == f.fit_id) => {}
                None => {
                    if let Some(i) = sim.sessions().iter().position(|s| s.id == f.fit_id) {
                        sim.activate(i);
                    } else {
                        sim.request(HostRequest::LoadFit(f.fit_id.clone()));
                    }
                }
            }
        }
        Action::Fleet(at) => sim.new_fleet_of(&all[at].doctrine, now_ms),
        Action::Copy(at) => sim.copy_doctrine(&all[at].doctrine, now_ms),
        Action::Link(at) => sim.link_doctrine(&all[at].doctrine, now_ms),
        Action::Edit(at) => {
            let id = all[at].doctrine.id.clone();
            if let Some(d) = sim.own_doctrines.iter().find(|d| d.id == id) {
                sim.doctrine_tab.edit = Some(Edit { tags: d.tags.join(", "), doctrine: d.clone(), query: String::new() });
            }
        }
        Action::Delete(at) => {
            let id = all[at].doctrine.id.clone();
            sim.own_doctrines.retain(|d| d.id != id);
            sim.request(HostRequest::DeleteOwnDoctrine(id));
        }
        Action::Publish(at) => {
            if let Some(d) = sim.supplied_from_own(&all[at].doctrine) {
                let side = d.origin.as_deref().and_then(|o| sim.supplied.iter().find(|s| s.doctrine.id == o)).map_or(DoctrineSide::Imperium, |s| s.side);
                sim.doctrine_tab.publish = Some(Publish { doctrines: vec![d], side });
            }
        }
        Action::DeleteSupplied(id) => {
            sim.supplied.retain(|s| s.doctrine.id != id);
            sim.request(HostRequest::DeleteSupplied(id));
        }
    }
}

fn doctrine_tip(ui: &mut egui::Ui, s: &Shown, now_s: i64) {
    let d = &s.doctrine;
    ui.set_max_width(360.0);
    ui.label(egui::RichText::new(&d.name).strong());
    let place = [d.alliance.as_str(), d.category.as_str()].into_iter().filter(|p| !p.trim().is_empty()).collect::<Vec<_>>().join(" › ");
    if !place.is_empty() {
        ui.label(egui::RichText::new(place).weak());
    }
    if let Some(t) = d.updated {
        ui.label(trf!("{icon} Updated {date}, {age}", icon = icon::CLOCK, date = date(t), age = age(t, now_s)));
    }
    match s.kind {
        Kind::Supplied(side) | Kind::Preset(side) => ui.label(trf!("{icon} Supplied, {side}", icon = icon::SEAL_CHECK, side = side.name())),
        Kind::Link(true) => ui.label(trf!("{icon} A link: shows the supplied doctrine as it is now", icon = icon::LINK)),
        Kind::Link(false) => ui.label(trf!("{icon} A link to a supplied doctrine that is not here: sign in with an Imperium character", icon = icon::LINK_SIMPLE)),
        Kind::Own => ui.label(trf!("{icon} Your own", icon = icon::USER)),
    };
    if !d.tags.is_empty() {
        ui.label(trf!("{icon} {tags}", icon = icon::TAG, tags = d.tags.join(", ")));
    }
    let n = d.fits.len();
    ui.label(trf!("{n} fits", n = n));
    if !d.notes.trim().is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(d.notes.trim()).weak()).wrap());
    }
}

fn fit_tip(ui: &mut egui::Ui, f: &DoctrineFit, label: &str) {
    ui.set_max_width(360.0);
    ui.label(egui::RichText::new(label).strong());
    let role = if f.main { trf!("{role}, mainline damage", role = role_name(f.role)) } else { role_name(f.role).to_owned() };
    ui.label(egui::RichText::new(role).weak());
    if !f.note.is_empty() {
        ui.add(egui::Label::new(&f.note).wrap());
    }
    if let Some(fit) = &f.fit {
        let keys: Vec<String> = fit.variations.iter().map(|v| v.full_name()).collect();
        if !keys.is_empty() {
            ui.label(trf!("Variants: {list}", list = keys.join(", ")));
        }
        ui.label(egui::RichText::new(tr!("Opens as a copy")).weak());
    }
}

/// Doctrines dropped onto the tab as a JSON file, for an admin to publish.
fn take_dropped(sim: &mut SimUi, ctx: &egui::Context) {
    let files = ctx.input(|i| i.raw.dropped_files.clone());
    for f in files {
        let bytes = match (&f.bytes, &f.path) {
            (Some(b), _) => b.to_vec(),
            (None, Some(p)) => std::fs::read(p).unwrap_or_default(),
            _ => continue,
        };
        match serde_json::from_slice::<Vec<Doctrine>>(&bytes) {
            Ok(list) if !list.is_empty() => sim.doctrine_tab.publish = Some(Publish { doctrines: list, side: DoctrineSide::Imperium }),
            _ => sim.note(tr!("That file holds no doctrines").to_owned()),
        }
    }
}

/// The admin's confirmation before supplied doctrines go out, and the own doctrine editor.
pub fn windows(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    publish_window(sim, ctx);
    edit_window(sim, ctx, now_ms);
}

fn publish_window(sim: &mut SimUi, ctx: &egui::Context) {
    use spai_ui::widgets::SteadySelect as _;
    let Some(p) = &mut sim.doctrine_tab.publish else { return };
    let mut open = true;
    let mut go = false;
    let size = egui::vec2(440.0, 320.0);
    egui::Window::new(tr!("Publish doctrines")).id(egui::Id::new("sim_publish_supplied")).open(&mut open).collapsible(false).resizable(false).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        ui.add(egui::Label::new(trf!("{n} doctrines go to everyone who sees this side. Publishing one again replaces it, by its id.", n = p.doctrines.len())).wrap());
        ui.horizontal(|ui| {
            ui.label(tr!("Side"));
            crate::combo::combo(ui, "sim_publish_supplied_side", 160.0, p.side.name(), |ui| {
                for side in DoctrineSide::ALL {
                    ui.menu_value(&mut p.side, side, side.name());
                }
            });
        });
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            for d in &p.doctrines {
                ui.label(format!("{} › {} › {}  ({})", d.alliance, d.category, d.name, d.fits.len()));
            }
        });
        ui.add_space(6.0);
        if ui.button(trf!("{icon} Publish", icon = icon::UPLOAD_SIMPLE)).clicked() {
            go = true;
        }
    });
    if go {
        if let Some(p) = sim.doctrine_tab.publish.take() {
            for d in p.doctrines {
                sim.request(HostRequest::PublishSupplied { side: p.side, doc: serde_json::to_string(&d).unwrap_or_default() });
            }
        }
    } else if !open {
        sim.doctrine_tab.publish = None;
    }
}

fn edit_window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    use spai_ui::widgets::SteadySelect as _;
    if sim.doctrine_tab.edit.is_none() {
        return;
    }
    let library: Vec<(String, String, TypeId)> = sim.library.iter().map(|e| (e.id.clone(), e.name.clone(), e.ship)).collect();
    let open_fit = sim.session().filter(|s| library.iter().any(|(id, _, _)| *id == s.id)).map(|s| s.id.clone());
    let labels: Vec<String> = sim.doctrine_tab.edit.as_ref().map(|e| e.doctrine.fits.iter().map(|f| fit_label(sim, f)).collect()).unwrap_or_default();
    let dogma = sim.dogma.clone();
    let mut alliances: Vec<String> = sim.supplied.iter().map(|s| s.doctrine.alliance.clone()).chain(sim.own_doctrines.iter().map(|d| d.alliance.clone())).filter(|a| !a.trim().is_empty()).collect();
    alliances.sort();
    alliances.dedup();
    let Some(e) = &mut sim.doctrine_tab.edit else { return };
    let mut open = true;
    let mut save = false;
    let size = egui::vec2(520.0, 520.0);
    egui::Window::new(tr!("Doctrine")).id(egui::Id::new("sim_doctrine_edit")).open(&mut open).collapsible(false).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        let d = &mut e.doctrine;
        egui::Grid::new("sim_doctrine_edit_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            let w = 360.0;
            ui.label(tr!("Name"));
            ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(w));
            ui.end_row();
            ui.label(tr!("Alliance"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut d.alliance).desired_width(w - 34.0).hint_text(mine_title()));
                if !alliances.is_empty() {
                    ui.menu_button(icon::CARET_DOWN, |ui| {
                        for a in &alliances {
                            if ui.button(a).clicked() {
                                d.alliance = a.clone();
                                ui.close();
                            }
                        }
                    });
                }
            });
            ui.end_row();
            ui.label(tr!("Kind"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut d.category).desired_width(w - 34.0).hint_text(tr!("Strategic, Peacetime…")));
                ui.menu_button(icon::CARET_DOWN, |ui| {
                    for c in spai_fit::doctrine::CATEGORIES {
                        if ui.button(category_name(c)).clicked() {
                            d.category = c.to_owned();
                            ui.close();
                        }
                    }
                });
            });
            ui.end_row();
            ui.label(tr!("Tags"));
            ui.add(egui::TextEdit::singleline(&mut e.tags).desired_width(w).hint_text(tr!("comma separated")));
            ui.end_row();
            ui.label(tr!("Notes"));
            ui.add(egui::TextEdit::multiline(&mut d.notes).desired_width(w).desired_rows(2));
            ui.end_row();
        });
        ui.separator();
        if d.link {
            ui.add(egui::Label::new(egui::RichText::new(tr!("A link shows the supplied doctrine's fits as they are now. Copy it to change them.")).weak()).wrap());
        } else {
            ui.label(egui::RichText::new(tr!("Fits")).strong());
            let mut remove: Option<usize> = None;
            egui::ScrollArea::vertical().id_salt("sim_doctrine_edit_fits").max_height(220.0).show(ui, |ui| {
                if d.fits.is_empty() {
                    ui.label(egui::RichText::new(tr!("No fits yet: add saved fits below.")).weak());
                }
                for (i, f) in d.fits.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let ship = library.iter().find(|(id, _, _)| *id == f.fit_id).map_or(0, |(_, _, s)| *s);
                        crate::icons::type_icon(ui, ship, 18.0);
                        let w = (ui.available_width() - 250.0).max(80.0);
                        ui.allocate_ui_with_layout(egui::vec2(w, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_width(w);
                            ui.add(egui::Label::new(labels.get(i).cloned().unwrap_or_default()).truncate());
                        });
                        crate::combo::combo(ui, ("sim_doctrine_role", i), 96.0, role_name(f.role), |ui| {
                            for r in FitRole::ALL {
                                ui.menu_value(&mut f.role, r, role_name(r));
                            }
                        });
                        ui.checkbox(&mut f.main, tr!("Main")).on_hover_text(tr!("The doctrine's mainline damage; several hulls may share it"));
                        if icon_button(ui, icon::TRASH).on_hover_text(tr!("Remove from the doctrine")).clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
            if let Some(i) = remove {
                d.fits.remove(i);
            }
            ui.horizontal(|ui| {
                let mut add: Option<String> = None;
                ui.menu_button(trf!("{icon} Add a saved fit", icon = icon::PLUS), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut e.query).hint_text(trf!("{icon} Name or hull", icon = icon::MAGNIFYING_GLASS)).desired_width(260.0));
                    let q = e.query.to_lowercase();
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for (id, name, ship) in &library {
                            let hull = dogma.as_ref().and_then(|dg| dg.type_(*ship)).map_or(String::new(), |t| t.name.clone());
                            if !q.is_empty() && !name.to_lowercase().contains(&q) && !hull.to_lowercase().contains(&q) {
                                continue;
                            }
                            if ui.button(format!("{hull}: {name}")).clicked() {
                                add = Some(id.clone());
                                ui.close();
                            }
                        }
                    });
                });
                if let Some(id) = &open_fit {
                    if ui.button(trf!("{icon} Add the open fit", icon = icon::PLUS)).clicked() {
                        add = Some(id.clone());
                    }
                }
                if let Some(id) = add {
                    if !d.fits.iter().any(|f| f.fit_id == id) {
                        let ship = library.iter().find(|(x, _, _)| *x == id).map_or(0, |(_, _, s)| *s);
                        let role = dogma.as_deref().map_or(FitRole::Line, |dg| FitRole::of_hull(dg, ship));
                        let main = d.fits.is_empty();
                        d.fits.push(DoctrineFit { fit_id: id, role, main, ..DoctrineFit::default() });
                    }
                }
            });
        }
        ui.add_space(6.0);
        if ui.add_enabled(!d.name.trim().is_empty(), egui::Button::new(tr!("Save"))).clicked() {
            save = true;
        }
    });
    if save {
        if let Some(mut e) = sim.doctrine_tab.edit.take() {
            e.doctrine.tags = crate::library_panel::parse_tags(&e.tags);
            e.doctrine.name = e.doctrine.name.trim().to_owned();
            sim.save_own_doctrine(e.doctrine, now_ms);
        }
    } else if !open {
        sim.doctrine_tab.edit = None;
    }
}

/// A kind's name in the reader's language; the stored one stays English.
pub fn category_name(c: &str) -> &str {
    match c {
        "Strategic" => tr!("Strategic"),
        "Peacetime" => tr!("Peacetime"),
        "Capitals" => tr!("Capitals"),
        "SIGs" => tr!("SIGs"),
        "Utility" => tr!("Utility"),
        "Retired" => tr!("Retired"),
        c => c,
    }
}

/// A refit key's name in the reader's language.
pub fn refit_name(key: &str) -> String {
    if key.contains('|') {
        return key.split('|').map(refit_name).collect::<Vec<_>>().join(" · ");
    }
    match key {
        "prop:ab" => tr!("Afterburner").to_owned(),
        "prop:mwd" => tr!("Microwarpdrive").to_owned(),
        "point:scram" => tr!("Warp scrambler").to_owned(),
        "point:disruptor" => tr!("Warp disruptor").to_owned(),
        "ewar:web" => tr!("Webifier").to_owned(),
        "ewar:damp" => tr!("Sensor dampener").to_owned(),
        "ewar:paint" => tr!("Target painter").to_owned(),
        "ewar:td" => tr!("Tracking disruptor").to_owned(),
        "ewar:gd" => tr!("Guidance disruptor").to_owned(),
        "ecm:ladar" => tr!("Ladar ECM").to_owned(),
        "ecm:magnetometric" => tr!("Magnetometric ECM").to_owned(),
        "ecm:radar" => tr!("Radar ECM").to_owned(),
        "ecm:gravimetric" => tr!("Gravimetric ECM").to_owned(),
        "ecm:multispectral" => tr!("Multispectral ECM").to_owned(),
        "tier:t1" => tr!("T1 fit").to_owned(),
        k => spai_fit::doctrine::refit_label(k),
    }
}

impl SimUi {
    /// Every doctrine fit the fleet screen can fly, by id: presets and supplied doctrines' fits.
    pub fn doctrine_fit_names(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self.doctrines.iter().map(|d| (d.id.clone(), trf!("{name} (doctrine)", name = d.name))).collect();
        for s in &self.supplied {
            for f in &s.doctrine.fits {
                if f.fit.is_some() {
                    out.push((f.fit_id.clone(), format!("{} ({})", fit_label(self, f), s.doctrine.name)));
                }
            }
        }
        out
    }

    /// A supplied fit opened to look at: a copy, saved to the library on its first edit.
    pub fn open_doctrine_fit(&mut self, fit: Fit, now_ms: u64) {
        self.open_fit(fit, now_ms);
        if let Some(s) = self.session_mut() {
            s.dirty = false;
        }
    }

    pub fn new_own_doctrine(&mut self, now_ms: u64) {
        let id = self.new_id(now_ms);
        let mut d = Doctrine { id, name: tr!("New doctrine").to_owned(), category: "Strategic".into(), ..Doctrine::default() };
        if let Some(s) = self.session().filter(|s| self.library.iter().any(|e| e.id == s.id)) {
            let role = self.dogma.as_deref().map_or(FitRole::Line, |dg| FitRole::of_hull(dg, s.fit.ship));
            d.fits.push(DoctrineFit { fit_id: s.id.clone(), role, main: true, ..DoctrineFit::default() });
        }
        self.doctrine_tab.edit = Some(Edit { doctrine: d, tags: String::new(), query: String::new() });
    }

    pub fn save_own_doctrine(&mut self, d: Doctrine, _now_ms: u64) {
        let doc = serde_json::to_string(&d).unwrap_or_default();
        self.request(HostRequest::SaveOwnDoctrine { id: d.id.clone(), name: d.name.clone(), doc });
        match self.own_doctrines.iter_mut().find(|x| x.id == d.id) {
            Some(x) => *x = d,
            None => self.own_doctrines.push(d),
        }
    }

    /// The pilot's own copy: its fits into the library, the doctrine naming them.
    pub fn copy_doctrine(&mut self, d: &Doctrine, now_ms: u64) {
        let mut source = d.clone();
        if source.link {
            if let Some(s) = source.origin.as_deref().and_then(|o| self.supplied.iter().find(|s| s.doctrine.id == o)) {
                source = s.doctrine.clone();
            }
        }
        let mut ids: Vec<String> = (0..=source.fits.len()).map(|_| self.new_id(now_ms)).collect();
        ids.reverse();
        let (mut copy, fits) = source.copy_into_library(&mut || ids.pop().unwrap_or_default());
        copy.name = trf!("{name} (copy)", name = source.name);
        for fit in fits {
            let json = spai_fit::doc::to_json(&fit);
            self.request(HostRequest::SaveFit { id: fit.id.clone(), name: fit.name.clone(), ship: fit.ship, doc: json });
            self.fit_docs.insert(fit.id.clone(), fit);
        }
        self.save_own_doctrine(copy, now_ms);
        self.doctrine_tab.source = Source::Mine;
        self.note(trf!("Copied {name} to your doctrines", name = source.name));
    }

    pub fn link_doctrine(&mut self, d: &Doctrine, now_ms: u64) {
        if self.own_doctrines.iter().any(|o| o.link && o.origin.as_deref() == Some(d.id.as_str())) {
            self.note(tr!("Already linked in your doctrines").to_owned());
            return;
        }
        let id = self.new_id(now_ms);
        self.save_own_doctrine(d.link_to(id), now_ms);
        self.note(trf!("Linked {name} in your doctrines", name = d.name));
    }

    /// A fleet of the doctrine, opened in the fleet screen.
    pub fn new_fleet_of(&mut self, d: &Doctrine, now_ms: u64) {
        let Some(dogma) = self.dogma.clone() else { return };
        let docs = self.fit_docs.clone();
        let mut fleet = d.fleet(&dogma, &|id| docs.get(id).cloned(), self.skills());
        if fleet.slots.is_empty() {
            self.note(tr!("The doctrine's fits are still loading; try again in a moment").to_owned());
            return;
        }
        fleet.size = 50;
        let id = self.new_id(now_ms);
        self.fleet = Some(crate::FleetSession { id, fleet, dirty: true, enemy: None });
        self.mode = crate::Mode::Fleets;
    }

    /// An own doctrine as a supplied one: its fits inline under ids of their own, the id of the
    /// supplied doctrine it was copied from, so publishing it replaces that one.
    pub fn supplied_from_own(&self, d: &Doctrine) -> Option<Doctrine> {
        let id = d.origin.clone().unwrap_or_else(|| d.id.clone());
        let mut out = Doctrine { id: id.clone(), origin: None, link: false, fits: Vec::new(), ..d.clone() };
        let mut keys: Vec<String> = Vec::new();
        for f in &d.fits {
            let mut fit = fit_of(&self.fit_docs, f)?.clone();
            let base = self.dogma.as_ref().and_then(|dg| dg.type_(fit.ship)).map_or_else(|| "fit".to_owned(), |t| t.name.to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
            let mut key = base.clone();
            let mut n = 2;
            while keys.contains(&key) {
                key = format!("{base}-{n}");
                n += 1;
            }
            keys.push(key.clone());
            fit.id = spai_fit::doctrine::supplied_fit_id(&id, &key);
            out.fits.push(DoctrineFit { fit_id: fit.id.clone(), fit: Some(fit), ..f.clone() });
        }
        Some(out)
    }
}

#[cfg(test)]
pub(crate) fn lines_for_test(all: &[Shown], source: Source, query: &str, folded: &BTreeSet<String>, opened: &BTreeSet<String>) -> Vec<Line> {
    lines(all, source, query, folded, opened, &|f| f.fit.as_ref().map_or(f.fit_id.clone(), |x| x.name.clone()))
}
