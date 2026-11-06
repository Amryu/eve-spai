//! The Overview tab: overview windows and their tabs, presets, the colours states are marked
//! with, columns and options, one page each.

use super::sidebar_tab::scroll;
use super::*;
use crate::evesettings::overview::{markup_color, plain, preset_states, state_name, Mark, Overview, Preset, CATEGORIES, OPTIONS};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Tabs,
    Presets,
    Colours,
    Columns,
}

/// A tab being edited in its dialog, apart from the settings until it is saved.
struct TabEdit {
    index: i64,
    name: String,
    preset: String,
    bracket: String,
    color: Option<[f32; 3]>,
    columns: Vec<String>,
}

/// Rows no wider than this: on a wide screen a row that ran the whole way put its buttons far
/// from the name they act on.
const ROW_MAX: f32 = 460.0;
/// Groups a preset has that no category here claims, or that have no name yet.
const OTHER: i64 = -1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PresetPage {
    #[default]
    Types,
    States,
}

/// A preset being edited in its dialog, apart from the settings until it is applied.
pub(crate) struct PresetEdit {
    name: String,
    work: Preset,
    page: PresetPage,
    category: i64,
    filter: String,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) page: Page,
    pub(crate) preset: Option<PresetEdit>,
    edit: Option<TabEdit>,
    /// The preset being renamed and the text typed so far.
    rename: Option<(String, String)>,
    filter: String,
}

enum Op {
    MoveTab(i64, i32),
    TabToWindow(i64, usize),
    RemoveTab(i64),
    /// The filter for what a tab lists (`false`) or for its brackets (`true`).
    SetFilter(i64, bool, String),
    Duplicate(String),
    RemovePreset(String),
    MoveColumn(usize, i32),
    MoveMark(Mark, i64, i32),
    MarkOn(Mark, i64, bool),
    MarkColor(Mark, i64, Option<[f32; 3]>),
    Blink(Mark, i64, bool),
}

fn rgb(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// The colour a name is shown in: the one its markup asks for, as the game would draw it.
fn name_color(ui: &egui::Ui, name: &str) -> egui::Color32 {
    markup_color(name).map_or(ui.visuals().text_color(), rgb)
}

/// One truncating label in two tones: `lead` in its colour, `rest` dimmed after it.
fn two_tone(ui: &mut egui::Ui, lead: &str, lead_color: egui::Color32, rest: &str) -> egui::Response {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    job.append(lead, 0.0, egui::TextFormat { font_id: font.clone(), color: lead_color, ..Default::default() });
    job.append(rest, 0.0, egui::TextFormat { font_id: font, color: ui.visuals().weak_text_color(), ..Default::default() });
    ui.add(egui::Label::new(job).truncate()).on_hover_text(format!("{lead}{rest}"))
}

fn window_title(wi: usize) -> String {
    if wi == 0 {
        tr!("Overview").to_owned()
    } else {
        format!("{} {}", tr!("Overview"), wi + 1)
    }
}

/// A dropdown of every filter, each in its own colour. Any filter can be what a tab lists or
/// what it draws brackets for, so both dropdowns of a tab offer the same list.
fn preset_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, value: &mut String, presets: &[String], width: f32) {
    let color = name_color(ui, value);
    combo(ui, id, width, egui::RichText::new(plain(value)).color(color), |ui| {
        for p in presets {
            let text = egui::RichText::new(plain(p)).color(name_color(ui, p));
            ui.menu_value(value, p.clone(), text);
        }
    })
    .on_hover_text(plain(value));
}

impl State {
    /// Opens the editor for what a preset shows.
    pub(crate) fn edit_preset(&mut self, o: &Overview, name: &str) {
        if let Some(p) = o.presets.iter().find(|p| p.name == name) {
            self.preset = Some(PresetEdit { name: name.to_owned(), work: p.clone(), page: PresetPage::Types, category: CATEGORIES[0].0, filter: String::new() });
        }
    }
}

#[cfg(test)]
impl State {
    pub(crate) fn show_preset_states(&mut self) {
        if let Some(e) = &mut self.preset {
            e.page = PresetPage::States;
        }
    }
}

