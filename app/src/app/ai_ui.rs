//! The Assistant tab: the conversation, its tool calls and the actions waiting for the user.

use super::*;
use crate::ai::session::{AiHandle, CardState, Command, Turn};
use crate::ai::links::{Link, Names};
use crate::ai::tools::ActionKind;

/// How often the tools' copy of the UI-thread state is refreshed when nothing else asks for it.
const FACTS_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

impl SpaiApp {
    pub(crate) fn ai_on(&self) -> bool {
        self.settings.ai.enabled
    }

    /// The session, started on first use. Headless builds get one with no thread behind it.
    pub(crate) fn ai_handle(&mut self, ctx: &egui::Context) -> AiHandle {
        if let Some(h) = &self.ai {
            return h.clone();
        }
        if !std::mem::replace(&mut self.ai_memories_loaded, true) {
            let loaded = crate::ai::memory::Memories::load(self.store.as_ref());
            let mut mems = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner());
            if !loaded.list.is_empty() || mems.list.is_empty() {
                *mems = loaded;
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let view: crate::ai::session::SharedView = Default::default();
        let cancel: std::sync::Arc<std::sync::atomic::AtomicBool> = Default::default();
        let handle = AiHandle { tx, view: view.clone(), cancel: cancel.clone() };
        self.ai_push_facts(true);
        if !self.headless {
            let deps = crate::ai::deps::AiDeps {
                intel_state: self.intel_state.clone(),
                player: self.player.clone(),
                system_status: self.system_status.clone(),
                jabber: self.jabber.clone(),
                killfeed: self.killfeed.clone(),
                battles: self.battles.clone(),
                camps: self.camps.clone(),
                rescue: self.rescue.clone(),
                fleet: self.fleet.clone(),
                lookup_table: self.lookup_table.clone(),
                facts: self.ai_facts.clone(),
                memories: self.ai_memories.clone(),
                watches: self.ai_watches.clone(),
                feeds: self.ai_feeds.clone(),
                opsec: self.ai_opsec.clone(),
                last_question: Default::default(),
                alerts: self.recent_alerts.clone(),
                standings: self.standings.clone(),
                jump_skills: self.jump_skills.clone(),
                online: true,
            };
            let secrets = self.ai_secrets.clone();
            let make: crate::ai::session::ProviderFactory = Box::new(move |f| crate::ai::session::provider_for(f, secrets.as_ref()));
            let session = crate::ai::session::Session::new(deps, view, cancel, make, Some(ctx.clone()));
            crate::ai::session::spawn(session, rx);
        }
        self.ai = Some(handle.clone());
        handle
    }

    /// Copies what the tools need from the UI thread, every couple of seconds or when `now`.
    pub(crate) fn ai_push_facts(&mut self, now: bool) {
        if !self.ai_on() {
            return;
        }
        if !now && self.ai_facts_at.is_some_and(|t| t.elapsed() < FACTS_EVERY) {
            return;
        }
        self.ai_facts_at = Some(std::time::Instant::now());
        let s = &self.settings;
        let facts = crate::ai::deps::AiFacts {
            systems: self.systems.clone(),
            perms: s.ai.perms.clone(),
            unlocked: crate::ai::perms::Unlocked { fleet: self.fleet_on(), rescue: self.rescue_on() },
            chat_dir: self.chat_dir.clone(),
            severity: s.severity.clone(),
            cyno_generators: s.cyno_generators.clone(),
            jump_bridges: s.jump_bridges.clone(),
            sov_upgrades: s.sov_upgrades.clone(),
            fleet_presets: s.fleet_presets.clone(),
            fleet_backend: self.fleet_on().then(|| self.fleet_backend.clone()),
            notes_view: Some(self.notes_view.clone()),
            ai: s.ai.clone(),
            lookup_current: self.lookup_current.clone(),
            alert_rules: s.alerts.rules.clone(),
            doctrines: self.ai_doctrine_facts(),
            route_anchors: self.map_route_anchors.clone(),
            route_destination: self.route_destination,
            last_dscan: self.ai_last_dscan(),
            comms: self.comms_directory(),
            setup: crate::ai::deps::Setup {
                staging: s.rescue_staging_system.clone(),
                capital: s.ansiblex_capital.clone(),
                avoid_gate: s.route_avoid_gate.clone(),
                avoid_jump: s.route_avoid_jump.clone(),
                avoid_sov: s.route_avoid_sov.clone(),
                sec: s.route_sec,
                routes: s.saved_map_routes.clone(),
            },
            coalitions: s.coalitions.clone(),
            chat_windows: self.jabber_popouts.iter().map(|w| (w.id, w.tabs.clone())).collect(),
            jabber_domain: s.jabber_jid.split('@').nth(1).unwrap_or("").split('/').next().unwrap_or("").to_owned(),
            opsec: false,
        };
        *self.ai_facts.lock().unwrap_or_else(|e| e.into_inner()) = facts;
        *self.ai_feed_defs.lock().unwrap_or_else(|e| e.into_inner()) = s.ai.feeds.clone();
        if !self.ai_feeds_started && !self.headless && !self.settings.ai.feeds.is_empty() {
            self.ai_feeds_started = true;
            {
                let loaded = crate::ai::feeds::FeedStore::load(self.store.as_ref());
                let mut st = self.ai_feeds.lock().unwrap_or_else(|e| e.into_inner());
                if st.items.is_empty() {
                    st.items = loaded.items;
                }
            }
            crate::ai::feeds::spawn(self.ai_feeds.clone(), self.ai_feed_defs.clone(), self.ai_secrets.clone(), self.ui_ctx.clone());
        }
    }

    /// Carries out an action card the user applied. Returns the note the model gets about it.
    pub(crate) fn ai_apply(&mut self, kind: &ActionKind, summary: &str) -> String {
        match kind {
            ActionKind::SendJabber { to, room, join, body, .. } => {
                let Some(tx) = self.jabber_tx.clone() else {
                    self.toast_error("Jabber is not connected; nothing was sent");
                    return format!("Not sent, Jabber is not connected: {summary}");
                };
                if *join {
                    // A room takes messages only once we are in it: the message waits for the join.
                    let _ = tx.send(crate::jabber::Cmd::JoinRoom { room: to.clone() });
                    self.ai_room_waits.push((to.clone(), body.clone(), std::time::Instant::now()));
                } else if *room {
                    let _ = tx.send(crate::jabber::Cmd::SendRoom { room: to.clone(), body: body.clone() });
                } else {
                    let _ = tx.send(crate::jabber::Cmd::Send { to: to.clone(), body: body.clone() });
                }
            }
            ActionKind::OpenChat { jid, window } => {
                use super::chat_tabs::ChatWinKey;
                use crate::ai::tools::ChatWindowPick;
                self.ai_push_facts(true);
                match window {
                    ChatWindowPick::Main => {
                        self.view = View::Jabber;
                        let owner = self.tab_set().owner(jid);
                        if owner != Some(ChatWinKey::Main) {
                            if owner.is_some() {
                                self.tab_set().move_tab(jid, ChatWinKey::Main, None);
                            }
                        }
                        self.jabber_open(jid, ChatWinKey::Main);
                    }
                    ChatWindowPick::New => {
                        if self.tab_set().owner(jid).is_none() {
                            self.tab_set().attach(jid, ChatWinKey::Main, None);
                        }
                        match self.new_popout(jid, None) {
                            Some(id) => self.raise_popout(id),
                            None => {
                                self.jabber_open(jid, ChatWinKey::Main);
                                self.view = View::Jabber;
                                return format!("No room for another chat window; opened in the Jabber tab instead: {summary}");
                            }
                        }
                    }
                    ChatWindowPick::Existing(id) => {
                        if self.tab_set().owner(jid).is_some() {
                            self.tab_set().move_tab(jid, ChatWinKey::Popout(*id), None);
                        } else {
                            self.tab_set().attach(jid, ChatWinKey::Popout(*id), None);
                        }
                        self.win_set_active(ChatWinKey::Popout(*id), Some(jid.clone()));
                        self.raise_popout(*id);
                    }
                }
            }
            // Through the Fleet tab's own way in: a mumble:// link goes straight to Mumble, a short link
            // is resolved first and only opened in the browser when that fails.
            ActionKind::JoinMumble { url } if url.starts_with("mumble://") => crate::mumble::open_url(url),
            ActionKind::JoinMumble { url } => self.comms_join(crate::fleets::comms::Links { mumble: None, short: Some(url.clone()) }),
            ActionKind::KeepWatching(id) => {
                let now = crate::clock::utc().timestamp();
                if let Some(w) = self.ai_watches.lock().unwrap_or_else(|e| e.into_inner()).iter_mut().find(|w| w.id == *id) {
                    w.resume(now);
                }
            }
            ActionKind::Highlight(ids) => {
                self.ai_highlight = ids.clone();
                self.view = View::Map;
            }
            ActionKind::Focus(id) => {
                self.focus_map_on_select(*id);
                self.view = View::Map;
            }
            ActionKind::PlanRoute { from, to } => {
                self.map_set_route_mode("gate");
                self.map_route_anchors = vec![*from, *to];
                self.map_replan_route();
                self.view = View::Map;
            }
            ActionKind::SetDestination { system, character } => {
                let who = character.clone().unwrap_or_else(|| self.active_character.clone());
                self.set_destination_for(&[who], *system);
            }
            ActionKind::EditMapData(e) => {
                let s = &mut self.settings;
                if e.apply(&mut s.jump_bridges, &mut s.cyno_generators, &mut s.sov_upgrades) {
                    self.needs_save = true;
                }
            }
            ActionKind::AddAlertRule(rule) => {
                self.settings.alerts.rules.push((**rule).clone());
                crate::settings::ensure_rule_ids(&mut self.settings.alerts.rules);
                self.needs_save = true;
            }
        }
        format!("The user applied: {summary}")
    }

    /// The Assistant tab: an introduction while the assistant is off, a note while it is in its own
    /// window, else the conversation.
    pub(crate) fn assistant_view(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        if !self.ai_on() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(520.0);
                ui.label(egui::RichText::new(format!("{}  Assistant", icon::SPARKLE)).heading());
                ui.add_space(8.0);
                ui.label(
                    "Ask about intel, kills, routes, wormholes and fleets in plain words, typed or spoken while you play. \
                     It reads only the data you allow, can keep watch for you, and every change it proposes waits for your click.",
                );
                ui.add_space(4.0);
                ui.label(egui::RichText::new("It needs a model service: an API key, a local model, or a Claude or ChatGPT subscription.").weak());
                ui.add_space(12.0);
                if ui.button(format!("{}  Set it up in Settings", icon::GEAR_SIX)).clicked() {
                    self.view = View::Settings;
                    self.settings_scroll_to_ai = true;
                }
            });
            return;
        }
        if self.settings.ai.popped && !self.ai_in_window {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("The assistant is in its own window.").weak());
            if ui.button(format!("{}  Dock it here", icon::ARROW_SQUARE_IN)).clicked() {
                self.settings.ai.popped = false;
                self.needs_save = true;
            }
            return;
        }
        let handle = self.ai_handle(ui.ctx());
        self.ai_push_facts(false);
        let (turns, busy, usage) = {
            let v = handle.view.lock().unwrap_or_else(|e| e.into_inner());
            (v.turns.clone(), v.busy, v.usage)
        };
        let (model, _) = crate::ai::session::model_of(&self.ai_facts.lock().unwrap_or_else(|e| e.into_inner()));
        let provider = self.settings.ai.provider.label();

        egui::Panel::top("ai_top").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner()).list.len();
                    if self.ai_in_window {
                        super::ontop_pin_ui(ui, "ai_window");
                        if ui.button(icon::ARROW_SQUARE_IN).on_hover_text("Back into the main window").clicked() {
                            self.settings.ai.popped = false;
                            self.needs_save = true;
                        }
                    } else if ui.button(icon::ARROW_SQUARE_OUT).on_hover_text("Into its own window, over the game").clicked() {
                        self.settings.ai.popped = true;
                        self.ai_geom_applied = false;
                        self.needs_save = true;
                    }
                    if ui
                        .add(egui::Button::new(format!("{}  Memories ({n})", icon::BRAIN)).selected(self.ai_memories_open))
                        .on_hover_text("What the assistant remembers between conversations")
                        .clicked()
                    {
                        self.ai_memories_open = !self.ai_memories_open;
                    }
                    if !matches!(self.settings.ai.voice.tts, crate::ai::config::TtsKind::Off | crate::ai::config::TtsKind::Unknown) {
                        let speaking = self.ai_speaker.as_ref().is_some_and(|s| s.speaking.load(std::sync::atomic::Ordering::Relaxed));
                        let on = self.settings.ai.voice.speak_replies;
                        let (glyph, tip) = match (speaking, on) {
                            (true, _) => (icon::STOP, "Stop speaking"),
                            (false, true) => (icon::SPEAKER_HIGH, "Answers are spoken; click to turn that off"),
                            (false, false) => (icon::SPEAKER_SLASH, "Answers are not spoken; click to speak them"),
                        };
                        if ui.button(glyph).on_hover_text(tip).clicked() {
                            if speaking {
                                self.ai_voice_stop();
                            } else {
                                self.settings.ai.voice.speak_replies = !on;
                                self.needs_save = true;
                            }
                        }
                        if speaking {
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
                        }
                    }
                    if ui.button(format!("{}  Data access\u{2026}", icon::KEY)).on_hover_text("What the assistant may read and do").clicked() {
                        self.ai_perms_open = true;
                    }
                    if ui.add_enabled(!turns.is_empty(), egui::Button::new(format!("{}  New chat", icon::PLUS))).clicked() {
                        handle.send(Command::NewChat);
                    }
                    let used = format!("{} in \u{00b7} {} out", fmt_count(usage.input as i64), fmt_count(usage.output as i64));
                    ui.label(egui::RichText::new(used).weak()).on_hover_text(format!("Tokens this session; {} read from cache", fmt_count(usage.cached as i64)));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let _ = (&model, provider);
                        self.ai_model_picker(ui);
                    });
                });
            });
            ui.add_space(4.0);
        });

        let mut send: Option<String> = None;
        // The tab already sits on the window's bottom margin, so the room below the field is that.
        let input_frame = egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(egui::Margin { left: 8, right: 8, top: 8, bottom: 0 });
        egui::Panel::bottom("ai_input").frame(input_frame).show_inside(ui, |ui| {
            // The field's height, so the buttons beside it centre on it rather than its top.
            // Last frame's: the field grows with the text, and the buttons follow its middle.
            let h_id = egui::Id::new("ai_input_h");
            // No spacing after the row: the panel's margin is all the room below it.
            ui.spacing_mut().item_spacing.y = 0.0;
            let field_h = ui.data(|d| d.get_temp::<f32>(h_id)).unwrap_or(2.0 * ui.text_style_height(&egui::TextStyle::Body) + 4.0);
            {
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), field_h), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if busy {
                        if ui.button(format!("{}  Stop", icon::STOP)).clicked() {
                            handle.stop();
                            self.ai_voice_stop();
                        }
                    } else if ui
                        .add_enabled(!self.ai_input.trim().is_empty(), egui::Button::new(format!("{}  Ask", icon::PAPER_PLANE_RIGHT)))
                        .clicked()
                    {
                        send = Some(std::mem::take(&mut self.ai_input));
                    }
                    if self.ai_stt_on() {
                        let rec = self.ai_listen.recording();
                        let glyph = if self.ai_listen.transcribing() { icon::DOTS_THREE } else { icon::MICROPHONE };
                        let mut b = egui::Button::new(egui::RichText::new(glyph).color(if rec { crate::theme::standing::HOSTILE } else { ui.visuals().text_color() }))
                            .sense(egui::Sense::click_and_drag());
                        if rec {
                            b = b.selected(true);
                        }
                        let tip = match self.settings.ai.voice.ptt.as_ref().filter(|_| crate::ai::ptt::SUPPORTED) {
                            Some(k) => format!("Hold to talk, or hold {} anywhere", k.label),
                            None => "Hold to talk".to_owned(),
                        };
                        let r = ui.add(b).on_hover_text(tip);
                        let held = r.is_pointer_button_down_on();
                        if held && !rec {
                            self.ai_listen_start(super::ai_voice_in::Source::Button);
                        } else if !held && rec {
                            self.ai_listen_stop(super::ai_voice_in::Source::Button);
                        }
                    }
                    let edit = egui::TextEdit::multiline(&mut self.ai_input)
                        .id(egui::Id::new("ai_input"))
                        .desired_rows(2)
                        .desired_width(ui.available_width())
                        .hint_text("Ask about intel, kills, routes, wormholes\u{2026} (Enter sends, Shift+Enter for a new line)");
                    let r = ui.add(edit);
                    if (r.rect.height() - field_h).abs() > 0.5 {
                        ui.data_mut(|d| d.insert_temp(h_id, r.rect.height()));
                        ui.ctx().request_repaint();
                    }
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                    if enter && !busy && !self.ai_input.trim().is_empty() {
                        send = Some(std::mem::take(&mut self.ai_input).trim_end_matches('\n').to_owned());
                        ui.ctx().memory_mut(|m| m.request_focus(r.id));
                    }
                });
            }
        });

        if self.ai_memories_open {
            // Never more than half the tab, so the conversation keeps its room in a small window.
            let max = (ui.available_width() * 0.5).max(220.0);
            egui::Panel::right("ai_memories")
                .resizable(true)
                .default_size(320.0_f32.min(max))
                .size_range(220.0..=max.max(221.0))
                .show_inside(ui, |ui| self.ai_memories_ui(ui));
        }

        let mut card_click: Option<(u64, u8)> = None;
        let mut link_click: Option<Link> = None;
        if self.ai_ship_names.0 != self.ship_by_id.len() {
            self.ai_ship_names = (self.ship_by_id.len(), self.ship_by_id.iter().map(|(id, n)| (n.clone(), *id)).collect());
        }
        let systems = self.systems.clone();
        let ship_names = std::mem::take(&mut self.ai_ship_names);
        let names = AppNames { systems: systems.as_deref(), ships: &ship_names.1 };
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6))).show_inside(ui, |ui| {
            self.ai_watch_strip(ui);
            if turns.is_empty() {
                self.ai_empty_state(ui);
                return;
            }
            // One width for every turn, measured outside the scroll area and short of its bar, so
            // frames and wrapped lines all end at the same edge.
            let w = (ui.available_width() - 28.0).max(120.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                for t in &turns {
                    ui.allocate_ui_with_layout(egui::vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(w);
                        if let Some(c) = turn_ui(ui, t, w, &names, &mut link_click) {
                            card_click = Some(c);
                        }
                    });
                    ui.add_space(8.0);
                }
            });
        });

        self.ai_ship_names = ship_names;
        if let Some(l) = link_click {
            self.ai_open_link(l, ui.ctx());
        }
        if let Some(text) = send.filter(|t| !t.trim().is_empty()) {
            handle.send(Command::Send { text, voice: false });
        }
        if let Some((id, how)) = card_click {
            let applied = how > 0;
            if how == 2 {
                let key = turns.iter().flat_map(|t| t.cards.iter()).find(|c| c.action.id == id).and_then(|c| c.action.kind.perm_key());
                if let Some(k) = key {
                    if !self.settings.ai.auto_actions.iter().any(|a| a == k) {
                        self.settings.ai.auto_actions.push(k.to_owned());
                        self.needs_save = true;
                        self.ai_push_facts(true);
                    }
                }
            }
            let card = turns.iter().flat_map(|t| t.cards.iter()).find(|c| c.action.id == id).cloned();
            if let Some(card) = card {
                if let (false, ActionKind::KeepWatching(wid)) = (applied, &card.action.kind) {
                    if let Some(w) = self.ai_watches.lock().unwrap_or_else(|e| e.into_inner()).iter_mut().find(|w| w.id == *wid) {
                        w.stop("stopped by you");
                    }
                }
                let note = if applied { self.ai_apply(&card.action.kind, &card.action.summary) } else { format!("The user dismissed: {}", card.action.summary) };
                handle.send(Command::ActionResult { id, applied, note });
            }
        }
    }

    /// The doctrines set up for fleets, by setup, with their hulls, tank, fit link and ping line.
    fn ai_doctrine_facts(&self) -> Vec<crate::ai::deps::DoctrineFacts> {
        let s = &self.settings;
        let mut ids: Vec<i32> = s.fleet_hulls.iter().map(|h| h.setup_id).chain(s.fleet_doctrine_urls.iter().map(|x| x.0)).chain(s.fleet_doctrine_tanks.iter().map(|x| x.0)).filter(|id| *id != 0).collect();
        ids.sort_unstable();
        ids.dedup();
        let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
        ids.into_iter()
            .map(|id| crate::ai::deps::DoctrineFacts {
                name: st.seed.setup_name(crate::fleets::model::SetupId(id)).map(str::to_owned).unwrap_or_else(|| format!("setup {id}")),
                main: s.fleet_hulls.iter().filter(|h| h.setup_id == id && h.main).map(|h| h.name.clone()).collect(),
                support: s.fleet_hulls.iter().filter(|h| (h.setup_id == id || h.setup_id == 0) && !h.main).map(|h| h.name.clone()).collect(),
                tank: s.fleet_doctrine_tanks.iter().find(|x| x.0 == id).map(|x| x.1.clone()),
                url: s.fleet_doctrine_urls.iter().find(|x| x.0 == id).map(|x| x.1.clone()).or_else(|| {
                    st.seed.setup_name(crate::fleets::model::SetupId(id)).and_then(crate::doctrines::link_for).map(str::to_owned)
                }),
                strict: s.fleet_doctrine_strict.contains(&id),
                boosts: s.fleet_boost_requirements.iter().filter(|b| b.setup_id == id).map(|b| (b.charge.clone(), b.priority.clone())).collect(),
                line: s.fleet_doctrine_lines.iter().find(|x| x.0 == id).map(|x| x.1.clone()),
            })
            .collect()
    }

    /// Carries out the actions that need no click as soon as they arrive.
    pub(crate) fn ai_immediate_actions(&mut self) {
        let Some(h) = self.ai.clone() else { return };
        let due: Vec<(u64, ActionKind, String)> = {
            let mut v = h.view.lock().unwrap_or_else(|e| e.into_inner());
            let mut out = Vec::new();
            for c in v.turns.iter_mut().flat_map(|t| t.cards.iter_mut()) {
                if c.state == CardState::Pending && c.action.kind.immediate(&self.settings.ai.auto_actions) {
                    c.state = CardState::Applied;
                    out.push((c.action.id, c.action.kind.clone(), c.action.summary.clone()));
                }
            }
            out
        };
        for (id, kind, summary) in due {
            let note = self.ai_apply(&kind, &summary);
            h.send(Command::ActionResult { id, applied: true, note });
        }
    }

    /// Sends messages that waited for a room to be joined, once it is; gives up after half a minute.
    pub(crate) fn ai_room_waits_tick(&mut self) {
        if self.ai_room_waits.is_empty() {
            return;
        }
        let joined: std::collections::BTreeSet<String> = self.jabber.lock().unwrap_or_else(|e| e.into_inner()).rooms.clone();
        let mut failed = Vec::new();
        let tx = self.jabber_tx.clone();
        self.ai_room_waits.retain(|(room, body, at)| {
            if joined.contains(room) {
                if let Some(tx) = &tx {
                    let _ = tx.send(crate::jabber::Cmd::SendRoom { room: room.clone(), body: body.clone() });
                }
                false
            } else if at.elapsed() > std::time::Duration::from_secs(30) {
                failed.push(room.clone());
                false
            } else {
                true
            }
        });
        for r in failed {
            self.toast_error(format!("Could not join {r}; the message was not sent"));
        }
        self.ui_ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// Watch news: spoken when answers are, and a toast when the user is not looking at the chat.
    pub(crate) fn ai_watch_news(&mut self) {
        let Some(h) = &self.ai else { return };
        let news = std::mem::take(&mut h.view.lock().unwrap_or_else(|e| e.into_inner()).watch_news);
        for n in news {
            if self.settings.ai.voice.speak_replies {
                if let Some(sp) = self.ai_voice() {
                    sp.say(crate::ai::voice::sentences::speakable(&n));
                }
            }
            if self.view != View::Assistant {
                self.toast(format!("{}  {n}", egui_phosphor::regular::BINOCULARS));
            }
        }
    }

    /// The voice as the settings and keychain have it now.
    fn ai_voice_cfg(&self) -> crate::ai::voice::speaker::VoiceCfg {
        let v = &self.settings.ai.voice;
        crate::ai::voice::speaker::VoiceCfg {
            kind: v.tts,
            openai_key: (v.tts == crate::ai::config::TtsKind::Openai).then(|| self.ai_secrets.get("openai:openai")).flatten(),
            openai_voice: v.cloud_voice.clone(),
            elevenlabs_key: (v.tts == crate::ai::config::TtsKind::Elevenlabs).then(|| self.ai_secrets.get("elevenlabs")).flatten(),
            elevenlabs_voice: v.elevenlabs_voice.clone(),
            piper_voices: v.piper_voices.clone(),
            volume: v.volume,
            language: self.settings.ai.language.clone(),
        }
    }

    /// The speaker, started on first use and kept in step with the settings. None when speech is
    /// off, and in a headless build.
    fn ai_voice(&mut self) -> Option<crate::ai::voice::speaker::Speaker> {
        if self.headless || matches!(self.settings.ai.voice.tts, crate::ai::config::TtsKind::Off | crate::ai::config::TtsKind::Unknown) {
            return None;
        }
        let stale = self.ai_voice_cfg_at.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(5));
        if self.ai_speaker.is_none() {
            self.ai_speaker = Some(crate::ai::voice::speaker::Speaker::spawn(self.ai_voice_cfg()));
            self.ai_voice_cfg_at = Some(std::time::Instant::now());
        } else if stale {
            let cfg = self.ai_voice_cfg();
            if let Some(sp) = &self.ai_speaker {
                sp.configure(cfg);
            }
            self.ai_voice_cfg_at = Some(std::time::Instant::now());
        }
        self.ai_speaker.clone()
    }

    /// Stops speaking at once.
    pub(crate) fn ai_voice_stop(&mut self) {
        if let Some(sp) = &self.ai_speaker {
            sp.stop();
        }
    }

    pub(crate) fn ai_voice_test(&mut self, lang: &str) {
        let line = match lang {
            "de" => "Die Frat-Gang ist in QX-LIJ, sechs Sprünge von dir, vor vier Minuten gesehen.",
            "es" => "La flota de Frat está en QX-LIJ, a seis saltos de ti, vista hace cuatro minutos.",
            "ru" => "Флот Frat в QX-LIJ, в шести прыжках от тебя, замечен четыре минуты назад.",
            "zh" => "Frat舰队在QX-LIJ，离你六跳，四分钟前出现。",
            _ => "The Frat gang is in QX-LIJ, six jumps from you, seen four minutes ago.",
        };
        self.ai_voice_cfg_at = None;
        if let Some(sp) = self.ai_voice() {
            sp.stop();
            sp.say(line.to_owned());
        }
    }

    /// Feeds the answer being written to the voice, sentence by sentence, when it is to be spoken.
    pub(crate) fn ai_voice_tick(&mut self) {
        let Some(h) = self.ai.clone() else { return };
        if let Some(e) = self.ai_speaker.as_ref().and_then(|s| s.error.lock().unwrap_or_else(|e| e.into_inner()).take()) {
            self.toast_error(format!("Voice: {e}"));
        }
        let (turn, text, voice_turn, finished) = {
            let mut v = h.view.lock().unwrap_or_else(|e| e.into_inner());
            let Some(i) = v.speak_turn else { return };
            let text = std::mem::take(&mut v.speak);
            let t = v.turns.get(i);
            (i, text, t.is_some_and(|t| t.voice), !v.busy && t.is_none_or(|t| !t.streaming))
        };
        if self.ai_speak_turn.map(|(i, _)| i) != Some(turn) {
            // A new answer cuts off whatever was still being said.
            self.ai_voice_stop();
            self.ai_chunker = Default::default();
            self.ai_speak_turn = Some((turn, false));
        }
        if !(self.settings.ai.voice.speak_replies || voice_turn) {
            return;
        }
        let Some(sp) = self.ai_voice() else { return };
        for s in self.ai_chunker.push(&text) {
            sp.say(s);
        }
        if finished && self.ai_speak_turn.is_some_and(|(_, done)| !done) {
            if let Some(rest) = self.ai_chunker.finish() {
                sp.say(rest);
            }
            self.ai_speak_turn = Some((turn, true));
        }
    }

    /// The watches, one line each, with stop or resume. Nothing when there are none.
    fn ai_watch_strip(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let now = crate::clock::utc().timestamp();
        let list: Vec<(u64, String, String, bool, String)> = self
            .ai_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|w| (w.id, w.goal.clone(), w.status(now), w.running(), w.stopped_why().map(|y| format!("Stopped: {y}")).unwrap_or_default()))
            .collect();
        if list.is_empty() {
            return;
        }
        let mut act: Option<(u64, u8)> = None;
        for (id, goal, status, running, why) in &list {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !running && ui.button(icon::X).on_hover_text("Remove").clicked() {
                        act = Some((*id, 2));
                    }
                    if *running {
                        if ui.button(format!("{}  Stop", icon::STOP)).clicked() {
                            act = Some((*id, 0));
                        }
                    } else if ui.button(format!("{}  Resume", icon::PLAY)).clicked() {
                        act = Some((*id, 1));
                    }
                    let s = ui.label(egui::RichText::new(status).weak());
                    if !why.is_empty() {
                        s.on_hover_text(why);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let col = if *running { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
                        ui.add(egui::Label::new(egui::RichText::new(format!("{}  {goal}", icon::BINOCULARS)).color(col)).truncate()).on_hover_text(goal);
                    });
                });
            });
        }
        if let Some((id, what)) = act {
            let mut ws = self.ai_watches.lock().unwrap_or_else(|e| e.into_inner());
            match what {
                0 => ws.iter_mut().filter(|w| w.id == id).for_each(|w| w.stop("stopped by you")),
                1 => ws.iter_mut().filter(|w| w.id == id).for_each(|w| w.resume(now)),
                _ => ws.retain(|w| w.id != id),
            }
        }
        ui.separator();
    }

    /// The model in use, picked in the tab: the service's models, then the other services.
    fn ai_model_picker(&mut self, ui: &mut egui::Ui) {
        use crate::ai::config::{model_choices, ProviderKind};
        use egui_phosphor::regular as icon;
        let a = &self.settings.ai;
        let provider = a.provider;
        let cur = match provider {
            ProviderKind::Anthropic => a.anthropic.model.clone(),
            ProviderKind::OpenaiCompat => a.openai.model.clone(),
            ProviderKind::Gemini => a.gemini.model.clone(),
            ProviderKind::ClaudeCli => a.claude_cli.model.clone(),
            ProviderKind::CodexCli => a.codex_cli.model.clone(),
            ProviderKind::Unknown => String::new(),
        };
        let choices = model_choices(provider);
        let label = choices.iter().find(|(k, _)| *k == cur).map(|(_, l)| (*l).to_owned()).unwrap_or_else(|| if cur.is_empty() { "default model".into() } else { cur.clone() });
        let shown = format!("{}  {} \u{b7} {label}", icon::SPARKLE, provider.label());
        // The server's own list, asked for once per address.
        let mut fetched: Vec<String> = Vec::new();
        if provider == ProviderKind::OpenaiCompat {
            let base = a.openai.base();
            let mut st = self.ai_oa_models.lock().unwrap_or_else(|e| e.into_inner());
            if st.0 != base && !base.is_empty() {
                st.0 = base.clone();
                st.1 = None;
                let key = self.ai_secrets.get(&a.openai.preset.key_account());
                let slot = self.ai_oa_models.clone();
                let ctx = ui.ctx().clone();
                let _ = std::thread::Builder::new().name("ai-models".into()).spawn(move || {
                    let got = crate::ai::openai_compat::list_models(&base, key.as_deref()).unwrap_or_default();
                    let mut s = slot.lock().unwrap_or_else(|e| e.into_inner());
                    if s.0 == base {
                        s.1 = Some(got);
                    }
                    ctx.request_repaint();
                });
            }
            fetched = st.1.clone().unwrap_or_default();
        }
        let mut changed = false;
        let width = ui.available_width().clamp(160.0, 340.0);
        egui::ComboBox::from_id_salt("ai_tab_model").selected_text(shown).width(width).truncate().height(420.0).show_ui(ui, |ui| {
            let a = &mut self.settings.ai;
            if let Some(m) = a.model_mut() {
                for &(k, l) in choices {
                    changed |= ui.menu_value(m, k.to_owned(), l).changed();
                }
                for id in &fetched {
                    changed |= ui.menu_value(m, id.clone(), id).changed();
                }
                if !cur.is_empty() && !choices.iter().any(|(k, _)| *k == cur) && !fetched.contains(&cur) {
                    changed |= ui.menu_value(m, cur.clone(), &cur).changed();
                }
            }
            if provider == ProviderKind::OpenaiCompat && fetched.is_empty() {
                ui.label(egui::RichText::new("The server listed no models; type one in Settings").weak());
            }
            ui.separator();
            ui.label(egui::RichText::new("Service").weak());
            for p in ProviderKind::CHOICES {
                changed |= ui.menu_value(&mut a.provider, p, p.label()).changed();
            }
        });
        if changed {
            self.needs_save = true;
            self.ai_push_facts(true);
        }
    }

    /// The Assistant tab in its own window, kept over the game.
    #[allow(deprecated)]
    pub(crate) fn ai_popout_window(&mut self, ctx: &egui::Context) {
        let mut builder = egui::ViewportBuilder::default()
            .with_icon(super::app_icon())
            .with_title("EVE Spai - Assistant")
            .with_min_inner_size([420.0, 360.0])
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        if !self.ai_geom_applied {
            let (w, h) = self.settings.ai.popout_size.unwrap_or((560.0, 680.0));
            builder = builder.with_inner_size([w, h]);
            if let Some((x, y)) = self.settings.ai.popout_pos {
                builder = builder.with_position([x, y]);
            }
            self.ai_pos_fix = super::alert_window::PosFix::new(self.settings.ai.popout_pos);
            self.ai_geom_applied = true;
        }
        let mut keep = true;
        let mut geom: Option<((f32, f32), Option<(f32, f32)>)> = None;
        let mut pos_fix = self.ai_pos_fix;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("ai_window"), builder, |ctx, _| {
            super::alert_window::apply_pos_fix(ctx, &mut pos_fix);
            self.ai_in_window = true;
            egui::CentralPanel::default().frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill)).show(ctx, |ui| self.assistant_view(ui));
            self.ai_in_window = false;
            let sz = ctx.content_rect().size();
            if sz.x > 100.0 && sz.y > 100.0 {
                geom = Some(((sz.x, sz.y), ctx.input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y)))));
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                keep = false;
            }
        });
        self.ai_pos_fix = pos_fix;
        if let Some((sz, pos)) = geom.filter(|_| pos_fix.is_none()) {
            if let Some(s) = super::alert_window::geometry_update(self.settings.ai.popout_size, sz, 2.0) {
                self.settings.ai.popout_size = Some(s);
                self.needs_save = true;
            }
            if let Some(p) = pos.and_then(|p| super::alert_window::geometry_update(self.settings.ai.popout_pos, p, 1.0)) {
                self.settings.ai.popout_pos = Some(p);
                self.needs_save = true;
            }
        }
        if !keep {
            self.settings.ai.popped = false;
            self.ai_geom_applied = false;
            self.needs_save = true;
        }
    }

    fn ai_open_link(&mut self, l: Link, ctx: &egui::Context) {
        match l {
            Link::System(id) => self.open_system(id),
            Link::Ship(id) => self.open_ship(id),
            Link::Pilot(name) => self.open_pilot(name, ctx),
            Link::Kill(id) => self.open_killmail(id, None),
            Link::Battle(kid) => {
                self.battle_select_pending = Some((vec![kid], std::time::Instant::now()));
                self.view = View::Battles;
            }
            Link::Fleet(id) if self.fleet_on() => self.fleet_show(crate::fleets::model::FleetId(id)),
            Link::Fleet(_) => {}
            Link::Chat(jid) => self.jabber_open(&jid, super::chat_tabs::ChatWinKey::Main),
            Link::Pings => self.jabber_open(crate::jabber::PING_FEED_KEY, super::chat_tabs::ChatWinKey::Main),
            Link::Wormholes(id) => {
                self.wh_info = Some(id);
                self.view = View::Wormholes;
            }
            // A link the model wrote could carry what it read out in its address.
            Link::Url(_) if self.ai_opsec.load(std::sync::atomic::Ordering::Relaxed) => {
                self.toast("Web links are off in a conversation that read Jabber or feed messages; start a new chat");
            }
            Link::Url(u) => ctx.open_url(egui::OpenUrl::new_tab(u)),
            Link::Page(p) => {
                let v = match p.as_str() {
                    "overview" => Some(View::Dashboard),
                    "map" => Some(View::Map),
                    "wormholes" => Some(View::Wormholes),
                    "intel" => Some(View::Intel),
                    "alerts" => Some(View::Alerts),
                    "battles" => Some(View::Battles),
                    "lookup" => Some(View::Lookup),
                    "characters" => Some(View::Characters),
                    "jabber" => Some(View::Jabber),
                    "fleet" if self.fleet_on() => Some(View::Fleet),
                    "rescue" if self.rescue_on() => Some(View::Rescue),
                    "settings" => Some(View::Settings),
                    _ => None,
                };
                if let Some(v) = v {
                    self.view = v;
                }
            }
        }
    }

    fn ai_empty_state(&mut self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(520.0);
            ui.label(egui::RichText::new("Ask about what is going on around you.").heading());
            ui.add_space(8.0);
            for ex in ["Where did the Frat gang go?", "Anything hostile within 5 jumps of me?", "Fastest safe route to Jita?", "Which wormholes are near 1DQ1-A?"] {
                if ui.link(ex).clicked() {
                    self.ai_input = ex.to_owned();
                }
            }
            ui.add_space(12.0);
            let shown = self.settings.ai.perms.values().filter(|v| **v).count();
            if shown == 0 {
                ui.label(egui::RichText::new("It can only read static game data until you give it access to more.").weak());
                if ui.button(format!("{}  Data access\u{2026}", egui_phosphor::regular::KEY)).clicked() {
                    self.ai_perms_open = true;
                }
            }
        });
    }
}

