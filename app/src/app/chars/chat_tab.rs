//! The Channels tab: the chat channels a character rejoins at login.

use super::sidebar_tab::{row, scroll};
use super::*;
use crate::evesettings::chat::Channel;

fn kind(c: &Channel) -> &'static str {
    match c.key.split('_').next().unwrap_or_default() {
        "local" => tr!("Local"),
        "corp" => tr!("Corporation"),
        "alliance" => tr!("Alliance"),
        "fleet" => tr!("Fleet"),
        "faction" => tr!("Militia"),
        "private" => tr!("Private conversation"),
        "system" => tr!("Game channel"),
        _ => tr!("Player channel"),
    }
}

fn place(s: &Session, c: &Channel) -> String {
    let id = c.window_id();
    let l = &s.layout.now;
    let Some(w) = l.wins.get(&id) else {
        return tr!("no window stored").to_owned();
    };
    match &w.stack {
        Some(stack) => {
            let mates: Vec<String> = l
                .members(stack)
                .into_iter()
                .filter(|m| *m != id && l.chats.contains_key(m))
                .map(|m| l.label(&m))
                .collect();
            if mates.is_empty() {
                tr!("own window").to_owned()
            } else {
                trf!("tabbed with {names}", names = mates.join(", "))
            }
        }
        None => tr!("own window").to_owned(),
    }
}

pub(crate) fn ui(ui: &mut egui::Ui, s: &mut Session, known: &[Channel]) {
    ui.add_space(4.0);
    let Some(list) = s.chat.now.clone() else {
        ui.label(egui::RichText::new(tr!("The game has not stored a channel list for this character yet.")).weak());
        return;
    };
    let spare: Vec<&Channel> = known.iter().filter(|k| !list.iter().any(|c| c.key == k.key)).collect();
    let mut next = list.clone();
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!spare.is_empty(), |ui| {
            ui.menu_button(trf!("{icon}  Add channel", icon = icon::PLUS), |ui| {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for c in &spare {
                        if ui.button(&c.name).clicked() {
                            next.push((*c).clone());
                            ui.close();
                        }
                    }
                });
            })
            .response
            .on_hover_text(tr!("Channels another character in this profile has joined"))
            .on_disabled_hover_text(tr!("No other character in this profile is in a channel this one is not."));
        });
        let hint = tr!("Channels the character rejoins at login. Where each one opens is set on the Windows tab.");
        clipped(ui, egui::RichText::new(hint).weak(), hint);
    });
    ui.add_space(4.0);
    scroll(ui, "chat_rows", |ui| {
        if list.is_empty() {
            ui.label(egui::RichText::new(tr!("No channels.")).weak());
        }
        for (i, c) in list.iter().enumerate() {
            let label = format!("{}   ·   {}   ·   {}", c.name, kind(c), place(s, c));
            row(ui, "chat_rows", &c.key, 0.0, &label, false, |ui| {
                let x = ui.add_enabled_ui(!c.built_in(), |ui| spai_ui::widgets::icon_button(ui, icon::X)).inner;
                if x.on_hover_text(tr!("Leave at next login")).on_disabled_hover_text(tr!("The game joins this channel by itself.")).clicked() {
                    next.retain(|n| n.key != c.key);
                }
                let down = ui.add_enabled_ui(i + 1 < list.len(), |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN)).inner;
                if down.on_hover_text(tr!("Move down")).clicked() {
                    next.swap(i, i + 1);
                }
                let up = ui.add_enabled_ui(i > 0, |ui| spai_ui::widgets::icon_button(ui, icon::ARROW_UP)).inner;
                if up.on_hover_text(tr!("Move up")).clicked() {
                    next.swap(i, i - 1);
                }
            });
        }
    });
    if next != list {
        s.chat.now = Some(next);
    }
}
