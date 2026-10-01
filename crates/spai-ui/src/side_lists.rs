//! The wormhole map's side panel lists, shared by both apps: a system's connections and its
//! pasted signatures. Both are tables whose middle column takes the width the panel has left, so
//! resizing the panel widens what is worth reading, and nothing wraps.

use egui_phosphor::regular as icon;
use spai_core::geo::Systems;
use spai_core::wormholes::{SystemSig, Wormhole};

use crate::wh_form::WhForm;
use crate::wh_graph::{hole_code, life_badge, mass_color, short_group, sig_hole, unidentified_type, who_lines, wh_route_toggle};
use crate::widgets::{age_color, found_at, found_hover, human_ago, icon_button, sig_icon};

/// What a click in the connections list asks for. Holes by uid.
#[derive(Default)]
pub struct ConnAct {
    pub select: Option<i64>,
    pub edit: Option<String>,
    pub kill: Option<String>,
    pub toggle: Option<String>,
}

/// What a click in the signatures list asks for.
#[derive(Default)]
pub struct SigRowAct {
    pub select: Option<i64>,
    pub delete: Option<SystemSig>,
    /// The hole a signature is, to edit, by uid.
    pub edit: Option<String>,
    pub new_hole: Option<WhForm>,
}

fn text_w(ui: &egui::Ui, t: &str) -> f32 {
    ui.painter().layout_no_wrap(t.to_owned(), egui::TextStyle::Body.resolve(ui.style()), egui::Color32::WHITE).size().x
}

/// Room kept free at a table's right edge, so a scrollbar drawn over it covers no button.
pub fn scrollbar_gutter(ui: &egui::Ui) -> f32 {
    let s = &ui.spacing().scroll;
    s.bar_width + s.bar_inner_margin + s.bar_outer_margin
}

/// Width of `n` icon buttons in a row.
fn buttons_w(ui: &egui::Ui, n: usize) -> f32 {
    let one = text_w(ui, icon::PENCIL_SIMPLE).max(text_w(ui, icon::PROHIBIT)) + 8.0;
    n as f32 * one + n.saturating_sub(1) as f32 * 2.0
}