pub(crate) fn ui(ui: &mut egui::Ui, st: &mut State, s: &mut Session, groups: &[(i64, i64, String)]) {
    let shared = s.account_id.is_some();
    let o = &mut s.overview.now;
    let mut ops = Vec::new();
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let tabs: usize = o.windows.iter().map(|w| w.len()).sum();
        for (page, label) in [
            (Page::Tabs, trf!("Tabs ({n})", n = tabs)),
            (Page::Presets, trf!("Filters ({n})", n = o.presets.len())),
            (Page::Colours, tr!("Colours").to_owned()),
            (Page::Columns, tr!("Columns and options").to_owned()),
        ] {
            if ui.menu_label(st.page == page, label).clicked() {
                st.page = page;
            }
        }
        if shared {
            let hint = tr!("Stored per account: every character on the account shares these.");
            clipped(ui, egui::RichText::new(hint).weak(), hint);
        }
    });
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(4.0);

    match st.page {
        Page::Tabs => tabs_page(ui, st, o, &mut ops),
        Page::Presets => presets_page(ui, st, o, &mut ops),
        Page::Colours => colours_page(ui, o, &mut ops),
        Page::Columns => columns_page(ui, o, &mut ops),
    }
    for op in ops {
        match op {
            Op::MoveTab(i, by) => o.move_tab(i, by),
            Op::TabToWindow(i, w) => o.move_tab_to_window(i, w),
            Op::RemoveTab(i) => o.remove_tab(i),
            Op::SetFilter(i, brackets, name) => {
                if let Some(tab) = o.tab_mut(i) {
                    *(if brackets { &mut tab.bracket } else { &mut tab.preset }) = name;
                }
            }
            Op::Duplicate(name) => {
                o.duplicate_preset(&name);
            }
            Op::RemovePreset(name) => {
                o.remove_preset(&name);
            }
            Op::MoveColumn(i, by) => {
                let to = (i as i32 + by).clamp(0, o.column_order.len() as i32 - 1) as usize;
                let c = o.column_order.remove(i);
                o.column_order.insert(to, c);
            }
            Op::MoveMark(mark, id, by) => o.marks_mut(mark).move_by(id, by),
            Op::MarkOn(mark, id, on) => o.marks_mut(mark).set_on(id, on),
            Op::MarkColor(mark, id, Some(c)) => {
                o.colors.insert((mark, id), c);
            }
            Op::MarkColor(mark, id, None) => {
                o.colors.remove(&(mark, id));
            }
            Op::Blink(mark, id, on) => {
                o.blinks.insert((mark, id), on);
            }
        }
    }
    dialogs(ui.ctx(), st, o, groups);
}

/// The tab's dialogs, whichever is open.
pub(crate) fn dialogs(ctx: &egui::Context, st: &mut State, o: &mut Overview, groups: &[(i64, i64, String)]) {
    tab_dialog(ctx, st, o);
    rename_dialog(ctx, st, o);
    preset_dialog(ctx, st, o, groups);
}

/// A tab row holds two dropdowns, which are taller than a list row's text.
const TAB_ROW_H: f32 = 32.0;
const TAB_NAME_W: f32 = 150.0;
/// An icon button with the gap after it.
const ICON_W: f32 = 21.0 + spai_fit_ui::rows::GAP;

