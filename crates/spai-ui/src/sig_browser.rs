//! The wormhole tab's signature browser: every probe scan signature pasted, in every system, to
//! search, filter and clean up. Each app loads the rows and stores deletes itself.

use std::collections::BTreeSet;

use egui_phosphor::regular as icon;
use spai_core::geo::Systems;
use spai_core::wormholes::{SystemSig, Wormhole};

use crate::wh_form::WhForm;
use crate::widgets::{human_ago, SteadySelect as _};

/// Deletes kept for undo, oldest dropped first.
const UNDO_DEPTH: usize = 50;

/// The groups to filter on, by code, with the label they show.
const GROUPS: [(&str, &str); 7] = [
    ("wormhole", "Wormhole"),
    ("combat", "Combat"),
    ("data", "Data"),
    ("relic", "Relic"),
    ("gas", "Gas"),
    ("ore", "Ore"),
    ("unscanned", "Unscanned"),
];

/// Signatures, anomalies, or both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SigKinds {
    #[default]
    Both,
    Signatures,
    Anomalies,
}

impl SigKinds {
    const ALL: [SigKinds; 3] = [SigKinds::Both, SigKinds::Signatures, SigKinds::Anomalies];

    pub fn label(self) -> &'static str {
        match self {
            SigKinds::Both => "Sigs & anoms",
            SigKinds::Signatures => "Signatures",
            SigKinds::Anomalies => "Anomalies",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SigSort {
    System,
    Id,
    Group,
    Info,
    Found,
    #[default]
    Seen,
    Who,
}

#[derive(Default)]
pub struct SigBrowser {
    pub query: String,
    pub groups: BTreeSet<String>,
    pub kinds: SigKinds,
    /// Rows under their system, each system with its latest paste.
    pub tree: bool,
    /// Systems folded shut in the tree.
    pub collapsed: BTreeSet<i64>,
    /// Only signatures seen in a paste this many hours ago or less; 0 for any.
    pub within_h: u32,
    pub sort: SigSort,
    pub descending: bool,
    pub picked: BTreeSet<(i64, String)>,
    /// Every signature, by system: the app fills it.
    pub rows: Vec<(i64, SystemSig)>,
    /// What each delete took, the latest last, to put back while the app runs.
    undo: Vec<Vec<(i64, SystemSig)>>,
}

/// What a click in the browser asks the app for.
#[derive(Default)]
pub struct SigAct {
    /// Signatures taken out of the list, to delete from storage and share.
    pub deleted: Vec<(i64, SystemSig)>,
    /// Signatures put back by undo, to store and share again.
    pub restored: Vec<(i64, SystemSig)>,
    /// A system to show on the wormhole map, on its Signatures tab.
    pub open: Option<i64>,
    /// The hole a signature is, to edit, by uid.
    pub edit: Option<String>,
    /// A form for a wormhole signature not yet a hole.
    pub new_hole: Option<WhForm>,
}

pub fn is_anomaly(s: &SystemSig) -> bool {
    s.kind.to_lowercase().contains("anomal")
}

pub use crate::widgets::{age_color, found_at, found_hover};

/// Room kept free at a table's right edge, so a scrollbar drawn over it covers no button.
pub fn scrollbar_gutter(ui: &egui::Ui) -> f32 {
    let s = &ui.spacing().scroll;
    s.bar_width + s.bar_inner_margin + s.bar_outer_margin
}

/// Which filter group a signature falls in.
pub fn sig_group(s: &SystemSig) -> &'static str {
    match s.group.as_str() {
        "" => "unscanned",
        "Wormhole" => "wormhole",
        "Combat Site" => "combat",
        "Data Site" => "data",
        "Relic Site" => "relic",
        "Gas Site" => "gas",
        "Ore Site" => "ore",
        _ => "",
    }
}

/// Whether a signature in `system` passes the browser's filter. `name` is the system's name.
pub fn sig_shown(b: &SigBrowser, system_name: &str, s: &SystemSig, now: i64) -> bool {
    let q = b.query.trim().to_lowercase();
    let text_ok = q.is_empty()
        || [system_name, &s.sig, &s.name, &s.group, &s.kind, &s.who].iter().any(|f| f.to_lowercase().contains(&q));
    let group_ok = b.groups.is_empty() || b.groups.contains(sig_group(s));
    let kind_ok = match b.kinds {
        SigKinds::Both => true,
        SigKinds::Signatures => !is_anomaly(s),
        SigKinds::Anomalies => is_anomaly(s),
    };
    let age_ok = b.within_h == 0 || now - s.updated_at <= b.within_h as i64 * 3600;
    text_ok && group_ok && kind_ok && age_ok
}


impl SigBrowser {
    /// Takes signatures out of the list and keeps them for undo.
    pub fn take(&mut self, which: &[(i64, String)]) -> Vec<(i64, SystemSig)> {
        let gone: BTreeSet<&(i64, String)> = which.iter().collect();
        let rows: Vec<(i64, SystemSig)> = self.rows.iter().filter(|(id, s)| gone.contains(&(*id, s.sig.clone()))).cloned().collect();
        self.forget(rows.clone());
        rows
    }