/// One turn. Returns an action card's Apply (true) or Dismiss (false), by the card's id.
/// A card clicked: its id and 0 dismiss, 1 apply, 2 apply and never ask again for its kind.
fn turn_ui(ui: &mut egui::Ui, t: &Turn, w: f32, names: &dyn Names, link: &mut Option<Link>) -> Option<(u64, u8)> {
    use egui_phosphor::regular as icon;
    let mut click = None;
    if t.user {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if t.voice {
                    ui.label(egui::RichText::new(icon::MICROPHONE).weak());
                }
                ui.add(egui::Label::new(&t.text).wrap());
            });
        return None;
    }
    if !t.chips.is_empty() {
        // Rows filled by measured width: a frame cannot wrap inside `horizontal_wrapped`.
        let font = egui::TextStyle::Body.resolve(ui.style());
        let gap = 4.0;
        let mut rows: Vec<Vec<(&crate::ai::session::Chip, String, f32)>> = vec![Vec::new()];
        let mut used = 0.0;
        for c in &t.chips {
            let action = crate::ai::tools::kind_of(&c.name) == Some(crate::ai::tools::Kind::Action);
            let glyph = match &c.error {
                Some(_) => icon::WARNING,
                None if action => icon::PLAY,
                None => icon::MAGNIFYING_GLASS,
            };
            let text = format!("{glyph} {}", c.name.replace('_', " "));
            let cw = ui.painter().layout_no_wrap(text.clone(), font.clone(), egui::Color32::WHITE).size().x + 14.0;
            if used > 0.0 && used + cw > w {
                rows.push(Vec::new());
                used = 0.0;
            }
            used += cw + gap;
            rows.last_mut().unwrap().push((c, text, cw));
        }
        for row in rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (c, text, _) in row {
                    let action = crate::ai::tools::kind_of(&c.name) == Some(crate::ai::tools::Kind::Action);
                    let col = match &c.error {
                        Some(_) => crate::theme::standing::WARNING,
                        None if action => ui.visuals().hyperlink_color,
                        None => ui.visuals().weak_text_color(),
                    };
                    let r = egui::Frame::new()
                        .stroke(egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .show(ui, |ui| ui.add(egui::Label::new(egui::RichText::new(text).color(col)).extend()))
                        .response;
                    let mut tip = if c.args.is_empty() { "No filters".to_owned() } else { c.args.clone() };
                    if let Some(e) = &c.error {
                        tip.push_str(&format!("\n{e}"));
                    }
                    r.on_hover_text(tip);
                }
            });
        }
        ui.add_space(4.0);
    }
    if t.watch.is_some() {
        ui.label(egui::RichText::new(format!("{}  Watch", icon::BINOCULARS)).weak());
    }
    if !t.text.is_empty() {
        if let Some(l) = render_text(ui, &t.text, w, names) {
            *link = Some(l);
        }
    }
    if t.streaming && t.text.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("Thinking\u{2026}").weak());
        });
    } else if t.streaming {
        ui.spinner();
    }
    for c in &t.cards {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            let inner = w - 16.0;
            ui.set_width(inner);
            ui.allocate_ui_with_layout(egui::vec2(inner, 0.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                {
                    match c.state {
                        CardState::Pending => {
                            let watch = matches!(c.action.kind, ActionKind::KeepWatching(_));
                            if c.action.kind.perm_key().is_some()
                                && ui.button("Always").on_hover_text("Apply, and do this kind of thing without asking from now on (Data access can take it back)").clicked()
                            {
                                click = Some((c.action.id, 2));
                            }
                            if ui.button(if watch { "Stop" } else { "Dismiss" }).clicked() {
                                click = Some((c.action.id, 0));
                            }
                            let yes = if watch { format!("{}  Keep watching", icon::BINOCULARS) } else { format!("{}  Apply", icon::CHECK) };
                            if ui.button(yes).clicked() {
                                click = Some((c.action.id, 1));
                            }
                        }
                        CardState::Applied => {
                            ui.label(egui::RichText::new(format!("{}  Applied", icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        }
                        CardState::Dismissed => {
                            ui.label(egui::RichText::new("Dismissed").weak());
                        }
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if !matches!(c.action.kind, ActionKind::KeepWatching(_)) {
                            ui.add(egui::Label::new(&c.action.summary).wrap());
                        }
                    });
                }
            });
        });
    }
    if let Some(e) = &t.error {
        ui.label(egui::RichText::new(format!("{}  {e}", icon::WARNING)).color(crate::theme::standing::WARNING));
    }
    click
}

