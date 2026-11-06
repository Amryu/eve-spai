//! The Neocom tab: the buttons on the client's left-hand bar, in order.

use super::*;
use crate::evesettings::neocom::{self, Button};

/// An icon for a Neocom button, close to what the game shows for it.
pub(crate) fn glyph(id: &str) -> &'static str {
    match id {
        "agency" => icon::COMPASS,
        "job_board" => icon::CLIPBOARD_TEXT,
        "mail" => icon::ENVELOPE_SIMPLE,
        "inventory" => icon::PACKAGE,
        "assets" => icon::CUBE,
        "wallet" => icon::WALLET,
        "market" => icon::CHART_LINE_UP,
        "marketOrders" => icon::RECEIPT,
        "fitting" => icon::WRENCH,
        "shipTree" => icon::TREE_STRUCTURE,
        "map_beta" => icon::MAP_TRIFOLD,
        "locations" => icon::MAP_PIN,
        "chat" => icon::CHATS_CIRCLE,
        "contracts" => icon::HANDSHAKE,
        "industry" => icon::FACTORY,
        "planets" => icon::PLANET,
        "fleet" => icon::USERS_THREE,
        "ProjectDiscovery" => icon::DNA,
        "help" => icon::QUESTION,
        "airCareerProgram" => icon::GRADUATION_CAP,
        _ => icon::PUZZLE_PIECE,
    }
}

enum Op {
    Move(Vec<usize>, i32),
    Remove(Vec<usize>),
    /// Out of its folder, to the end of the bar.
    Lift(Vec<usize>),
    Add(Button),
}

fn list_mut<'a>(bar: &'a mut Vec<Button>, path: &[usize]) -> Option<&'a mut Vec<Button>> {
    match path {
        [] => Some(bar),
        [i, rest @ ..] => list_mut(bar.get_mut(*i)?.children.as_mut()?, rest),
    }
}

fn apply(bar: &mut Vec<Button>, op: Op) {
    match op {
        Op::Add(b) => bar.push(b),
        Op::Move(path, by) => {
            let Some((at, parent)) = path.split_last() else { return };
            if let Some(list) = list_mut(bar, parent) {
                let to = (*at as i32 + by).clamp(0, list.len() as i32 - 1) as usize;
                if *at < list.len() {
                    let b = list.remove(*at);
                    list.insert(to, b);
                }
            }
        }
        Op::Remove(path) | Op::Lift(path) if path.is_empty() => {}
        Op::Remove(path) => {
            let (at, parent) = path.split_last().unwrap();
            if let Some(list) = list_mut(bar, parent).filter(|l| *at < l.len()) {
                list.remove(*at);
            }
        }
        Op::Lift(path) => {
            let (at, parent) = path.split_last().unwrap();
            let taken = list_mut(bar, parent).filter(|l| *at < l.len()).map(|l| l.remove(*at));
            bar.extend(taken);
        }
    }
}

/// A fixed-height list row: the label takes what the buttons on the right leave. `list` names
/// the list for the layout checks, which hold every row of one list to one height.
pub(crate) fn row(ui: &mut egui::Ui, list: &str, id: impl std::hash::Hash, indent: f32, label: &str, weak: bool, buttons: impl FnOnce(&mut egui::Ui)) {
    spai_fit_ui::rows::row(ui, list, id, ROW_H - 4.0, false, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            buttons(ui);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(indent);
                let text = egui::RichText::new(label);
                clipped(ui, if weak { text.weak() } else { text }, label);
            });
        });
    });
}

/// A vertical scroll area that tells the layout checks what it clips.
pub(crate) fn scroll(ui: &mut egui::Ui, id: impl std::hash::Hash, body: impl FnOnce(&mut egui::Ui)) {
    let out = egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, body);
    spai_fit_ui::rows::note_scrolled(ui.ctx(), &out);
}