fn tabs_page(ui: &mut egui::Ui, st: &mut State, o: &Overview, ops: &mut Vec<Op>) {
    let windows = o.windows.len();
    if windows == 0 {
        ui.label(egui::RichText::new(tr!("The game has not stored any overview tabs for this account yet.")).weak());
        return;
    }
    let hint = tr!("Each tab has a filter for what it lists and one for the brackets drawn in space. Any filter can be used for either.");
    clipped(ui, egui::RichText::new(hint).weak(), hint);
    ui.add_space(4.0);
    let presets: Vec<String> = o.presets.iter().map(|p| p.name.clone()).collect();
    scroll(ui, "ov_tabs_scroll", |ui| {
        // Both filters as dropdowns in the row while there is room for them; under that the row
        // names what the tab lists and the pencil opens the rest.
        let avail = ui.available_width();
        let combo = if avail >= 860.0 {
            Some(200.0)
        } else if avail >= 680.0 {
            Some(140.0)
        } else {
            None
        };
        let buttons = ICON_W * if windows > 1 { 5.0 } else { 4.0 };
        let row_w = match combo {
            Some(w) => TAB_NAME_W + 2.0 * (w + 8.0 + ICON_W) + buttons + 5.0 * spai_fit_ui::rows::GAP,
            None => ROW_MAX,
        };
        ui.set_max_width(avail.min(row_w));
        for wi in 0..windows {
            ui.label(egui::RichText::new(window_title(wi)).strong());
            let tabs = &o.windows[wi];
            if tabs.is_empty() {
                ui.label(egui::RichText::new(tr!("No tabs.")).weak());
            }
            if let (Some(w), false) = (combo, tabs.is_empty()) {
                spai_fit_ui::rows::row(ui, &format!("ov_tabs_head_{wi}"), "head", ROW_H - 6.0, false, |ui| {
                    spai_fit_ui::rows::head_cell(ui, TAB_NAME_W, tr!("Tab"), false);
                    spai_fit_ui::rows::head_cell(ui, w + 8.0 + ICON_W, tr!("Lists"), false);
                    spai_fit_ui::rows::head_cell(ui, w + 8.0 + ICON_W, tr!("Brackets"), false);
                });
            }
            for (ti, index) in tabs.iter().copied().enumerate() {
                let Some(tab) = o.tab(index) else {
                    continue;
                };
                let color = tab.color.map_or(ui.visuals().text_color(), color32);
                let name = plain(&tab.name);
                let lead = if name.is_empty() { tr!("(no name)").to_owned() } else { name };
                // A list per window: the checks hold one list to one pitch, and a heading sits
                // between two windows' rows.
                spai_fit_ui::rows::row(ui, &format!("ov_tabs_{wi}"), index, TAB_ROW_H, false, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Remove this tab")).clicked() {
                            ops.push(Op::RemoveTab(index));
                        }
                        if windows > 1 {
                            let button = spai_ui::widgets::icon_button(ui, icon::ARROW_SQUARE_OUT).on_hover_text(tr!("Move to another overview window"));
                            egui::Popup::menu(&button).show(|ui| {
                                for to in (0..windows).filter(|to| *to != wi) {
                                    if ui.button(window_title(to)).clicked() {
                                        ops.push(Op::TabToWindow(index, to));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        let down = ui.add_enabled_ui(ti + 1 < tabs.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
                        if down.on_hover_text(tr!("Later tab")).clicked() {
                            ops.push(Op::MoveTab(index, 1));
                        }
                        let up = ui.add_enabled_ui(ti > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
                        if up.on_hover_text(tr!("Earlier tab")).clicked() {
                            ops.push(Op::MoveTab(index, -1));
                        }
                        if spai_ui::widgets::icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text(tr!("Name, colour, filters and columns")).clicked() {
                            st.edit = Some(TabEdit {
                                index,
                                name: tab.name.clone(),
                                preset: tab.preset.clone(),
                                bracket: tab.bracket.clone(),
                                color: tab.color,
                                columns: tab.columns.clone(),
                            });
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| match combo {
                            Some(w) => {
                                spai_fit_ui::rows::cell(ui, TAB_NAME_W, egui::RichText::new(&lead).color(color), &lead);
                                for (brackets, current) in [(false, &tab.preset), (true, &tab.bracket)] {
                                    let mut pick = current.clone();
                                    preset_combo(ui, ("ov_row_filter", index, brackets), &mut pick, &presets, w);
                                    if pick != *current {
                                        ops.push(Op::SetFilter(index, brackets, pick));
                                    }
                                    let known = presets.contains(current);
                                    let edit = ui.add_enabled_ui(known, |ui| spai_ui::widgets::icon_button(ui, icon::FUNNEL)).inner;
                                    if edit.on_hover_text(tr!("What this filter takes in")).on_disabled_hover_text(tr!("One of the game's own filters, which is not stored in the settings.")).clicked() {
                                        st.edit_preset(o, current);
                                    }
                                }
                            }
                            None => {
                                two_tone(ui, &lead, color, &format!("   ·   {}", plain(&tab.preset)));
                            }
                        });
                    });
                });
            }
            ui.add_space(8.0);
        }
    });
}

fn presets_page(ui: &mut egui::Ui, st: &mut State, o: &Overview, ops: &mut Vec<Op>) {
    ui.horizontal(|ui| {
        ui.add_sized([220.0f32.min(ui.available_width() * 0.5), 22.0], egui::TextEdit::singleline(&mut st.filter).hint_text(tr!("Find a filter")));
        let hint = tr!("A filter decides what a tab lists or which brackets are drawn in space, and any filter can do either. One in use by a tab cannot be deleted.");
        clipped(ui, egui::RichText::new(hint).weak(), hint);
    });
    ui.add_space(4.0);
    let needle = st.filter.trim().to_lowercase();
    let shown: Vec<_> = o.presets.iter().filter(|p| needle.is_empty() || plain(&p.name).to_lowercase().contains(&needle)).collect();
    let mut one = |ui: &mut egui::Ui, p: &crate::evesettings::overview::Preset| {
        let uses = o.uses(&p.name);
        spai_fit_ui::rows::row(ui, "ov_presets", &p.name, ROW_H - 4.0, false, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let del = ui.add_enabled_ui(uses == 0, |ui| spai_ui::widgets::icon_button(ui, icon::TRASH)).inner;
                if del.on_hover_text(tr!("Delete this filter")).on_disabled_hover_text(tr!("A tab still uses it.")).clicked() {
                    ops.push(Op::RemovePreset(p.name.clone()));
                }
                if spai_ui::widgets::icon_button(ui, icon::COPY).on_hover_text(tr!("Duplicate")).clicked() {
                    ops.push(Op::Duplicate(p.name.clone()));
                }
                if spai_ui::widgets::icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text(tr!("Rename")).clicked() {
                    st.rename = Some((p.name.clone(), p.name.clone()));
                }
                if spai_ui::widgets::icon_button(ui, icon::FUNNEL).on_hover_text(tr!("What this filter takes in")).clicked() {
                    st.edit_preset(o, &p.name);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let rest = if uses == 0 {
                        trf!("   ·   groups: {n}", n = p.groups.len())
                    } else {
                        trf!("   ·   groups: {n}   ·   tabs: {uses}", n = p.groups.len(), uses = uses)
                    };
                    two_tone(ui, &plain(&p.name), name_color(ui, &p.name), &rest);
                });
            });
        });
    };
    scroll(ui, "ov_presets_scroll", |ui| {
        if shown.is_empty() {
            ui.label(egui::RichText::new(tr!("No filters.")).weak());
        }
        // Two columns once each is wide enough for a name: packs bring dozens of presets.
        let two = ui.available_width() >= 760.0 && shown.len() > 8;
        ui.set_max_width(ui.available_width().min(if two { ROW_MAX * 2.0 } else { ROW_MAX }));
        if two {
            let half = shown.len().div_ceil(2);
            ui.columns(2, |cols| {
                for (ci, ui) in cols.iter_mut().enumerate() {
                    for p in shown.iter().skip(ci * half).take(half) {
                        one(ui, p);
                    }
                }
            });
        } else {
            for p in &shown {
                one(ui, p);
            }
        }
    });
}