/// The model's text with the little markdown it uses (headings, bullets, **bold**), and its
/// systems, ships and the things it linked made clickable. Returns the link clicked.
fn render_text(ui: &mut egui::Ui, text: &str, w: f32, names: &dyn Names) -> Option<Link> {
    let mut clicked = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            ui.add_space(4.0);
            continue;
        }
        let (heading, body) = match trimmed.strip_prefix("### ").or_else(|| trimmed.strip_prefix("## ")).or_else(|| trimmed.strip_prefix("# ")) {
            Some(h) => (true, h),
            None => (false, trimmed),
        };
        let (bullet, body) = match body.strip_prefix("- ").or_else(|| body.strip_prefix("* ")) {
            Some(b) if !heading => (true, b),
            _ => (false, body),
        };
        let font = egui::TextStyle::Body.resolve(ui.style());
        let normal = ui.visuals().text_color();
        let strong = ui.visuals().strong_text_color();
        let link_col = ui.visuals().hyperlink_color;
        let mut job = egui::text::LayoutJob::default();
        if bullet {
            job.append("\u{2022}  ", 0.0, egui::TextFormat::simple(font.clone(), normal));
        }
        // Char ranges of the links, for finding the one under the pointer.
        let mut ranges: Vec<(std::ops::Range<usize>, Link)> = Vec::new();
        let mut chars = job.text.chars().count();
        for sp in crate::ai::links::spans(body, names) {
            let n = sp.text.chars().count();
            let mut fmt = egui::TextFormat::simple(font.clone(), if sp.bold || heading { strong } else { normal });
            if let Some(l) = sp.link {
                fmt.color = link_col;
                ranges.push((chars..chars + n, l));
            }
            job.append(&sp.text, 0.0, fmt);
            chars += n;
        }
        job.wrap.max_width = w;
        let sense = if ranges.is_empty() { egui::Sense::hover() } else { egui::Sense::click() };
        let (pos, galley, resp) = egui::Label::new(job).sense(sense).layout_in_ui(ui);
        let under = resp
            .hover_pos()
            .map(|p| galley.cursor_from_pos(p - pos).index)
            .and_then(|i| ranges.iter().find(|(r, _)| r.contains(&i)).map(|(_, l)| l.clone()));
        ui.painter().galley(pos, galley, normal);
        if let Some(l) = under {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let resp = resp.on_hover_text(link_hint(&l));
            if resp.clicked() {
                clicked = Some(l);
            }
        }
    }
    clicked
}

