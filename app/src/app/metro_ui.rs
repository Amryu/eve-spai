//! EVE Metro: its subscribers' Pochven holes, polled with a key its team hands out.

use crate::app::SteadySelect as _;
use crate::wormholes::{MetroConfig, MetroStatus};
use spai_ui::{tr, trf};

impl crate::app::SpaiApp {
    /// The character EVE Metro is asked about: the chosen one while it is signed in, else the first.
    fn metro_character(&self) -> Option<i64> {
        let chosen = self.settings.evemetro_character;
        self.characters.iter().find(|c| c.id == chosen).or(self.characters.first()).map(|c| c.id)
    }

    pub(crate) fn metro_sync(&mut self) {
        let s = &self.settings;
        let config = (s.evemetro_enabled && !s.evemetro_key.trim().is_empty())
            .then(|| self.metro_character())
            .flatten()
            .map(|character| MetroConfig { key: s.evemetro_key.trim().to_owned(), character });
        crate::wormholes::metro_sync(&self.metro, config, self.systems.as_ref());
    }

    pub(crate) fn metro_window(&mut self, ctx: &egui::Context) {
        if !self.metro_open {
            return;
        }
        use egui_phosphor::regular as icon;
        let mut open = true;
        let status = self.metro.0.lock().map(|s| s.status.clone()).unwrap_or_default();
        let chars: Vec<(i64, String)> = self.characters.iter().map(|c| (c.id, c.name.clone())).collect();
        let current = self.metro_character();
        let mut changed = false;
        egui::Window::new(trf!("{icon}  EVE Metro", icon = icon::TRAIN))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            // Below the toolbars, clear of the tab's own buttons.
            .default_pos(egui::pos2(ctx.content_rect().center().x - 210.0, ctx.content_rect().top() + 150.0))
            .constrain_to(ctx.content_rect().with_min_x(ctx.content_rect().left() + self.nav_width() + 8.0))
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(egui::RichText::new(tr!("EVE Metro maps the holes between Pochven and the rest of New Eden for its subscribers. With a key from the EVE Metro team they show here like EVE-Scout's.")).weak());
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to(trf!("{icon}  evemetro.com", icon = icon::LINK), "https://evemetro.com");
                    ui.label(egui::RichText::new(tr!("API keys: ask the EVE Metro team on their Discord")).weak());
                });
                ui.add_space(6.0);
                changed |= ui.checkbox(&mut self.settings.evemetro_enabled, tr!("Fetch EVE Metro's holes")).changed();
                ui.add_enabled_ui(self.settings.evemetro_enabled, |ui| {
                    egui::Grid::new("metro_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        ui.label(tr!("API key"));
                        changed |= ui
                            .add(egui::TextEdit::singleline(&mut self.settings.evemetro_key).password(true).hint_text(tr!("from the EVE Metro team")).desired_width(260.0))
                            .changed();
                        ui.end_row();
                        ui.label(tr!("Subscriber"));
                        if chars.is_empty() {
                            ui.label(egui::RichText::new(tr!("Sign in a character first")).color(crate::theme::standing::WARNING));
                        } else {
                            let name = chars.iter().find(|c| Some(c.0) == current).map_or("", |c| c.1.as_str());
                            egui::ComboBox::from_id_salt("metro_char")
                                .width(260.0)
                                .truncate()
                                .selected_text(name)
                                .show_ui(ui, |ui| {
                                    for (id, name) in &chars {
                                        if ui.menu_label(Some(*id) == current, name).clicked() {
                                            self.settings.evemetro_character = *id;
                                            changed = true;
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(tr!("EVE Metro checks this character, its corporation and its alliance for a subscription"));
                        }
                        ui.end_row();
                    });
                });
                ui.add_space(6.0);
                // One line at one height, so the window keeps its size as the state changes.
                let row = ui.spacing().interact_size.y;
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_height(row);
                    let warn = crate::theme::standing::WARNING;
                    match &status {
                        MetroStatus::Off => {
                            ui.label(egui::RichText::new(if self.settings.evemetro_enabled { tr!("Needs a key and a character") } else { tr!("Off") }).weak());
                        }
                        MetroStatus::Asking => {
                            ui.add(egui::Spinner::new().size(row * 0.7));
                            ui.label(tr!("Asking EVE Metro"));
                        }
                        MetroStatus::Holes { count, at } => {
                            let ago = crate::clock::utc().timestamp() - at;
                            ui.label(trf!("{n} holes, {ago} ago", n = count, ago = spai_ui::widgets::human_ago(ago)));
                            if ui.button(icon::ARROWS_CLOCKWISE).on_hover_text(tr!("Ask again now")).clicked() {
                                crate::wormholes::metro_refresh(&self.metro);
                            }
                        }
                        MetroStatus::NoAccess => {
                            let t = tr!("No subscription for this character, its corporation or alliance");
                            ui.add(egui::Label::new(egui::RichText::new(t.to_string()).color(warn)).truncate()).on_hover_text(t);
                        }
                        MetroStatus::BadKey => {
                            let t = tr!("EVE Metro refused the API key");
                            ui.add(egui::Label::new(egui::RichText::new(t.to_string()).color(warn)).truncate()).on_hover_text(t);
                        }
                        MetroStatus::Failed(why) => {
                            let t = trf!("Could not ask EVE Metro: {why}", why = why);
                            ui.add(egui::Label::new(egui::RichText::new(t.to_string()).color(warn)).truncate()).on_hover_text(t);
                            if ui.button(icon::ARROWS_CLOCKWISE).on_hover_text(tr!("Ask again now")).clicked() {
                                crate::wormholes::metro_refresh(&self.metro);
                            }
                        }
                    }
                });
                ui.label(egui::RichText::new(tr!("Its holes stay on this machine: they are never shared to wormhole folders.")).weak());
            });
        self.metro_open = open;
        if changed {
            self.needs_save = true;
        }
    }
}