fn colours_page(ui: &mut egui::Ui, o: &Overview, ops: &mut Vec<Op>) {
    let hint = tr!("How the overview marks a pilot's state. The first state in the list that applies is the one shown.");
    clipped(ui, egui::RichText::new(hint).weak(), hint);
    ui.add_space(4.0);
    let mut list = |ui: &mut egui::Ui, mark: Mark| {
        let (title, list_id) = match mark {
            Mark::Tag => (tr!("Colour tags"), "ov_tags"),
            Mark::Background => (tr!("Backgrounds"), "ov_backgrounds"),
        };
        ui.label(egui::RichText::new(title).strong());
        let marks = o.marks(mark);
        if marks.order.is_empty() {
            ui.label(egui::RichText::new(tr!("The game has not stored these yet.")).weak());
        }
        for (i, id) in marks.order.iter().copied().enumerate() {
            spai_fit_ui::rows::row(ui, list_id, id, ROW_H - 4.0, false, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let down = ui.add_enabled_ui(i + 1 < marks.order.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
                    if down.on_hover_text(tr!("Check later")).clicked() {
                        ops.push(Op::MoveMark(mark, id, 1));
                    }
                    let up = ui.add_enabled_ui(i > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
                    if up.on_hover_text(tr!("Check earlier")).clicked() {
                        ops.push(Op::MoveMark(mark, id, -1));
                    }
                    let blink = o.blinks.get(&(mark, id)).copied().unwrap_or(false);
                    let b = ui.scope(|ui| {
                        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                        ui.add(egui::Button::new(icon::LIGHTNING).small().selected(blink))
                    });
                    if b.inner.on_hover_text(if blink { tr!("Blinks. Click to stop.") } else { tr!("Does not blink. Click to make it blink.") }).clicked() {
                        ops.push(Op::Blink(mark, id, !blink));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let mut on = marks.on.contains(&id);
                        if ui.checkbox(&mut on, "").on_hover_text(tr!("Mark this state")).changed() {
                            ops.push(Op::MarkOn(mark, id, on));
                        }
                        if let Some(pick) = color_menu(ui, o.colors.get(&(mark, id)).copied(), tr!("Game default"), true, tr!("Colour")) {
                            ops.push(Op::MarkColor(mark, id, pick));
                        }
                        let name = state_name(id);
                        let text = egui::RichText::new(&name);
                        clipped(ui, if on { text } else { text.weak() }, &name);
                    });
                });
            });
        }
    };
    scroll(ui, "ov_colours_scroll", |ui| {
        let two = ui.available_width() >= 720.0;
        ui.set_max_width(ui.available_width().min(if two { ROW_MAX * 2.0 } else { ROW_MAX }));
        if two {
            ui.columns(2, |cols| {
                list(&mut cols[0], Mark::Tag);
                list(&mut cols[1], Mark::Background);
            });
        } else {
            list(ui, Mark::Tag);
            ui.add_space(10.0);
            list(ui, Mark::Background);
        }
    });
}

