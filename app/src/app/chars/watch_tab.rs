//! The Watch lists tab: the fleet watch lists the client has saved for a character.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::sidebar_tab::{row, scroll};
use super::*;
use crate::evesettings::watchlist::{Watchlist, MAX_MEMBERS};

type Lookup = Arc<Mutex<Option<Result<(i64, String), String>>>>;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) selected: Option<i64>,
    add_name: String,
    fleet_id: String,
    lookup: Lookup,
    busy: bool,
}

enum Op {
    Remove(i64),
    Swap(bool, usize, usize),
    Across(i64),
    Color(i64, Option<[f32; 3]>),
}

fn find_pilot(st: &mut State, ctx: &egui::Context) {
    let name = st.add_name.trim().to_owned();
    if name.is_empty() || st.busy {
        return;
    }
    st.busy = true;
    let (slot, ctx) = (st.lookup.clone(), ctx.clone());
    std::thread::spawn(move || {
        let found = crate::http::client(15)
            .map_err(|e| e.to_string())
            .and_then(|c| crate::universe::character(&c, &name).map_err(|e| e.to_string()))
            .and_then(|hit| hit.ok_or_else(|| trf!("No pilot named {name}.", name = name)));
        if let (Ok((id, name)), Ok(store)) = (&found, Store::open()) {
            store.set_char_name(*id, name);
        }
        *slot.lock().unwrap() = Some(found);
        ctx.request_repaint();
    });
}