    /// Drops rows deleted elsewhere from the list, keeping them for undo.
    pub fn forget(&mut self, rows: Vec<(i64, SystemSig)>) {
        if rows.is_empty() {
            return;
        }
        self.rows.retain(|(id, s)| !rows.iter().any(|(i, r)| i == id && r.sig == s.sig));
        for (sys, s) in &rows {
            self.picked.remove(&(*sys, s.sig.clone()));
        }
        if self.undo.len() >= UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push(rows);
    }

    /// Puts the latest delete back in the list and returns it, to store again.
    pub fn undo(&mut self) -> Vec<(i64, SystemSig)> {
        let Some(rows) = self.undo.pop() else { return Vec::new() };
        self.rows.retain(|(id, s)| !rows.iter().any(|(i, r)| i == id && r.sig == s.sig));
        self.rows.extend(rows.iter().cloned());
        rows
    }

    /// Fresh rows from storage; picks of rows gone are dropped.
    pub fn load(&mut self, rows: Vec<(i64, SystemSig)>) {
        self.rows = rows;
        let keep: BTreeSet<(i64, String)> = self.rows.iter().map(|(id, s)| (*id, s.sig.clone())).collect();
        self.picked.retain(|p| keep.contains(p));
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// What undo would put back, for its button.
    pub fn undo_hint(&self, geo: Option<&Systems>) -> String {
        let Some(rows) = self.undo.last() else { return "Nothing deleted to undo".to_owned() };
        let name = |id: i64| geo.and_then(|g| g.info_of(id)).map_or_else(|| format!("#{id}"), |i| i.name.clone());
        let mut list: Vec<String> = rows.iter().take(8).map(|(sys, s)| format!("{} in {}", s.sig, name(*sys))).collect();
        if rows.len() > 8 {
            list.push(format!("and {} more", rows.len() - 8));
        }
        format!("Undo the last delete (Ctrl+Z): {}", list.join(", "))
    }

    /// An undo button, always there so nothing shifts when it becomes usable, and Ctrl+Z while
    /// no text field has the keyboard. Returns what it put back.
    pub fn undo_button(&mut self, ui: &mut egui::Ui, text: &str, geo: Option<&Systems>) -> Vec<(i64, SystemSig)> {
        let can = self.can_undo();
        let hint = self.undo_hint(geo);
        let clicked = ui.add_enabled(can, egui::Button::new(text)).on_hover_text(&hint).on_disabled_hover_text(&hint).clicked();
        let key = can
            && ui.memory(|m| m.focused().is_none())
            && ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z)));
        if clicked || key { self.undo() } else { Vec::new() }
    }
}