fn link_hint(l: &Link) -> &'static str {
    match l {
        Link::System(_) => "Open system info",
        Link::Ship(_) => "Open ship info",
        Link::Pilot(_) => "Open pilot",
        Link::Kill(_) => "Open the killmail",
        Link::Battle(_) => "Open the battle report",
        Link::Fleet(_) => "Open the fleet",
        Link::Chat(_) => "Open the conversation",
        Link::Pings => "Open the ping feed",
        Link::Wormholes(_) => "Show its wormholes",
        Link::Url(_) => "Open in the browser",
        Link::Page(_) => "Go there in the app",
    }
}

/// System and ship names as the game spells them.
pub(crate) struct AppNames<'a> {
    pub systems: Option<&'a crate::geo::Systems>,
    pub ships: &'a std::collections::HashMap<String, i64>,
}

impl Names for AppNames<'_> {
    fn system(&self, name: &str) -> Option<i64> {
        self.systems?.lookup(name).filter(|i| i.name == name).map(|i| i.id)
    }
    fn ship(&self, name: &str) -> Option<i64> {
        self.ships.get(name).copied()
    }
}

#[cfg(test)]
pub(crate) fn seed_ai_view(app: &mut SpaiApp, ctx: &egui::Context, turns: Vec<Turn>) {
    let h = app.ai_handle(ctx);
    h.view.lock().unwrap().turns = turns;
}