fn rows(ui: &mut egui::Ui, list: &[Button], path: &mut Vec<usize>, ops: &mut Vec<Op>) {
    for (i, b) in list.iter().enumerate() {
        path.push(i);
        let depth = path.len() - 1;
        let label = format!("{}  {}", if b.is_folder() { icon::FOLDER } else { glyph(&b.id) }, b.label());
        row(ui, "neocom_on", &path, depth as f32 * 18.0, &label, false, |ui| {
            if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Take off the Neocom")).clicked() {
                ops.push(Op::Remove(path.clone()));
            }
            if depth > 0 && spai_ui::widgets::icon_button(ui, icon::ARROW_SQUARE_OUT).on_hover_text(tr!("Move out of the folder")).clicked() {
                ops.push(Op::Lift(path.clone()));
            }
            let down = ui.add_enabled_ui(i + 1 < list.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
            if down.on_hover_text(tr!("Move down")).clicked() {
                ops.push(Op::Move(path.clone(), 1));
            }
            let up = ui.add_enabled_ui(i > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
            if up.on_hover_text(tr!("Move up")).clicked() {
                ops.push(Op::Move(path.clone(), -1));
            }
        });
        if let Some(children) = b.children.as_ref().filter(|c| !c.is_empty()) {
            rows(ui, children, path, ops);
        }
        path.pop();
    }
}

pub(crate) fn ui(ui: &mut egui::Ui, s: &mut Session) {
    ui.add_space(4.0);
    let defaults = s.neocom_defaults.clone();
    let Some(bar) = &mut s.neocom.now else {
        ui.add(egui::Label::new(egui::RichText::new(tr!("This character still has the Neocom the game started it with. The game stores its own copy once a button is moved in game.")).weak()).wrap());
        ui.add_space(6.0);
        if ui.button(tr!("Start from the default Neocom")).clicked() {
            let start = if defaults.is_empty() {
                neocom::known_ids().iter().filter_map(|id| Button::known(id)).collect()
            } else {
                defaults
            };
            s.neocom.now = Some(start);
        }
        return;
    };
    let on_bar = neocom::ids(bar);
    let mut spare: Vec<Button> = neocom::known_ids().iter().filter_map(|id| Button::known(id)).collect();
    for d in defaults {
        if !spare.iter().any(|b| b.id == d.id) {
            spare.push(d);
        }
    }
    spare.retain(|b| !on_bar.contains(&b.id));
    spare.sort_by_key(|b| b.label().to_lowercase());

    let mut ops = Vec::new();
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.label(egui::RichText::new(tr!("On the Neocom, top to bottom")).strong());
        scroll(ui, "neocom_on", |ui| {
            if bar.is_empty() {
                ui.label(egui::RichText::new(tr!("Nothing on the Neocom.")).weak());
            }
            rows(ui, bar, &mut Vec::new(), &mut ops);
        });
        let ui = &mut cols[1];
        ui.label(egui::RichText::new(tr!("Not on the Neocom")).strong());
        scroll(ui, "neocom_off", |ui| {
            if spare.is_empty() {
                ui.label(egui::RichText::new(tr!("Every known button is on the Neocom.")).weak());
            }
            for b in &spare {
                row(ui, "neocom_off", &b.id, 0.0, &format!("{}  {}", glyph(&b.id), b.label()), false, |ui| {
                    if spai_ui::widgets::icon_button(ui, icon::PLUS).on_hover_text(tr!("Add to the Neocom")).clicked() {
                        ops.push(Op::Add(b.clone()));
                    }
                });
            }
        });
    });
    for op in ops {
        apply(bar, op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(bar: &[Button]) -> Vec<String> {
        neocom::ids(bar)
    }

    #[test]
    fn buttons_move_leave_and_join() {
        let mut bar = neocom::sample(&["mail", "wallet", "market", "fleet"]);
        apply(&mut bar, Op::Move(vec![0], 2));
        assert_eq!(ids(&bar), ["wallet", "market", "mail", "fleet"]);
        apply(&mut bar, Op::Move(vec![3], 5));
        apply(&mut bar, Op::Move(vec![0], -3));
        assert_eq!(ids(&bar), ["wallet", "market", "mail", "fleet"]);
        apply(&mut bar, Op::Remove(vec![1]));
        apply(&mut bar, Op::Remove(vec![9]));
        apply(&mut bar, Op::Add(Button::known("assets").unwrap()));
        assert_eq!(ids(&bar), ["wallet", "mail", "fleet", "assets"]);
    }

    #[test]
    fn folder_buttons_reorder_and_lift_out() {
        let mut bar = neocom::sample(&["mail", "chat"]);
        bar[1].children = Some(neocom::sample(&["wallet", "market", "assets"]));
        apply(&mut bar, Op::Move(vec![1, 2], -2));
        assert_eq!(ids(&bar), ["mail", "chat", "assets", "wallet", "market"]);
        apply(&mut bar, Op::Lift(vec![1, 1]));
        assert_eq!(ids(&bar), ["mail", "chat", "assets", "market", "wallet"]);
        assert_eq!(bar.len(), 3);
        apply(&mut bar, Op::Remove(vec![1, 0]));
        assert_eq!(ids(&bar), ["mail", "chat", "market", "wallet"]);
    }
}