/// The browser: filters, actions and the table. `holes` tells which signatures are known holes.
pub fn view(ui: &mut egui::Ui, b: &mut SigBrowser, geo: Option<&Systems>, holes: &[Wormhole], now: i64, eve: bool) -> SigAct {
    let mut act = SigAct::default();
    let name_of = |id: i64| geo.and_then(|g| g.info_of(id)).map_or_else(|| format!("#{id}"), |i| i.name.clone());
    ui.add_space(4.0);
    // Two fixed rows, filters then actions, so the table starts at the same height whatever
    // is picked and however narrow the window.
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut b.query).hint_text("Search system, signature, site, who").desired_width(220.0));
        let n = b.groups.len();
        let label = match n {
            0 => "All groups".to_owned(),
            1 => GROUPS.iter().find(|g| b.groups.contains(g.0)).map_or_else(String::new, |g| g.1.to_owned()),
            n => format!("{n} groups"),
        };
        let button = ui.button(format!("{label}  {}", icon::CARET_DOWN));
        egui::Popup::from_toggle_button_response(&button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for (code, label) in GROUPS {
                let on = b.groups.contains(code);
                if ui.menu_label(on, label).clicked() {
                    if on {
                        b.groups.remove(code);
                    } else {
                        b.groups.insert(code.to_owned());
                    }
                }
            }
        });
        egui::ComboBox::from_id_salt("sig_browser_kinds").selected_text(b.kinds.label()).show_ui(ui, |ui| {
            for k in SigKinds::ALL {
                ui.menu_value(&mut b.kinds, k, k.label());
            }
        });
        let windows = [(0u32, "Any time"), (1, "Last hour"), (6, "Last 6 h"), (24, "Last day"), (72, "Last 3 days")];
        let current = windows.iter().find(|w| w.0 == b.within_h).map_or("Any time", |w| w.1);
        egui::ComboBox::from_id_salt("sig_browser_within").selected_text(format!("Seen: {current}")).show_ui(ui, |ui| {
            for (h, label) in windows {
                ui.menu_value(&mut b.within_h, h, label);
            }
        });
    });

    let mut rows: Vec<(i64, String, SystemSig)> = b
        .rows
        .iter()
        .map(|(id, s)| (*id, name_of(*id), s.clone()))
        .filter(|(_, n, s)| sig_shown(&*b, n, s, now))
        .collect();
    let sort = b.sort;
    rows.sort_by(|x, y| {
        let o = match sort {
            SigSort::System => x.1.cmp(&y.1).then(x.2.sig.cmp(&y.2.sig)),
            SigSort::Id => x.2.sig.cmp(&y.2.sig),
            SigSort::Group => x.2.group.cmp(&y.2.group),
            SigSort::Info => x.2.name.cmp(&y.2.name),
            SigSort::Found => x.2.added_at.cmp(&y.2.added_at),
            SigSort::Seen => x.2.updated_at.cmp(&y.2.updated_at),
            SigSort::Who => x.2.who.cmp(&y.2.who),
        };
        if b.descending { o.reverse() } else { o }
    });

    let shown: Vec<(i64, String)> = rows.iter().map(|(id, _, s)| (*id, s.sig.clone())).collect();
    let picked: Vec<(i64, String)> = b.picked.iter().filter(|p| shown.contains(p)).cloned().collect();
    let mut delete: Vec<(i64, String)> = Vec::new();
    ui.horizontal(|ui| {
        let systems: BTreeSet<i64> = rows.iter().map(|r| r.0).collect();
        ui.label(egui::RichText::new(format!("{} of {} signatures in {} systems", rows.len(), b.rows.len(), systems.len())).weak());
        ui.add_space(8.0);
        if ui.add_enabled(!picked.is_empty(), egui::Button::new(format!("{}  Delete {} picked", icon::TRASH, picked.len()))).clicked() {
            delete = picked.clone();
        }
        act.restored = b.undo_button(ui, &format!("{}  Undo", icon::ARROW_COUNTER_CLOCKWISE), geo);
        ui.add_space(8.0);
        if ui.menu_label(!b.tree, icon::ROWS).on_hover_text("One list").clicked() {
            b.tree = false;
        }
        if ui.menu_label(b.tree, icon::TREE_VIEW).on_hover_text("By system: signatures under their system, with each system's latest paste").clicked() {
            b.tree = true;
        }
        if !rows.is_empty() {
            let all = picked.len() == rows.len();
            if ui.button(if all { "Pick none" } else { "Pick all shown" }).clicked() {
                if all {
                    b.picked.clear();
                } else {
                    b.picked.extend(shown.iter().cloned());
                }
            }
        }
    });
    ui.separator();
    if b.rows.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No probe scans pasted yet.").weak());
            ui.label(egui::RichText::new("Select a system on the map and paste the probe scanner on its Signatures tab.").weak());
        });
        return act;
    }

    // Rows under their system in the tree, each system with its count and latest paste.
    enum Line {
        System { id: i64, n: usize, anoms: usize, last: i64, who: String },
        Sig(usize),
    }
    let lines: Vec<Line> = if b.tree {
        let mut systems: Vec<(i64, &str, Vec<usize>)> = Vec::new();
        for (i, (id, n, _)) in rows.iter().enumerate() {
            match systems.iter_mut().find(|s| s.0 == *id) {
                Some(s) => s.2.push(i),
                None => systems.push((*id, n, vec![i])),
            }
        }
        let last = |ix: &[usize]| ix.iter().map(|i| rows[*i].2.updated_at).max().unwrap_or(0);
        match sort {
            SigSort::System if b.descending => systems.sort_by(|a, b| b.1.cmp(a.1)),
            SigSort::System => systems.sort_by(|a, b| a.1.cmp(b.1)),
            SigSort::Seen if !b.descending => systems.sort_by_key(|s| last(&s.2)),
            _ => systems.sort_by_key(|s| std::cmp::Reverse(last(&s.2))),
        }
        let mut out = Vec::new();
        for (id, _, ix) in systems {
            let latest = ix.iter().max_by_key(|i| rows[**i].2.updated_at).map(|i| &rows[*i].2);
            out.push(Line::System {
                id,
                n: ix.len(),
                anoms: ix.iter().filter(|i| is_anomaly(&rows[**i].2)).count(),
                last: last(&ix),
                who: latest.map(|s| s.who.clone()).unwrap_or_default(),
            });
            if !b.collapsed.contains(&id) {
                out.extend(ix.into_iter().map(Line::Sig));
            }
        }
        out
    } else {
        (0..rows.len()).map(Line::Sig).collect()
    };

    let mut new_hole: Option<(i64, String)> = None;
    let mut toggle: Option<(i64, String)> = None;
    let mut fold: Option<i64> = None;
    let mut resort: Option<SigSort> = None;
    let row_h = ui.spacing().interact_size.y + 4.0;
    // Fixed widths, measured from the widest thing each column can hold rather than what is
    // loaded, so filtering or a new paste never moves the columns.
    let text_w = |t: &str| ui.painter().layout_no_wrap(t.to_owned(), egui::TextStyle::Body.resolve(ui.style()), egui::Color32::WHITE).size().x;
    let head_w = |t: &str| {
        let strong = egui::WidgetText::from(egui::RichText::new(format!("{t} {}", icon::CARET_DOWN)).strong());
        strong.into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x
    };
    let fit = |label: &str, widest: &str| head_w(label).max(text_w(widest)) + 4.0;
    let sys_w = fit("System", "Mmmmmmmmm");
    let id_w = fit("Id", &format!("{} MMM-888", icon::MAGNIFYING_GLASS));
    let group_w = fit("Group", "Combat");
    let seen_w = fit("Seen", "88m");
    let by_w = fit("By", "Mmmmmmmm");
    let visuals = ui.visuals().clone();
    // Delete and edit, as wide as the map's side panel gives them, then the scrollbar's room.
    let actions_w = 76.0 + scrollbar_gutter(ui);
    // The first sighting's age too, where the window has room for it.
    let others = 24.0 + sys_w + id_w + group_w + 60.0 + seen_w + actions_w + ui.spacing().item_spacing.x * 8.0;
    // Narrower still, who pasted it moves into the Seen column's hover.
    let show_by = ui.available_width() >= others + by_w + fit("Found", "Wed 88:88");
    let found_w = fit("Found", "88h ago");
    {
        let mut table = egui_extras::TableBuilder::new(ui)
            .id_salt("sig_browser_table")
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(24.0))
            .column(egui_extras::Column::exact(sys_w).clip(true))
            .column(egui_extras::Column::exact(id_w))
            .column(egui_extras::Column::exact(group_w).clip(true))
            .column(egui_extras::Column::remainder().at_least(60.0).clip(true))
            .column(egui_extras::Column::exact(found_w))
            .column(egui_extras::Column::exact(seen_w));
        if show_by {
            table = table.column(egui_extras::Column::exact(by_w).clip(true));
        }
        table
            .column(egui_extras::Column::exact(actions_w))
            .header(row_h, |mut h| {
                h.col(|_| {});
                for (label, key) in [
                    ("System", SigSort::System),
                    ("Id", SigSort::Id),
                    ("Group", SigSort::Group),
                    ("Info", SigSort::Info),
                    ("Found", SigSort::Found),
                    ("Seen", SigSort::Seen),
                    ("By", SigSort::Who),
                ]
                .into_iter()
                .filter(|(l, _)| show_by || *l != "By")
                {
                    h.col(|ui| {
                        let arrow = match (sort == key, b.descending) {
                            (true, true) => format!(" {}", icon::CARET_DOWN),
                            (true, false) => format!(" {}", icon::CARET_UP),
                            _ => String::new(),
                        };
                        if ui.add(egui::Label::new(egui::RichText::new(format!("{label}{arrow}")).strong()).sense(egui::Sense::click())).on_hover_text("Sort").clicked() {
                            resort = Some(key);
                        }
                    });
                }
                h.col(|_| {});
            })
            .body(|body| {
                body.rows(row_h, lines.len(), |mut row| {
                    let i = match &lines[row.index()] {
                        Line::System { id, n, anoms, last, who } => {
                            let shut = b.collapsed.contains(id);
                            row.col(|ui| {
                                let caret = if shut { icon::CARET_RIGHT } else { icon::CARET_DOWN };
                                if ui.add(egui::Button::new(caret).frame(false)).on_hover_text(if shut { "Show its signatures" } else { "Fold it" }).clicked() {
                                    fold = Some(*id);
                                }
                            });
                            row.col(|ui| {
                                if ui.add(egui::Link::new(egui::RichText::new(name_of(*id)).strong())).on_hover_text("Show it on the wormhole map").clicked() {
                                    act.open = Some(*id);
                                }
                            });
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|ui| {
                                let sigs = |n: usize| if n == 1 { "1 signature".to_owned() } else { format!("{n} signatures") };
                                let anomalies = |n: usize| if n == 1 { "1 anomaly".to_owned() } else { format!("{n} anomalies") };
                                let text = match (n - anoms, *anoms) {
                                    (s, 0) => sigs(s),
                                    (0, a) => anomalies(a),
                                    (s, a) => format!("{} \u{b7} {}", sigs(s), anomalies(a)),
                                };
                                ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate());
                            });
                            row.col(|_| {});
                            row.col(|ui| {
                                let t = egui::RichText::new(human_ago(now - last)).strong();
                                ui.label(match age_color(&visuals, now, *last) {
                                    Some(c) => t.color(c),
                                    None => t,
                                })
                                .on_hover_text(format!("Latest paste {} ago by {who}", human_ago(now - last)));
                            });
                            if show_by {
                                row.col(|ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(who).weak()).truncate());
                                });
                            }
                            row.col(|_| {});
                            return;
                        }
                        Line::Sig(i) => *i,
                    };
                    let (sys, sys_name, s) = &rows[i];
                    let key = (*sys, s.sig.clone());
                    let aged = age_color(&visuals, now, s.updated_at);
                    let tint = |t: egui::RichText| match aged {
                        Some(c) => t.color(c),
                        None => t,
                    };
                    row.col(|ui| {
                        let mut on = b.picked.contains(&key);
                        if ui.checkbox(&mut on, "").changed() {
                            toggle = Some(key.clone());
                        }
                    });
                    row.col(|ui| {
                        // Under its system in the tree, the name would only repeat it.
                        let link = egui::RichText::new(sys_name).color(ui.visuals().hyperlink_color);
                        if !b.tree
                            && ui
                                .add(egui::Label::new(link).truncate().show_tooltip_when_elided(false).sense(egui::Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(format!("{sys_name}: show it on the wormhole map"))
                                .clicked()
                        {
                            act.open = Some(*sys);
                        }
                    });
                    row.col(|ui| {
                        let t = egui::RichText::new(format!("{} {}", crate::widgets::sig_icon(&s.kind), s.sig));
                        ui.label(if is_anomaly(s) && aged.is_none() { t.weak() } else { tint(t) }).on_hover_text(&s.kind);
                    });
                    row.col(|ui| {
                        ui.add(egui::Label::new(tint(egui::RichText::new(crate::wh_graph::short_group(&s.group)))).truncate());
                    });
                    let hole = crate::wh_graph::sig_hole(holes, *sys, &s.sig);
                    row.col(|ui| {
                        let info = match hole.map(|w| if w.system_id == *sys { w.dest_system_id } else { Some(w.system_id) }) {
                            Some(Some(far)) => format!("{} {}", icon::ARROW_RIGHT, name_of(far)),
                            Some(None) => format!("{} {}", icon::ARROW_RIGHT, hole.map_or("?", |w| w.dest.label())),
                            _ if s.name.is_empty() => "\u{2014}".to_owned(),
                            _ => s.name.clone(),
                        };
                        ui.add(egui::Label::new(tint(egui::RichText::new(info))).truncate());
                    });
                    row.col(|ui| {
                        let found = found_at(s.added_at, now, eve);
                        ui.label(tint(egui::RichText::new(found))).on_hover_text(found_hover(s.added_at, now, eve));
                    });
                    row.col(|ui| {
                        ui.label(tint(egui::RichText::new(human_ago(now - s.updated_at))))
                            .on_hover_text(format!("Last seen in a paste {} ago by {}", human_ago(now - s.updated_at), s.who));
                    });
                    if show_by {
                        row.col(|ui| {
                            ui.add(egui::Label::new(egui::RichText::new(&s.who).weak()).truncate());
                        });
                    }
                    row.col(|ui| {
                        if crate::widgets::icon_button(ui, icon::X).on_hover_text("Delete").clicked() {
                            delete = vec![key.clone()];
                        }
                        if (hole.is_some() || s.group == "Wormhole")
                            && crate::widgets::icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text(if hole.is_some() { "Edit this wormhole" } else { "Add it as a wormhole" }).clicked()
                        {
                            match hole {
                                Some(w) => act.edit = Some(w.uid.clone()),
                                None => new_hole = Some(key.clone()),
                            }
                        }
                    });
                });
            });
    }

    if let Some(id) = fold {
        if !b.collapsed.remove(&id) {
            b.collapsed.insert(id);
        }
    }
    if let Some(k) = resort {
        if b.sort == k {
            b.descending = !b.descending;
        } else {
            b.sort = k;
            b.descending = k == SigSort::Seen;
        }
    }
    if let Some(k) = toggle {
        if !b.picked.remove(&k) {
            b.picked.insert(k);
        }
    }
    if !delete.is_empty() {
        act.deleted = b.take(&delete);
    }
    if let Some((sys, sig)) = new_hole {
        let unidentified = b.rows.iter().find(|(i, s)| *i == sys && s.sig == sig).map(|(_, s)| s.name.clone()).unwrap_or_default();
        let wh_type = unidentified.to_lowercase().contains("unidentified").then(|| spai_core::whdata::drifter_code(sys)).flatten();
        act.new_hole = Some(WhForm::at(name_of(sys), sig, wh_type));
    }
    act
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(id: &str, group: &str, name: &str, seen: i64) -> SystemSig {
        SystemSig { sig: id.into(), kind: "Cosmic Signature".into(), group: group.into(), name: name.into(), added_at: seen, updated_at: seen, who: "Scout Alpha".into(), origin: None }
    }

    #[test]
    fn the_filter_searches_every_field_and_narrows_by_group_and_age() {
        let now = 100_000;
        let wh = sig("ABC-123", "Wormhole", "Unstable Wormhole", now - 60);
        let data = sig("XYZ-999", "Data Site", "Unsecured Frontier Server", now - 10 * 3600);
        let mut b = SigBrowser::default();
        assert!(sig_shown(&b, "Jita", &wh, now) && sig_shown(&b, "Jita", &data, now));
        b.query = "frontier".into();
        assert!(!sig_shown(&b, "Jita", &wh, now) && sig_shown(&b, "Jita", &data, now));
        b.query = "jit".into();
        assert!(sig_shown(&b, "Jita", &wh, now), "by system name");
        b.query.clear();
        b.groups.insert("wormhole".into());
        assert!(sig_shown(&b, "Jita", &wh, now) && !sig_shown(&b, "Jita", &data, now));
        b.groups.clear();
        b.within_h = 6;
        assert!(sig_shown(&b, "Jita", &wh, now) && !sig_shown(&b, "Jita", &data, now), "seen ten hours ago");
        let anomaly = SystemSig { kind: "Cosmic Anomaly".into(), ..sig("WKR-862", "Combat Site", "Angel Haven", now) };
        assert_eq!(sig_group(&anomaly), "combat", "an anomaly is still a combat site");
        b.within_h = 0;
        b.kinds = SigKinds::Anomalies;
        assert!(sig_shown(&b, "Jita", &anomaly, now) && !sig_shown(&b, "Jita", &wh, now));
        b.kinds = SigKinds::Signatures;
        assert!(!sig_shown(&b, "Jita", &anomaly, now) && sig_shown(&b, "Jita", &wh, now));
        assert_eq!(sig_group(&sig("QQQ-111", "", "", now)), "unscanned");
    }
}
