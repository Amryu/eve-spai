//! The Overview tab: overview windows and their tabs, presets, columns and options.

use super::sidebar_tab::{row, scroll};
use super::*;
use crate::evesettings::overview::{plain, OPTIONS};

#[derive(Default)]
pub(crate) struct State {
    /// The preset being renamed and the text typed so far.
    rename: Option<(String, String)>,
}

enum Op {
    MoveTab(i64, i32),
    TabToWindow(i64, usize),
    RemoveTab(i64),
    Duplicate(String),
    RemovePreset(String),
    Rename(String, String),
    MoveColumn(usize, i32),
}

fn preset_combo(ui: &mut egui::Ui, id: (&str, i64), value: &mut String, presets: &[String], width: f32) {
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(plain(value))
        .truncate()
        .show_ui(ui, |ui| {
            for p in presets {
                ui.menu_value(value, p.clone(), plain(p));
            }
        })
        .response
        .on_hover_text(plain(value));
}

pub(crate) fn ui(ui: &mut egui::Ui, st: &mut State, s: &mut Session) {
    let others = s.account_id.is_some();
    let o = &mut s.overview.now;
    let presets: Vec<String> = o.presets.iter().map(|p| p.name.clone()).collect();
    let mut ops = Vec::new();
    ui.add_space(4.0);
    if others {
        let hint = tr!("Stored per account: every character on the account shares these.");
        clipped(ui, egui::RichText::new(hint).weak(), hint);
    }
    scroll(ui, "overview_scroll", |ui| {
        let wide = ui.available_width() >= 760.0;
        let field = if wide { 190.0 } else { 120.0 };
        for wi in 0..o.windows.len() {
            ui.add_space(6.0);
            let title = if wi == 0 { tr!("Overview").to_owned() } else { format!("{} {}", tr!("Overview"), wi + 1) };
            ui.label(egui::RichText::new(title).strong());
            let tabs = o.windows[wi].clone();
            if tabs.is_empty() {
                ui.label(egui::RichText::new(tr!("No tabs.")).weak());
            }
            for (ti, index) in tabs.iter().copied().enumerate() {
                let windows = o.windows.len();
                let Some(tab) = o.tab_mut(index) else {
                    continue;
                };
                ui.push_id(("ov_tab", index), |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ROW_H - 4.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Remove this tab")).clicked() {
                                ops.push(Op::RemoveTab(index));
                            }
                            if windows > 1 {
                                ui.menu_button(icon::ARROW_SQUARE_OUT, |ui| {
                                    for to in (0..windows).filter(|to| *to != wi) {
                                        if ui.button(format!("{} {}", tr!("Overview"), to + 1)).clicked() {
                                            ops.push(Op::TabToWindow(index, to));
                                            ui.close();
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(tr!("Move to another overview window"));
                            }
                            let down = ui.add_enabled_ui(ti + 1 < tabs.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
                            if down.on_hover_text(tr!("Later tab")).clicked() {
                                ops.push(Op::MoveTab(index, 1));
                            }
                            let up = ui.add_enabled_ui(ti > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
                            if up.on_hover_text(tr!("Earlier tab")).clicked() {
                                ops.push(Op::MoveTab(index, -1));
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add_sized([if wide { 150.0 } else { 100.0 }, 22.0], egui::TextEdit::singleline(&mut tab.name))
                                    .on_hover_text(tr!("Tab name. The game's own markup for size and colour is kept as typed."));
                                preset_combo(ui, ("ov_preset", index), &mut tab.preset, &presets, field);
                                if wide {
                                    ui.label(egui::RichText::new(tr!("brackets")).weak());
                                    preset_combo(ui, ("ov_bracket", index), &mut tab.bracket, &presets, field);
                                }
                            });
                        },
                    );
                });
            }
        }

        ui.add_space(12.0);
        ui.label(egui::RichText::new(trf!("Presets ({n})", n = o.presets.len())).strong());
        for p in &o.presets {
            let uses = o.uses(&p.name);
            let name = plain(&p.name);
            let renaming = st.rename.as_ref().is_some_and(|(from, _)| *from == p.name);
            if renaming {
                ui.horizontal(|ui| {
                    let Some((from, text)) = st.rename.as_mut() else { return };
                    let edit = ui.add_sized([(ui.available_width() - 150.0).max(120.0), 22.0], egui::TextEdit::singleline(text));
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button(tr!("Rename")).clicked() || enter {
                        ops.push(Op::Rename(from.clone(), text.clone()));
                    }
                    if ui.button(tr!("Cancel")).clicked() {
                        ops.push(Op::Rename(String::new(), String::new()));
                    }
                });
                continue;
            }
            let label = if uses == 0 {
                trf!("{name}   ·   groups: {n}   ·   not on a tab", name = name, n = p.groups)
            } else {
                trf!("{name}   ·   groups: {n}   ·   tabs: {uses}", name = name, n = p.groups, uses = uses)
            };
            row(ui, "ov_presets", &p.name, 0.0, &label, false, |ui| {
                let del = ui.add_enabled_ui(uses == 0, |ui| spai_ui::widgets::icon_button(ui, icon::TRASH)).inner;
                if del.on_hover_text(tr!("Delete this preset")).on_disabled_hover_text(tr!("A tab still uses it.")).clicked() {
                    ops.push(Op::RemovePreset(p.name.clone()));
                }
                if spai_ui::widgets::icon_button(ui, icon::COPY).on_hover_text(tr!("Duplicate")).clicked() {
                    ops.push(Op::Duplicate(p.name.clone()));
                }
                if spai_ui::widgets::icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text(tr!("Rename")).clicked() {
                    st.rename = Some((p.name.clone(), p.name.clone()));
                }
            });
        }

        ui.add_space(12.0);
        ui.label(egui::RichText::new(tr!("Options")).strong());
        ui.horizontal_wrapped(|ui| {
            for (key, on) in &mut o.options {
                let label = OPTIONS.iter().find(|(k, _)| k == key).map_or(key.as_str(), |(_, l)| spai_ui::i18n::t(l));
                ui.checkbox(on, label);
            }
        });

        ui.add_space(12.0);
        ui.label(egui::RichText::new(tr!("Columns, left to right")).strong());
        let order = o.column_order.clone();
        for (i, col) in order.iter().enumerate() {
            let mut on = o.columns.contains(col);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width().min(360.0), ROW_H - 4.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let down = ui.add_enabled_ui(i + 1 < order.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
                    if down.on_hover_text(tr!("Move right")).clicked() {
                        ops.push(Op::MoveColumn(i, 1));
                    }
                    let up = ui.add_enabled_ui(i > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
                    if up.on_hover_text(tr!("Move left")).clicked() {
                        ops.push(Op::MoveColumn(i, -1));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if ui.checkbox(&mut on, col.as_str()).changed() {
                            if on {
                                o.columns.push(col.clone());
                                o.columns.sort();
                            } else {
                                o.columns.retain(|c| c != col);
                            }
                        }
                    });
                },
            );
        }
    });
    for op in ops {
        match op {
            Op::MoveTab(i, by) => o.move_tab(i, by),
            Op::TabToWindow(i, w) => o.move_tab_to_window(i, w),
            Op::RemoveTab(i) => o.remove_tab(i),
            Op::Duplicate(name) => {
                o.duplicate_preset(&name);
            }
            Op::RemovePreset(name) => {
                o.remove_preset(&name);
            }
            Op::Rename(from, to) => {
                if from.is_empty() || from == to || o.rename_preset(&from, &to) {
                    st.rename = None;
                }
            }
            Op::MoveColumn(i, by) => {
                let to = (i as i32 + by).clamp(0, o.column_order.len() as i32 - 1) as usize;
                let c = o.column_order.remove(i);
                o.column_order.insert(to, c);
            }
        }
    }
}