/// Returns a note for the status line when adding a pilot did not work.
pub(crate) fn ui(ui: &mut egui::Ui, st: &mut State, s: &mut Session, names: &HashMap<i64, String>, mine: &[Row]) -> Option<String> {
    let mut note = None;
    let own: HashMap<i64, String> = mine.iter().filter_map(|r| Some((r.id, r.name.clone()?))).collect();
    let name_of = |id: i64| names.get(&id).or_else(|| own.get(&id)).cloned().unwrap_or_else(|| trf!("Character {id}", id = id));
    let now = crate::clock::utc().timestamp();
    let w = &mut s.watch.now;
    if !st.selected.is_some_and(|id| w.lists.iter().any(|l| l.fleet_id == id)) {
        st.selected = w.lists.first().map(|l| l.fleet_id);
    }
    let done = st.lookup.lock().unwrap().take();
    if let Some(done) = done {
        st.busy = false;
        match (done, st.selected.and_then(|id| w.lists.iter_mut().find(|l| l.fleet_id == id))) {
            (Ok((id, _)), Some(list)) if list.contains(id) => note = Some(tr!("Already on the list.").to_owned()),
            (Ok(_), Some(list)) if list.len() >= MAX_MEMBERS => note = Some(tr!("The list is full.").to_owned()),
            (Ok((id, _)), Some(list)) => {
                list.left.push(id);
                st.add_name.clear();
            }
            (Err(e), _) => note = Some(e),
            (Ok(_), None) => {}
        }
    }

    ui.add_space(4.0);
    let hint = tr!("The game keeps one watch list per fleet and shows it again while the character is in that fleet.");
    clipped(ui, egui::RichText::new(hint).weak(), hint);
    ui.add_space(4.0);

    let wide = ui.available_width() >= 660.0;
    let label_of = |l: &Watchlist| trf!("Pilots: {n}, {when}", n = l.len(), when = ago(now, l.saved));
    // A new list needs the id of the fleet it is for; the field and its two buttons.
    let new_list = |ui: &mut egui::Ui, st: &mut State, w: &mut crate::evesettings::watchlist::Watchlists| {
        ui.add_sized([110.0, 22.0], egui::TextEdit::singleline(&mut st.fleet_id).hint_text(tr!("fleet id")))
            .on_hover_text(tr!("The id of the fleet a new list is for. It is the number in a fleet's ESI address."));
        let id = st.fleet_id.trim().parse::<i64>().ok().filter(|id| *id > 0 && !w.lists.iter().any(|l| l.fleet_id == *id));
        let copy_of = st.selected.and_then(|sel| w.lists.iter().find(|l| l.fleet_id == sel).cloned());
        // Plain widgets, not nested uis: on a narrow window this row wraps.
        let add = ui.add_enabled(id.is_some(), egui::Button::new(icon::PLUS));
        if add.on_hover_text(tr!("New empty list for this fleet")).on_disabled_hover_text(tr!("Enter a fleet id that has no list yet.")).clicked() {
            if let Some(id) = id {
                w.lists.insert(0, Watchlist { fleet_id: id, saved: now, left: Vec::new(), right: Vec::new() });
                st.selected = Some(id);
            }
        }
        let dup = ui.add_enabled(id.is_some() && copy_of.is_some(), egui::Button::new(icon::COPY));
        if dup.on_hover_text(tr!("Copy the selected list to this fleet")).on_disabled_hover_text(tr!("Enter a fleet id that has no list yet.")).clicked() {
            if let (Some(id), Some(mut list)) = (id, copy_of) {
                list.fleet_id = id;
                list.saved = now;
                w.lists.insert(0, list);
                st.selected = Some(id);
            }
        }
    };
    if wide {
        egui::Panel::left("watch_lists").resizable(false).exact_size(220.0).frame(egui::Frame::NONE.inner_margin(egui::Margin { right: 8, ..Default::default() })).show_inside(ui, |ui| {
            ui.horizontal(|ui| new_list(ui, st, w));
            ui.add_space(4.0);
            scroll(ui, "watch_list_rows", |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                if w.lists.is_empty() {
                    ui.label(egui::RichText::new(tr!("No watch lists saved.")).weak());
                }
                for l in &w.lists {
                    let r = person_row(ui, st.selected == Some(l.fleet_id), None, Some(icon::EYE), &label_of(l), None);
                    if r.on_hover_text(trf!("Fleet {id}, saved {day}", id = l.fleet_id, day = day_text(l.saved))).clicked() {
                        st.selected = Some(l.fleet_id);
                    }
                }
            });
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            let current = st.selected.and_then(|id| w.lists.iter().find(|l| l.fleet_id == id)).map_or_else(|| tr!("No watch lists saved.").to_owned(), label_of);
            combo(ui, "watch_pick", 190.0, current, |ui| {
                for l in &w.lists {
                    if ui.menu_label(st.selected == Some(l.fleet_id), label_of(l)).clicked() {
                        st.selected = Some(l.fleet_id);
                    }
                }
            });
            new_list(ui, st, w);
        });
        ui.add_space(4.0);
    }

    let Some(at) = st.selected.and_then(|id| w.lists.iter().position(|l| l.fleet_id == id)) else {
        return note;
    };
    let mut ops = Vec::new();
    let mut delete = false;
    ui.horizontal_wrapped(|ui| {
        let full = w.lists[at].len() >= MAX_MEMBERS;
        ui.add_enabled_ui(!full && !st.busy, |ui| {
            let edit = ui.add_sized([170.0, 22.0], egui::TextEdit::singleline(&mut st.add_name).hint_text(tr!("Pilot name")));
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button(trf!("{icon}  Add", icon = icon::PLUS)).on_disabled_hover_text(trf!("A watch list holds {n} pilots.", n = MAX_MEMBERS)).clicked() || enter {
                find_pilot(st, ui.ctx());
            }
        });
        let mut two = w.two_columns.unwrap_or(true);
        if ui.checkbox(&mut two, tr!("Two columns")).changed() {
            w.two_columns = Some(two);
        }
        if ui.button(trf!("{icon}  Delete list", icon = icon::TRASH)).clicked() {
            delete = true;
        }
        if st.busy {
            ui.spinner();
        }
    });
    ui.add_space(4.0);
    let list = &w.lists[at];
    let colors = &w.colors;
    let mut column = |ui: &mut egui::Ui, left: bool| {
        let ids = if left { &list.left } else { &list.right };
        ui.label(egui::RichText::new(if left { tr!("Left column") } else { tr!("Right column") }).strong());
        if ids.is_empty() {
            ui.label(egui::RichText::new(tr!("Empty.")).weak());
        }
        for (i, id) in ids.iter().copied().enumerate() {
            row(ui, if left { "watch_left" } else { "watch_right" }, id, 0.0, &name_of(id), false, |ui| {
                if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Take off the list")).clicked() {
                    ops.push(Op::Remove(id));
                }
                let across = if left { icon::ARROW_RIGHT } else { icon::ARROW_LEFT };
                if spai_ui::widgets::icon_button(ui, across).on_hover_text(tr!("Move to the other column")).clicked() {
                    ops.push(Op::Across(id));
                }
                let down = ui.add_enabled_ui(i + 1 < ids.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
                if down.on_hover_text(tr!("Move down")).clicked() {
                    ops.push(Op::Swap(left, i, i + 1));
                }
                let up = ui.add_enabled_ui(i > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
                if up.on_hover_text(tr!("Move up")).clicked() {
                    ops.push(Op::Swap(left, i, i - 1));
                }
                if let Some(pick) = color_menu(ui, colors.get(&id).copied(), tr!("None"), true, tr!("Colour on the watch list")) {
                    ops.push(Op::Color(id, pick));
                }
            });
        }
    };
    if wide {
        ui.columns(2, |cols| {
            scroll(&mut cols[0], "watch_col_left", |ui| column(ui, true));
            scroll(&mut cols[1], "watch_col_right", |ui| column(ui, false));
        });
    } else {
        scroll(ui, "watch_cols", |ui| {
            column(ui, true);
            ui.add_space(8.0);
            column(ui, false);
        });
    }
    for op in ops {
        let list = &mut w.lists[at];
        match op {
            Op::Remove(id) => {
                list.left.retain(|m| *m != id);
                list.right.retain(|m| *m != id);
            }
            Op::Swap(left, a, b) => (if left { &mut list.left } else { &mut list.right }).swap(a, b),
            Op::Across(id) => {
                if let Some(i) = list.left.iter().position(|m| *m == id) {
                    list.left.remove(i);
                    list.right.push(id);
                } else if let Some(i) = list.right.iter().position(|m| *m == id) {
                    list.right.remove(i);
                    list.left.push(id);
                }
            }
            Op::Color(id, Some(c)) => {
                w.colors.insert(id, c);
            }
            Op::Color(id, None) => {
                w.colors.remove(&id);
            }
        }
    }
    if delete {
        w.lists.remove(at);
        st.selected = None;
    }
    note
}