/// The holes at `sel`, each a row: its signature, where it leads with the type, time left and mass
/// under it, who added it, and edit, close and the switch for routes. `off` says which holes are
/// switched off for routes; without `can_edit` only that switch shows.
pub fn connections(ui: &mut egui::Ui, sel: i64, here: &[&Wormhole], now: i64, geo: &Systems, can_edit: bool, off: &dyn Fn(&Wormhole) -> bool) -> ConnAct {
    let mut act = ConnAct::default();
    if here.is_empty() {
        return act;
    }
    let name = |id: i64| geo.info_of(id).map_or_else(|| format!("#{id}"), |i| i.name.clone());
    let line_h = ui.text_style_height(&egui::TextStyle::Body);
    let row_h = line_h * 2.0 + 6.0;
    let sig_w = text_w(ui, "MMM-888");
    let who_w = text_w(ui, "Mmmmmmmmm");
    let actions_w = buttons_w(ui, if can_edit { 3 } else { 1 }) + scrollbar_gutter(ui);
    // The table takes one column gap more than it is given; in a resizable panel that grows the
    // panel a little every frame until it settles. Give it one gap less.
    let room = egui::vec2(ui.available_width() - ui.spacing().item_spacing.x, 0.0);
    ui.allocate_ui(room, |ui| {
        egui_extras::TableBuilder::new(ui)
            .id_salt(("wh_side_conns", sel))
            .striped(true)
            .vscroll(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(sig_w))
            .column(egui_extras::Column::remainder().at_least(60.0).clip(true))
            .column(egui_extras::Column::exact(who_w).clip(true))
            .column(egui_extras::Column::exact(actions_w))
            .body(|mut body| {
                for w in here {
                    body.row(row_h, |mut row| {
                        // Seen from the selected side.
                        let (sig, far, far_sig) = if w.system_id == sel { (&w.signature, w.dest_system_id, &w.dest_signature) } else { (&w.dest_signature, Some(w.system_id), &w.signature) };
                        row.col(|ui| {
                            ui.label(sig.as_deref().unwrap_or("\u{2014}"));
                        });
                        let two_lines = hole_code(w).is_some() || life_badge(w, now, &egui::Visuals::dark()).is_some() || w.mass.is_some();
                        row.col(|ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                // One line sits level with the rest of the row.
                                if !two_lines {
                                    ui.add_space((row_h - line_h) / 2.0 - 3.0);
                                }
                                match far {
                                    Some(f) => {
                                        let text = match far_sig {
                                            Some(s) => format!("{} {} {s}", icon::ARROW_RIGHT, name(f)),
                                            None => format!("{} {}", icon::ARROW_RIGHT, name(f)),
                                        };
                                        let link = egui::RichText::new(text).color(ui.visuals().hyperlink_color);
                                        if ui.add(egui::Label::new(link).truncate().sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                            act.select = Some(f);
                                        }
                                    }
                                    None => {
                                        ui.add(egui::Label::new(format!("{} {}", icon::ARROW_RIGHT, w.dest.label())).truncate());
                                    }
                                }
                                // One label, so a narrow column cuts it short with an ellipsis.
                                let font = egui::TextStyle::Body.resolve(ui.style());
                                let weak = ui.visuals().weak_text_color();
                                let mut job = egui::text::LayoutJob::default();
                                let part = |job: &mut egui::text::LayoutJob, text: &str, color: egui::Color32| {
                                    if !job.text.is_empty() {
                                        job.append("  ", 0.0, egui::TextFormat::simple(font.clone(), weak));
                                    }
                                    job.append(text, 0.0, egui::TextFormat::simple(font.clone(), color));
                                };
                                if let Some(ty) = hole_code(w) {
                                    part(&mut job, &ty, weak);
                                }
                                if let Some((text, color)) = life_badge(w, now, ui.visuals()) {
                                    part(&mut job, &text, color);
                                }
                                if let Some(m) = w.mass {
                                    part(&mut job, m.short(), mass_color(Some(m)));
                                }
                                let read = w.observed_at.map(|t| format!(", read {} ago", human_ago(now - t))).unwrap_or_default();
                                let hover = [hole_code(w), w.hours_left(now).map(|h| format!("Time left: {h}h{read}")), w.mass.map(|m| format!("Mass: {}", m.label()))].into_iter().flatten().collect::<Vec<_>>().join("\n");
                                if !job.text.is_empty() {
                                    ui.add(egui::Label::new(job).truncate().show_tooltip_when_elided(false)).on_hover_text(hover);
                                }
                            });
                        });
                        row.col(|ui| {
                            let (added, edited) = who_lines(w, now);
                            let hover = match &edited {
                                Some(e) => format!("{added}\n{e}"),
                                None => added,
                            };
                            // The latest hand on it: the edit when there was one, else who added it.
                            let (at, by) = match (&edited, &w.edited_by) {
                                (Some(_), Some((who, at))) => (format!("{} {} ago", icon::PENCIL_SIMPLE, human_ago(now - at)), who.clone()),
                                _ => (format!("{} ago", human_ago(now - w.reported_at)), w.created_by.as_ref().or(w.detected_by.as_ref()).cloned().unwrap_or_else(|| w.source.label().to_owned())),
                            };
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.add(egui::Label::new(egui::RichText::new(at).weak()).truncate().show_tooltip_when_elided(false)).on_hover_text(&hover);
                                ui.add(egui::Label::new(egui::RichText::new(by).weak()).truncate().show_tooltip_when_elided(false)).on_hover_text(&hover);
                            });
                        });
                        row.col(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if can_edit {
                                if icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text("Edit this hole").clicked() {
                                    act.edit = Some(w.uid.clone());
                                }
                                if icon_button(ui, icon::X).on_hover_text("Mark this hole dead").clicked() {
                                    act.kill = Some(w.uid.clone());
                                }
                            }
                            if wh_route_toggle(ui, off(w)) {
                                act.toggle = Some(w.uid.clone());
                            }
                        });
                    });
                }
            });
    });
    act
}