impl SpaiApp {
    /// What the assistant remembers, by kind, each one editable and deletable.
    fn ai_memories_ui(&mut self, ui: &mut egui::Ui) {
        use crate::ai::memory::MemKind;
        use egui_phosphor::regular as icon;
        let now = crate::clock::utc().timestamp();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Memories").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{}  Add", icon::PLUS)).on_hover_text("Tell it something to keep").clicked() {
                    self.ai_mem_edit = Some((0, MemKind::About, String::new()));
                }
            });
        });
        ui.label(egui::RichText::new("Kept between conversations. It saves them as it learns; you can change or delete any.").weak());
        ui.separator();
        let list = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner()).list.clone();
        let mut delete: Option<u64> = None;
        let mut save: Option<(u64, MemKind, String)> = None;
        let mut cancel = false;
        let edit_ui = |ui: &mut egui::Ui, e: &mut (u64, MemKind, String), save: &mut Option<(u64, MemKind, String)>, cancel: &mut bool| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ComboBox::from_id_salt(("mem_kind", e.0)).selected_text(e.1.label()).show_ui(ui, |ui| {
                    for k in MemKind::CHOICES {
                        ui.menu_value(&mut e.1, k, k.label());
                    }
                });
                ui.add(egui::TextEdit::multiline(&mut e.2).desired_rows(2).desired_width(f32::INFINITY).hint_text("e.g. I stage in 1DQ1-A and fly Eagles"));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!e.2.trim().is_empty(), egui::Button::new("Save")).clicked() {
                        *save = Some(e.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        *cancel = true;
                    }
                });
            });
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if let Some(e) = self.ai_mem_edit.as_mut().filter(|e| e.0 == 0) {
                edit_ui(ui, e, &mut save, &mut cancel);
            }
            if list.is_empty() && self.ai_mem_edit.is_none() {
                ui.label(egui::RichText::new("Nothing yet.").weak());
            }
            for k in MemKind::CHOICES {
                let of: Vec<&crate::ai::memory::Memory> = list.iter().filter(|m| MemKind::from_code(m.kind.code()) == k).collect();
                if of.is_empty() {
                    continue;
                }
                ui.add_space(6.0);
                ui.label(egui::RichText::new(format!("{} ({})", k.label(), of.len())).weak());
                for m in of {
                    if let Some(e) = self.ai_mem_edit.as_mut().filter(|e| e.0 == m.id) {
                        edit_ui(ui, e, &mut save, &mut cancel);
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.button(icon::TRASH).on_hover_text("Delete").clicked() {
                                delete = Some(m.id);
                            }
                            if ui.button(icon::PENCIL_SIMPLE).on_hover_text("Edit").clicked() {
                                self.ai_mem_edit = Some((m.id, m.kind, m.text.clone()));
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                                let who = if m.by_user { "you" } else { "the assistant" };
                                ui.add(egui::Label::new(&m.text).wrap()).on_hover_text(format!("Saved by {who}, {}", crate::ai::tools::fmt_age(now, m.updated)));
                            });
                        });
                    });
                }
            }
        });
        let store = self.store.as_ref();
        let mut mems = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = delete {
            mems.remove(id);
            mems.save(store);
        }
        if let Some((id, kind, text)) = save {
            if id == 0 {
                mems.add(kind, &text, true, now);
            } else {
                mems.update(id, Some(kind), &text, true, now);
            }
            mems.save(store);
            cancel = true;
        }
        drop(mems);
        if cancel {
            self.ai_mem_edit = None;
        }
    }
}