fn columns_page(ui: &mut egui::Ui, o: &mut Overview, ops: &mut Vec<Op>) {
    let order = o.column_order.clone();
    let mut columns = |ui: &mut egui::Ui, o: &mut Overview| {
        ui.label(egui::RichText::new(tr!("Columns, left to right")).strong());
        if order.is_empty() {
            ui.label(egui::RichText::new(tr!("The game has not stored these yet.")).weak());
        }
        for (i, col) in order.iter().enumerate() {
            spai_fit_ui::rows::row(ui, "ov_columns", col, ROW_H - 4.0, false, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let down = ui.add_enabled_ui(i + 1 < order.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
                    if down.on_hover_text(tr!("Move right")).clicked() {
                        ops.push(Op::MoveColumn(i, 1));
                    }
                    let up = ui.add_enabled_ui(i > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
                    if up.on_hover_text(tr!("Move left")).clicked() {
                        ops.push(Op::MoveColumn(i, -1));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let mut on = o.columns.contains(col);
                        if ui.checkbox(&mut on, col.as_str()).changed() {
                            if on {
                                o.columns.push(col.clone());
                                o.columns.sort();
                            } else {
                                o.columns.retain(|c| c != col);
                            }
                        }
                    });
                });
            });
        }
    };
    let options = |ui: &mut egui::Ui, o: &mut Overview| {
        ui.label(egui::RichText::new(tr!("Options")).strong());
        if o.options.is_empty() {
            ui.label(egui::RichText::new(tr!("The game has not stored these yet.")).weak());
        }
        for (key, on) in &mut o.options {
            let label = OPTIONS.iter().find(|(k, _)| k == key).map_or(key.as_str(), |(_, l)| spai_ui::i18n::t(l));
            ui.checkbox(on, label);
        }
    };
    scroll(ui, "ov_columns_scroll", |ui| {
        let two = ui.available_width() >= 640.0;
        ui.set_max_width(ui.available_width().min(if two { 760.0 } else { 380.0 }));
        if two {
            ui.columns(2, |cols| {
                columns(&mut cols[0], o);
                options(&mut cols[1], o);
            });
        } else {
            columns(ui, o);
            ui.add_space(10.0);
            options(ui, o);
        }
    });
}