/// The signatures pasted at `sel` (named `sys_name`): id, when found, and what it is or where the
/// hole it is leads, with remove and, for a wormhole, edit. `every` is every hole known, filtered
/// or not, so a signature still says where it goes. `eve` shows times in EVE time.
#[allow(clippy::too_many_arguments)]
pub fn sig_table(ui: &mut egui::Ui, sel: i64, sys_name: &str, sigs: &[SystemSig], every: &[Wormhole], now: i64, eve: bool, geo: &Systems, can_edit: bool) -> SigRowAct {
    let mut act = SigRowAct::default();
    let name = |id: i64| geo.info_of(id).map_or_else(|| format!("#{id}"), |i| i.name.clone());
    let row_h = ui.spacing().interact_size.y + 4.0;
    let id_w = text_w(ui, &format!("{} MMM-888", icon::MAGNIFYING_GLASS));
    // As wide as the times shown: a weekday only for the ones not from today.
    let found_head = egui::WidgetText::from(egui::RichText::new("Found").strong()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x;
    let found_w = sigs.iter().map(|sg| text_w(ui, &found_at(sg.added_at, now, eve))).fold(found_head, f32::max);
    let room = egui::vec2(ui.available_width() - ui.spacing().item_spacing.x, 0.0);
    let actions_w = buttons_w(ui, 2) + scrollbar_gutter(ui);
    let visuals = ui.visuals().clone();
    ui.allocate_ui(room, |ui| {
        egui_extras::TableBuilder::new(ui)
            .id_salt("wh_graph_scan")
            .striped(true)
            .vscroll(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(id_w))
            .column(egui_extras::Column::exact(found_w))
            // Narrow enough for the panel as it is: pasting must never widen it.
            .column(egui_extras::Column::remainder().at_least(40.0).clip(true))
            .column(egui_extras::Column::exact(actions_w))
            .header(row_h, |mut header| {
                for h in ["Id", "Found", "Info", ""] {
                    header.col(|ui| {
                        ui.label(egui::RichText::new(h).strong());
                    });
                }
            })
            .body(|mut body| {
                for sg in sigs {
                    body.row(row_h, |mut row| {
                        let anomaly = sg.kind.to_lowercase().contains("anomal");
                        let aged = age_color(&visuals, now, sg.updated_at);
                        row.col(|ui| {
                            let text = format!("{} {}", sig_icon(&sg.kind), sg.sig);
                            let id = ui.label(match aged {
                                Some(c) => egui::RichText::new(text).color(c),
                                None if anomaly => egui::RichText::new(text).weak(),
                                None => egui::RichText::new(text),
                            });
                            id.on_hover_text(format!("{}\nAdded {} ago by {}, last seen in a paste {} ago", sg.kind, human_ago(now - sg.added_at), sg.who, human_ago(now - sg.updated_at)));
                        });
                        row.col(|ui| {
                            let t = egui::RichText::new(found_at(sg.added_at, now, eve));
                            ui.label(match aged {
                                Some(c) => t.color(c),
                                None => t,
                            })
                            .on_hover_text(found_hover(sg.added_at, now, eve));
                        });
                        // A wormhole signature we know the far side of says where it goes.
                        let hole = sig_hole(every, sel, &sg.sig);
                        row.col(|ui| {
                            // The group in front, short and grey, so the site's name gets the room.
                            ui.label(egui::RichText::new(short_group(&sg.group)).weak());
                            match hole.map(|w| if w.system_id == sel { w.dest_system_id } else { Some(w.system_id) }) {
                                Some(Some(f)) => {
                                    let code = hole.and_then(hole_code).map(|c| format!("{c} ")).unwrap_or_default();
                                    let text = egui::RichText::new(format!("{code}{} {}", icon::ARROW_RIGHT, name(f))).color(ui.visuals().hyperlink_color);
                                    if ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        act.select = Some(f);
                                    }
                                }
                                // Known to be a hole, its far side only a kind of space so far.
                                Some(None) => {
                                    let code = hole.and_then(hole_code).map(|c| format!("{c} ")).unwrap_or_default();
                                    let dest = hole.map_or("?", |w| w.dest.label());
                                    ui.add(egui::Label::new(format!("{code}{} {dest}", icon::ARROW_RIGHT)).truncate());
                                }
                                _ => {
                                    let text = match unidentified_type(sel, &sg.name) {
                                        Some(code) => format!("{code} \u{b7} {}", sg.name),
                                        None if sg.name.is_empty() => "\u{2014}".to_owned(),
                                        None => sg.name.clone(),
                                    };
                                    let text = egui::RichText::new(text);
                                    ui.add(egui::Label::new(if let Some(c) = aged { text.color(c) } else { text }).truncate());
                                }
                            }
                        });
                        row.col(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            // Remove first, so those line up whether or not an edit button follows.
                            if icon_button(ui, icon::X).on_hover_text("Remove").clicked() {
                                act.delete = Some(sg.clone());
                            }
                            let is_hole = hole.is_some() || sg.group == "Wormhole";
                            if can_edit && is_hole && icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text(if hole.is_some() { "Edit this wormhole" } else { "Add it as a wormhole" }).clicked() {
                                match hole {
                                    Some(w) => act.edit = Some(w.uid.clone()),
                                    None => act.new_hole = Some(WhForm::at(sys_name.to_owned(), sg.sig.clone(), unidentified_type(sel, &sg.name))),
                                }
                            }
                        });
                    });
                }
            });
    });
    act
}
