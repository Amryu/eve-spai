//! The wormhole tab's signature browser: every probe scan signature pasted, in every system, to
//! search, filter and clean up.

use std::collections::BTreeSet;

use super::*;
use crate::store::SystemSig;

/// How often the list is read again while it is on screen: pastes and shares land from elsewhere.
const RELOAD: std::time::Duration = std::time::Duration::from_secs(3);

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
pub(crate) enum SigKinds {
    #[default]
    Both,
    Signatures,
    Anomalies,
}

impl SigKinds {
    const ALL: [SigKinds; 3] = [SigKinds::Both, SigKinds::Signatures, SigKinds::Anomalies];

    fn label(self) -> &'static str {
        match self {
            SigKinds::Both => "Sigs & anoms",
            SigKinds::Signatures => "Signatures",
            SigKinds::Anomalies => "Anomalies",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SigSort {
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
pub(crate) struct SigBrowser {
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
    rows: Vec<(i64, SystemSig)>,
    /// What each delete took, the latest last, to put back while the app runs.
    undo: Vec<Vec<(i64, SystemSig)>>,
    read_at: Option<std::time::Instant>,
    /// Rows given by a test, never read again from the store.
    fixed: bool,
}

pub(crate) fn is_anomaly(s: &SystemSig) -> bool {
    s.kind.to_lowercase().contains("anomal")
}

pub(crate) use spai_ui::widgets::{age_color, found_at, found_hover};

/// Room kept free at a table's right edge, so a scrollbar drawn over it covers no button.
pub(crate) fn scrollbar_gutter(ui: &egui::Ui) -> f32 {
    let s = &ui.spacing().scroll;
    s.bar_width + s.bar_inner_margin + s.bar_outer_margin
}

/// Which filter group a signature falls in.
pub(crate) fn sig_group(s: &SystemSig) -> &'static str {
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
pub(crate) fn sig_shown(b: &SigBrowser, system_name: &str, s: &SystemSig, now: i64) -> bool {
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

impl SpaiApp {
    #[cfg(test)]
    pub(crate) fn sig_browser_seed(&mut self, rows: Vec<(i64, SystemSig)>) {
        self.sig_browser.rows = rows;
        self.sig_browser.fixed = true;
    }

    #[cfg(test)]
    pub(crate) fn sig_browser_set_tree(&mut self, on: bool) {
        self.sig_browser.tree = on;
    }

    /// Reads the list again on its next frame.
    pub(crate) fn sig_browser_refresh(&mut self) {
        self.sig_browser.read_at = None;
    }

    fn sig_browser_reload(&mut self) {
        let b = &mut self.sig_browser;
        if b.fixed || b.read_at.is_some_and(|t| t.elapsed() < RELOAD) {
            return;
        }
        b.read_at = Some(std::time::Instant::now());
        if let Some(store) = self.store.as_ref() {
            b.rows = store.all_system_sigs();
        }
        let keep: BTreeSet<(i64, String)> = b.rows.iter().map(|(id, s)| (*id, s.sig.clone())).collect();
        b.picked.retain(|p| keep.contains(p));
    }

    fn sig_browser_delete(&mut self, which: &[(i64, String)]) {
        let gone: BTreeSet<&(i64, String)> = which.iter().collect();
        let rows: Vec<(i64, SystemSig)> =
            self.sig_browser.rows.iter().filter(|(id, s)| gone.contains(&(*id, s.sig.clone()))).cloned().collect();
        self.sig_delete(rows);
    }

    /// Deletes signatures everywhere, sharing it, and keeps them for [`Self::sig_undo`].
    pub(crate) fn sig_delete(&mut self, rows: Vec<(i64, SystemSig)>) {
        if rows.is_empty() {
            return;
        }
        if let Some(store) = self.store.as_ref() {
            for (sys, s) in &rows {
                store.delete_system_sig(*sys, &s.sig);
            }
        }
        let b = &mut self.sig_browser;
        b.rows.retain(|(id, s)| !rows.iter().any(|(i, r)| i == id && r.sig == s.sig));
        for (sys, s) in &rows {
            b.picked.remove(&(*sys, s.sig.clone()));
        }
        if b.undo.len() >= UNDO_DEPTH {
            b.undo.remove(0);
        }
        b.undo.push(rows);
        // The map's side panel keeps its own copy of the selected system's list.
        self.wh_graph.sigs = None;
    }

    /// Puts the latest delete back, sharing it again.
    pub(crate) fn sig_undo(&mut self) {
        let Some(rows) = self.sig_browser.undo.pop() else { return };
        if let Some(store) = self.store.as_ref() {
            let systems: BTreeSet<i64> = rows.iter().map(|r| r.0).collect();
            for sys in systems {
                let sigs: Vec<SystemSig> = rows.iter().filter(|r| r.0 == sys).map(|r| r.1.clone()).collect();
                store.restore_system_sigs(sys, &sigs);
            }
        }
        let b = &mut self.sig_browser;
        b.rows.retain(|(id, s)| !rows.iter().any(|(i, r)| i == id && r.sig == s.sig));
        b.rows.extend(rows);
        self.wh_graph.sigs = None;
    }

    /// What undo would put back, for its button.
    pub(crate) fn sig_undo_hint(&self) -> String {
        let Some(rows) = self.sig_browser.undo.last() else { return "Nothing deleted to undo".to_owned() };
        let geo = self.systems.as_ref();
        let name = |id: i64| geo.and_then(|g| g.info_of(id)).map_or_else(|| format!("#{id}"), |i| i.name.clone());
        let mut list: Vec<String> = rows.iter().take(8).map(|(sys, s)| format!("{} in {}", s.sig, name(*sys))).collect();
        if rows.len() > 8 {
            list.push(format!("and {} more", rows.len() - 8));
        }
        format!("Undo the last delete (Ctrl+Z): {}", list.join(", "))
    }

    /// An undo button, always there so nothing shifts when it becomes usable, and Ctrl+Z while
    /// no text field has the keyboard.
    pub(crate) fn sig_undo_button(&mut self, ui: &mut egui::Ui, text: &str) {
        let can = !self.sig_browser.undo.is_empty();
        let hint = self.sig_undo_hint();
        let clicked = ui.add_enabled(can, egui::Button::new(text)).on_hover_text(&hint).on_disabled_hover_text(&hint).clicked();
        let key = can
            && ui.memory(|m| m.focused().is_none())
            && ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z)));
        if clicked || key {
            self.sig_undo();
        }
    }

    pub(crate) fn sig_browser_view(&mut self, ui: &mut egui::Ui) {
        use crate::app::SteadySelect as _;
        use egui_phosphor::regular as icon;
        self.sig_browser_reload();
        ui.ctx().request_repaint_after(RELOAD);
        let now = crate::clock::utc().timestamp();
        let geo = self.systems.clone();
        let name_of = |id: i64| geo.as_ref().and_then(|g| g.info_of(id)).map_or_else(|| format!("#{id}"), |i| i.name.clone());

        ui.add_space(4.0);
        // Two fixed rows, filters then actions, so the table starts at the same height whatever
        // is picked and however narrow the window.
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.sig_browser.query).hint_text("Search system, signature, site, who").desired_width(220.0));
            let n = self.sig_browser.groups.len();
            let label = match n {
                0 => "All groups".to_owned(),
                1 => GROUPS.iter().find(|g| self.sig_browser.groups.contains(g.0)).map_or_else(String::new, |g| g.1.to_owned()),
                n => format!("{n} groups"),
            };
            let button = ui.button(format!("{label}  {}", icon::CARET_DOWN));
            egui::Popup::from_toggle_button_response(&button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                for (code, label) in GROUPS {
                    let on = self.sig_browser.groups.contains(code);
                    if ui.menu_label(on, label).clicked() {
                        if on {
                            self.sig_browser.groups.remove(code);
                        } else {
                            self.sig_browser.groups.insert(code.to_owned());
                        }
                    }
                }
            });
            egui::ComboBox::from_id_salt("sig_browser_kinds").selected_text(self.sig_browser.kinds.label()).show_ui(ui, |ui| {
                for k in SigKinds::ALL {
                    ui.menu_value(&mut self.sig_browser.kinds, k, k.label());
                }
            });
            let windows = [(0u32, "Any time"), (1, "Last hour"), (6, "Last 6 h"), (24, "Last day"), (72, "Last 3 days")];
            let current = windows.iter().find(|w| w.0 == self.sig_browser.within_h).map_or("Any time", |w| w.1);
            egui::ComboBox::from_id_salt("sig_browser_within").selected_text(format!("Seen: {current}")).show_ui(ui, |ui| {
                for (h, label) in windows {
                    ui.menu_value(&mut self.sig_browser.within_h, h, label);
                }
            });
        });

        let mut rows: Vec<(i64, String, SystemSig)> = self
            .sig_browser
            .rows
            .iter()
            .map(|(id, s)| (*id, name_of(*id), s.clone()))
            .filter(|(_, n, s)| sig_shown(&self.sig_browser, n, s, now))
            .collect();
        let sort = self.sig_browser.sort;
        rows.sort_by(|a, b| {
            let o = match sort {
                SigSort::System => a.1.cmp(&b.1).then(a.2.sig.cmp(&b.2.sig)),
                SigSort::Id => a.2.sig.cmp(&b.2.sig),
                SigSort::Group => a.2.group.cmp(&b.2.group),
                SigSort::Info => a.2.name.cmp(&b.2.name),
                SigSort::Found => a.2.added_at.cmp(&b.2.added_at),
                SigSort::Seen => a.2.updated_at.cmp(&b.2.updated_at),
                SigSort::Who => a.2.who.cmp(&b.2.who),
            };
            if self.sig_browser.descending { o.reverse() } else { o }
        });

        let shown: Vec<(i64, String)> = rows.iter().map(|(id, _, s)| (*id, s.sig.clone())).collect();
        let picked: Vec<(i64, String)> = self.sig_browser.picked.iter().filter(|p| shown.contains(p)).cloned().collect();
        let mut delete: Vec<(i64, String)> = Vec::new();
        ui.horizontal(|ui| {
            let systems: BTreeSet<i64> = rows.iter().map(|r| r.0).collect();
            ui.label(egui::RichText::new(format!("{} of {} signatures in {} systems", rows.len(), self.sig_browser.rows.len(), systems.len())).weak());
            ui.add_space(8.0);
            if ui.add_enabled(!picked.is_empty(), egui::Button::new(format!("{}  Delete {} picked", icon::TRASH, picked.len()))).clicked() {
                delete = picked.clone();
            }
            self.sig_undo_button(ui, &format!("{}  Undo", icon::ARROW_COUNTER_CLOCKWISE));
            ui.add_space(8.0);
            if ui.menu_label(!self.sig_browser.tree, icon::ROWS).on_hover_text("One list").clicked() {
                self.sig_browser.tree = false;
            }
            if ui.menu_label(self.sig_browser.tree, icon::TREE_VIEW).on_hover_text("By system: signatures under their system, with each system's latest paste").clicked() {
                self.sig_browser.tree = true;
            }
            if !rows.is_empty() {
                let all = picked.len() == rows.len();
                if ui.button(if all { "Pick none" } else { "Pick all shown" }).clicked() {
                    if all {
                        self.sig_browser.picked.clear();
                    } else {
                        self.sig_browser.picked.extend(shown.iter().cloned());
                    }
                }
            }
        });
        ui.separator();
        if self.sig_browser.rows.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No probe scans pasted yet.").weak());
                ui.label(egui::RichText::new("Select a system on the map and paste the probe scanner on its Signatures tab.").weak());
            });
            return;
        }

        // Rows under their system in the tree, each system with its count and latest paste.
        enum Line {
            System { id: i64, n: usize, anoms: usize, last: i64, who: String },
            Sig(usize),
        }
        let lines: Vec<Line> = if self.sig_browser.tree {
            let mut systems: Vec<(i64, &str, Vec<usize>)> = Vec::new();
            for (i, (id, n, _)) in rows.iter().enumerate() {
                match systems.iter_mut().find(|s| s.0 == *id) {
                    Some(s) => s.2.push(i),
                    None => systems.push((*id, n, vec![i])),
                }
            }
            let last = |ix: &[usize]| ix.iter().map(|i| rows[*i].2.updated_at).max().unwrap_or(0);
            match sort {
                SigSort::System if self.sig_browser.descending => systems.sort_by(|a, b| b.1.cmp(a.1)),
                SigSort::System => systems.sort_by(|a, b| a.1.cmp(b.1)),
                SigSort::Seen if !self.sig_browser.descending => systems.sort_by_key(|s| last(&s.2)),
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
                if !self.sig_browser.collapsed.contains(&id) {
                    out.extend(ix.into_iter().map(Line::Sig));
                }
            }
            out
        } else {
            (0..rows.len()).map(Line::Sig).collect()
        };

        let mut open: Option<i64> = None;
        let mut edit: Option<i64> = None;
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
        let id_w = fit("Id", "MMM-888");
        let group_w = fit("Group", "Combat");
        let seen_w = fit("Seen", "88m");
        let by_w = fit("By", "Mmmmmmmm");
        let visuals = ui.visuals().clone();
        let eve = self.settings.use_eve_time;
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
                            let arrow = match (sort == key, self.sig_browser.descending) {
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
                                let shut = self.sig_browser.collapsed.contains(id);
                                row.col(|ui| {
                                    let caret = if shut { icon::CARET_RIGHT } else { icon::CARET_DOWN };
                                    if ui.add(egui::Button::new(caret).frame(false)).on_hover_text(if shut { "Show its signatures" } else { "Fold it" }).clicked() {
                                        fold = Some(*id);
                                    }
                                });
                                row.col(|ui| {
                                    if ui.add(egui::Link::new(egui::RichText::new(name_of(*id)).strong())).on_hover_text("Show it on the wormhole map").clicked() {
                                        open = Some(*id);
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
                                    let t = egui::RichText::new(super::human_ago(now - last)).strong();
                                    ui.label(match age_color(&visuals, now, *last) {
                                        Some(c) => t.color(c),
                                        None => t,
                                    })
                                    .on_hover_text(format!("Latest paste {} ago by {who}", super::human_ago(now - last)));
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
                            let mut on = self.sig_browser.picked.contains(&key);
                            if ui.checkbox(&mut on, "").changed() {
                                toggle = Some(key.clone());
                            }
                        });
                        row.col(|ui| {
                            // Under its system in the tree, the name would only repeat it.
                            let link = egui::RichText::new(sys_name).color(ui.visuals().hyperlink_color);
                            if !self.sig_browser.tree
                                && ui
                                    .add(egui::Label::new(link).truncate().show_tooltip_when_elided(false).sense(egui::Sense::click()))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .on_hover_text(format!("{sys_name}: show it on the wormhole map"))
                                    .clicked()
                            {
                                open = Some(*sys);
                            }
                        });
                        row.col(|ui| {
                            let t = egui::RichText::new(format!("{} {}", spai_ui::widgets::sig_icon(&s.kind), s.sig));
                            ui.label(if is_anomaly(s) && aged.is_none() { t.weak() } else { tint(t) }).on_hover_text(&s.kind);
                        });
                        row.col(|ui| {
                            ui.add(egui::Label::new(tint(egui::RichText::new(super::wh_graph::short_group(&s.group)))).truncate());
                        });
                        let hole = super::wh_graph::sig_hole(&self.wh_cache, *sys, &s.sig);
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
                            ui.label(tint(egui::RichText::new(super::human_ago(now - s.updated_at))))
                                .on_hover_text(format!("Last seen in a paste {} ago by {}", super::human_ago(now - s.updated_at), s.who));
                        });
                        if show_by {
                            row.col(|ui| {
                                ui.add(egui::Label::new(egui::RichText::new(&s.who).weak()).truncate());
                            });
                        }
                        row.col(|ui| {
                            if ui.small_button(icon::X).on_hover_text("Delete").clicked() {
                                delete = vec![key.clone()];
                            }
                            if (hole.is_some() || s.group == "Wormhole")
                                && ui.small_button(icon::PENCIL_SIMPLE).on_hover_text(if hole.is_some() { "Edit this wormhole" } else { "Add it as a wormhole" }).clicked()
                            {
                                match hole {
                                    Some(w) => edit = Some(w.id),
                                    None => new_hole = Some(key.clone()),
                                }
                            }
                        });
                    });
                });
        }

        if let Some(id) = fold {
            if !self.sig_browser.collapsed.remove(&id) {
                self.sig_browser.collapsed.insert(id);
            }
        }
        if let Some(k) = resort {
            if self.sig_browser.sort == k {
                self.sig_browser.descending = !self.sig_browser.descending;
            } else {
                self.sig_browser.sort = k;
                self.sig_browser.descending = k == SigSort::Seen;
            }
        }
        if let Some(k) = toggle {
            if !self.sig_browser.picked.remove(&k) {
                self.sig_browser.picked.insert(k);
            }
        }
        if !delete.is_empty() {
            self.sig_browser_delete(&delete);
        }
        if let Some(id) = open {
            self.wh_graph.sig_browser = false;
            self.wh_graph.table = false;
            self.wh_graph.selected = Some(id);
            self.wh_graph.side_tab = super::wh_graph::SideTab::Sigs;
        }
        if let Some(id) = edit {
            self.wh_edit(id);
        }
        if let Some((sys, sig)) = new_hole {
            let unidentified = self.sig_browser.rows.iter().find(|(i, s)| *i == sys && s.sig == sig).map(|(_, s)| s.name.clone()).unwrap_or_default();
            let wh_type = unidentified.to_lowercase().contains("unidentified").then(|| crate::whdata::drifter_code(sys)).flatten();
            self.wh_form = Some(super::wormholes_ui::WhForm::at(name_of(sys), sig, wh_type));
        }
    }
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

    #[test]
    fn a_delete_comes_back_on_undo_the_latest_first() {
        let mut a = crate::app::SpaiApp::build(&egui::Context::default(), true);
        let rows = vec![(1, sig("AAA-111", "Wormhole", "", 10)), (1, sig("BBB-222", "Data Site", "", 20)), (2, sig("CCC-333", "", "", 30))];
        a.sig_browser_seed(rows.clone());
        a.sig_browser_delete(&[(1, "AAA-111".into()), (2, "CCC-333".into())]);
        a.sig_delete(vec![rows[1].clone()]);
        assert!(a.sig_browser.rows.is_empty());
        a.sig_undo();
        assert_eq!(a.sig_browser.rows, vec![rows[1].clone()], "the latest delete first");
        a.sig_undo();
        let mut back = a.sig_browser.rows.clone();
        back.sort_by(|x, y| x.1.sig.cmp(&y.1.sig));
        assert_eq!(back, rows);
        a.sig_undo();
        assert_eq!(a.sig_browser.rows.len(), 3, "nothing left to undo");
    }

        #[test]
    fn signatures_turn_yellow_after_a_day_and_grey_after_three() {
        let v = egui::Visuals::dark();
        let now = 1_000_000;
        assert_eq!(age_color(&v, now, now - 23 * 3600), None);
        assert_eq!(age_color(&v, now, now - 25 * 3600), Some(crate::theme::standing::WARNING));
        assert_eq!(age_color(&v, now, now - 73 * 3600), Some(v.weak_text_color()));
    }
}