fn tab_dialog(ctx: &egui::Context, st: &mut State, o: &mut Overview) {
    let Some(e) = &mut st.edit else {
        return;
    };
    let presets: Vec<String> = o.presets.iter().map(|p| p.name.clone()).collect();
    let offered: Vec<String> = o.column_order.iter().chain(e.columns.iter()).cloned().fold(Vec::new(), |mut v, c| {
        if !v.contains(&c) {
            v.push(c);
        }
        v
    });
    let (mut save, mut cancel) = (false, false);
    let outside = dialogs::modal(ctx, "ov_tab_edit", tr!("Overview tab"), |ui| {
        let label_w = 110.0;
        let field = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(label_w, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| ui.label(label));
                add(ui);
            });
        };
        field(ui, tr!("Name"), &mut |ui| {
            ui.add_sized([ui.available_width(), 22.0], egui::TextEdit::singleline(&mut e.name))
                .on_hover_text(tr!("The game's own markup for size and colour is kept as typed."));
        });
        field(ui, tr!("Colour"), &mut |ui| {
            if let Some(pick) = color_menu(ui, e.color, tr!("None"), true, tr!("Colour of the tab's name")) {
                e.color = pick;
            }
            let shown = plain(&e.name);
            ui.label(egui::RichText::new(shown).color(e.color.map_or(ui.visuals().text_color(), color32)));
        });
        field(ui, tr!("Lists"), &mut |ui| {
            let w = ui.available_width().min(320.0);
            preset_combo(ui, "ov_edit_preset", &mut e.preset, &presets, w);
        });
        field(ui, tr!("Brackets"), &mut |ui| {
            let w = ui.available_width().min(320.0);
            preset_combo(ui, "ov_edit_bracket", &mut e.bracket, &presets, w);
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr!("Columns on this tab")).strong());
        ui.horizontal_wrapped(|ui| {
            for col in &offered {
                let mut on = e.columns.contains(col);
                if ui.checkbox(&mut on, col.as_str()).changed() {
                    if on {
                        e.columns.push(col.clone());
                    } else {
                        e.columns.retain(|c| c != col);
                    }
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            save = ui.button(tr!("Apply")).on_hover_text(tr!("Keep these changes; they are written with Save")).clicked();
            cancel = ui.button(tr!("Cancel")).clicked();
        });
    });
    if save {
        if let Some(tab) = o.tab_mut(e.index) {
            tab.name = e.name.clone();
            tab.preset = e.preset.clone();
            tab.bracket = e.bracket.clone();
            tab.color = e.color;
            // In the order the overview lists its columns, as the game keeps them.
            tab.columns = offered.iter().filter(|c| e.columns.contains(c)).cloned().collect();
        }
    }
    if save || cancel || outside {
        st.edit = None;
    }
}

fn rename_dialog(ctx: &egui::Context, st: &mut State, o: &mut Overview) {
    let Some((from, text)) = &mut st.rename else {
        return;
    };
    let taken = text != from && o.presets.iter().any(|p| p.name == *text);
    let (mut save, mut cancel) = (false, false);
    let outside = dialogs::modal(ctx, "ov_preset_rename", tr!("Rename filter"), |ui| {
        let edit = ui.add_sized([ui.available_width(), 22.0], egui::TextEdit::singleline(text));
        save = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let note = if taken { tr!("Another filter has this name.") } else { tr!("Colour markup in the name is kept as typed. Tabs that use the filter follow the new name.") };
        ui.add(egui::Label::new(egui::RichText::new(note).weak()).wrap());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            save |= ui.add_enabled(!taken && !text.trim().is_empty(), egui::Button::new(tr!("Rename"))).clicked();
            cancel = ui.button(tr!("Cancel")).clicked();
        });
    });
    if save && !taken && !text.trim().is_empty() {
        if from != text {
            o.rename_preset(from, text);
        }
        st.rename = None;
    } else if cancel || outside {
        st.rename = None;
    }
}

/// The category a group is listed under: its own when the overview has such groups, else Other.
fn category_of(groups: &[(i64, i64, String)], id: i64) -> i64 {
    groups
        .iter()
        .find(|(g, _, _)| *g == id)
        .map(|(_, c, _)| *c)
        .filter(|c| CATEGORIES.iter().any(|(k, _)| k == c))
        .unwrap_or(OTHER)
}

fn group_name(groups: &[(i64, i64, String)], id: i64) -> String {
    groups.iter().find(|(g, _, _)| *g == id).map_or_else(|| trf!("Group {id}", id = id), |(_, _, n)| n.clone())
}

