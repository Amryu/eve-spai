//! The Scopes tab: which ESI permissions a character granted, which are missing, and what each
//! one is for.

use super::*;

/// What the app does with each scope it asks for.
const USES: &[(&str, &str)] = &[
    ("esi-location.read_location.v1", tr_noop!("Where the character is, for distances to intel and the map")),
    ("esi-location.read_online.v1", tr_noop!("Whether the character is logged in to the game")),
    ("esi-location.read_ship_type.v1", tr_noop!("The ship the character is flying")),
    ("esi-characters.read_contacts.v1", tr_noop!("Personal contacts, to tell friends from hostiles")),
    ("esi-corporations.read_contacts.v1", tr_noop!("Corporation contacts, to tell friends from hostiles")),
    ("esi-alliances.read_contacts.v1", tr_noop!("Alliance contacts, to tell friends from hostiles")),
    ("esi-ui.write_waypoint.v1", tr_noop!("Setting destinations and waypoints in the game")),
    ("esi-ui.open_window.v1", tr_noop!("Opening information windows in the game")),
    ("esi-fittings.write_fittings.v1", tr_noop!("Saving fits to the character")),
    ("esi-skills.read_skills.v1", tr_noop!("Skill checks for fits and doctrines")),
    ("esi-fleets.read_fleet.v1", tr_noop!("Reading the fleet the character is in")),
    ("esi-fleets.write_fleet.v1", tr_noop!("Fleet command: invites, moves and the message of the day")),
    ("esi-clones.read_clones.v1", tr_noop!("Jump clones and where they are")),
    ("esi-characters.read_fatigue.v1", tr_noop!("Jump fatigue timers")),
    ("esi-clones.read_implants.v1", tr_noop!("Implants, for the fitting simulator")),
    ("esi-universe.read_structures.v1", tr_noop!("Names of player structures")),
];

fn use_of(scope: &str) -> &'static str {
    USES.iter().find(|(s, _)| *s == scope).map_or("", |(_, u)| spai_ui::i18n::t(u))
}

fn mark(ui: &mut egui::Ui, granted: bool) {
    let (glyph, color) = if granted { (icon::CHECK_CIRCLE, standing::FRIENDLY) } else { (icon::X_CIRCLE, standing::HOSTILE) };
    ui.label(egui::RichText::new(glyph).color(color).size(16.0))
        .on_hover_text(if granted { tr!("Granted") } else { tr!("Missing") });
}

