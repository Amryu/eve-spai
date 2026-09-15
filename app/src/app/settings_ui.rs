//! The setup wizard, the settings view and its configuration windows: alert rules, severity, coalitions, bridges, sov upgrades and channels.

use super::*;

impl SpaiApp {
    pub(crate) fn setup_wizard(&mut self, ctx: &egui::Context) {
        if !self.wizard_open {
            return;
        }
        use egui_phosphor::regular as icon;

        #[derive(Clone, Copy, PartialEq)]
        enum S {
            Shortcut,
            Welcome,
            Logs,
            Channels,
            JumpBridges,
            SovUpgrades,
            Jabber,
            Character,
            Theme,
        }
        let mut steps = Vec::new();
        // Offer a launcher entry first, but only when the installer didn't already make one.
        if matches!(crate::tray::menu_entry_exists(), Some(false)) {
            steps.push(S::Shortcut);
        }
        steps.extend([S::Welcome, S::Logs, S::Channels]);
        if self.settings.configuration_pack == "The Imperium" {
            steps.extend([S::JumpBridges, S::SovUpgrades, S::Jabber]);
        }
        steps.extend([S::Character, S::Theme]);
        let last = steps.len() - 1;
        let mut idx = (self.wizard_step as usize).min(last);
        let cur = steps[idx];
        let total = steps.len();

        let mut close = false;
        let mut finish = false;
        egui::Window::new(format!("{}  Setup", icon::MAGIC_WAND))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(format!("Step {} of {total}", idx + 1)).weak());
                ui.separator();
                ui.add_space(4.0);
                match cur {
                    S::Shortcut => {
                        let kind = crate::tray::menu_entry_label();
                        ui.heading(format!("{}  Add a shortcut", icon::ROCKET_LAUNCH));
                        ui.label(format!(
                            "EVE Spai has no {kind} yet, so it only launches from where the \
                             binary lives. Add one to start it like any other app.",
                        ));
                        ui.add_space(6.0);
                        match &self.wizard_shortcut {
                            Some(Ok(())) => {
                                ui.label(
                                    egui::RichText::new(format!("{}  Shortcut created", icon::CHECK_CIRCLE))
                                        .color(crate::theme::standing::FRIENDLY),
                                );
                            }
                            _ => {
                                if ui
                                    .button(format!("{}  Create {kind}", icon::PLUS))
                                    .clicked()
                                {
                                    self.wizard_shortcut =
                                        Some(crate::tray::create_menu_entry().map_err(|e| e.to_string()));
                                }
                                if let Some(Err(e)) = &self.wizard_shortcut {
                                    ui.label(
                                        egui::RichText::new(format!("Couldn't create it: {e}"))
                                            .color(crate::theme::standing::WARNING),
                                    );
                                }
                            }
                        }
                    }
                    S::Welcome => {
                        ui.heading("Welcome to EVE Spai");
                        ui.label(
                            "A quick setup to get intel flowing. Everything here can be changed \
                             later in Settings, and you can re-run this wizard from there.",
                        );
                    }
                    S::Logs => {
                        ui.heading(format!("{}  EVE chat logs", icon::FOLDER_OPEN));
                        ui.label(
                            "EVE Spai reads your in-game intel-channel logs. Leave blank to \
                             auto-detect the standard location.",
                        );
                        ui.add_space(4.0);
                        let hint = crate::logpaths::chat_logs_dir("")
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "auto-detect".into());
                        let resolved = crate::logpaths::chat_logs_dir(&self.settings.eve_logs_dir);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.settings.eve_logs_dir)
                                    .hint_text(hint)
                                    .desired_width(380.0),
                            );
                            if resolved.is_some() {
                                ui.label(
                                    egui::RichText::new(icon::CHECK_CIRCLE)
                                        .color(crate::theme::standing::FRIENDLY),
                                )
                                .on_hover_text("Valid EVE chat-log folder");
                            } else {
                                ui.label(
                                    egui::RichText::new(icon::X_CIRCLE)
                                        .color(crate::theme::standing::HOSTILE),
                                )
                                .on_hover_text("No EVE chat-log folder found here");
                            }
                        });
                        match &resolved {
                            Some(p) => {
                                ui.label(
                                    egui::RichText::new(format!("Using {}", p.display()))
                                        .weak(),
                                );
                            }
                            None if self.settings.eve_logs_dir.trim().is_empty() => {
                                ui.label(
                                    egui::RichText::new(
                                        "Couldn't auto-detect — enter the path to your EVE \
                                         Chatlogs folder.",
                                    )
                                    .color(crate::theme::standing::WARNING),
                                );
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new("That folder has no EVE chat logs.")
                                        .color(crate::theme::standing::WARNING),
                                );
                            }
                        }
                    }
                    S::Channels => {
                        ui.heading(format!("{}  Intel channels", icon::BROADCAST));
                        ui.label("Apply the Imperium preset channels, or add them manually.");
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            // Packs with no channels exist only for battle-report coalition
                            // tagging; an "Apply" button for them would do nothing.
                            for pack in br_core::packs::PACKS.iter().filter(|p| !p.channels.is_empty())
                            {
                                let selected = self.settings.configuration_pack == pack.name;
                                if ui
                                    .add(egui::Button::new(format!("Apply {}", pack.name)).selected(selected))
                                    .clicked()
                                {
                                    for ch in pack.channels {
                                        if !self
                                            .settings
                                            .intel_channels
                                            .iter()
                                            .any(|c| c.eq_ignore_ascii_case(ch))
                                        {
                                            self.settings.intel_channels.push((*ch).to_owned());
                                        }
                                    }
                                    self.settings.configuration_pack = pack.name.to_owned();
                                    self.needs_save = true;
                                }
                            }
                        });
                        ui.add_space(4.0);
                        if ui.button("Configure channels manually…").clicked() {
                            self.intel_channels_open = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} channel(s) configured",
                                self.settings.intel_channels.len()
                            ))
                            .weak(),
                        );
                    }
                    S::JumpBridges => {
                        ui.heading(format!("{}  Jump bridges (optional)", icon::MAP_TRIFOLD));
                        ui.label(
                            "Import your alliance's jump-bridge network so it's drawn on the map \
                             and used for jump-range filters.",
                        );
                        ui.add_space(4.0);
                        if ui.button("Configure jump bridges…").clicked() {
                            self.jump_bridges_open = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} bridge(s) configured",
                                self.settings.jump_bridges.len()
                            ))
                            .weak(),
                        );
                    }
                    S::SovUpgrades => {
                        ui.heading(format!("{}  Sov upgrades (optional)", icon::GEAR_SIX));
                        ui.label(
                            "Paste your alliance's iHub sov-upgrade data for the map overlay \
                             (cyno jammers, Ansiblex enablement, …).",
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    "The data lives in a formatted list linked inside the alliance \
                                     forum post, not on the forum page itself. Open the post, follow \
                                     that link, and copy the list.",
                                )
                                .weak(),
                            );
                        });
                        ui.add_space(4.0);
                        if ui.button("Configure sov upgrades…").clicked() {
                            self.sov_upgrades_open = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} system(s) configured",
                                self.settings.sov_upgrades.len()
                            ))
                            .weak(),
                        );
                    }
                    S::Jabber => {
                        ui.heading(format!("{}  Jabber (optional)", icon::CHAT_TEXT));
                        ui.label("Connect to alliance Jabber (XMPP) for chat and fleet pings.");
                        ui.add_space(4.0);
                        let connected = self.settings.jabber_enabled
                            && crate::jabber::has_password(self.settings.jabber_jid.trim());
                        if connected {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}  Connected as {}",
                                    icon::CHECK_CIRCLE,
                                    self.settings.jabber_jid
                                ))
                                .color(crate::theme::standing::ALLIANCE),
                            );
                        } else {
                            egui::Grid::new("wiz_jabber").num_columns(2).spacing([8.0, 6.0]).show(
                                ui,
                                |ui| {
                                    ui.label("JID");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.jabber_jid)
                                            .hint_text("MyCharacter@goonfleet.com")
                                            .desired_width(260.0),
                                    );
                                    ui.end_row();
                                    ui.label("Server");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.settings.jabber_server)
                                            .hint_text("jabber-server.goonfleet.com")
                                            .desired_width(260.0),
                                    );
                                    ui.end_row();
                                    ui.label("Password");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.jabber_pw_input)
                                            .password(true)
                                            .desired_width(260.0),
                                    );
                                    ui.end_row();
                                },
                            );
                            if ui.button("Connect").clicked() {
                                let jid = self.settings.jabber_jid.trim().to_owned();
                                if !jid.is_empty() && !self.jabber_pw_input.is_empty() {
                                    match crate::jabber::save_password(&jid, &self.jabber_pw_input) {
                                        Ok(()) => {
                                            self.jabber_pw_input.clear();
                                            self.settings.jabber_enabled = true;
                                            self.needs_save = true;
                                        }
                                        Err(e) => {
                                            self.jabber.lock().unwrap().status =
                                                format!("Keychain error: {e}");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    S::Character => {
                        ui.heading(format!("{}  Log in a character", icon::SIGN_IN));
                        ui.label(
                            "Log in with EVE SSO so EVE Spai knows your location for \
                             distance / near-me filters and the map.",
                        );
                        ui.add_space(4.0);
                        if ui.button(format!("{}  Log in with EVE", icon::SIGN_IN)).clicked() {
                            self.start_login(ctx);
                        }
                        if !self.characters.is_empty() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}  {} character(s) linked",
                                    icon::CHECK_CIRCLE,
                                    self.characters.len()
                                ))
                                .color(crate::theme::standing::ALLIANCE),
                            );
                        }
                    }
                    S::Theme => {
                        ui.heading(format!("{}  Theme", icon::PALETTE));
                        ui.label("Pick a colour preset (fine-tune fully in Settings).");
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for preset in Theme::presets() {
                                if ui.button(&preset.name).clicked() {
                                    self.settings.theme = preset.clone();
                                    self.needs_save = true;
                                }
                            }
                        });
                    }
                }
                ui.add_space(10.0);
                ui.separator();
                ui.horizontal(|ui| {
                    let mut step_changed = false;
                    if ui.button("Skip setup").clicked() {
                        close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if idx >= last {
                            if ui.button(format!("{}  Finish", icon::CHECK_CIRCLE)).clicked() {
                                finish = true;
                            }
                        } else if ui.button("Next").clicked() {
                            idx += 1;
                            step_changed = true;
                        }
                        if idx > 0 && ui.button("Back").clicked() {
                            idx -= 1;
                            step_changed = true;
                        }
                    });
                    // Leaving a step closes any config dialog it opened, so it does not
                    // linger over the next step.
                    if step_changed || finish || close {
                        self.intel_channels_open = false;
                        self.jump_bridges_open = false;
                        self.sov_upgrades_open = false;
                    }
                });
            });
        self.wizard_step = idx.min(last) as u8;
        if finish || close {
            self.settings.wizard_done = true;
            self.needs_save = true;
            self.wizard_open = false;
        }
    }

    pub(crate) fn open_filter_picker(&mut self, kind: crate::pickers::PickerKind, rule_idx: usize) {
        use crate::pickers::{
            build_geo_picker, build_ship_tree, seed_selection, FilterPicker, PickerData, PickerKind,
        };
        let Some(store) = &self.store else { return };
        let Some(rule) = self.settings.alerts.rules.get(rule_idx) else { return };
        let mut picker = FilterPicker::new(kind, rule_idx);
        match kind {
            PickerKind::Systems => {
                let (roots, flat) = build_geo_picker(&store.all_systems_geo());
                picker.geo_roots = roots;
                picker.geo_flat = flat;
                picker.geo_regions = rule.regions.iter().cloned().collect();
                picker.geo_consts = rule.constellations.iter().cloned().collect();
                picker.geo_systems = rule.systems.iter().cloned().collect();
            }
            PickerKind::Ships => {
                picker.data = PickerData::Tree(build_ship_tree(&store.all_ships()));
                picker.selected = seed_selection(&rule.ships, &picker.data);
            }
            PickerKind::Channels => {
                let mut opts = self.settings.intel_channels.clone();
                for c in &rule.channels {
                    if !opts.iter().any(|o| o.eq_ignore_ascii_case(c)) {
                        opts.push(c.clone());
                    }
                }
                picker.data = PickerData::List(opts);
                picker.selected = seed_selection(&rule.channels, &picker.data);
            }
            PickerKind::Characters => {
                let mut chars = store.known_pilot_names();
                for c in &self.characters {
                    if !chars.iter().any(|(n, _)| n.eq_ignore_ascii_case(&c.name)) {
                        chars.push((c.name.clone(), c.id));
                    }
                }
                for c in &rule.characters {
                    if !chars.iter().any(|(n, _)| n.eq_ignore_ascii_case(c)) {
                        chars.push((c.clone(), 0));
                    }
                }
                chars.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                picker.data = PickerData::Chars(chars);
                picker.selected = seed_selection(&rule.characters, &picker.data);
            }
        }
        *self.filter_add_result.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.filter_picker = Some(picker);
    }

    pub(crate) fn filter_picker_dialog(&mut self, ctx: &egui::Context) {
        if self.filter_picker.is_none() {
            return;
        }
        let add_res = self.filter_add_result.lock().unwrap_or_else(|e| e.into_inner()).take();
        let mut open = true;
        let mut changed = false;
        let mut add_to_resolve: Option<String> = None;
        {
            let picker = self.filter_picker.as_mut().unwrap();
            if let Some(res) = add_res {
                match res {
                    Ok(name) => {
                        picker.selected.insert(name.clone());
                        picker.add_status = Some(format!("Added {name}"));
                        picker.add_name.clear();
                        changed = true;
                    }
                    Err(e) => picker.add_status = Some(e),
                }
            }
            let title = format!("{}  filter: {}", egui_phosphor::regular::FUNNEL, picker.kind.title());
            let mut actions = crate::pickers::PickerActions::default();
            egui::Window::new(title)
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(340.0)
                .show(ctx, |ui| {
                    actions = crate::pickers::body(ui, picker);
                });
            changed |= actions.changed;
            if actions.add_clicked {
                add_to_resolve = Some(picker.add_name.trim().to_owned());
            }
        }
        if let Some(name) = add_to_resolve {
            if !name.is_empty() {
                self.spawn_char_resolve(name, ctx);
            }
        }
        if changed {
            let sorted = |set: &std::collections::HashSet<String>| {
                let mut v: Vec<String> = set.iter().cloned().collect();
                v.sort_by_key(|s| s.to_lowercase());
                v
            };
            let (kind, idx, sel, geo) = {
                let p = self.filter_picker.as_ref().unwrap();
                (
                    p.kind,
                    p.rule_idx,
                    sorted(&p.selected),
                    (sorted(&p.geo_regions), sorted(&p.geo_consts), sorted(&p.geo_systems)),
                )
            };
            if let Some(rule) = self.settings.alerts.rules.get_mut(idx) {
                use crate::pickers::PickerKind::*;
                match kind {
                    Ships => rule.ships = sel,
                    Channels => rule.channels = sel,
                    Characters => rule.characters = sel,
                    Systems => {
                        rule.regions = geo.0;
                        rule.constellations = geo.1;
                        rule.systems = geo.2;
                    }
                }
                self.needs_save = true;
            }
        }
        if !open {
            self.filter_picker = None;
        }
    }

    pub(crate) fn spawn_char_resolve(&self, name: String, ctx: &egui::Context) {
        let out = self.filter_add_result.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = crate::http::client(15)
                .map_err(|e| e.to_string())
                .and_then(|c| match crate::universe::character(&c, &name) {
                    Ok(Some((_, found))) => Ok(found),
                    Ok(None) => Err(format!("No pilot named \"{}\"", name.trim())),
                    Err(e) => Err(format!("lookup failed: {e}")),
                });
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(res);
            ctx.request_repaint();
        });
    }

    pub(crate) fn alert_rule_config(
        ui: &mut egui::Ui,
        ru: &mut crate::settings::AlertRule,
        i: usize,
        global_volume: f32,
        notes: &crate::notes::NotesView,
    ) -> (bool, Option<crate::pickers::PickerKind>) {
        use crate::settings::Severity::*;
        let mut changed = false;
        let mut open_picker: Option<crate::pickers::PickerKind> = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("if severity ≥");
            egui::ComboBox::from_id_salt(("rsev", i))
                .selected_text(format!("{:?}", ru.min_severity))
                .show_ui(ui, |ui| {
                    for lvl in [Info, Warning, Danger, Critical] {
                        changed |= ui
                            .selectable_value(&mut ru.min_severity, lvl, format!("{lvl:?}"))
                            .changed();
                    }
                });
            ui.label("within");
            let mut mj = ru.max_jumps.unwrap_or(0);
            if ui
                .add(egui::DragValue::new(&mut mj).range(0..=50).custom_formatter(|n, _| {
                    if n == 0.0 { "any".into() } else { format!("{n}j") }
                }))
                .changed()
            {
                ru.max_jumps = if mj == 0 { None } else { Some(mj) };
                changed = true;
            }
            if ru.max_jumps.is_some() {
                changed |= ui
                    .checkbox(&mut ru.count_bridges, "bridges")
                    .on_hover_text(
                        "Count jump bridges in the range. Off = gate-only \
                         (how far a hostile, who can't use your bridges, really is).",
                    )
                    .changed();
            }
            ui.label("count ≥");
            let mut mc = ru.min_count.unwrap_or(0);
            if ui.add(egui::DragValue::new(&mut mc).range(0..=999)).changed() {
                ru.min_count = if mc == 0 { None } else { Some(mc) };
                changed = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("requires:");
            for tag in [
                "bubble", "camp", "cyno", "dropper", "captackled", "kill", "ess", "spike",
                "wormhole", "help",
            ] {
                let label = if tag == "captackled" { "cap tackled" } else { tag };
                let mut on = ru.require.iter().any(|t| t == tag);
                if selectable_chip(ui, on, label).clicked() {
                    on = !on;
                    ru.require.retain(|t| t != tag);
                    if on {
                        ru.require.push(tag.to_owned());
                    }
                    changed = true;
                }
            }
        });
        {
            use crate::pickers::PickerKind;
            let row = |ui: &mut egui::Ui, label: &str, list: &[String], any_hint: &str| -> bool {
                let mut clicked = false;
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.button("Edit").clicked() {
                        clicked = true;
                    }
                    let s = if list.is_empty() {
                        any_hint.to_owned()
                    } else if list.len() <= 3 {
                        list.join(", ")
                    } else {
                        format!("{} selected", list.len())
                    };
                    ui.label(egui::RichText::new(s).weak());
                });
                clicked
            };
            let mut want: Option<PickerKind> = None;
            ui.horizontal(|ui| {
                ui.label("location:");
                if ui.button("Edit").clicked() {
                    want = Some(PickerKind::Systems);
                }
                let total = ru.regions.len() + ru.constellations.len() + ru.systems.len();
                let s = if total == 0 { "any".to_owned() } else { format!("{total} selected") };
                ui.label(egui::RichText::new(s).weak());
            });
            if row(ui, "channels:", &ru.channels, "any") {
                want = Some(PickerKind::Channels);
            }
            if row(ui, "ships:", &ru.ships, "any") {
                want = Some(PickerKind::Ships);
            }
            if row(ui, "characters:", &ru.characters, "any enabled") {
                want = Some(PickerKind::Characters);
            }
            changed |= rule_tag_row(ui, ("pilot_tags", i), "pilot tags:", notes, crate::notes::NoteKind::Pilot, &mut ru.pilot_tags);
            changed |= rule_tag_row(ui, ("system_tags", i), "system tags:", notes, crate::notes::NoteKind::System, &mut ru.system_tags);
            if let Some(kind) = want {
                open_picker = Some(kind);
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("then:");
            changed |= ui.checkbox(&mut ru.suppress, "suppress").changed();
            if !ru.suppress {
                changed |= ui.checkbox(&mut ru.system_notification, "notify").changed();
                changed |= ui.checkbox(&mut ru.custom_window, "window").changed();
                changed |= ui.checkbox(&mut ru.push, "push").changed();
                ui.label("sound");
                let eff_vol = ru.volume.unwrap_or(global_volume);
                changed |= sound_picker(ui, ("alert_rule", i), true, &mut ru.sound, eff_vol);
                ui.label("severity");
                egui::ComboBox::from_id_salt(("rsevover", i))
                    .selected_text(match ru.severity_override {
                        None => "keep".to_owned(),
                        Some(s) => format!("{s:?}"),
                    })
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(&mut ru.severity_override, None, "keep")
                            .changed();
                        for lvl in [Info, Warning, Danger, Critical] {
                            changed |= ui
                                .selectable_value(
                                    &mut ru.severity_override,
                                    Some(lvl),
                                    format!("{lvl:?}"),
                                )
                                .changed();
                        }
                    })
                    .response
                    .on_hover_text(
                        "Override the alert's severity (sound + colour). Leave 'keep' \
                         to use the event's own severity. Set Info to show it silently.",
                    );
                let mut custom = ru.volume.is_some();
                if ui
                    .checkbox(&mut custom, "custom volume")
                    .on_hover_text("Override the global intel-alert volume for this rule")
                    .changed()
                {
                    ru.volume = if custom { Some(global_volume) } else { None };
                    changed = true;
                }
                if let Some(v) = ru.volume.as_mut() {
                    changed |= volume_slider(ui, v);
                }
            }
            ui.label("cooldown");
            changed |= ui
                .add(egui::DragValue::new(&mut ru.cooldown_secs).range(0..=3600).suffix("s"))
                .changed();
        });
        (changed, open_picker)
    }

    pub(crate) fn alert_rules_editor(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as ic;
        let mut changed = false;
        let mut remove: Option<usize> = None;
        let mut move_up: Option<usize> = None;
        let mut move_down: Option<usize> = None;
        let mut dnd: Option<(usize, usize)> = None;
        let mut open_picker: Option<(crate::pickers::PickerKind, usize)> = None;

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(format!("{}  Back", ic::ARROW_LEFT))
                .on_hover_text("Back to alerts")
                .clicked()
            {
                self.alert_rules_open = false;
            }
            ui.separator();
            if ui.button(format!("{}  Add rule", ic::PLUS)).clicked() {
                self.settings
                    .alerts
                    .rules
                    .push(crate::settings::AlertRule { expanded: true, ..Default::default() });
                crate::settings::ensure_rule_ids(&mut self.settings.alerts.rules);
                self.alert_selected_rule = self.settings.alerts.rules.last().map(|r| r.id);
                changed = true;
            }
        });
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Top rule wins. A matching rule's actions apply (or it suppresses the alert). \
                 Empty condition fields mean \"any\". Jumps are measured from the rule's \
                 characters (or any enabled character). Drag a rule's handle to reorder it, or \
                 select it and use the arrows under the list.",
            )
            .weak(),
        );
        ui.add_space(4.0);
        ui.separator();

        // Keep the selection valid (first rule by default, cleared rules fall back).
        let ids: Vec<u64> = self.settings.alerts.rules.iter().map(|r| r.id).collect();
        if self.alert_selected_rule.map_or(true, |id| !ids.contains(&id)) {
            self.alert_selected_rule = ids.first().copied();
        }
        let n_rules = self.settings.alerts.rules.len();

        egui::Panel::left("alert_rules_split")
            .resizable(true)
            .default_size(240.0)
            .size_range(180.0..=400.0)
            .show_inside(ui, |ui| {
                let sel_idx = self
                    .alert_selected_rule
                    .and_then(|id| self.settings.alerts.rules.iter().position(|r| r.id == id));
                // In the row these reserved 82px of a 240px panel and cut every name to eight
                // characters; under the list they cost the name nothing.
                egui::Panel::bottom("alert_rule_reorder").resizable(false).show_inside(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                sel_idx.is_some_and(|i| i > 0),
                                egui::Button::new(ic::ARROW_UP),
                            )
                            .on_hover_text("Move the selected rule up")
                            .clicked()
                        {
                            move_up = sel_idx;
                        }
                        if ui
                            .add_enabled(
                                sel_idx.is_some_and(|i| i + 1 < n_rules),
                                egui::Button::new(ic::ARROW_DOWN),
                            )
                            .on_hover_text("Move the selected rule down")
                            .clicked()
                        {
                            move_down = sel_idx;
                        }
                    });
                    ui.add_space(4.0);
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt("alert_rule_list")
                    .show(ui, |ui| {
                        for i in 0..n_rules {
                            let (id, enabled, name) = {
                                let r = &self.settings.alerts.rules[i];
                                (r.id, r.enabled, r.name.clone())
                            };
                            let selected = self.alert_selected_rule == Some(id);
                            let (_, payload) = ui.dnd_drop_zone::<usize, _>(
                                egui::Frame::default().inner_margin(2.0),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        let mut en = enabled;
                                        if ui.checkbox(&mut en, "").changed() {
                                            self.settings.alerts.rules[i].enabled = en;
                                            changed = true;
                                        }
                                        ui.dnd_drag_source(
                                            egui::Id::new(("alert_rule_dnd", id)),
                                            i,
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(ic::DOTS_SIX_VERTICAL).weak(),
                                                )
                                                .on_hover_text("Drag to reorder");
                                            },
                                        );
                                        let label =
                                            if name.is_empty() { "(unnamed rule)" } else { &name };
                                        let txt = if enabled {
                                            egui::RichText::new(label)
                                        } else {
                                            egui::RichText::new(label).weak().strikethrough()
                                        };
                                        if ui
                                            .add(egui::Button::selectable(selected, txt).truncate())
                                            .on_hover_text(label)
                                            .clicked()
                                        {
                                            self.alert_selected_rule = Some(id);
                                        }
                                    });
                                },
                            );
                            if let Some(from) = payload {
                                dnd = Some((*from, i));
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let Some(sel_id) = self.alert_selected_rule else {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Select a rule to configure it.").weak());
                return;
            };
            let Some(idx) = self.settings.alerts.rules.iter().position(|r| r.id == sel_id) else {
                return;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("alert_rule_config")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut en = self.settings.alerts.rules[idx].enabled;
                        if ui.checkbox(&mut en, "").changed() {
                            self.settings.alerts.rules[idx].enabled = en;
                            changed = true;
                        }
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut self.settings.alerts.rules[idx].name)
                                    .desired_width(240.0),
                            )
                            .changed();
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui
                                    .button(format!("{}  Delete", ic::TRASH))
                                    .on_hover_text("Delete rule")
                                    .clicked()
                                {
                                    remove = Some(idx);
                                }
                            },
                        );
                    });
                    ui.add_space(6.0);
                    let global_volume = self.settings.alerts.alert_volume;
                    let (c, want) = Self::alert_rule_config(
                        ui,
                        &mut self.settings.alerts.rules[idx],
                        idx,
                        global_volume,
                        &self.notes_view,
                    );
                    changed |= c;
                    if let Some(kind) = want {
                        open_picker = Some((kind, idx));
                    }
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Recent matches").strong());
                    ui.add_space(4.0);
                    self.rule_feed_ui(ui, sel_id);
                });
        });

        if let Some(i) = remove {
            let removed_id = self.settings.alerts.rules.get(i).map(|r| r.id);
            self.settings.alerts.rules.remove(i);
            if self.alert_selected_rule == removed_id {
                self.alert_selected_rule = self.settings.alerts.rules.first().map(|r| r.id);
            }
            changed = true;
        }
        if let Some(i) = move_up {
            self.settings.alerts.rules.swap(i, i - 1);
            changed = true;
        }
        if let Some(i) = move_down {
            self.settings.alerts.rules.swap(i, i + 1);
            changed = true;
        }
        if let Some((from, to)) = dnd {
            let len = self.settings.alerts.rules.len();
            if from != to && from < len && to < len {
                let item = self.settings.alerts.rules.remove(from);
                let dst = if from < to { to - 1 } else { to };
                self.settings.alerts.rules.insert(dst, item);
                changed = true;
            }
        }
        if changed {
            self.needs_save = true;
        }
        if let Some((kind, idx)) = open_picker {
            self.open_filter_picker(kind, idx);
        }
    }

    pub(crate) fn severity_window(&mut self, ctx: &egui::Context) {
        if !self.severity_open {
            return;
        }
        let mut changed = false;
        let mut threat_text = self.settings.severity.threat_ships.join("\n");
        let keep = Self::dialog_viewport(
            ctx,
            "severity_window",
            "EVE Spai - Intel severity",
            [620.0, 480.0],
            |ui| {
                ui.label(
                    egui::RichText::new("Pick the severity (card colour) for each condition.")
                        .weak(),
                );
                ui.add_space(4.0);
                let sv = &mut self.settings.severity;
                let combo = |ui: &mut egui::Ui, label: &str, val: &mut crate::settings::Severity| -> bool {
                    use crate::settings::Severity::*;
                    let mut ch = false;
                    ui.horizontal(|ui| {
                        ui.label(label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            egui::ComboBox::from_id_salt(label)
                                .selected_text(format!("{val:?}"))
                                .show_ui(ui, |ui| {
                                    for lvl in [Info, Warning, Danger, Critical] {
                                        if ui.selectable_value(val, lvl, format!("{lvl:?}")).changed() {
                                            ch = true;
                                        }
                                    }
                                });
                        });
                    });
                    ch
                };
                ui.horizontal(|ui| {
                    ui.label("Big-gang threshold (≥)");
                    changed |=
                        ui.add(egui::DragValue::new(&mut sv.big_gang_threshold).range(2..=100)).changed();
                });
                ui.columns(2, |c| {
                    changed |= combo(&mut c[0], "Small gang (< threshold)", &mut sv.small_gang);
                    changed |= combo(&mut c[0], "Big gang (≥ threshold)", &mut sv.big_gang);
                    changed |= combo(&mut c[0], "Bubble", &mut sv.bubble);
                    changed |= combo(&mut c[0], "Gate camp", &mut sv.gate_camp);
                    changed |= combo(&mut c[0], "Spike (local)", &mut sv.spike);
                    changed |= combo(&mut c[0], "Cyno", &mut sv.cyno);
                    changed |= combo(&mut c[1], "Capital tackled", &mut sv.cap_tackled);
                    changed |= combo(&mut c[1], "Kill", &mut sv.kill);
                    changed |= combo(&mut c[1], "No visual", &mut sv.no_visual);
                    changed |= combo(&mut c[1], "Wormhole", &mut sv.wormhole);
                    changed |= combo(&mut c[1], "ESS", &mut sv.ess);
                    changed |= combo(&mut c[1], "High-threat ships", &mut sv.threat_ship);
                });
                ui.separator();
                ui.label(egui::RichText::new("High-threat hulls (one per line)").weak());
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut threat_text)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    sv.threat_ships =
                        threat_text.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()).collect();
                    changed = true;
                }
                if ui.button("Reset to defaults").clicked() {
                    *sv = crate::settings::SeverityRules::default();
                    changed = true;
                }
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.severity_open = false;
        }
    }

    pub(crate) fn coalitions_window(&mut self, ctx: &egui::Context) {
        if !self.coalitions_open {
            return;
        }
        let mut remove: Option<usize> = None;
        let mut add = false;
        let mut reset = false;
        let mut coal_color: Vec<(String, Option<(u8, u8, u8)>)> = Vec::new();
        let mut ally_color: Vec<(usize, Option<(u8, u8, u8)>)> = Vec::new();
        let mut ally_remove: Option<usize> = None;
        let mut ally_assign: Option<(String, Option<String>)> = None;
        let mut ally_add = false;
        let keep = Self::dialog_viewport(
            ctx,
            "coalitions_window",
            "EVE Spai - Coalitions",
            [520.0, 680.0],
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "Group alliances into coalitions for the map's sovereignty overlay. \
                         Alliance names must match the sov holder exactly (some end with a \
                         period). Unlisted alliances are shown as independent.",
                    )
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.button("Add coalition").clicked() {
                        add = true;
                    }
                    if ui.button("Reset to defaults").clicked() {
                        reset = true;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("coal_scroll").max_height(280.0).show(ui, |ui| {
                    for (i, (name, alliances)) in self.coal_edit.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Coalition").weak());
                                ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
                                let cur = self
                                    .settings
                                    .coalitions
                                    .iter()
                                    .find(|c| c.name == name.trim())
                                    .and_then(|c| c.color);
                                let mut rgb = cur.map(|(r, g, b)| [r, g, b]).unwrap_or_else(|| {
                                    let c = name_color(name);
                                    [c.r(), c.g(), c.b()]
                                });
                                if ui.color_edit_button_srgb(&mut rgb).changed() {
                                    coal_color.push((name.trim().to_owned(), Some((rgb[0], rgb[1], rgb[2]))));
                                }
                                if ui.button("Remove").clicked() {
                                    remove = Some(i);
                                }
                            });
                            ui.add(
                                egui::TextEdit::multiline(alliances)
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("One alliance name per line\nGoonswarm Federation"),
                            );
                        });
                    }
                });

                ui.separator();
                ui.label(egui::RichText::new("Alliances (sov holders)").strong());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.alliance_add)
                            .desired_width(220.0)
                            .hint_text("Add alliance by name"),
                    );
                    if ui.button("Add").clicked() {
                        ally_add = true;
                    }
                });
                egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("ally_scroll").show(ui, |ui| {
                    for (i, a) in self.settings.alliances.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let mut rgb = a.color.map(|(r, g, b)| [r, g, b]).unwrap_or_else(|| {
                                let c = name_color(&a.name);
                                [c.r(), c.g(), c.b()]
                            });
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                ally_color.push((i, Some((rgb[0], rgb[1], rgb[2]))));
                            }
                            ui.label(&a.name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(egui_phosphor::regular::X).clicked() {
                                    ally_remove = Some(i);
                                }
                                let current = self
                                    .settings
                                    .coalitions
                                    .iter()
                                    .find(|c| c.alliances.iter().any(|x| x.eq_ignore_ascii_case(&a.name)))
                                    .map(|c| c.name.clone());
                                egui::ComboBox::from_id_salt(("coal_of", i))
                                    .selected_text(current.clone().unwrap_or_else(|| "—".to_owned()))
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(current.is_none(), "— independent").clicked() {
                                            ally_assign = Some((a.name.clone(), None));
                                        }
                                        for c in &self.settings.coalitions {
                                            if ui
                                                .selectable_label(
                                                    current.as_deref() == Some(c.name.as_str()),
                                                    &c.name,
                                                )
                                                .clicked()
                                            {
                                                ally_assign = Some((a.name.clone(), Some(c.name.clone())));
                                            }
                                        }
                                    });
                            });
                        });
                    }
                });
            },
        );
        if add {
            self.coal_edit.push(("New coalition".to_owned(), String::new()));
        }
        if reset {
            self.coal_edit = crate::settings::default_coalitions()
                .into_iter()
                .map(|c| (c.name, c.alliances.join("\n")))
                .collect();
        }
        if let Some(i) = remove {
            self.coal_edit.remove(i);
        }
        for (name, col) in coal_color {
            if let Some(c) = self.settings.coalitions.iter_mut().find(|c| c.name == name) {
                c.color = col;
                self.needs_save = true;
            }
        }
        for (i, col) in ally_color {
            if let Some(a) = self.settings.alliances.get_mut(i) {
                a.color = col;
                self.needs_save = true;
            }
        }
        if let Some(i) = ally_remove {
            if i < self.settings.alliances.len() {
                self.settings.alliances.remove(i);
                self.needs_save = true;
            }
        }
        if let Some((ally, target)) = ally_assign {
            for c in &mut self.settings.coalitions {
                c.alliances.retain(|x| !x.eq_ignore_ascii_case(&ally));
            }
            if let Some(t) = target {
                if let Some(c) = self.settings.coalitions.iter_mut().find(|c| c.name == t) {
                    c.alliances.push(ally);
                }
            }
            self.coal_edit = self
                .settings
                .coalitions
                .iter()
                .map(|c| (c.name.clone(), c.alliances.join("\n")))
                .collect();
            self.needs_save = true;
        }
        if ally_add {
            let name = self.alliance_add.trim().to_owned();
            if !name.is_empty()
                && !self.settings.alliances.iter().any(|a| a.name.eq_ignore_ascii_case(&name))
            {
                self.settings.alliances.push(crate::settings::AllianceConfig { name, color: None });
                self.settings.alliances.sort_by(|a, b| a.name.cmp(&b.name));
                self.needs_save = true;
            }
            self.alliance_add.clear();
        }
        let parsed: Vec<crate::settings::Coalition> = self
            .coal_edit
            .iter()
            .filter(|(n, _)| !n.trim().is_empty())
            .map(|(n, a)| crate::settings::Coalition {
                name: n.trim().to_owned(),
                alliances: a.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()).collect(),
                color: self
                    .settings
                    .coalitions
                    .iter()
                    .find(|c| c.name == n.trim())
                    .and_then(|c| c.color),
            })
            .collect();
        if parsed != self.settings.coalitions {
            self.settings.coalitions = parsed;
            self.needs_save = true;
        }
        if !keep {
            self.coalitions_open = false;
        }
    }

    pub(crate) fn jump_bridges_window(&mut self, ctx: &egui::Context) {
        if !self.jump_bridges_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "jump_bridges_window",
            "EVE Spai - Jump bridges",
            [440.0, 520.0],
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Paste a jump-bridge list (one bridge per line).").weak(),
                    );
                    ui.label(egui::RichText::new(egui_phosphor::regular::QUESTION).weak()).on_hover_text(
                        "Imperium members: open the alliance jump-bridge map, copy the bridge \
                         list, and paste it here. Each line's first two systems form a bridge \
                         (any separator works).",
                    );
                    ui.hyperlink_to("Imperium stargates", "https://wiki.goonswarm.org/w/Alliance:Stargate");
                });
                egui::ScrollArea::vertical()
                    .max_height(110.0)
                    .auto_shrink([false, false])
                    .id_salt("jb_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.jb_paste)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g.  1DQ1-A » O-EIMK   (one bridge per line, or paste the whole wiki page)"),
                        );
                    });
                ui.horizontal(|ui| {
                    if ui.button("Add from paste").clicked() {
                        if let Some(g) = self.systems.clone() {
                            for b in parse_bridges(&self.jb_paste, &g) {
                                if !self.settings.jump_bridges.contains(&b) {
                                    self.settings.jump_bridges.push(b);
                                    changed = true;
                                }
                            }
                        }
                        self.jb_paste.clear();
                    }
                    if !self.settings.jump_bridges.is_empty() && ui.button("Delete all").clicked() {
                        self.settings.jump_bridges.clear();
                        changed = true;
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(format!("{} bridges", self.settings.jump_bridges.len())).strong());
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove = None;
                    for (i, b) in self.settings.jump_bridges.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} » {}", b.from, b.to));
                            if ui.button(egui_phosphor::regular::X).clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.jump_bridges.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.jump_bridges_open = false;
        }
    }

    pub(crate) fn sov_upgrades_window(&mut self, ctx: &egui::Context) {
        if !self.sov_upgrades_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "sov_upgrades_window",
            "EVE Spai - Sov upgrades",
            [460.0, 520.0],
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Paste sov-upgrade data (one per line).").weak());
                    ui.label(egui::RichText::new(egui_phosphor::regular::QUESTION).weak()).on_hover_text(
                        "Imperium members: open the forum topic, then follow the link inside it to \
                         the formatted upgrade list and copy THAT. The forum page itself is not the \
                         paste. The first system matched on each line is used; the rest of the line \
                         becomes the upgrade label.",
                    );
                    ui.hyperlink_to(
                        "Equinox upgrades",
                        "https://goonfleet.com/index.php/topic/371770-equinox-upgrade-information-station",
                    );
                });
                egui::ScrollArea::vertical()
                    .max_height(110.0)
                    .auto_shrink([false, false])
                    .id_salt("sov_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.sov_paste)
                                .desired_rows(4)
                                .desired_width(f32::INFINITY)
                                .hint_text("e.g.  1DQ1-A Cynosural Suppression   (or paste the in-game I-Hub window)"),
                        );
                    });
                ui.horizontal(|ui| {
                    if ui.button("Add from paste").clicked() {
                        if let Some(g) = self.systems.clone() {
                            for u in parse_sov_upgrades(&self.sov_paste, &g) {
                                if !self.settings.sov_upgrades.contains(&u) {
                                    self.settings.sov_upgrades.push(u);
                                    changed = true;
                                }
                            }
                        }
                        self.sov_paste.clear();
                    }
                    if !self.settings.sov_upgrades.is_empty() && ui.button("Delete all").clicked() {
                        self.settings.sov_upgrades.clear();
                        changed = true;
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(format!("{} upgrades", self.settings.sov_upgrades.len())).strong());
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove = None;
                    for (i, u) in self.settings.sov_upgrades.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&u.system).strong());
                            ui.label(egui::RichText::new(&u.upgrade).weak());
                            if ui.button(egui_phosphor::regular::X).clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.sov_upgrades.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.sov_upgrades_open = false;
        }
    }

    pub(crate) fn intel_channels_window(&mut self, ctx: &egui::Context) {
        if !self.intel_channels_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "intel_channels_window",
            "EVE Spai - Intel channels",
            [420.0, 480.0],
            |ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "EVE chat channels to watch for intel. Match the in-game channel name.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                if ui.button("Add channel").clicked() {
                    self.settings.intel_channels.push(String::new());
                    changed = true;
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove: Option<usize> = None;
                    for (i, ch) in self.settings.intel_channels.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.text_edit_singleline(ch).changed() {
                                changed = true;
                            }
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.intel_channels.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.intel_channels_open = false;
        }
    }

    #[cfg(feature = "fc-rescue")]
    pub(crate) fn rescue_doctrines_window(&mut self, ctx: &egui::Context) {
        if !self.rescue_doctrines_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "rescue_doctrines_window",
            "EVE Spai - Rescue doctrines",
            [360.0, 420.0],
            |ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Name is shown in the selector; description is the ping's \"Doctrine:\" line.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rescue_doctrine_input)
                            .hint_text("New doctrine name")
                            .desired_width(180.0),
                    );
                    if ui.button("Add").clicked() {
                        let name = self.rescue_doctrine_input.trim().to_string();
                        if !name.is_empty()
                            && !self.settings.rescue_doctrines.iter().any(|d| d.name == name)
                        {
                            self.settings.rescue_doctrines.push(crate::settings::RescueDoctrine {
                                name,
                                description: String::new(),
                            });
                            self.rescue_doctrine_input.clear();
                            changed = true;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove: Option<usize> = None;
                    for (i, d) in self.settings.rescue_doctrines.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut d.name)
                                        .desired_width(120.0)
                                        .hint_text("name"),
                                )
                                .changed();
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut d.description)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("ping Doctrine: line"),
                            )
                            .changed();
                        ui.add_space(4.0);
                    }
                    if let Some(i) = remove {
                        self.settings.rescue_doctrines.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.rescue_doctrines_open = false;
        }
    }

    pub(crate) fn cyno_generators_window(&mut self, ctx: &egui::Context) {
        if !self.cyno_generators_open {
            return;
        }
        let mut changed = false;
        let systems = self.systems.clone();
        let keep = Self::dialog_viewport(
            ctx,
            "cyno_generators_window",
            "EVE Spai - Cyno generators",
            [380.0, 460.0],
            |ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Systems with a friendly cyno generator. ESI can't list these, so add them \
                         by name. Shown as the Cyno generators map layer.",
                    )
                    .weak(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rescue_cyno_input)
                            .hint_text("Add system by name")
                            .desired_width(200.0),
                    );
                    let can_add = systems.is_some();
                    if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                        if let Some(s) = &systems {
                            let tok = self.rescue_cyno_input.trim();
                            if let Some(info) = s.lookup(tok).or_else(|| s.lookup_prefix(tok)) {
                                if !self.settings.cyno_generators.contains(&info.id) {
                                    self.settings.cyno_generators.push(info.id);
                                    changed = true;
                                }
                                self.rescue_cyno_input.clear();
                            }
                        }
                    }
                });
                if systems.is_none() {
                    ui.label(egui::RichText::new("(map data still loading…)").weak());
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove: Option<usize> = None;
                    for (i, id) in self.settings.cyno_generators.iter().enumerate() {
                        let name = systems
                            .as_ref()
                            .and_then(|s| s.info_of(*id).map(|inf| inf.name.clone()))
                            .unwrap_or_else(|| format!("#{id}"));
                        ui.horizontal(|ui| {
                            ui.label(name);
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.settings.cyno_generators.remove(i);
                        changed = true;
                    }
                });
            },
        );
        if changed {
            self.needs_save = true;
        }
        if !keep {
            self.cyno_generators_open = false;
        }
    }

    pub(crate) fn settings_view(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut new_theme: Option<Theme> = None;
        let imp_target = if self.is_imperium() { "adashboard.info" } else { "dscan.info" };
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
                    if ui
                        .button(format!("{}  Run setup wizard", egui_phosphor::regular::MAGIC_WAND))
                        .clicked()
                    {
                        self.wizard_step = 0;
                        self.wizard_open = true;
                    }
                    ui.separator();

                    ui.label(egui::RichText::new("Theme (3 colours)").strong());
                    ui.horizontal_wrapped(|ui| {
                        for preset in Theme::presets() {
                            if ui.button(&preset.name).clicked() {
                                new_theme = Some(preset.clone());
                            }
                        }
                    });
                    ui.add_space(4.0);

                    changed |= color_row(ui, "Background", &mut self.settings.theme.background);
                    changed |= color_row(ui, "Foreground", &mut self.settings.theme.foreground);
                    changed |= color_row(ui, "Accent", &mut self.settings.theme.accent);

                    ui.separator();

                    ui.label(egui::RichText::new("General").strong());
                    changed |= ui
                        .checkbox(&mut self.settings.use_eve_time, "Show EVE time (UTC)")
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.settings.dscan_autoprompt,
                            "Offer to share d-scans from the clipboard",
                        )
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.settings.dscan_autoupload,
                            "Auto-upload detected d-scans (skip the prompt)",
                        )
                        .changed();
                    ui.horizontal(|ui| {
                        ui.label("D-scan service");
                        use crate::settings::DscanService as Dsc;
                        egui::ComboBox::from_id_salt("dscan_service")
                            .selected_text(match self.settings.dscan_service {
                                Dsc::Auto => format!("Auto ({imp_target})"),
                                Dsc::DscanInfo => "dscan.info".to_owned(),
                                Dsc::Adashboard => "adashboard.info".to_owned(),
                            })
                            .show_ui(ui, |ui| {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.dscan_service,
                                        Dsc::Auto,
                                        format!("Auto ({imp_target})"),
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.dscan_service,
                                        Dsc::DscanInfo,
                                        "dscan.info",
                                    )
                                    .changed();
                                changed |= ui
                                    .selectable_value(
                                        &mut self.settings.dscan_service,
                                        Dsc::Adashboard,
                                        "adashboard.info (Imperium)",
                                    )
                                    .changed();
                            });
                    })
                    .response
                    .on_hover_text(
                        "Auto uses adashboard.info/intel when an *.imperium intel channel is configured, \
                         else dscan.info. adashboard opens in your browser to paste (it needs your login).",
                    );
                    changed |= ui
                        .checkbox(
                            &mut self.settings.minimize_to_tray,
                            "Close to system tray (keep running)",
                        )
                        .changed();
                    if ui
                        .checkbox(&mut self.settings.autostart, "Start automatically on login")
                        .changed()
                    {
                        if let Err(e) = crate::tray::set_autostart(self.settings.autostart) {
                            eprintln!("[autostart] {e}");
                        }
                        changed = true;
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(format!(
                                "{}  Check for updates",
                                egui_phosphor::regular::ARROWS_CLOCKWISE
                            ))
                            .clicked()
                        {
                            self.update_dismissed = false;
                            self.settings.update_skip_version.clear();
                            changed = true;
                            crate::update::spawn_check(
                                self.update.clone(),
                                String::new(),
                                true,
                                ui.ctx().clone(),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .weak(),
                        );
                    });

                    ui.add_space(6.0);
                    ui.label("Fit preview site").on_hover_text("Where the fit window's \"Open in\" button sends a loss");
                    ui.horizontal_wrapped(|ui| {
                        for (id, label) in FIT_SITES {
                            if selectable_chip(ui, self.settings.fit_site == *id, *label).clicked() {
                                self.settings.fit_site = (*id).to_owned();
                                changed = true;
                            }
                        }
                        if selectable_chip(ui, self.settings.fit_site.is_empty(), "Ask each time").clicked() {
                            self.settings.fit_site.clear();
                            changed = true;
                        }
                    });

                    ui.add_space(6.0);
                    let logs_hint = crate::logpaths::chat_logs_dir("")
                        .and_then(|p| p.parent().map(|p| p.display().to_string()))
                        .unwrap_or_else(|| "auto-detect".to_owned());
                    ui.label("EVE chat-log directory");
                    changed |= dir_picker_row(ui, &logs_hint, &mut self.settings.eve_logs_dir);
                    let settings_hint = crate::charsettings::settings_root("")
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "auto-detect".to_owned());
                    ui.label("EVE settings directory")
                        .on_hover_text("Used by Characters > Copy settings");
                    if dir_picker_row(ui, &settings_hint, &mut self.settings.eve_settings_dir) {
                        changed = true;
                        if let Ok(mut slot) = self.eve_settings_path.lock() {
                            slot.clone_from(&self.settings.eve_settings_dir);
                        }
                        self.copy_settings.invalidate();
                    }

                    ui.separator();

                    ui.label(egui::RichText::new("Alerts").strong());
                    changed |= ui
                        .checkbox(&mut self.settings.alert_enabled, "Enable intel alerts")
                        .on_hover_text("Master switch. Configure what fires in the Alerts tab.")
                        .changed();
                    changed |= ui
                        .checkbox(&mut self.settings.alert_only_undocked, "Only alert while undocked")
                        .changed();

                    ui.add_space(6.0);
                    {
                        use crate::settings::OnTop;
                        let a = &mut self.settings.alerts;
                        ui.horizontal(|ui| {
                            ui.label("Alert window stays");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut a.window_timeout)
                                        .range(0.0..=300.0)
                                        .custom_formatter(|n, _| {
                                            if n <= 0.0 { "never hides".to_owned() } else { format!("{n}s") }
                                        }),
                                )
                                .on_hover_text("0 = never auto-hide")
                                .changed();
                            ui.label("· on top");
                            egui::ComboBox::from_id_salt("on_top")
                                .selected_text(match a.on_top {
                                    OnTop::Always => "Always",
                                    OnTop::Smart => "Smart (EVE active)",
                                    OnTop::Never => "Never",
                                })
                                .show_ui(ui, |ui| {
                                    changed |= ui.selectable_value(&mut a.on_top, OnTop::Always, "Always").changed();
                                    changed |= ui.selectable_value(&mut a.on_top, OnTop::Smart, "Smart (only when EVE is active)").changed();
                                    changed |= ui.selectable_value(&mut a.on_top, OnTop::Never, "Never").changed();
                                });
                        });
                        changed |= ui
                            .checkbox(&mut a.compact_mode, "Compact alert window")
                            .on_hover_text("Tighter rows and title bar. Hover cards pop out in their own window.")
                            .changed();
                        ui.label(egui::RichText::new("Sounds (preset: off/info/warning/danger/critical/beep/chime, or a file path)").weak());
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(64.0, ui.spacing().interact_size.y),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label("Volume");
                                },
                            );
                            changed |= volume_slider(ui, &mut a.alert_volume);
                        });
                        let alert_vol = a.alert_volume;
                        for (i, lbl) in ["Info", "Warning", "Danger", "Critical"].iter().enumerate() {
                            if a.sounds.len() <= i {
                                a.sounds.resize(i + 1, "off".to_owned());
                            }
                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(64.0, ui.spacing().interact_size.y),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.label(*lbl);
                                    },
                                );
                                changed |= sound_picker(ui, ("severity_sound", i), false, &mut a.sounds[i], alert_vol);
                            });
                        }
                        changed |= ui
                            .checkbox(&mut a.push_enabled, "Mobile push (Pushover)")
                            .on_hover_text("Install the Pushover app; create an application for the token")
                            .changed();
                        if a.push_enabled {
                            ui.horizontal(|ui| {
                                ui.label("App token");
                                changed |= ui.add(egui::TextEdit::singleline(&mut a.pushover_token).desired_width(220.0)).changed();
                            });
                            ui.horizontal(|ui| {
                                ui.label("User key ");
                                changed |= ui.add(egui::TextEdit::singleline(&mut a.pushover_user).desired_width(220.0)).changed();
                            });
                        }
                    }
                    ui.label(
                        egui::RichText::new("Alert rules live in the Alerts tab.").weak(),
                    );

                    ui.separator();

                    changed |= self.web_settings_section(ui);

                    ui.separator();

                    ui.label(egui::RichText::new("Battle reports").strong());
                    if ui
                        .checkbox(
                            &mut self.settings.battles_enabled,
                            "Enable battle report generation",
                        )
                        .on_hover_text(
                            "Turn off to stop all battle-report clustering and computation. \
                             Gate-camp warnings and the kill feed keep working.",
                        )
                        .changed()
                    {
                        self.battles_enabled_shared.store(
                            self.settings.battles_enabled,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        changed = true;
                    }

                    ui.separator();

                    ui.label(egui::RichText::new("Configuration packs").strong());
                    ui.label(
                        egui::RichText::new("Apply the Imperium preset intel channels.").weak(),
                    );
                    for pack in br_core::packs::PACKS.iter().filter(|p| !p.channels.is_empty()) {
                        ui.horizontal(|ui| {
                            if ui.button(format!("Apply {}", pack.name)).clicked() {
                                for ch in pack.channels {
                                    if !self
                                        .settings
                                        .intel_channels
                                        .iter()
                                        .any(|c| c.eq_ignore_ascii_case(ch))
                                    {
                                        self.settings.intel_channels.push((*ch).to_owned());
                                    }
                                }
                                self.settings.configuration_pack = pack.name.to_owned();
                                changed = true;
                            }
                            ui.label(
                                egui::RichText::new(format!("{} channels", pack.channels.len()))
                                    .weak(),
                            );
                        });
                    }
                    if !self.settings.configuration_pack.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "Applied: {}",
                                self.settings.configuration_pack
                            ))
                            .weak(),
                        );
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Intel channels").strong());
                        ui.label(
                            egui::RichText::new(format!("{} configured", self.settings.intel_channels.len()))
                                .weak(),
                        );
                    });
                    if ui.button("Configure intel channels…").clicked() {
                        self.intel_channels_open = true;
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Coalition data").strong());
                        ui.label(
                            egui::RichText::new(format!(
                                "{} bridges · {} upgrades",
                                self.settings.jump_bridges.len(),
                                self.settings.sov_upgrades.len()
                            ))
                            .weak(),
                        );
                    });
                    if ui.button("Configure jump bridges…").clicked() {
                        self.jump_bridges_open = true;
                    }
                    if ui.button("Configure sov upgrades…").clicked() {
                        self.sov_upgrades_open = true;
                    }
                    if ui.button("Configure coalitions…").clicked() {
                        self.coal_edit = self
                            .settings
                            .coalitions
                            .iter()
                            .map(|c| (c.name.clone(), c.alliances.join("\n")))
                            .collect();
                        self.coalitions_open = true;
                    }
                    if ui.button("Configure cyno generators…").clicked() {
                        self.cyno_generators_open = true;
                    }
                    #[cfg(feature = "fc-rescue")]
                    {
                        ui.add_space(12.0);
                        ui.separator();
                        changed |= self.rescue_settings_section(ui);
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.heading("About");
                    ui.label(format!("EVE Spai v{}", env!("CARGO_PKG_VERSION")));
                    ui.horizontal(|ui| {
                        ui.label("Project:");
                        ui.hyperlink_to(
                            "github.com/Amryu/eve-spai",
                            "https://github.com/Amryu/eve-spai",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Community:");
                        ui.hyperlink_to("Discord", "https://discord.gg/u4bDqB9rjn");
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(eve_portrait_url(2119400938_i64, 48.0))
                                .fit_to_exact_size(egui::Vec2::splat(48.0)),
                        );
                        ui.vertical(|ui| {
                            ui.label("Built by Amryu.");
                            ui.label(
                                egui::RichText::new(
                                    "If you find it useful, ISK donations to Amryu in-game are welcome.",
                                )
                                .weak(),
                            );
                        });
                    });
                });

        if let Some(theme) = new_theme {
            self.settings.theme = theme;
            changed = true;
        }
        if changed {
            self.needs_save = true;
        }
    }
}

impl SpaiApp {
}