/// The groups to offer under a category: every known group of it, and for Other the ones the
/// preset already has that nothing else lists.
fn offered(groups: &[(i64, i64, String)], preset: &Preset, category: i64) -> Vec<i64> {
    let mut out: Vec<i64> = if category == OTHER {
        preset.groups.iter().copied().filter(|g| category_of(groups, *g) == OTHER).collect()
    } else {
        groups.iter().filter(|(_, c, _)| *c == category).map(|(g, _, _)| *g).collect()
    };
    out.sort_by_key(|g| group_name(groups, *g).to_lowercase());
    out
}

fn preset_dialog(ctx: &egui::Context, st: &mut State, o: &mut Overview, groups: &[(i64, i64, String)]) {
    let Some(e) = &mut st.preset else {
        return;
    };
    let (mut save, mut cancel) = (false, false);
    let title = trf!("What {name} takes in", name = plain(&e.name));
    let outside = dialogs::modal_sized(ctx, "ov_preset_edit", &title, 760.0, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (page, label) in [
                (PresetPage::Types, trf!("Item groups ({n})", n = e.work.groups.len())),
                (PresetPage::States, trf!("Pilot states ({n})", n = e.work.hidden.len() + e.work.always.len())),
            ] {
                if ui.menu_label(e.page == page, label).clicked() {
                    e.page = page;
                }
            }
            if e.page == PresetPage::Types {
                ui.add_sized([200.0, 22.0], egui::TextEdit::singleline(&mut e.filter).hint_text(tr!("Find a group")));
            }
        });
        ui.separator();
        // One height for both pages, so switching between them does not resize the dialog.
        let height = (ctx.content_rect().height() - 220.0).clamp(160.0, 380.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
        match e.page {
            PresetPage::Types => preset_types(&mut body, e, groups),
            PresetPage::States => preset_states_page(&mut body, e),
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            save = ui.button(tr!("Apply")).on_hover_text(tr!("Keep these changes; they are written with Save")).clicked();
            cancel = ui.button(tr!("Cancel")).clicked();
            if groups.is_empty() {
                let note = tr!("The group names are still being fetched; until then groups show by number.");
                clipped(ui, egui::RichText::new(note).weak(), note);
            }
        });
    });
    if save {
        if let Some(p) = o.presets.iter_mut().find(|p| p.name == e.name) {
            *p = e.work.clone();
        }
    }
    if save || cancel || outside {
        st.preset = None;
    }
}

fn preset_types(ui: &mut egui::Ui, e: &mut PresetEdit, groups: &[(i64, i64, String)]) {
    let needle = e.filter.trim().to_lowercase();
    let categories: Vec<(i64, String)> = CATEGORIES
        .iter()
        .map(|(id, name)| (*id, spai_ui::i18n::t(name).to_owned()))
        .chain([(OTHER, tr!("Other").to_owned())])
        .filter(|(id, _)| !offered(groups, &e.work, *id).is_empty())
        .collect();
    if !categories.iter().any(|(id, _)| *id == e.category) {
        e.category = categories.first().map_or(OTHER, |(id, _)| *id);
    }
    egui::Panel::left("ov_preset_categories").resizable(false).exact_size(210.0).frame(egui::Frame::NONE.inner_margin(egui::Margin { right: 8, ..Default::default() })).show_inside(ui, |ui| {
        scroll(ui, "ov_preset_categories_scroll", |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for (id, name) in &categories {
                let all = offered(groups, &e.work, *id);
                let on = all.iter().filter(|g| e.work.shows(**g)).count();
                let label = format!("{name}   ·   {on} / {}", all.len());
                // While searching, the list on the right runs across every category.
                let selected = needle.is_empty() && e.category == *id;
                if person_row(ui, selected, None, None, &label, None).clicked() {
                    e.category = *id;
                    e.filter.clear();
                }
            }
        });
    });
    let shown: Vec<i64> = if needle.is_empty() {
        offered(groups, &e.work, e.category)
    } else {
        categories
            .iter()
            .flat_map(|(id, _)| offered(groups, &e.work, *id))
            .filter(|g| group_name(groups, *g).to_lowercase().contains(&needle))
            .collect()
    };
    ui.horizontal(|ui| {
        if ui.button(tr!("All")).on_hover_text(tr!("Tick every group listed here")).clicked() {
            for g in &shown {
                e.work.set_group(*g, true);
            }
        }
        if ui.button(tr!("None")).on_hover_text(tr!("Untick every group listed here")).clicked() {
            for g in &shown {
                e.work.set_group(*g, false);
            }
        }
        let on = shown.iter().filter(|g| e.work.shows(**g)).count();
        let text = format!("{on} / {}", shown.len());
        ui.label(egui::RichText::new(text).weak());
    });
    scroll(ui, "ov_preset_groups", |ui| {
        if shown.is_empty() {
            ui.label(egui::RichText::new(tr!("No group by that name.")).weak());
        }
        // Down the first column, then the second: a list read top to bottom, not across.
        let columns = if ui.available_width() >= 420.0 { 2 } else { 1 };
        let per = shown.len().div_ceil(columns);
        ui.columns(columns, |cols| {
            for (ci, ui) in cols.iter_mut().enumerate() {
                for g in shown.iter().skip(ci * per).take(per) {
                    let mut on = e.work.shows(*g);
                    let name = group_name(groups, *g);
                    if ui.add(egui::Checkbox::new(&mut on, egui::RichText::new(&name))).changed() {
                        e.work.set_group(*g, on);
                    }
                }
            }
        });
    });
}