impl SpaiApp {
    pub(super) fn chars_scopes_tab(&mut self, ui: &mut egui::Ui, st: &mut CharsUi) {
        let Pick::Char(id) = st.selected else {
            return;
        };
        let me = self.characters.iter().find(|c| c.id == id).cloned();
        let granted = |c: &CharacterRow| -> Vec<String> { c.scopes.split(' ').filter(|s| !s.is_empty()).map(str::to_owned).collect() };
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.menu_label(!st.scopes_all, tr!("This character")).clicked() {
                st.scopes_all = false;
            }
            if ui.menu_label(st.scopes_all, tr!("All characters")).clicked() {
                st.scopes_all = true;
            }
            ui.separator();
            let label = if me.is_some() { trf!("{icon}  Sign in again", icon = icon::SIGN_IN) } else { trf!("{icon}  Sign in", icon = icon::SIGN_IN) };
            if ui.button(label).on_hover_text(tr!("Log in through EVE SSO to grant every scope the app uses")).clicked() {
                self.start_login(&ui.ctx().clone());
            }
            if let Some(c) = &me {
                let have = granted(c);
                let n = auth::DEFAULT_SCOPES.iter().filter(|s| have.iter().any(|h| h == *s)).count();
                let text = trf!("{n} of {all} granted", n = n, all = auth::DEFAULT_SCOPES.len());
                let color = if n == auth::DEFAULT_SCOPES.len() { standing::FRIENDLY } else { standing::WARNING };
                ui.label(egui::RichText::new(text).color(color));
            }
        });
        ui.add_space(6.0);

        if st.scopes_all {
            let chars = self.characters.clone();
            if chars.is_empty() {
                ui.label(egui::RichText::new(tr!("No character is signed in.")).weak());
                return;
            }
            // A column per character while they fit beside the descriptions; a line per
            // character when they do not, so nothing scrolls sideways.
            const MARK_W: f32 = 30.0;
            let text_w = ui.available_width() - chars.len() as f32 * (MARK_W + spai_fit_ui::rows::GAP) - 2.0 * spai_fit_ui::rows::GAP;
            if text_w >= 220.0 {
                spai_fit_ui::rows::row(ui, "scopes_head", "head", ROW_H, false, |ui| {
                    spai_fit_ui::rows::cell(ui, text_w, egui::RichText::new(""), "");
                    for c in &chars {
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(MARK_W, ROW_H), egui::Sense::hover());
                        let face = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(24.0));
                        egui::Image::new(eve_portrait_url(c.id, 32.0)).corner_radius(2.0).paint_at(ui, face);
                        resp.on_hover_text(&c.name);
                    }
                });
                super::sidebar_tab::scroll(ui, "scopes_matrix", |ui| {
                    for scope in auth::DEFAULT_SCOPES {
                        spai_fit_ui::rows::row(ui, "scopes_matrix", scope, ROW_H - 4.0, false, |ui| {
                            spai_fit_ui::rows::cell(ui, text_w, egui::RichText::new(use_of(scope)), scope);
                            for c in &chars {
                                let granted = c.scopes.split(' ').any(|s| s == *scope);
                                let (rect, resp) = ui.allocate_exact_size(egui::vec2(MARK_W, ROW_H - 4.0), egui::Sense::hover());
                                let (glyph, color) = if granted { (icon::CHECK_CIRCLE, standing::FRIENDLY) } else { (icon::X_CIRCLE, standing::HOSTILE) };
                                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(16.0), color);
                                let state = if granted { tr!("Granted") } else { tr!("Missing") };
                                resp.on_hover_text(format!("{}: {state}", c.name));
                            }
                        });
                    }
                });
            } else {
                super::sidebar_tab::scroll(ui, "scopes_summary", |ui| {
                    for c in &chars {
                        let missing: Vec<&str> = auth::DEFAULT_SCOPES.iter().copied().filter(|s| !c.scopes.split(' ').any(|h| h == *s)).collect();
                        let all = auth::DEFAULT_SCOPES.len();
                        let line = if missing.is_empty() {
                            trf!("{name}   ·   all {all} granted", name = c.name, all = all)
                        } else {
                            trf!("{name}   ·   {n} of {all}, missing: {list}", name = c.name, n = all - missing.len(), all = all, list = missing.iter().map(|s| use_of(s)).collect::<Vec<_>>().join("; "))
                        };
                        spai_fit_ui::rows::row(ui, "scopes_summary", c.id, ROW_H - 4.0, false, |ui| {
                            mark(ui, missing.is_empty());
                            clipped(ui, egui::RichText::new(&line), &line);
                        });
                    }
                });
            }
            return;
        }

        let Some(c) = me else {
            ui.add(egui::Label::new(egui::RichText::new(tr!("This character is not signed in, so it has granted nothing. Its settings files work without a login; signing in adds location, fleet and skill features.")).weak()).wrap());
            return;
        };
        let have = granted(&c);
        super::sidebar_tab::scroll(ui, "scopes_list", |ui| {
            for scope in auth::DEFAULT_SCOPES {
                ui.horizontal(|ui| {
                    mark(ui, have.iter().any(|h| h == scope));
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(use_of(scope)).truncate());
                        ui.add(egui::Label::new(egui::RichText::new(*scope).weak()).truncate());
                    });
                });
                ui.add_space(2.0);
            }
            let extra: Vec<&String> = have.iter().filter(|h| !auth::DEFAULT_SCOPES.contains(&h.as_str())).collect();
            if !extra.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr!("Also granted, and not used by this version")).strong());
                for e in extra {
                    ui.add(egui::Label::new(egui::RichText::new(e).weak()).truncate());
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_requested_scope_says_what_it_is_for() {
        for scope in auth::DEFAULT_SCOPES {
            assert!(!use_of(scope).is_empty(), "{scope} has no description on the Scopes tab");
        }
        assert_eq!(USES.len(), auth::DEFAULT_SCOPES.len());
    }
}