#[cfg(test)]
mod open_chat_tests {
    use crate::ai::tools::{ActionKind, ChatWindowPick};

    #[test]
    fn a_conversation_opens_in_a_new_window_or_moves_into_an_open_one() {
        crate::uitest::harness::scratch_profile();
        let ctx = egui::Context::default();
        let mut a = crate::app::SpaiApp::build(&ctx, true);
        let jid = "ops@conference.example.invalid".to_owned();
        a.ai_apply(&ActionKind::OpenChat { jid: jid.clone(), window: ChatWindowPick::New }, "Opened ops");
        assert_eq!(a.jabber_popouts.len(), 1);
        assert!(a.jabber_popouts[0].tabs.contains(&jid));
        let first = a.jabber_popouts[0].id;
        let other = "someone@example.invalid".to_owned();
        a.ai_apply(&ActionKind::OpenChat { jid: other.clone(), window: ChatWindowPick::Existing(first) }, "Opened someone");
        assert!(a.jabber_popouts[0].tabs.contains(&other));
        assert_eq!(a.jabber_popouts[0].active.as_deref(), Some(other.as_str()), "the one asked for is in front");
        a.ai_apply(&ActionKind::OpenChat { jid: other.clone(), window: ChatWindowPick::Main }, "Opened someone");
        assert!(!a.jabber_popouts.iter().any(|w| w.tabs.contains(&other)), "moved back to the Jabber tab");
    }
}