fn preset_states_page(ui: &mut egui::Ui, e: &mut PresetEdit) {
    let hint = tr!("Pilots in a state can be left out of the list, or listed whatever else applies to them.");
    ui.add(egui::Label::new(egui::RichText::new(hint).weak()).wrap());
    let how = |h: Option<bool>| match h {
        None => tr!("Listed as usual"),
        Some(false) => tr!("Left out"),
        Some(true) => tr!("Always listed"),
    };
    scroll(ui, "ov_preset_states", |ui| {
        ui.set_max_width(ui.available_width().min(ROW_MAX + 60.0));
        // A state the preset names that this build has no text for still gets its row.
        let mut ids = preset_states();
        for extra in e.work.hidden.iter().chain(&e.work.always) {
            if !ids.contains(extra) {
                ids.push(*extra);
            }
        }
        for id in ids {
            let current = e.work.state(id);
            spai_fit_ui::rows::row(ui, "ov_preset_states", id, ROW_H - 2.0, false, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut pick = current;
                    combo(ui, ("ov_state", id), 170.0, how(current), |ui| {
                        for option in [None, Some(false), Some(true)] {
                            ui.menu_value(&mut pick, option, how(option));
                        }
                    });
                    if pick != current {
                        e.work.set_state(id, pick);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let name = state_name(id);
                        let text = egui::RichText::new(&name);
                        clipped(ui, if current.is_some() { text } else { text.weak() }, &name);
                    });
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evesettings::overview;

    fn catalog() -> Vec<(i64, i64, String)> {
        vec![
            (25, 6, "Frigate".into()),
            (26, 6, "Cruiser".into()),
            (100, 18, "Combat Drone".into()),
            (10, 2, "Stargate".into()),
            (900, 7, "Not an overview category".into()),
        ]
    }

    fn preset(groups: &[i64]) -> Preset {
        let mut p = overview::read(&overview::sample_file()).presets.remove(0);
        p.groups = groups.to_vec();
        p
    }

    #[test]
    fn groups_are_offered_under_their_category_by_name() {
        let groups = catalog();
        let p = preset(&[25, 900, 4242]);
        assert_eq!(offered(&groups, &p, 6), vec![26, 25], "Cruiser before Frigate");
        assert_eq!(offered(&groups, &p, 18), vec![100]);
        // What the preset has that no listed category claims can still be found and unticked.
        assert_eq!(offered(&groups, &p, OTHER), vec![4242, 900]);
        assert_eq!(group_name(&groups, 4242), "Group 4242");
        assert_eq!(category_of(&groups, 900), OTHER);
    }

    #[test]
    fn without_names_a_preset_still_lists_its_own_groups() {
        let p = preset(&[25, 26]);
        assert_eq!(offered(&[], &p, OTHER), vec![25, 26]);
        assert!(offered(&[], &p, 6).is_empty());
    }
}
