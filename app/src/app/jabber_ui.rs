//! Jabber in the main window and its pop-outs: connection, rooms, conversations, tabs and fleet ping rules.

use super::*;

impl SpaiApp {
    pub(crate) fn maybe_start_jabber(&mut self, ctx: &egui::Context) {
        // A terminal failure (bad credentials, unreachable) drops back to the login form and stops
        // auto-restarting; the reason stays in state until the user retries.
        if self.jabber.lock().unwrap().fatal.is_some() && self.settings.jabber_enabled {
            self.settings.jabber_enabled = false;
            self.needs_save = true;
        }
        let enabled = self.settings.jabber_enabled && !self.settings.jabber_jid.trim().is_empty();
        {
            let mut s = self.jabber.lock().unwrap();
            s.enabled = enabled;
            if s.running || !enabled {
                return;
            }
        }
        let Some(systems) = self.systems.clone() else { return };
        let jid = self.settings.jabber_jid.trim().to_owned();
        let Some(pw) = crate::jabber::load_password(&jid) else { return };
        let resolve: crate::jabber::Resolver = std::sync::Arc::new(move |t: &str| {
            systems.lookup(t).or_else(|| systems.lookup_prefix(t)).map(|i| i.id)
        });
        let server = self.settings.jabber_server.clone();
        let rooms = self.jabber_rooms_to_join();
        self.jabber_tx = Some(crate::jabber::spawn(
            jid,
            pw,
            server,
            rooms,
            resolve,
            self.jabber.clone(),
            self.ping_shared.clone(),
            ctx.clone(),
        ));
    }

    /// The rooms Rescue Mode feeds on, while this build has the feature and the user has it
    /// enabled: delve911, which `ingest_delve911_jabber` parses into rescue events, and
    /// skirmish_commanders, which the rescue window reads and posts `!bping` requests to. A left
    /// room's messages are dropped before they are stored, so losing either one breaks capital
    /// rescue with no error anywhere. Both are pinned: always joined, never removable.
    pub(crate) fn jabber_rescue_rooms(&self) -> Vec<String> {
        #[cfg(feature = "fc-rescue")]
        if self.settings.fc_rescue_enabled {
            return [
                goon_jid(&self.settings.rescue_delve911_jid, "delve911@conference.goonfleet.com"),
                goon_jid(
                    &self.settings.rescue_skirmish_jid,
                    "skirmish_commanders@conference.goonfleet.com",
                ),
            ]
            .into_iter()
            .filter(|j| !j.is_empty())
            .collect();
        }
        Vec::new()
    }

    /// The rooms we join ourselves on connect. A deliberately left room is never in here; only the
    /// server can put us back into one. The pinned rescue room is always in here.
    pub(crate) fn jabber_rooms_to_join(&self) -> Vec<String> {
        let left = &self.settings.jabber_left_rooms;
        let pinned = self.jabber_rescue_rooms();
        let mut v: Vec<String> = self
            .settings
            .jabber_rooms
            .iter()
            .filter(|r| !left.contains(r) || pinned.contains(r))
            .cloned()
            .collect();
        for p in pinned {
            if !v.contains(&p) {
                v.push(p);
            }
        }
        v
    }

    /// MUC service to join and browse rooms on: the explicit setting, else `conference.<account
    /// domain>` (the XMPP convention). Empty only when no account is configured at all.
    pub(crate) fn muc_domain(&self) -> String {
        muc_domain_of(&self.settings.jabber_muc_domain, &self.settings.jabber_jid)
    }

    pub(crate) fn full_room_jid(&self, input: &str) -> String {
        let input = input.trim();
        if input.contains('@') {
            return input.to_owned();
        }
        format!("{input}@{}", self.muc_domain())
    }

    /// What counts as being named in a room: the Jabber username, plus whatever the user added.
    pub(crate) fn mention_names(&self) -> Vec<String> {
        let node = self.settings.jabber_jid.split('@').next().unwrap_or_default().trim();
        std::iter::once(node.to_owned())
            .chain(self.settings.jabber_mention_keywords.iter().cloned())
            .filter(|n| !n.trim().is_empty())
            .collect()
    }

    pub(crate) fn jabber_mark_read(&self, jid: &str) {
        let mut st = self.jabber.lock().unwrap();
        st.unread.remove(jid);
        st.unread_counts.remove(jid);
        st.mentions.remove(jid);
    }

    pub(crate) fn jabber_is_muted(&self, key: &str) -> bool {
        self.settings
            .jabber_muted
            .get(key)
            .is_some_and(|&until| until == i64::MAX || chrono::Utc::now().timestamp() < until)
    }

    pub(crate) fn jabber_has_unread(&self) -> bool {
        self.jabber_unread_total() > 0
    }

    /// Badge the taskbar entry with the same count as the tray.
    ///
    /// Two mechanisms, because no single one covers the desktops this runs on. Windows uses the
    /// badged window icon. Plasma matches the window to a `.desktop` file and uses that file's
    /// `Icon=`, ignoring the badged icon, so `launcher::set_count` covers it.
    ///
    /// Only on a change: `ViewportCommand::Icon` hands the window manager a fresh image to decode.
    pub(crate) fn sync_taskbar_badge(&mut self, ctx: &egui::Context, count: u32) {
        crate::launcher::set_count(count);
        if self.taskbar_badge == Some(count) {
            return;
        }
        self.taskbar_badge = Some(count);
        let base = app_icon();
        let mut rgba = base.rgba.clone();
        crate::badge::draw(&mut rgba, base.width, base.height, count);
        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(
            egui::IconData { rgba, width: base.width, height: base.height },
        ))));
    }

    /// Every unread message across conversations that are not muted, for the tray and taskbar badge.
    ///
    /// A muted conversation is one the user has said they do not want to hear about, so it does not
    /// get to drive a number on the taskbar either. An unread ping feed counts as one: it has no
    /// per-message count of its own.
    pub(crate) fn jabber_unread_total(&self) -> u32 {
        let st = self.jabber.lock().unwrap();
        let mut n: u32 = st
            .unread_counts
            .iter()
            .filter(|(k, _)| !self.jabber_is_muted(k))
            .map(|(_, c)| *c)
            .sum();
        if st.pings_unread && !self.jabber_is_muted(crate::jabber::PING_FEED_KEY) {
            n = n.saturating_add(1);
        }
        n
    }

    pub(crate) fn cache_op_links(&mut self, pings: &[crate::pings::Ping]) {
        use crate::pings::{Comms, Ping};
        let mut changed = false;
        for p in pings {
            if let Ping::Fleet { comms: Some(Comms::Mumble { channel, link }), .. } = p {
                if let Some(k) = op_key(channel) {
                    if self.settings.op_channel_links.get(&k) != Some(link) {
                        self.settings.op_channel_links.insert(k, link.clone());
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.needs_save = true;
        }
    }

    pub(crate) fn poll_jabber_notify(&mut self, ctx: &egui::Context) {
        let events: Vec<(String, bool)> = {
            let mut st = self.jabber.lock().unwrap();
            st.notify_cfg.sound_enabled = self.settings.jabber_sound_enabled;
            st.notify_cfg.ping_sound = self.settings.jabber_ping_sound.clone();
            st.notify_cfg.msg_sound = self.settings.jabber_msg_sound.clone();
            st.notify_cfg.mention_sound = self.settings.jabber_mention_sound.clone();
            st.notify_cfg.ping_volume = self.settings.jabber_ping_volume;
            st.notify_cfg.msg_volume = self.settings.jabber_msg_volume;
            st.notify_cfg.mention_volume = self.settings.jabber_mention_volume;
            st.notify_cfg.mention_names = self.mention_names();
            st.notify_cfg.mention_ignores_mute = self.settings.jabber_mention_ignores_mute;
            st.notify_cfg.ping_rules = self.settings.jabber_ping_rules.clone();
            st.notify_cfg.muted = self.settings.jabber_muted.clone();
            std::mem::take(&mut st.notify)
        };
        if events.is_empty() {
            return;
        }
        let recent: Vec<crate::pings::Ping> =
            { self.jabber.lock().unwrap().pings.iter().rev().take(10).cloned().collect() };
        self.cache_op_links(&recent);
        let mut any = false;
        for (key, is_ping) in events {
            if self.jabber_is_muted(&key) {
                continue;
            }
            let suppress = if is_ping {
                let latest = self.jabber.lock().unwrap().pings.last().cloned();
                match latest.as_ref().and_then(|p| self.matching_ping_rule(p)) {
                    Some(r) => r.suppress,
                    None => false,
                }
            } else {
                false
            };
            if suppress {
                continue;
            }
            any = true;
        }
        if any && !ctx.input(|i| i.focused) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
    }

    pub(crate) fn matching_ping_rule(&self, p: &crate::pings::Ping) -> Option<&crate::settings::PingRule> {
        crate::pings::match_ping_rule(&self.settings.jabber_ping_rules, p)
    }

    pub(crate) fn full_user_jid(&self, input: &str) -> String {
        let input = input.trim();
        if input.contains('@') {
            return input.to_owned();
        }
        let domain = self.settings.jabber_jid.split('@').nth(1).unwrap_or("");
        format!("{input}@{domain}")
    }

    pub(crate) fn ping_rules_dialog(&mut self, ctx: &egui::Context) {
        if !self.ping_rules_open {
            return;
        }
        let mut changed = false;
        let keep = Self::dialog_viewport(
            ctx,
            "jabber_alerts_window",
            "EVE Spai - Jabber alerts",
            [540.0, 620.0],
            |ui| {
              egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("snd").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    changed |= ui
                        .checkbox(&mut self.settings.jabber_sound_enabled, "Notification sounds")
                        .changed();
                    ui.end_row();
                    let msg_vol = self.settings.jabber_msg_volume;
                    let ping_vol = self.settings.jabber_ping_volume;
                    let mention_vol = self.settings.jabber_mention_volume;
                    ui.label("Message sound");
                    changed |= sound_picker(ui, "jabber_msg", false, &mut self.settings.jabber_msg_sound, msg_vol);
                    ui.end_row();
                    ui.label("Message volume");
                    changed |= volume_slider(ui, &mut self.settings.jabber_msg_volume);
                    ui.end_row();
                    ui.label("Default ping sound");
                    changed |= sound_picker(ui, "jabber_ping", false, &mut self.settings.jabber_ping_sound, ping_vol);
                    ui.end_row();
                    ui.label("Fleet ping volume");
                    changed |= volume_slider(ui, &mut self.settings.jabber_ping_volume);
                    ui.end_row();
                    ui.label("Mention sound");
                    changed |= sound_picker(ui, "jabber_mention", false, &mut self.settings.jabber_mention_sound, mention_vol);
                    ui.end_row();
                    ui.label("Mention volume");
                    changed |= volume_slider(ui, &mut self.settings.jabber_mention_volume);
                    ui.end_row();
                    ui.label("");
                    // At body size this line is wider than the dialog, and the grid cell it sits
                    // in imposes no wrap width of its own.
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "presets: horn · chime · beep · sweep · info · warning · danger · \
                                 critical · off, or a file path",
                            )
                            .weak(),
                        )
                        .wrap(),
                    );
                    ui.end_row();
                    ui.label("Mention words");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.mention_input)
                                .hint_text("extra words, comma separated"),
                        )
                        .changed()
                    {
                        self.settings.jabber_mention_keywords = self
                            .mention_input
                            .split(',')
                            .map(str::trim)
                            .filter(|w| !w.is_empty())
                            .map(str::to_owned)
                            .collect();
                        changed = true;
                    }
                    ui.end_row();
                    ui.label("");
                    let me = self.settings.jabber_jid.split('@').next().unwrap_or_default();
                    ui.label(
                        egui::RichText::new(format!(
                            "your name \"{me}\" always counts as a mention"
                        ))
                        .weak(),
                    );
                    ui.end_row();
                    ui.label("");
                    changed |= ui
                        .checkbox(
                            &mut self.settings.jabber_mention_ignores_mute,
                            "Mentions notify even in muted chats",
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Doctrine link");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.doctrine_url)
                                .hint_text("URL or file:/// path shown on fleet pings"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Fleet ping window");
                    changed |= ui
                        .checkbox(
                            &mut self.settings.fleet_ping_window,
                            "Pop a focused window on fleet pings",
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Keep on top");
                    ui.horizontal(|ui| {
                        use crate::settings::OnTop;
                        changed |= ui
                            .menu_value(&mut self.settings.fleet_ping_on_top, OnTop::Always, "Always")
                            .changed();
                        changed |= ui
                            .menu_value(&mut self.settings.fleet_ping_on_top, OnTop::Smart, "When EVE focused")
                            .changed();
                        changed |= ui
                            .menu_value(&mut self.settings.fleet_ping_on_top, OnTop::Never, "Never")
                            .changed();
                    });
                    ui.end_row();
                    ui.label("Ping bot JID");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.jabber_ping_bot)
                                .hint_text("directorbot@…"),
                        )
                        .changed();
                    ui.end_row();
                });
                ui.separator();
                ui.label(
                    egui::RichText::new("Fleet-ping rules. A match plays its sound and highlights the ping.")
                        .weak(),
                );
                let mut remove: Option<usize> = None;
                let mut move_up: Option<usize> = None;
                let mut move_down: Option<usize> = None;
                let mut edit: Option<usize> = None;
                let n = self.settings.jabber_ping_rules.len();
                use egui_phosphor::regular as ic;
                for (i, r) in self.settings.jabber_ping_rules.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            changed |= ui.checkbox(&mut r.enabled, "").changed();
                            let nm = if r.name.is_empty() { "(unnamed rule)" } else { &r.name };
                            let txt = if r.enabled {
                                egui::RichText::new(nm).strong()
                            } else {
                                egui::RichText::new(nm).weak().strikethrough()
                            };
                            if ui
                                .add(egui::Label::new(txt).sense(egui::Sense::click()))
                                .on_hover_text("Edit rule")
                                .clicked()
                            {
                                edit = Some(i);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(ic::X).on_hover_text("Delete").clicked() {
                                    remove = Some(i);
                                }
                                if i + 1 < n && ui.button(ic::ARROW_DOWN).on_hover_text("Move down").clicked() {
                                    move_down = Some(i);
                                }
                                if i > 0 && ui.button(ic::ARROW_UP).on_hover_text("Move up").clicked() {
                                    move_up = Some(i);
                                }
                                if ui.button(ic::PENCIL_SIMPLE).on_hover_text("Edit rule").clicked() {
                                    edit = Some(i);
                                }
                            });
                        });
                    });
                }
                // A structural change invalidates the editor's index; close it rather than risk
                // editing the wrong rule.
                if let Some(i) = remove {
                    self.settings.jabber_ping_rules.remove(i);
                    self.ping_rule_editing = None;
                    changed = true;
                }
                if let Some(i) = move_up {
                    self.settings.jabber_ping_rules.swap(i, i - 1);
                    self.ping_rule_editing = None;
                    changed = true;
                }
                if let Some(i) = move_down {
                    self.settings.jabber_ping_rules.swap(i, i + 1);
                    self.ping_rule_editing = None;
                    changed = true;
                }
                if let Some(i) = edit {
                    self.ping_rule_editing = Some(i);
                }
                ui.separator();
                if ui.button("+ Add rule").clicked() {
                    self.settings.jabber_ping_rules.push(crate::settings::PingRule::default());
                    self.ping_rule_editing = Some(self.settings.jabber_ping_rules.len() - 1);
                    changed = true;
                }
              });
            },
        );
        if !keep {
            self.ping_rules_open = false;
            self.ping_rule_editing = None;
        }
        if changed {
            self.needs_save = true;
        }
        self.ping_rule_editor(ctx);
    }

    /// Config dialog for a single fleet-ping rule, opened on top of the Jabber alerts window. Only
    /// one is open at a time (`ping_rule_editing`).
    pub(crate) fn ping_rule_editor(&mut self, ctx: &egui::Context) {
        let Some(i) = self.ping_rule_editing else { return };
        if i >= self.settings.jabber_ping_rules.len() {
            self.ping_rule_editing = None;
            return;
        }
        let mut changed = false;
        let global_ping_vol = self.settings.jabber_ping_volume;
        let keep = Self::dialog_viewport(
            ctx,
            "ping_rule_editor",
            "EVE Spai - Fleet ping rule",
            [420.0, 460.0],
            |ui| {
              // Scope the rule borrow so the "Done" button below can touch `self.ping_rule_editing`.
              {
                let r = &mut self.settings.jabber_ping_rules[i];
                ui.horizontal(|ui| {
                    ui.label("Name");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut r.name).desired_width(240.0))
                        .changed();
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Match on (blank = any). A ping must match every filled field.")
                        .weak(),
                );
                egui::Grid::new("rule").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    let wide = 250.0;
                    ui.label("FC");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.fc).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("PAP type");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.pap).hint_text("any  (strategic / peacetime)").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("Doctrine");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.doctrine).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("Form-up");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.formup).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                    ui.label("Keyword");
                    changed |= ui.add(egui::TextEdit::singleline(&mut r.keyword).hint_text("any").desired_width(wide)).changed();
                    ui.end_row();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    changed |= ui
                        .checkbox(&mut r.suppress, "Suppress")
                        .on_hover_text("Ignore matching pings: no sound, no highlight, no push")
                        .changed();
                    if r.suppress {
                        r.notify = false;
                        r.push = false;
                    }
                    ui.add_enabled_ui(!r.suppress, |ui| {
                        changed |= ui.checkbox(&mut r.notify, "Notify").changed();
                        changed |= ui.checkbox(&mut r.push, "Push").changed();
                    });
                });
                ui.add_enabled_ui(!r.suppress && r.notify, |ui| {
                    let eff_vol = r.volume.unwrap_or(global_ping_vol);
                    ui.horizontal(|ui| {
                        ui.label("Sound");
                        changed |= sound_picker(ui, ("ping_rule", i), true, &mut r.sound, eff_vol);
                    });
                    ui.horizontal(|ui| {
                        let mut custom = r.volume.is_some();
                        if ui
                            .checkbox(&mut custom, "Custom volume")
                            .on_hover_text("Override the global fleet-ping volume for this rule")
                            .changed()
                        {
                            r.volume = if custom { Some(global_ping_vol) } else { None };
                            changed = true;
                        }
                        if let Some(v) = r.volume.as_mut() {
                            changed |= volume_slider(ui, v);
                        }
                    });
                });
              }
                ui.add_space(8.0);
                ui.separator();
                if ui.button("Done").clicked() {
                    self.ping_rule_editing = None;
                }
            },
        );
        if !keep {
            self.ping_rule_editing = None;
        }
        if changed {
            self.needs_save = true;
        }
    }

    pub(crate) fn poll_kill_fetches(&self) {
        let Some(tx) = &self.kill_tx else { return };
        let mut to_fetch: Vec<i64> = Vec::new();
        {
            let cache = self.kill_cache.lock().unwrap();
            let st = self.intel_state.lock().unwrap();
            for r in &st.reports {
                for l in &r.links {
                    if l.kind == crate::intel::LinkKind::Killmail {
                        if let Some(id) = l.kill_id {
                            if !cache.contains_key(&id) {
                                to_fetch.push(id);
                            }
                        }
                    }
                }
            }
        }
        for id in to_fetch {
            self.kill_cache.lock().unwrap().entry(id).or_insert(None);
            let _ = tx.send(id);
        }
    }

    pub(crate) fn tab_set(&mut self) -> TabSet<'_> {
        TabSet {
            main: &mut self.jabber_tabs,
            main_active: &mut self.jabber_chat,
            popouts: &mut self.jabber_popouts,
        }
    }

    pub(crate) fn popout(&self, id: u64) -> Option<&ChatWindow> {
        self.jabber_popouts.iter().find(|w| w.id == id)
    }

    pub(crate) fn popout_mut(&mut self, id: u64) -> Option<&mut ChatWindow> {
        self.jabber_popouts.iter_mut().find(|w| w.id == id)
    }

    pub(crate) fn win_tabs(&self, win: ChatWinKey) -> &[String] {
        match win {
            ChatWinKey::Main => &self.jabber_tabs,
            ChatWinKey::Popout(id) => self.popout(id).map_or(&[][..], |w| w.tabs.as_slice()),
        }
    }

    /// The window's selected conversation. `None` on `Main` is the Fleet pings pseudo-tab; a
    /// pop-out never has that state.
    pub(crate) fn win_active(&self, win: ChatWinKey) -> Option<String> {
        match win {
            ChatWinKey::Main => self.jabber_chat.clone(),
            ChatWinKey::Popout(id) => self.popout(id).and_then(|w| w.active.clone()),
        }
    }

    pub(crate) fn win_set_active(&mut self, win: ChatWinKey, jid: Option<String>) {
        match win {
            ChatWinKey::Main => self.jabber_chat = jid,
            ChatWinKey::Popout(id) => {
                if let Some(w) = self.popout_mut(id) {
                    w.active = jid;
                }
            }
        }
    }

    /// Select `jid`, opening a tab for it in `prefer` if no window owns it yet. An existing owner
    /// keeps the conversation and is raised instead, so a jid is never in two windows at once.
    pub(crate) fn jabber_open(&mut self, jid: &str, prefer: ChatWinKey) {
        let mut t = self.tab_set();
        let win = match t.owner(jid) {
            Some(w) => w,
            None => {
                t.attach(jid, prefer, None);
                prefer
            }
        };
        self.win_set_active(win, Some(jid.to_owned()));
        if let ChatWinKey::Popout(id) = win {
            self.focus_window = Some(popout_viewport(id));
        }
    }

    /// Drop a room or private chat from the sidebar. Stored messages are kept, so rejoining or a
    /// new message brings the whole backlog back; only the listing is suppressed. A joined room is
    /// left on the way out, through the same path as the tab close, so the leave sticks.
    pub(crate) fn jabber_forget(&mut self, jid: &str, is_room: bool) {
        if self.jabber_rescue_rooms().iter().any(|r| r == jid) {
            return;
        }
        if is_room {
            if let Some(tx) = &self.jabber_tx {
                let _ = tx.send(crate::jabber::Cmd::LeaveRoom { room: jid.to_owned() });
            }
            crate::jabber::note_room_left(&self.jabber, jid);
            if !self.settings.jabber_left_rooms.iter().any(|r| r == jid) {
                self.settings.jabber_left_rooms.push(jid.to_owned());
            }
        }
        self.settings.jabber_rooms.retain(|r| r != jid);
        self.settings.jabber_closed_rooms.retain(|r| r != jid);
        self.settings.jabber_closed_dms.retain(|d| d != jid);
        self.settings.jabber_inaccessible_rooms.retain(|r| r != jid);
        self.settings.jabber_contacts.retain(|c| c != jid);
        self.settings.jabber_room_subjects.remove(jid);
        if !self.settings.jabber_forgotten.iter().any(|j| j == jid) {
            self.settings.jabber_forgotten.push(jid.to_owned());
        }
        {
            let mut st = self.jabber.lock().unwrap();
            st.rooms_inaccessible.remove(jid);
            st.room_subjects.remove(jid);
            st.unread.remove(jid);
            st.unread_counts.remove(jid);
            st.mentions.remove(jid);
        }
        self.remove_jabber_tab(jid);
        if self.jabber_motd_window.as_deref() == Some(jid) {
            self.jabber_motd_window = None;
        }
        if self.jabber_chat.as_deref() == Some(jid) {
            self.jabber_chat = None;
        }
        self.needs_save = true;
    }

    /// Anything that puts a conversation back in front of the user undoes a forget.
    pub(crate) fn jabber_unforget(&mut self, jid: &str) {
        if self.settings.jabber_forgotten.iter().any(|j| j == jid) {
            self.settings.jabber_forgotten.retain(|j| j != jid);
            self.needs_save = true;
        }
    }

    /// Rejoining by hand undoes a deliberate leave, in settings and in the live session.
    pub(crate) fn jabber_unleave(&mut self, jid: &str) {
        if self.settings.jabber_left_rooms.iter().any(|r| r == jid) {
            self.settings.jabber_left_rooms.retain(|r| r != jid);
            self.needs_save = true;
        }
        self.jabber.lock().unwrap().rooms_left.remove(jid);
    }

    pub(crate) fn remove_jabber_tab(&mut self, jid: &str) {
        self.tab_set().detach(jid);
    }

    /// The X hides. Closing a tab is not destructive and does not ask: leaving a room is the
    /// sidebar's remove button and nothing else.
    pub(crate) fn close_jabber_tab(&mut self, jid: &str, is_room: bool) {
        // Rescue Mode holds its rooms open, so closing one would reopen on the next frame.
        if self.jabber_rescue_rooms().iter().any(|r| r == jid) {
            return;
        }
        if is_room {
            if !self.settings.jabber_closed_rooms.iter().any(|r| r == jid) {
                self.settings.jabber_closed_rooms.push(jid.to_owned());
            }
        } else if !self.settings.jabber_closed_dms.iter().any(|d| d == jid) {
            self.settings.jabber_closed_dms.push(jid.to_owned());
        }
        self.needs_save = true;
        self.remove_jabber_tab(jid);
    }

    /// Tear `jid` off into a brand-new pop-out placed at `at`. Returns the new window's id, or
    /// `None` when nobody owns the jid or the cap is already reached.
    pub(crate) fn new_popout(&mut self, jid: &str, at: Option<egui::Pos2>) -> Option<u64> {
        if self.jabber_popouts.len() >= MAX_POPOUTS {
            return None;
        }
        let id = self.tab_set().detach_to_new(jid, at.map(|p| (p.x, p.y)))?;
        self.focus_window = Some(popout_viewport(id));
        Some(id)
    }

    /// A pop-out closed with the native X: its conversations come back to the main bar, unmarked.
    /// This must not go through `close_jabber_tab`, which would persist them as closed.
    pub(crate) fn return_popout_tabs(&mut self, id: u64) {
        self.tab_set().dissolve(id);
    }

    /// Mirror the live pop-out windows into settings, only when they actually differ: geometry is
    /// already gated by `geometry_update`, and writing on every frame would hammer SQLite.
    pub(crate) fn sync_popout_settings(&mut self) {
        let want: Vec<crate::settings::ChatWindowCfg> =
            self.jabber_popouts.iter().map(popout_cfg).collect();
        if self.settings.jabber_popout_windows != want {
            self.settings.jabber_popout_windows = want;
            self.needs_save = true;
        }
        // The main window's tab bar, on the same terms as the pop-outs.
        if self.settings.jabber_main_tabs != self.jabber_tabs {
            self.settings.jabber_main_tabs = self.jabber_tabs.clone();
            self.needs_save = true;
        }
        let active = self.jabber_chat.clone().unwrap_or_default();
        if self.settings.jabber_main_active != active {
            self.settings.jabber_main_active = active;
            self.needs_save = true;
        }
    }

    /// Should `win` paint a drop-target border? Every window asks this of the shared drag state,
    /// because the OS pointer grab means only the source window ever sees the pointer.
    pub(crate) fn jabber_drop_highlight(&self, win: ChatWinKey) -> bool {
        let Some(d) = &self.jabber_tab_drag else { return false };
        let Some(at) = d.at else { return false };
        if d.from == win {
            return false;
        }
        let rect = match win {
            ChatWinKey::Main => self.jabber_main_rect.map(|(outer, _)| outer),
            ChatWinKey::Popout(id) => self.popout(id).and_then(|w| w.outer),
        };
        rect.is_some_and(|r| r.contains(at))
    }

    /// Puts the app in the state a live tab drag leaves behind, so a scene can render the mid-drag
    /// look without synthesizing the pointer grab kittest cannot give a painter-only tab.
    #[cfg(test)]
    pub(crate) fn seed_tab_drag(&mut self, jid: &str, from: ChatWinKey) {
        self.jabber_tab_drag =
            Some(TabDrag { jid: jid.to_owned(), from, at: None, alive: true });
    }

    /// One immediate viewport per pop-out chat window, modelled on `char_popout_windows`: ids are
    /// collected up front and every mutation is applied after the loop.
    #[allow(deprecated)]
    pub(crate) fn jabber_popout_windows(&mut self, ctx: &egui::Context, f: &JabberFrame) {
        if self.jabber_popouts.is_empty() || !f.configured || !f.ever_online {
            // Skipping the call destroys the viewports, so re-arm the one-shot: when jabber comes
            // back the windows must reopen where the user left them, not at the default size.
            for w in self.jabber_popouts.iter_mut() {
                w.geom_applied = false;
            }
            return;
        }
        let ids: Vec<u64> = self.jabber_popouts.iter().map(|w| w.id).collect();
        let mut out: Vec<TabAction> = Vec::new();
        for id in ids {
            let Some(w) = self.popout(id) else { continue };
            let head = w.active.clone().or_else(|| w.tabs.first().cloned()).unwrap_or_default();
            let head = head.split('@').next().unwrap_or(&head).to_owned();
            let (pos, size, applied) = (w.pos, w.size, w.geom_applied);
            let mut builder = egui::ViewportBuilder::default()
                .with_icon(app_icon())
                .with_title(format!("EVE Spai - {head}"))
                .with_min_inner_size([360.0, 260.0]);
            // Saved geometry is fed to the builder on the first frame only. Re-applying it every
            // frame would fight the user dragging or resizing the window.
            if !applied {
                let sz = size.unwrap_or((520.0, 480.0));
                builder = builder.with_inner_size([sz.0, sz.1]);
                if let Some((x, y)) = pos {
                    builder = builder.with_position([x, y]);
                }
                if let Some(w) = self.popout_mut(id) {
                    w.geom_applied = true;
                }
            }
            let win = ChatWinKey::Popout(id);
            let vp_id = format!("jabberwin_{id}");
            let mut keep = true;
            let mut geom: WinGeom = None;
            let mut rects: (Option<egui::Rect>, Option<egui::Rect>) = (None, None);
            let mut focused = false;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(&vp_id),
                builder,
                |ctx, _class| {
                    let (visible, inner, outer, foc) = ctx.input(|i| {
                        let vp = i.viewport();
                        (vp.visible() != Some(false), vp.inner_rect, vp.outer_rect, i.focused)
                    });
                    rects = (outer, inner);
                    focused = foc;
                    if visible {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            self.jabber_window_body(ui, win, f, &mut out);
                        });
                    }
                    let sz = ctx.content_rect().size();
                    if sz.x > 100.0 && sz.y > 100.0 {
                        geom = Some(((sz.x, sz.y), outer.map(|r| (r.min.x, r.min.y))));
                    }
                    if ctx.input(|i| i.viewport().close_requested()) {
                        keep = false;
                    }
                },
            );
            if let Some(w) = self.popout_mut(id) {
                w.outer = rects.0;
                w.inner = rects.1;
                w.focused = focused;
                if let Some((sz, pos)) = geom {
                    if geometry_update(w.size, sz, 2.0).is_some() {
                        w.size = Some(sz);
                    }
                    if let Some(p) = pos.and_then(|p| geometry_update(w.pos, p, 1.0)) {
                        w.pos = Some(p);
                    }
                }
            }
            if !keep {
                out.push(TabAction::CloseWindow(id));
            }
        }
        self.apply_tab_actions(out);
    }

    /// A room's MOTD in full, which is the only place it is shown whole.
    ///
    /// Selectable, because a MOTD is where the fleet ping format, the comms details and the forum
    /// link live and those get copied out. Scrolled rather than grown: some of them are very long.
    pub(crate) fn jabber_motd_dialog(&mut self, ctx: &egui::Context, channels: &[ChannelRow]) {
        let Some(jid) = self.jabber_motd_window.clone() else {
            return;
        };
        let Some(motd) = channels
            .iter()
            .find(|c| c.jid == jid)
            .map(|c| c.motd.clone())
            .filter(|m| !m.trim().is_empty())
        else {
            self.jabber_motd_window = None;
            return;
        };
        let name = jid.split('@').next().unwrap_or(&jid).to_owned();
        let mut open = true;
        egui::Window::new(format!("{}  {name} MOTD", egui_phosphor::regular::ARTICLE))
            .id(egui::Id::new("jabber_motd"))
            .collapsible(false)
            .resizable(true)
            .default_size([420.0, 320.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                if ui.button(format!("{}  Copy", egui_phosphor::regular::COPY)).clicked() {
                    ui.ctx().copy_text(motd.clone());
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // Line by line, each in its own wrapping row: `render_linked_text` emits inline
                    // widgets and knows nothing about newlines, and a MOTD's own line breaks are
                    // half of what makes it readable. The links matter because a MOTD is where the
                    // doctrine and forum links live.
                    for line in motd.lines() {
                        if line.trim().is_empty() {
                            ui.add_space(4.0);
                            continue;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            render_linked_text(ui, line, false);
                        });
                    }
                });
            });
        if !open {
            self.jabber_motd_window = None;
        }
    }

    pub(crate) fn jabber_join_dialog(
        &mut self,
        ctx: &egui::Context,
        convos: &[Convo],
        channels: &[ChannelRow],
    ) {
        if !self.jabber_join_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        let mut pick: Option<String> = None;
        // Rooms are in `channels`, but `convos` is built from every chat there is history for, which
        // includes the rooms. Without this the DM list offers rooms and the room list offers people.
        let room_jids: std::collections::HashSet<&str> =
            channels.iter().map(|c| c.jid.as_str()).collect();
        let rooms_mode = self.jabber_join_rooms;
        egui::Window::new(if rooms_mode {
            format!("{}  Join a room", egui_phosphor::regular::USERS_THREE)
        } else {
            format!("{}  Start a DM", egui_phosphor::regular::CHAT_CIRCLE_DOTS)
        })
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            // Size fields to a constant, NOT ui.available_width(): this window auto-fits its content
            // (resizable=false), so a field derived from available_width feeds the window width back
            // into itself and the dialog creeps wider every frame.
            const DIALOG_W: f32 = 320.0;
            const FIELD_W: f32 = DIALOG_W - 70.0;
            // A hundred with a scroll bar covers recent history without the dialog growing past the
            // screen.
            const RECENT_CAP: usize = 100;
            ui.set_min_width(DIALOG_W);
            if rooms_mode {
            let room_go = ui
                .horizontal(|ui| {
                    let resp = ui.add_sized(
                        [FIELD_W, 22.0],
                        egui::TextEdit::singleline(&mut self.jabber_room_input)
                            .hint_text("room@conference.…"),
                    );
                    let enter =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.button("Join").clicked() || enter
                })
                .inner;
            // Recently active rooms, newest first, filtered by whatever is in the field. Typing an
            // exact name still works; this is for the far more common case of half-remembering one.
            let mut recent_rooms: Vec<&ChannelRow> = channels
                .iter()
                .filter(|c| c.last_at > 0)
                .filter(|c| {
                    let q = self.jabber_room_input.trim().to_lowercase();
                    q.is_empty()
                        || c.name.to_lowercase().contains(&q)
                        || c.jid.to_lowercase().contains(&q)
                })
                .collect();
            recent_rooms.sort_by_key(|c| std::cmp::Reverse(c.last_at));
            recent_rooms.truncate(RECENT_CAP);
            Self::jabber_recent_list(ui, "rooms", &recent_rooms.iter().map(|c| (c.jid.clone(), c.name.clone())).collect::<Vec<_>>(), egui_phosphor::regular::USERS_THREE, &mut pick);

            if room_go && !self.jabber_room_input.trim().is_empty() {
                let room = self.full_room_jid(&self.jabber_room_input);
                self.jabber_room_input.clear();
                if let Some(tx) = &self.jabber_tx {
                    let _ = tx.send(crate::jabber::Cmd::JoinRoom { room: room.clone() });
                }
                if !self.settings.jabber_rooms.contains(&room) {
                    self.settings.jabber_rooms.push(room.clone());
                }
                self.settings.jabber_closed_rooms.retain(|r| r != &room);
                self.jabber_unleave(&room);
                self.jabber_unforget(&room);
                self.needs_save = true;
                self.jabber_open(&room, ChatWinKey::Main);
                close = true;
            }

            } else {
            let dm_go = ui
                .horizontal(|ui| {
                    let resp = ui.add_sized(
                        [FIELD_W, 22.0],
                        egui::TextEdit::singleline(&mut self.jabber_dm_input)
                            .hint_text("Message someone…"),
                    );
                    let enter =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.button("Open").clicked() || enter
                })
                .inner;
            let mut recent_dms: Vec<&Convo> = convos
                .iter()
                .filter(|c| c.last_at > 0)
                .filter(|c| !room_jids.contains(c.jid.as_str()))
                .filter(|c| c.jid != crate::jabber::PING_FEED_KEY)
                .filter(|c| {
                    let q = self.jabber_dm_input.trim().to_lowercase();
                    q.is_empty()
                        || c.name.to_lowercase().contains(&q)
                        || c.jid.to_lowercase().contains(&q)
                })
                .collect();
            recent_dms.sort_by_key(|c| std::cmp::Reverse(c.last_at));
            recent_dms.truncate(RECENT_CAP);
            Self::jabber_recent_list(ui, "dms", &recent_dms.iter().map(|c| (c.jid.clone(), c.name.clone())).collect::<Vec<_>>(), egui_phosphor::regular::CHAT_CIRCLE_DOTS, &mut pick);

            if dm_go && !self.jabber_dm_input.trim().is_empty() {
                let input = self.jabber_dm_input.trim().to_owned();
                let resolved = if input.contains('@') {
                    Some(input.clone())
                } else if let Some(c) = convos.iter().find(|c| {
                    c.name.eq_ignore_ascii_case(&input)
                        || c.jid
                            .split('@')
                            .next()
                            .is_some_and(|l| l.eq_ignore_ascii_case(&input))
                }) {
                    Some(c.jid.clone())
                } else if !input.contains(' ') {
                    Some(self.full_user_jid(&input))
                } else {
                    None
                };
                match resolved {
                    Some(jid) => {
                        self.jabber_dm_input.clear();
                        self.jabber_dm_error.clear();
                        self.settings.jabber_closed_dms.retain(|j| j != &jid);
                        self.jabber_unforget(&jid);
                        self.needs_save = true;
                        self.jabber_mark_read(&jid);
                        self.jabber_open(&jid, ChatWinKey::Main);
                        close = true;
                    }
                    None => {
                        self.jabber_dm_error = format!("No contact matching \"{input}\"");
                    }
                }
            }
            if !self.jabber_dm_error.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(&self.jabber_dm_error)
                        .color(crate::theme::standing::WARNING),
                );
            }
            }
        });
        if let Some(jid) = pick {
            self.jabber_room_input.clear();
            self.jabber_dm_input.clear();
            self.settings.jabber_closed_dms.retain(|j| j != &jid);
            self.settings.jabber_closed_rooms.retain(|j| j != &jid);
            self.jabber_unforget(&jid);
            self.jabber_unleave(&jid);
            self.jabber_mark_read(&jid);
            self.needs_save = true;
            self.jabber_open(&jid, ChatWinKey::Main);
            close = true;
        }
        if close || !open {
            self.jabber_join_open = false;
            self.jabber_dm_error.clear();
        }
    }

    pub(crate) fn jabber_view(&mut self, ui: &mut egui::Ui, f: &JabberFrame) {
        ui.add_space(8.0);
        self.jabber_ui(ui, f);
    }

    /// The Convos list: direct messages above rooms, each newest first.
    ///
    /// DMs go on top because they are addressed to you personally and are easy to miss beside
    /// everything else. Recency rather than name order, because the conversation you want next is
    /// usually the one that just moved.
    pub(crate) fn jabber_convos_list_ui(&mut self, ui: &mut egui::Ui, f: &JabberFrame, search: &str) {
        let matches = |name: &str, jid: &str| {
            search.is_empty()
                || name.to_lowercase().contains(search)
                || jid.to_lowercase().contains(search)
        };

        let contacts: std::collections::HashSet<&String> =
            self.settings.jabber_contacts.iter().collect();
        let dm_keys: std::collections::HashSet<&String> = f.dm_keys.iter().collect();
        let is_dm = |jid: &String| is_direct_message(jid, &dm_keys, &contacts);

        // Anything unread joins the list and stays for as long as the tab is open. Rooms have their
        // own list and must not be stuck into this one.
        for c in f.convos.iter().filter(|c| c.unread && is_dm(&c.jid)) {
            self.jabber_sticky.insert(c.jid.clone());
        }

        let closed: std::collections::HashSet<&String> =
            self.settings.jabber_closed_dms.iter().collect();
        let mut dms: Vec<&Convo> = f
            .convos
            .iter()
            .filter(|c| shows_in_dm_list(&c.jid, &dm_keys, &contacts, &closed, &self.jabber_sticky))
            .filter(|c| matches(&c.name, &c.jid))
            .collect();
        // Unread first, then most recent. An unread conversation with no history yet would sort to
        // the bottom on recency alone, which is the one place it must not be.
        dms.sort_by(|a, b| {
            b.unread
                .cmp(&a.unread)
                .then(b.last_at.cmp(&a.last_at))
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let mut rooms: Vec<&ChannelRow> =
            f.channels.iter().filter(|c| matches(&c.name, &c.jid)).collect();
        rooms.sort_by(|a, b| {
            b.unread
                .cmp(&a.unread)
                .then(b.last_at.cmp(&a.last_at))
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let accent = ui.visuals().hyperlink_color;
        let mut open: Option<String> = None;
        let mut start: Option<bool> = None;
        // Collected rather than applied inside the closure: the row renderer borrows `self`.
        let mut motd: Option<String> = None;
        egui::ScrollArea::vertical().id_salt("convos").auto_shrink([false, false]).show(ui, |ui| {
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.bg_stroke = egui::Stroke::NONE;
            w.hovered.bg_stroke = egui::Stroke::NONE;
            w.active.bg_stroke = egui::Stroke::NONE;

            if dms.is_empty() && rooms.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Nothing yet. Start one from the Directory.").weak(),
                );
                return;
            }

            let section = |ui: &mut egui::Ui, title: &str, n: usize| {
                if n == 0 {
                    return;
                }
                ui.add_space(7.0);
                ui.label(egui::RichText::new(title).strong().size(15.0).color(accent));
            };
            section(ui, "Direct messages", dms.len());
            ui.push_id("dmlist", |ui| {
            for c in &dms {
                let (r, g, b) = c.presence.color();
                if self
                    .jabber_convo_row(
                        ui,
                        &c.jid,
                        &c.name,
                        c.unread_count,
                        c.mention,
                        Some(egui::Color32::from_rgb(r, g, b)),
                        "",
                        false,
                    )
                    .clicked()
                {
                    open = Some(c.jid.clone());
                }
            }
            });
            if self.jabber_start_row(ui, egui_phosphor::regular::CHAT_CIRCLE_DOTS, "Start a DM") {
                start = Some(false);
            }
            section(ui, "Rooms", rooms.len());
            ui.push_id("roomlist", |ui| {
            for c in &rooms {
                let row = self.jabber_convo_row(
                    ui,
                    &c.jid,
                    &c.name,
                    c.unread_count,
                    c.mention,
                    None,
                    &c.motd,
                    c.inaccessible,
                );
                if !c.motd.trim().is_empty() {
                    row.context_menu(|ui| {
                        if ui.button("Show MOTD").clicked() {
                            motd = Some(c.jid.clone());
                            ui.close();
                        }
                    });
                }
                if row.clicked() {
                    open = Some(c.jid.clone());
                }
            }
            });
            if self.jabber_start_row(ui, egui_phosphor::regular::USERS_THREE, "Join a room") {
                start = Some(true);
            }
        });
        if let Some(jid) = motd {
            self.jabber_motd_window = Some(jid);
        }
        if let Some(rooms) = start {
            self.jabber_dm_error.clear();
            self.jabber_join_rooms = rooms;
            self.jabber_join_open = true;
        }
        if let Some(jid) = open {
            // Reopening has to undo every reason the conversation was hidden, not just one: a row
            // that is listed but stays closed looks like the click did nothing.
            self.settings.jabber_closed_dms.retain(|j| j != &jid);
            self.settings.jabber_closed_rooms.retain(|j| j != &jid);
            self.jabber_unforget(&jid);
            self.jabber_unleave(&jid);
            self.jabber_mark_read(&jid);
            self.needs_save = true;
            self.jabber_open(&jid, ChatWinKey::Main);
        }
    }

    /// A dialog's recent list: scrollable, and every row a full-width target that lights up.
    pub(crate) fn jabber_recent_list(
        ui: &mut egui::Ui,
        salt: &str,
        rows: &[(String, String)],
        icon: &str,
        pick: &mut Option<String>,
    ) {
        if rows.is_empty() {
            return;
        }
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .max_height(220.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // A fixed box, not a cap. A dialog that shrinks as the search narrows moves the row
                // you were reaching for out from under the pointer.
                ui.set_min_height(220.0);
                for (jid, name) in rows {
                    let bg = ui.painter().add(egui::Shape::Noop);
                    let inner = ui
                        .horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(icon).weak());
                            ui.add(egui::Label::new(name).truncate()).on_hover_text(jid);
                        })
                        .response;
                    let row = egui::Rect::from_min_max(
                        egui::pos2(ui.min_rect().left(), inner.rect.top() - 1.0),
                        egui::pos2(ui.min_rect().right(), inner.rect.bottom() + 1.0),
                    );
                    let resp = ui.interact(row, ui.id().with((salt, jid)), egui::Sense::click());
                    if resp.hovered() {
                        ui.painter().set(
                            bg,
                            egui::Shape::rect_filled(
                                row,
                                3.0,
                                ui.visuals().widgets.hovered.bg_fill,
                            ),
                        );
                    }
                    if resp.clicked() {
                        *pick = Some(jid.clone());
                    }
                }
            });
    }

    /// The last entry in a section: the way to start a conversation that is not listed yet.
    pub(crate) fn jabber_start_row(&self, ui: &mut egui::Ui, icon: &str, label: &str) -> bool {
        let bg = ui.painter().add(egui::Shape::Noop);
        let inner = ui
            .horizontal(|ui| {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!("{icon}  {label}"))
                        .color(ui.visuals().hyperlink_color),
                );
            })
            .response;
        let row = egui::Rect::from_min_max(
            egui::pos2(ui.min_rect().left(), inner.rect.top() - 1.0),
            egui::pos2(ui.min_rect().right(), inner.rect.bottom() + 1.0),
        );
        let resp = ui.interact(row, ui.id().with(label), egui::Sense::click());
        if resp.hovered() {
            ui.painter().set(
                bg,
                egui::Shape::rect_filled(row, 3.0, ui.visuals().widgets.hovered.bg_fill),
            );
        }
        resp.clicked()
    }

    /// One row: a presence dot, the name, an unread count, and a mention marker.
    ///
    /// The whole row is the hit target and the whole row carries the hover and selection fill. A
    /// label-sized target in a full-width list means most of the row does nothing when clicked and
    /// nothing when pointed at, which reads as the list being dead.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn jabber_convo_row(
        &self,
        ui: &mut egui::Ui,
        jid: &str,
        name: &str,
        unread: u32,
        mention: bool,
        presence: Option<egui::Color32>,
        motd: &str,
        inaccessible: bool,
    ) -> egui::Response {
        let selected = self.jabber_chat.as_deref() == Some(jid);
        // Reserved now, filled in once the row's own height is known: painting a background after
        // the content would paint over it.
        let bg = ui.painter().add(egui::Shape::Noop);
        let inner = ui
            .horizontal(|ui| {
                ui.add_space(2.0);
                match presence {
                    // Filled, not an outline glyph: a ring at this size reads as absent rather than
                    // as a status, and is invisible for anyone offline.
                    Some(c) => {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, c);
                    }
                    None => {
                        ui.label(egui::RichText::new(egui_phosphor::regular::USERS_THREE).weak());
                    }
                }
                let mut text = egui::RichText::new(truncate_to(
                    name,
                    fit_chars(ui.available_width() - 52.0),
                ));
                if unread > 0 {
                    text = text.strong();
                }
                if inaccessible {
                    text = text.strikethrough().weak();
                }
                ui.add(egui::Label::new(text).truncate().selectable(false));
                if unread > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (fg, bg) = if mention {
                            (egui::Color32::WHITE, ui.visuals().hyperlink_color)
                        } else {
                            (ui.visuals().text_color(), ui.visuals().widgets.inactive.bg_fill)
                        };
                        egui::Frame::new()
                            .fill(bg)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(6, 1))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(badge_count(unread)).color(fg).strong());
                            });
                    });
                }
            })
            .response;

        // The whole width, not just what the content happened to fill.
        let row = egui::Rect::from_min_max(
            egui::pos2(ui.min_rect().left(), inner.rect.top() - 1.0),
            egui::pos2(ui.min_rect().right(), inner.rect.bottom() + 1.0),
        );
        // Salted with the list this row is in, not just the jid: the same conversation can legitimately
        // appear in two lists (a search result, a contact that is also a recent chat), and two rows
        // sharing one id means egui hit-tests one of them and the other is painted but dead.
        let resp = ui.interact(row, ui.id().with(jid), egui::Sense::click());
        let fill = if selected {
            ui.visuals().selection.bg_fill
        } else if resp.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        if fill != egui::Color32::TRANSPARENT {
            ui.painter().set(bg, egui::Shape::rect_filled(row, 3.0, fill));
        }
        if motd.trim().is_empty() {
            resp
        } else {
            // Six lines, not the whole notice board: a full MOTD tooltip is taller than the sidebar
            // it hangs off and covers the list it is describing.
            resp.on_hover_text(motd_preview(motd, 6))
        }
    }

    /// One lock, one snapshot of everything a chat window needs to draw itself. Messages stay
    /// out of it: each window borrows its own conversation under the lock while it draws.
    pub(crate) fn jabber_frame(&self, focused: bool) -> JabberFrame {
        let mut st = self.jabber.lock().unwrap();
        let configured = self.settings.jabber_enabled
            && !self.settings.jabber_jid.trim().is_empty()
            && crate::jabber::has_password(self.settings.jabber_jid.trim())
            && st.fatal.is_none();
        let ever_online = st.ever_online;
        // The open conversation is being looked at, so its own incoming messages must not raise an
        // unread or mention marker. Clear it every focused frame, before reading the marker sets
        // below (activation via click also clears once, but later messages would re-mark it).
        if configured && ever_online {
            if focused {
                if let Some(active) = self.jabber_chat.clone() {
                    st.unread.remove(&active);
                    st.unread_counts.remove(&active);
                    st.mentions.remove(&active);
                }
            }
            // Same rule per pop-out, from the focus it recorded last frame: a conversation you are
            // staring at in a pop-out must not keep an unread dot either.
            for w in self.jabber_popouts.iter().filter(|w| w.focused) {
                if let Some(a) = &w.active {
                    st.unread.remove(a);
                    st.unread_counts.remove(a);
                    st.mentions.remove(a);
                }
            }
        }
        let st = &*st;
        let mut set: std::collections::BTreeMap<String, Convo> =
            std::collections::BTreeMap::new();
        for (jid, c) in &st.roster {
            set.entry(jid.clone()).or_insert_with(|| Convo {
                jid: jid.clone(),
                name: c.name.clone().unwrap_or_else(|| jid.split('@').next().unwrap_or(jid).to_owned()),
                unread: false,
                unread_count: 0,
                mention: false,
                last_at: 0,
                group: c.groups.first().cloned().unwrap_or_else(|| "Other".to_owned()),
                presence: c.presence,
                status_text: c.status_text.clone(),
                in_roster: true,
            });
        }
        let forgotten: std::collections::HashSet<&String> =
            self.settings.jabber_forgotten.iter().collect();
        for jid in st.chats.keys().filter(|j| !forgotten.contains(*j)) {
            let pres = st.presences.get(jid).map(|(p, _)| *p).unwrap_or_default();
            set.entry(jid.clone()).or_insert_with(|| Convo {
                jid: jid.clone(),
                name: jid.split('@').next().unwrap_or(jid).to_owned(),
                unread: false,
                unread_count: 0,
                mention: false,
                last_at: 0,
                group: "Other".to_owned(),
                presence: pres,
                status_text: String::new(),
                in_roster: false,
            });
        }
        for jid in &st.unread {
            if let Some(e) = set.get_mut(jid) {
                e.unread = true;
            }
        }
        for (jid, e) in set.iter_mut() {
            e.unread_count = st.unread_counts.get(jid).copied().unwrap_or(0);
            e.mention = st.mentions.contains(jid);
            // Recency comes from the history rather than a separate clock: the last message is
            // exactly what "most recent conversation" means, and it cannot drift from it.
            e.last_at = st.chats.get(jid).and_then(|c| c.last()).map(|m| m.time).unwrap_or(0);
        }
        let convos: Vec<Convo> = set.into_values().collect();
        let rooms: Vec<String> = st.rooms.iter().cloned().collect();
        let dm_keys: Vec<String> = st
            .chats
            .keys()
            .filter(|k| {
                !st.rooms.contains(*k)
                    && !st.rooms_left.contains(*k)
                    && !forgotten.contains(*k)
                    && k.as_str() != crate::jabber::PING_FEED_KEY
                    && valid_bare_jid(k)
            })
            .cloned()
            .collect();
        let unread = st.unread.clone();
        let mentions = st.mentions.clone();
        // Channels list = every known room (persisted + currently joined + kicked), so a room
        // stays listed after we're removed from it. History lives in `chats` regardless.
        let mut known: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
        known.extend(self.settings.jabber_rooms.iter());
        known.extend(st.rooms.iter());
        known.extend(st.rooms_inaccessible.iter());
        known.remove(&crate::jabber::PING_FEED_KEY.to_owned());
        known.retain(|j| !st.rooms_left.contains(*j) && !forgotten.contains(*j));
        let mut channels: Vec<ChannelRow> = known
            .into_iter()
            .map(|jid| ChannelRow {
                jid: jid.clone(),
                name: jid.split('@').next().unwrap_or(jid).to_owned(),
                unread: st.unread.contains(jid),
                unread_count: st.unread_counts.get(jid).copied().unwrap_or(0),
                mention: st.mentions.contains(jid),
                last_at: st.chats.get(jid).and_then(|c| c.last()).map(|m| m.time).unwrap_or(0),
                inaccessible: st.rooms_inaccessible.contains(jid),
                motd: st.room_subjects.get(jid).cloned().unwrap_or_default(),
            })
            .collect();
        channels.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let inaccessible_list: Vec<String> = st.rooms_inaccessible.iter().cloned().collect();
        let subjects_snapshot = st.room_subjects.clone();
        JabberFrame {
            configured,
            ever_online,
            connected: st.connected,
            status: st.status.clone(),
            convos,
            pings: st.pings.clone(),
            rooms,
            dm_keys,
            unread,
            mentions,
            pings_unread: st.pings_unread,
            channels,
            inaccessible: inaccessible_list,
            subjects: subjects_snapshot,
        }
    }

    /// Bring every chat window's tabs in line with the joined rooms and DM history, exactly once
    /// per frame and before any window renders. Running it per window would let one window re-add
    /// a tab another one owns.
    pub(crate) fn jabber_reconcile(&mut self, f: &JabberFrame) {
        // The pinned rescue room may have been left or forgotten before it was pinned, or while
        // Rescue Mode was off. Undo that even while offline, so the next connect joins it. Hiding
        // its tab is left alone: that is safe and keeps the room joined.
        for p in self.jabber_rescue_rooms() {
            let was = (self.settings.jabber_left_rooms.len(), self.settings.jabber_forgotten.len());
            let was_closed = self.settings.jabber_closed_rooms.len();
            self.settings.jabber_left_rooms.retain(|r| r != &p);
            self.settings.jabber_forgotten.retain(|j| j != &p);
            // Held open, so a hidden flag on one is state nothing can act on.
            self.settings.jabber_closed_rooms.retain(|r| r != &p);
            let mut changed =
                was != (self.settings.jabber_left_rooms.len(), self.settings.jabber_forgotten.len())
                    || was_closed != self.settings.jabber_closed_rooms.len();
            if !self.settings.jabber_rooms.contains(&p) {
                self.settings.jabber_rooms.push(p.clone());
                changed = true;
            }
            if changed {
                self.jabber.lock().unwrap().rooms_left.remove(&p);
                self.needs_save = true;
            }
        }
        if !f.configured || !f.ever_online {
            return;
        }
        let mut save = false;
        // An incoming DM (present in `unread`) reopens a conversation whose tab was closed. A
        // hidden room is deliberately hidden while still joined, so only being named in it is loud
        // enough to bring the tab back. Reopening on ordinary room traffic would undo the hide
        // within seconds of every reconnect.
        for k in &f.unread {
            if let Some(p) = self.settings.jabber_closed_dms.iter().position(|j| j == k) {
                self.settings.jabber_closed_dms.remove(p);
                save = true;
            }
            // A forgotten conversation is curation, not a mute. A new message outranks it, or
            // removing a row from the sidebar would silently drop mail.
            if let Some(p) = self.settings.jabber_forgotten.iter().position(|j| j == k) {
                self.settings.jabber_forgotten.remove(p);
                save = true;
            }
        }
        for k in &f.mentions {
            if let Some(p) = self.settings.jabber_closed_rooms.iter().position(|j| j == k) {
                self.settings.jabber_closed_rooms.remove(p);
                save = true;
            }
        }
        // A room the server put us in (bookmark, invite, force-join) is only known to this
        // session; persist it so we rejoin it ourselves next time. Being in `f.rooms` at all means
        // the server put us back in a room we had left, which overrides the leave.
        let mut force_joined: Vec<String> = Vec::new();
        for rjid in &f.rooms {
            if self.settings.jabber_left_rooms.iter().any(|r| r == rjid) {
                self.settings.jabber_left_rooms.retain(|r| r != rjid);
                save = true;
            }
            if self.settings.jabber_forgotten.iter().any(|j| j == rjid) {
                self.settings.jabber_forgotten.retain(|j| j != rjid);
                save = true;
            }
            if !self.settings.jabber_rooms.iter().any(|r| r == rjid) {
                self.settings.jabber_rooms.push(rjid.clone());
                // First sight of a room we never asked to be in. Open it once so it is not just a
                // new sidebar row, then it is an ordinary room. This branch cannot fire twice: the
                // next frame finds it in `jabber_rooms`. Hand-joins never reach here, they are put
                // in `jabber_rooms` before the join lands.
                self.settings.jabber_closed_rooms.retain(|r| r != rjid);
                force_joined.push(rjid.clone());
                save = true;
            }
        }
        let closed_dms: std::collections::HashSet<String> =
            self.settings.jabber_closed_dms.iter().cloned().collect();
        let closed_rooms: std::collections::HashSet<String> =
            self.settings.jabber_closed_rooms.iter().cloned().collect();
        let room_set: std::collections::HashSet<&String> = f.rooms.iter().collect();
        // The open tabs are the ones restored from settings at startup. Nothing is added because a
        // room happens to be joined or a DM has history, which would rebuild the whole tab bar on
        // every start.
        let mut want: Vec<String> = self
            .tab_set()
            .all_tabs()
            .into_iter()
            .filter(|t| {
                if room_set.contains(t) {
                    !closed_rooms.contains(t)
                } else {
                    !closed_dms.contains(t)
                }
            })
            .collect();
        // New traffic still surfaces a conversation, or an incoming DM from someone with no tab
        // would be invisible outside the sidebar. A room needs a mention, a DM needs a message,
        // and neither reopens something on a closed-list.
        for k in &f.unread {
            let is_room = room_set.contains(k);
            let closed = if is_room { closed_rooms.contains(k) } else { closed_dms.contains(k) };
            let loud = if is_room { f.mentions.contains(k) } else { true };
            if loud && !closed && !want.contains(k) {
                want.push(k.clone());
            }
        }
        for r in force_joined {
            if !want.contains(&r) {
                want.push(r);
            }
        }
        // Rescue Mode's rooms are held open, not just joined. The FC has to be able to see
        // delve911 and skirmish_commanders without going looking for them.
        for p in self.jabber_rescue_rooms() {
            if !want.contains(&p) {
                want.push(p);
            }
        }
        let mut t = self.tab_set();
        reconcile_tabs(&mut t, &want);
        t.normalize();
        let empty = t.empty_popouts();
        if !empty.is_empty() {
            self.jabber_popouts.retain(|w| !empty.contains(&w.id));
        }
        if save {
            self.needs_save = true;
        }
    }

    /// `convos` picks the Convos list; otherwise the Directory.
    #[cfg(test)]
    pub(crate) fn jabber_sidebar_for_test(&mut self, ui: &mut egui::Ui, f: &JabberFrame, convos: bool) {
        self.jabber_pane = if convos { JabberPane::Convos } else { JabberPane::Directory };
        self.jabber_ui(ui, f);
    }

    /// The start dialog on its own, so each half can be screenshotted without driving a click.
    #[cfg(test)]
    pub(crate) fn jabber_join_dialog_for_test(
        &mut self,
        ctx: &egui::Context,
        f: &JabberFrame,
        rooms: bool,
    ) {
        self.jabber_join_open = true;
        self.jabber_join_rooms = rooms;
        self.jabber_join_dialog(ctx, &f.convos, &f.channels);
        self.jabber_motd_dialog(ctx, &f.channels);
    }

    pub(crate) fn jabber_ui(&mut self, ui: &mut egui::Ui, f: &JabberFrame) {
        if !f.configured {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Connect to your alliance Jabber (XMPP) for chat and fleet pings.",
                )
                .weak(),
            );
            ui.label(egui::RichText::new("Imperium: jabber-server.goonfleet.com").weak());
            ui.add_space(6.0);
            egui::Grid::new("jabber_login").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
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
                )
                .on_hover_text("XMPP server host (the JID domain usually has no SRV record)");
                ui.end_row();
                ui.label("Password");
                let pw_hint = if crate::jabber::has_password(self.settings.jabber_jid.trim()) {
                    "<saved password>"
                } else {
                    ""
                };
                ui.add(
                    egui::TextEdit::singleline(&mut self.jabber_pw_input)
                        .password(true)
                        .hint_text(pw_hint)
                        .desired_width(260.0),
                );
                ui.end_row();
            });
            if ui.button("Connect").clicked() {
                let jid = self.settings.jabber_jid.trim().to_owned();
                if let Some(err) = crate::jabber::jid_format_error(&jid) {
                    let mut s = self.jabber.lock().unwrap();
                    s.fatal = Some(err.clone());
                    s.status = err;
                } else {
                    // Save a freshly typed password, otherwise reuse the stored one (e.g. retrying
                    // after a network error without re-typing it).
                    let ready = if !self.jabber_pw_input.is_empty() {
                        match crate::jabber::save_password(&jid, &self.jabber_pw_input) {
                            Ok(()) => {
                                self.jabber_pw_input.clear();
                                true
                            }
                            Err(e) => {
                                self.jabber.lock().unwrap().status = format!("Keychain error: {e}");
                                false
                            }
                        }
                    } else {
                        crate::jabber::has_password(&jid)
                    };
                    if ready {
                        let mut s = self.jabber.lock().unwrap();
                        s.fatal = None;
                        s.status = "Connecting…".to_owned();
                        drop(s);
                        self.settings.jabber_enabled = true;
                        self.needs_save = true;
                    } else {
                        self.jabber.lock().unwrap().status = "Enter your password".to_owned();
                    }
                }
            }
            let (status, fatal) = {
                let s = self.jabber.lock().unwrap();
                (s.status.clone(), s.fatal.is_some())
            };
            if !status.is_empty() {
                ui.add_space(4.0);
                let txt = egui::RichText::new(status);
                ui.label(if fatal { txt.color(crate::theme::standing::HOSTILE) } else { txt.weak() });
            }
            return;
        }

        if !f.ever_online {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(28.0));
                ui.add_space(10.0);
                let status = self.jabber.lock().unwrap().status.clone();
                let txt = if status.is_empty() { "Connecting…".to_owned() } else { status };
                ui.label(egui::RichText::new(txt).weak());
                ui.add_space(10.0);
                if ui.button("Cancel").clicked() {
                    self.settings.jabber_enabled = false;
                    self.needs_save = true;
                    self.jabber.lock().unwrap().status.clear();
                }
            });
            return;
        }

        // Mirror the live MOTD + kicked-room state into settings so history-only channels keep
        // their strike-through and last-known MOTD across restarts.
        if self.settings.jabber_inaccessible_rooms != f.inaccessible {
            self.settings.jabber_inaccessible_rooms = f.inaccessible.clone();
            self.needs_save = true;
        }
        if self.settings.jabber_room_subjects != f.subjects {
            self.settings.jabber_room_subjects = f.subjects.clone();
            self.needs_save = true;
        }

        let mut presence_changed = false;
        let mut pop_all = false;
        ui.horizontal(|ui| {
            if f.connected {
                use crate::jabber::Presence;
                let (r, g, b) = self.jabber_my_presence.color();
                status_dot(ui, egui::Color32::from_rgb(r, g, b), 10.0);
                ui.label(egui::RichText::new(&self.settings.jabber_jid).weak());
                egui::ComboBox::from_id_salt("my_presence")
                    .selected_text(self.jabber_my_presence.label())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for p in [Presence::Online, Presence::Away, Presence::Xa, Presence::Dnd] {
                            if ui
                                .menu_value(&mut self.jabber_my_presence, p, p.label())
                                .clicked()
                            {
                                presence_changed = true;
                            }
                        }
                    });
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.jabber_my_status)
                        .hint_text("status message")
                        .desired_width(150.0),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    presence_changed = true;
                }
            } else {
                status_dot(ui, crate::theme::standing::WARNING, 10.0);
                ui.label(egui::RichText::new(f.status.as_str()).weak());
                self.jabber_retry_button(ui);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Disconnect").clicked() {
                    self.settings.jabber_enabled = false;
                    self.needs_save = true;
                }
                if ui
                    .add_enabled(
                        !self.jabber_tabs.is_empty() && self.jabber_popouts.len() < MAX_POPOUTS,
                        egui::Button::new(egui_phosphor::regular::ARROW_SQUARE_OUT),
                    )
                    .on_hover_text("Pop out all conversations into one window")
                    .clicked()
                {
                    pop_all = true;
                }
                if ui
                    .button(egui_phosphor::regular::BELL_RINGING)
                    .on_hover_text("Ping alert rules")
                    .clicked()
                {
                    self.mention_input = self.settings.jabber_mention_keywords.join(", ");
                    self.ping_rules_open = true;
                }
                #[cfg(feature = "fc-rescue")]
                if self.settings.fc_rescue_enabled
                    && ui
                        .button(egui_phosphor::regular::WARNING_OCTAGON)
                        .on_hover_text("Open capital rescue (cap save)")
                        .clicked()
                {
                    self.view = nav::View::Rescue;
                }
            });
        });
        if presence_changed {
            if let Some(tx) = &self.jabber_tx {
                let _ = tx.send(crate::jabber::Cmd::SetPresence {
                    show: self.jabber_my_presence,
                    status: self.jabber_my_status.clone(),
                });
            }
        }
        if pop_all {
            let all: Vec<String> = self.jabber_tabs.clone();
            if let Some(first) = all.first() {
                if let Some(id) = self.new_popout(first, None) {
                    for j in all.iter().skip(1) {
                        self.tab_set().move_tab(j, ChatWinKey::Popout(id), None);
                    }
                }
            }
        }
        ui.separator();

        egui::Panel::left("jabber_split")
            .resizable(true)
            .default_size(210.0)
            .size_range(150.0..=460.0)
            .show_inside(ui, |ui| {
                let contacts: std::collections::HashSet<String> =
                    self.settings.jabber_contacts.iter().cloned().collect();
                ui.horizontal(|ui| {
                    let con = selectable_chip(ui, self.jabber_pane == JabberPane::Convos, "Convos");
                    if con.clicked() {
                        self.jabber_pane = JabberPane::Convos;
                    }
                    let dir = selectable_chip(ui, self.jabber_pane == JabberPane::Directory, "Directory");
                    if dir.clicked() {
                        self.jabber_pane = JabberPane::Directory;
                    }
                });
                // Conversations that surfaced because they went unread stay listed until you leave
                // the tab, so a DM you just read does not vanish out from under the pointer.
                if self.jabber_pane != JabberPane::Convos {
                    self.jabber_sticky.clear();
                }
                ui.add_sized(
                    [ui.available_width(), 20.0],
                    egui::TextEdit::singleline(&mut self.jabber_contact_search).hint_text("Search"),
                );
                let search = self.jabber_contact_search.to_lowercase();
                if self.jabber_pane == JabberPane::Convos {
                    self.jabber_convos_list_ui(ui, f, &search);
                } else {
                let show_dir = true;
                let shown: Vec<&Convo> = f
                    .convos
                    .iter()
                    .filter(|c| show_dir || contacts.contains(&c.jid))
                    .filter(|c| {
                        search.is_empty()
                            || c.name.to_lowercase().contains(&search)
                            || c.jid.to_lowercase().contains(&search)
                    })
                    .collect();
                let mut groups: std::collections::BTreeMap<&str, Vec<&Convo>> =
                    std::collections::BTreeMap::new();
                for c in shown {
                    let g = if c.group.trim().is_empty() { "Other" } else { c.group.as_str() };
                    groups.entry(g).or_default().push(c);
                }
                let accent = ui.visuals().hyperlink_color;
                let mut toggle_contact: Option<(String, bool)> = None;
                let mut forget_convo: Option<String> = None;
                let pinned = self.jabber_rescue_rooms();
                egui::ScrollArea::vertical().id_salt("convos").auto_shrink([false, false]).show(ui, |ui| {
                    // Roster rows are list items, not chips: a border here is too heavy and would pop
                    // in on hover. Keep the fill highlight, drop the stroke, so nothing shifts.
                    let w = &mut ui.visuals_mut().widgets;
                    w.inactive.bg_stroke = egui::Stroke::NONE;
                    w.hovered.bg_stroke = egui::Stroke::NONE;
                    w.active.bg_stroke = egui::Stroke::NONE;
                    if groups.is_empty() && !show_dir {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("No contacts yet. Add people from the Directory.").weak());
                    }
                    for (group, mut members) in groups {
                        members.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        let online = members.iter().filter(|c| c.presence.online()).count();
                        ui.add_space(7.0);
                        let collapsed = self.jabber_collapsed.contains(group);
                        let grp_unread = members.iter().any(|c| c.unread);
                        let hdr = ui
                            .horizontal(|ui| {
                                let caret = if collapsed {
                                    egui_phosphor::regular::CARET_RIGHT
                                } else {
                                    egui_phosphor::regular::CARET_DOWN
                                };
                                let gname = truncate_to(group, fit_chars(ui.available_width() - 40.0));
                                let r = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("{caret}  {gname}"))
                                            .strong()
                                            .size(15.0)
                                            .color(accent),
                                    )
                                    .truncate()
                                    .sense(egui::Sense::click()),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{online}/{}", members.len())).weak(),
                                );
                                if collapsed && grp_unread {
                                    ui.label(
                                        egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                            .color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C))
                                            .size(8.0),
                                    );
                                }
                                r
                            })
                            .inner;
                        if hdr.clicked() {
                            if collapsed {
                                self.jabber_collapsed.remove(group);
                            } else {
                                self.jabber_collapsed.insert(group.to_owned());
                            }
                        }
                        if collapsed {
                            continue;
                        }
                        for c in members {
                            let sel = self.jabber_chat.as_deref() == Some(c.jid.as_str());
                            let (r, g, b) = c.presence.color();
                            let disp = truncate_to(
                                &c.name,
                                fit_chars(
                                    ui.available_width()
                                        - 34.0
                                        - if c.in_roster { 0.0 } else { 20.0 }
                                        - if c.unread { 16.0 } else { 0.0 },
                                ),
                            );
                            let name = if c.unread {
                                egui::RichText::new(disp).strong()
                            } else if c.presence.online() {
                                egui::RichText::new(disp)
                            } else {
                                egui::RichText::new(disp).weak()
                            };
                            let is_contact = contacts.contains(&c.jid);
                            let resp = ui.horizontal(|ui| {
                                status_dot(ui, egui::Color32::from_rgb(r, g, b), 9.0);
                                let clicked = ui.menu_label(sel, name)
                                    .on_hover_text(&c.name)
                                    .clicked();
                                if c.unread {
                                    ui.label(
                                        egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                            .color(egui::Color32::from_rgb(0xE0, 0x4C, 0x4C))
                                            .size(8.0),
                                    );
                                }
                                let star_col = if is_contact {
                                    ui.visuals().hyperlink_color
                                } else {
                                    ui.visuals().weak_text_color()
                                };
                                if ui
                                    .add(
                                        // Not `.small()`: at 9px this is hard to hit on purpose
                                        // and easy to hit by accident, and it adds or drops a
                                        // contact.
                                        egui::Button::new(
                                            egui::RichText::new(egui_phosphor::regular::STAR)
                                                .size(15.0)
                                                .color(star_col),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text(if is_contact { "Remove from contacts" } else { "Add to contacts" })
                                    .clicked()
                                {
                                    toggle_contact = Some((c.jid.clone(), !is_contact));
                                }
                                // Roster rows come from the server and would be back on the next
                                // push, so only a conversation we remember ourselves can be dropped.
                                let blocked = pinned.contains(&c.jid).then_some(PINNED_ROOM_TIP);
                                if !c.in_roster && forget_button(ui, &c.name, blocked) {
                                    forget_convo = Some(c.jid.clone());
                                }
                                clicked
                            });
                            let tip = if c.status_text.is_empty() {
                                c.presence.label().to_owned()
                            } else {
                                format!("{} — {}", c.presence.label(), c.status_text)
                            };
                            resp.response.on_hover_text(tip);
                            if resp.inner {
                                self.settings.jabber_closed_dms.retain(|j| j != &c.jid);
                                self.jabber_open(&c.jid, ChatWinKey::Main);
                                self.jabber_mark_read(&c.jid);
                            }
                        }
                    }
                });
                if let Some((jid, add)) = toggle_contact {
                    if add {
                        if !self.settings.jabber_contacts.contains(&jid) {
                            self.settings.jabber_contacts.push(jid);
                        }
                    } else {
                        self.settings.jabber_contacts.retain(|j| j != &jid);
                    }
                    self.needs_save = true;
                }
                if let Some(jid) = forget_convo {
                    let is_room = f.rooms.iter().any(|r| r == &jid);
                    self.jabber_forget(&jid, is_room);
                }
                }
            });
        let mut out: Vec<TabAction> = Vec::new();
        self.jabber_window_body(ui, ChatWinKey::Main, f, &mut out);
        self.jabber_join_dialog(ui.ctx(), &f.convos, &f.channels);
        self.jabber_motd_dialog(ui.ctx(), &f.channels);
        self.apply_tab_actions(out);
    }

    /// Tab bar + body for one chat window. No sidebar: that belongs to the Jabber page.
    pub(crate) fn jabber_window_body(
        &mut self,
        ui: &mut egui::Ui,
        win: ChatWinKey,
        f: &JabberFrame,
        out: &mut Vec<TabAction>,
    ) {
        let mut bar: Vec<TabAction> = Vec::new();
        egui::Panel::top(egui::Id::new(("jabber_tab_bar", win)))
            .frame(egui::Frame::new().fill(ui.visuals().panel_fill))
            .show_inside(ui, |ui| self.jabber_tab_bar_ui(ui, win, f, &mut bar));
        // Applied between the bar and the body so a tab click switches the conversation in the
        // same frame.
        self.apply_tab_actions(bar);
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.push_id(("jwin", win), |ui| match (win, self.win_active(win)) {
                (ChatWinKey::Main, None) => self.jabber_pings_ui(ui, f),
                // A pop-out has no pings feed, and is briefly tabless on the frame its last
                // conversation moves out, before `jabber_reconcile` prunes the window.
                (_, None) => {
                    ui.add_space(12.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("No conversations in this window.").weak());
                    });
                }
                (_, Some(jid)) => self.jabber_conversation_ui(ui, win, &jid, f, out),
            });
        });
        if self.jabber_drop_highlight(win) {
            ui.painter().rect_stroke(
                ui.max_rect().shrink(1.0),
                4.0,
                egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }
    }

    pub(crate) fn jabber_tab_bar_ui(
        &mut self,
        ui: &mut egui::Ui,
        win: ChatWinKey,
        f: &JabberFrame,
        out: &mut Vec<TabAction>,
    ) {
        let tabs: Vec<String> = self.win_tabs(win).to_vec();
        let active = self.win_active(win);
        let is_main = win == ChatWinKey::Main;
        // Everything the context menu and the drop maths need, read off `self` before the render
        // closures so no borrow of `self` has to survive into them.
        let inner_origin: Option<egui::Pos2> = match win {
            ChatWinKey::Main => self.jabber_main_rect.map(|(_, inner)| inner.min),
            ChatWinKey::Popout(id) => self.popout(id).and_then(|w| w.inner).map(|r| r.min),
        };
        let own_outer: Option<egui::Rect> = match win {
            ChatWinKey::Main => self.jabber_main_rect.map(|(outer, _)| outer),
            ChatWinKey::Popout(id) => self.popout(id).and_then(|w| w.outer),
        };
        // Pop-out rects are one frame old, because pop-outs render after the main panel. Windows
        // do not teleport in 16ms, so a stale rect is good enough to hit-test a drop.
        let mut drop_rects: Vec<(ChatWinKey, egui::Rect)> = Vec::new();
        if let Some((outer, _)) = self.jabber_main_rect {
            drop_rects.push((ChatWinKey::Main, outer));
        }
        for w in &self.jabber_popouts {
            if let Some(r) = w.outer {
                drop_rects.push((ChatWinKey::Popout(w.id), r));
            }
        }
        // Named after each window's own title bar, so "Move to" is readable with several open.
        let mut move_targets: Vec<(ChatWinKey, String)> =
            vec![(ChatWinKey::Main, "Main window".to_owned())];
        for w in &self.jabber_popouts {
            let head = w.active.clone().or_else(|| w.tabs.first().cloned()).unwrap_or_default();
            let head = head.split('@').next().unwrap_or(&head).to_owned();
            let name = if head.is_empty() { format!("Window {}", w.id) } else { head };
            move_targets.push((ChatWinKey::Popout(w.id), name));
        }
        let can_new = self.jabber_popouts.len() < MAX_POPOUTS;
        let dragged: Option<String> = self
            .jabber_tab_drag
            .as_ref()
            .filter(|d| d.from == win)
            .map(|d| d.jid.clone());
        // The pop-out's always-on-top pin rides the right end of this row, so it is in the layout
        // and costs no height of its own. The main window is not its own viewport, so it has none.
        // The id is per window: a shared key would re-send WindowLevel every frame.
        let pin: Option<String> = match win {
            ChatWinKey::Popout(id) => Some(format!("jabberwin_{id}")),
            ChatWinKey::Main => None,
        };
        let mut focus: Option<Option<String>> = None;
        let mut close_tab: Option<(String, bool)> = None;
        let mut move_to: Option<(String, ChatWinKey)> = None;
        let mut move_new: Option<String> = None;
        let mut dragging: Option<(String, Option<egui::Pos2>)> = None;
        let mut drop: Option<(String, egui::Pos2)> = None;
        let mut centers: Vec<(String, f32)> = Vec::new();
        // Set when a not-currently-visible tab is picked from the dropdown, so it is moved to
        // the front of the bar (right after Fleet pings) and the rightmost tab overflows.
        let mut promote: Option<String> = None;
        // One non-scrolling row: Fleet pings (pinned) + as many chat tabs as fit, the rest in
        // a right-side overflow dropdown. Widths are estimated from the label galley so we can
        // decide inclusion without a horizontal scroll area.
        struct TabInfo {
            jid: String,
            is_room: bool,
            is_unread: bool,
            is_mention: bool,
            lead: TabLead,
            label: String,
        }
        let bar_rect = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // The overflow dropdown is a fixed-width pseudo-tab pinned to the right edge;
            // tabs fill the space to its left. A boundary tab is ellipsized (rather than
            // dropped) when it only partly fits, so the row is packed and the dropdown
            // never moves.
            const MIN_TAB_W: f32 = 96.0;
            let full = ui.available_width();
            // Reserve the dropdown's exact width for its widest state (caret + the largest
            // possible overflow count) so the badge never grows the button past the edge.
            let dd_w = {
                let body = egui::TextStyle::Body.resolve(ui.style());
                let widest = format!(
                    "{}  {}",
                    egui_phosphor::regular::CARET_DOWN,
                    tabs.len().max(1)
                );
                let text_w = ui
                    .painter()
                    .layout_no_wrap(widest, body, egui::Color32::WHITE)
                    .size()
                    .x;
                text_w + 2.0 * ui.spacing().button_padding.x + 4.0
            };
            let pin_w = pin.as_ref().map_or(0.0, |_| PIN_GAP + ontop_pin_w(ui));
            let tab_area = (full - dd_w - pin_w).max(0.0);

            let mut used = 0.0;
            if is_main {
                let pings_label = format!("Fleet pings ({})", f.pings.len());
                let hit = jabber_tab_box(
                    ui,
                    egui::Id::new(("jtab", win, "\u{0}pings")),
                    active.is_none(),
                    f.pings_unread,
                    false,
                    TabLead::Icon(egui_phosphor::regular::MEGAPHONE),
                    false,
                    false,
                    &pings_label,
                );
                if hit.select {
                    focus = Some(None);
                }
                used = jabber_tab_width(ui, false, f.pings_unread, &pings_label);
            }

            let infos: Vec<TabInfo> = tabs
                .iter()
                .map(|jid| {
                    let is_room = f.channels.iter().any(|c| &c.jid == jid);
                    let is_unread = f.unread.contains(jid);
                    let label = short_chip(jid.split('@').next().unwrap_or(jid));
                    let lead = if is_room {
                        TabLead::Icon(egui_phosphor::regular::USERS_THREE)
                    } else {
                        let pres = f
                            .convos
                            .iter()
                            .find(|c| &c.jid == jid)
                            .map(|c| c.presence)
                            .unwrap_or_default();
                        let (pr, pg, pb) = pres.color();
                        TabLead::Dot(egui::Color32::from_rgb(pr, pg, pb))
                    };
                    let is_mention = f.mentions.contains(jid);
                    TabInfo { jid: jid.clone(), is_room, is_unread, is_mention, lead, label }
                })
                .collect();

            // Plan the visible tabs (with the label actually rendered, possibly ellipsized)
            // and collect the rest into the dropdown.
            let mut plan: Vec<(&TabInfo, String)> = Vec::new();
            let mut overflow: Vec<&TabInfo> = Vec::new();
            let mut full_bar = false;
            for t in &infos {
                if full_bar {
                    overflow.push(t);
                    continue;
                }
                let w = jabber_tab_width(ui, true, t.is_unread, &t.label);
                let remaining = tab_area - used;
                if w <= remaining {
                    used += w;
                    plan.push((t, t.label.clone()));
                } else if remaining >= MIN_TAB_W {
                    let lbl = ellipsize_tab_label(ui, true, t.is_unread, &t.label, remaining);
                    used += jabber_tab_width(ui, true, t.is_unread, &lbl);
                    plan.push((t, lbl));
                    full_bar = true;
                } else {
                    overflow.push(t);
                    full_bar = true;
                }
            }
            // Guarantee the open conversation stays on the bar: evict trailing tabs until it
            // fits, then place it (ellipsized if needed).
            if let Some(sel_jid) = active.clone() {
                if !plan.iter().any(|(t, _)| t.jid == sel_jid) {
                    if let Some(pos) = overflow.iter().position(|t| t.jid == sel_jid) {
                        while tab_area - used < MIN_TAB_W && !plan.is_empty() {
                            let (t, lbl) = plan.pop().unwrap();
                            used -= jabber_tab_width(ui, true, t.is_unread, &lbl);
                            overflow.insert(0, t);
                        }
                        let t = overflow.remove(pos.min(overflow.len().saturating_sub(1)));
                        let remaining = tab_area - used;
                        let lbl =
                            ellipsize_tab_label(ui, true, t.is_unread, &t.label, remaining);
                        used += jabber_tab_width(ui, true, t.is_unread, &lbl);
                        plan.push((t, lbl));
                    }
                }
            }

            for (t, lbl) in &plan {
                let hit = jabber_tab_box(
                    ui,
                    egui::Id::new(("jtab", win, t.jid.as_str())),
                    active.as_deref() == Some(t.jid.as_str()),
                    t.is_unread,
                    t.is_mention,
                    t.lead,
                    true,
                    can_new,
                    lbl,
                );
                centers.push((t.jid.clone(), hit.resp.rect.center().x));
                if dragged.as_deref() == Some(t.jid.as_str()) {
                    jabber_tab_lifted(ui, hit.resp.rect);
                }
                if hit.select {
                    focus = Some(Some(t.jid.clone()));
                }
                if hit.close {
                    close_tab = Some((t.jid.clone(), t.is_room));
                }
                if hit.popout {
                    move_new = Some(t.jid.clone());
                }
                // Primary button only: a secondary-button drag belongs to the context menu, and
                // egui would otherwise report it as a tab drag and relocate the tab on release.
                if hit.resp.drag_started_by(egui::PointerButton::Primary)
                    || hit.resp.dragged_by(egui::PointerButton::Primary)
                {
                    dragging = Some((t.jid.clone(), hit.resp.interact_pointer_pos()));
                }
                if hit.resp.drag_stopped_by(egui::PointerButton::Primary) {
                    if let Some(at) = hit.resp.interact_pointer_pos() {
                        drop = Some((t.jid.clone(), at));
                    }
                }
                hit.resp.context_menu(|ui| {
                    if ui
                        .add_enabled(
                            can_new,
                            egui::Button::new(format!(
                                "{}  Open in new window",
                                egui_phosphor::regular::ARROW_SQUARE_OUT
                            )),
                        )
                        .clicked()
                    {
                        move_new = Some(t.jid.clone());
                        ui.close();
                    }
                    egui::containers::menu::SubMenuButton::new("Move to").ui(ui, |ui| {
                        for (k, name) in &move_targets {
                            if ui
                                .add_enabled(*k != win, egui::Button::new(name.as_str()))
                                .clicked()
                            {
                                move_to = Some((t.jid.clone(), *k));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.add_enabled(can_new, egui::Button::new("New window")).clicked() {
                            move_new = Some(t.jid.clone());
                            ui.close();
                        }
                    });
                    ui.separator();
                    if ui
                        .button(format!("{}  Close", egui_phosphor::regular::X))
                        .clicked()
                    {
                        close_tab = Some((t.jid.clone(), t.is_room));
                        ui.close();
                    }
                });
            }

            let pad = (full - used - dd_w - pin_w).max(0.0);
            if pad > 0.0 {
                ui.add_space(pad);
            }
            let any_unread = overflow.iter().any(|t| t.is_unread);
            let caret = if overflow.is_empty() {
                egui_phosphor::regular::CARET_DOWN.to_owned()
            } else {
                format!("{} {}", egui_phosphor::regular::CARET_DOWN, overflow.len())
            };
            let caret = if any_unread {
                egui::RichText::new(caret).strong()
            } else {
                egui::RichText::new(caret)
            };
            let dd_btn = egui::Button::new(caret)
                .min_size(egui::vec2(dd_w, TAB_H))
                .corner_radius(0.0);
            let menu_list: Vec<&TabInfo> =
                if overflow.is_empty() { infos.iter().collect() } else { overflow };
            egui::containers::menu::MenuButton::from_button(dd_btn).ui(ui, |ui| {
                for t in &menu_list {
                    ui.horizontal(|ui| {
                        match t.lead {
                            TabLead::Dot(c) => status_dot(ui, c, 9.0),
                            TabLead::Icon(ic) => {
                                ui.label(ic);
                            }
                        }
                        if ui.menu_label(false, t.label.as_str()).clicked() {
                            focus = Some(Some(t.jid.clone()));
                            if !plan.iter().any(|(pt, _)| pt.jid == t.jid) {
                                promote = Some(t.jid.clone());
                            }
                            ui.close();
                        }
                        if t.is_unread {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::CIRCLE)
                                    .color(UNREAD_RED)
                                    .size(8.0),
                            );
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(egui_phosphor::regular::X).small(),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Close")
                            .clicked()
                        {
                            close_tab = Some((t.jid.clone(), t.is_room));
                            ui.close();
                        }
                    });
                }
            });
            if let Some(vp) = &pin {
                ui.add_space(PIN_GAP);
                ontop_pin_ui(ui, vp);
            }
        }).response.rect;
        if let Some(jid) = focus {
            out.push(TabAction::Select { win, jid });
        }
        if let Some((jid, is_room)) = close_tab {
            out.push(TabAction::Close { jid, is_room });
        }
        if let Some(jid) = promote {
            out.push(TabAction::Promote { win, jid });
        }
        if let Some((jid, to)) = move_to {
            out.push(TabAction::Move { jid, to, index: None });
        }
        if let Some(jid) = move_new {
            out.push(TabAction::MoveToNew { jid, at: None });
        }
        if let Some((jid, local)) = dragging {
            let at = local.zip(inner_origin).map(|(l, o)| o + l.to_vec2());
            self.jabber_tab_drag = Some(TabDrag { jid, from: win, at, alive: true });
        }
        if let Some((jid, local)) = drop {
            self.jabber_tab_drag = None;
            let screen = inner_origin.map(|o| o + local.to_vec2());
            // Without a screen position (Wayland reports no window rect) the gesture degrades to
            // an in-bar reorder and can never tear a window off, which would otherwise spawn
            // stray windows at an unknown place.
            let over_self = bar_rect.contains(local)
                || screen.is_none()
                || own_outer.zip(screen).is_some_and(|(r, p)| r.contains(p));
            let target =
                if over_self { Some(win) } else { drop_target(&drop_rects, win, screen) };
            match target {
                Some(t) if t == win => {
                    let index = reorder_index(&tabs, &centers, &jid, local.x);
                    out.push(TabAction::Move { jid, to: win, index: Some(index) });
                }
                Some(t) => out.push(TabAction::Move { jid, to: t, index: None }),
                None => out.push(TabAction::MoveToNew { jid, at: screen }),
            }
        }
        // The pointer position local to this window, never `TabDrag.at`: that is monitor-space and
        // is `None` on Wayland, where the source window can still place a ghost perfectly well.
        if let Some(d) = self.jabber_tab_drag.as_ref().filter(|d| d.from == win) {
            if let Some(p) = ui.ctx().pointer_interact_pos() {
                let head = d.jid.split('@').next().unwrap_or(&d.jid);
                jabber_drag_ghost(ui, win, &short_chip(head), p);
            }
        }
    }

    /// The Fleet pings pseudo-tab. Main window only: a pop-out has no pings feed.
    pub(crate) fn jabber_pings_ui(&mut self, ui: &mut egui::Ui, f: &JabberFrame) {
        let systems = self.systems.clone();
        let pings = &f.pings;
        let hl: Vec<bool> =
            pings.iter().map(|p| self.matching_ping_rule(p).is_some_and(|r| !r.suppress)).collect();
        let doctrine_url = self.settings.doctrine_url.clone();
        let op_links = self.settings.op_channel_links.clone();
        let visible = self.jabber_pings_visible.min(pings.len());
        let out = egui::ScrollArea::vertical().id_salt("pings").auto_shrink([false, false]).show(ui, |ui| {
            if pings.is_empty() {
                ui.label(egui::RichText::new("No pings yet.").weak());
            }
            for (i, p) in pings.iter().enumerate().rev().take(visible) {
                render_ping(ui, p, &systems, hl[i], &doctrine_url, &op_links);
            }
            if visible < pings.len() {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("+{} older", pings.len() - visible))
                        .weak(),
                );
            }
        });
        if visible < pings.len()
            && out.state.offset.y + out.inner_rect.height()
                >= out.content_size.y - 200.0
        {
            self.jabber_pings_visible = (visible + 50).min(pings.len());
            ui.ctx().request_repaint();
        }
    }

    pub(crate) fn jabber_conversation_ui(
        &mut self,
        ui: &mut egui::Ui,
        win: ChatWinKey,
        jid: &str,
        f: &JabberFrame,
        out: &mut Vec<TabAction>,
    ) {
        let jid = jid.to_owned();
        if win == ChatWinKey::Main {
            // Main-window pagination only: a pop-out would otherwise reset it every frame.
            self.jabber_pings_visible = 50;
        }
        use egui_phosphor::regular as icon;
        let is_room = f.channels.iter().any(|c| c.jid == jid);
        // A kicked room is history only: its composer is disabled.
        let sel_accessible = f.accessible(&jid);
        let muted = self.jabber_is_muted(&jid);
        let motd = f
            .channels
            .iter()
            .find(|c| c.jid == jid)
            .map(|c| c.motd.clone())
            .filter(|m| !m.trim().is_empty());
        let mut show_motd = false;
        ui.horizontal(|ui| {
            let name = jid.split('@').next().unwrap_or(&jid);
            let glyph = if is_room { icon::USERS_THREE } else { icon::USER };
            ui.label(egui::RichText::new(format!("{glyph}  {name}")).strong());
            if muted {
                ui.label(egui::RichText::new(icon::BELL_SLASH).weak())
                    .on_hover_text("Muted");
            }
            // The room's topic, on the bar the room's name is on. Width-bounded and truncated
            // rather than allowed to take what it likes: a MOTD is a paragraph, and a label that
            // wraps here pushes the mute and close controls off the end of the bar.
            if let Some(m) = &motd {
                ui.separator();
                let keep = 76.0;
                let w = (ui.available_width() - keep).clamp(40.0, ui.available_width().max(40.0));
                ui.scope(|ui| {
                    ui.set_max_width(w);
                    ui.add(
                        egui::Label::new(egui::RichText::new(motd_one_line(m)).weak())
                            .truncate()
                            .selectable(false),
                    )
                    .on_hover_text(motd_preview(m, 6));
                });
                if ui
                    .add(egui::Button::new(egui::RichText::new(icon::ARTICLE)).frame(false))
                    .on_hover_text("Show the full MOTD")
                    .clicked()
                {
                    show_motd = true;
                }
            }
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if !is_room {
                        let is_contact =
                            self.settings.jabber_contacts.contains(&jid);
                        let col = if is_contact {
                            ui.visuals().hyperlink_color
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        if ui
                            .button(egui::RichText::new(icon::STAR).color(col))
                            .on_hover_text(if is_contact {
                                "Remove from contacts"
                            } else {
                                "Add as contact"
                            })
                            .clicked()
                        {
                            if is_contact {
                                self.settings.jabber_contacts.retain(|j| j != &jid);
                            } else {
                                self.settings.jabber_contacts.push(jid.clone());
                            }
                            self.needs_save = true;
                        }
                    }
                    let bell = if muted { icon::BELL_SLASH } else { icon::BELL };
                    ui.menu_button(bell, |ui| {
                        let now = chrono::Utc::now().timestamp();
                        let set = |app: &mut Self, until: i64| {
                            app.settings.jabber_muted.insert(jid.clone(), until);
                            app.needs_save = true;
                        };
                        if ui.button("Mute 1 hour").clicked() {
                            set(self, now + 3600);
                            ui.close();
                        }
                        if ui.button("Mute 8 hours").clicked() {
                            set(self, now + 8 * 3600);
                            ui.close();
                        }
                        if ui.button("Mute until I unmute").clicked() {
                            set(self, i64::MAX);
                            ui.close();
                        }
                        if muted && ui.button("Unmute").clicked() {
                            self.settings.jabber_muted.remove(&jid);
                            self.needs_save = true;
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Mute notifications");
                },
            );
        });
        if show_motd {
            self.jabber_motd_window = Some(jid.clone());
        }
        ui.separator();
        let body_h = ui.available_height();
        let composer_h = if is_room && !sel_accessible {
            32.0
        } else {
            composer_height(
                ui,
                self.jabber_drafts.get(&jid).map_or("", String::as_str),
                ui.available_width(),
            )
        }
        // A ten-row draft is taller than a small pop-out's whole body, so the history keeps its
        // floor and the composer scrolls sooner instead of running off the bottom edge.
        .min((body_h - HISTORY_MIN_H - 8.0).max(32.0));
        let session_start = self.session_start;
        let mut dm_click: Option<String> = None;
        let mut msg_mention: Option<String> = None;
        let mut msg_dm: Option<String> = None;
        // Don't snap to the bottom while the pointer is held, or every incoming message wipes a
        // text selection mid-drag. It resumes on release.
        let selecting = ui.input(|i| i.pointer.any_down());
        // Borrowed under the lock rather than deep-cloning up to 1000 messages per frame per open
        // window.
        let jabber = self.jabber.clone();
        let guard = jabber.lock().unwrap();
        let sel_msgs: &[crate::jabber::ChatMsg] =
            guard.chats.get(&jid).map_or(&[][..], Vec::as_slice);
        egui::ScrollArea::vertical()
            // Salted by conversation, not just by window, or each tab inherits the previous one's
            // offset and stuck-to-bottom flag.
            .id_salt(("msgs", jid.as_str()))
            .auto_shrink([false, false])
            .max_height((body_h - composer_h - 8.0).max(HISTORY_MIN_H))
            .stick_to_bottom(!selecting)
            .show_viewport(ui, |ui, viewport| {
                let accent = ui.visuals().hyperlink_color;
                let me_col = egui::Color32::from_rgb(0x5A, 0xC8, 0x6A);
                let now = chrono::Utc::now().timestamp();
                let names = self.mention_names();
                ui.spacing_mut().item_spacing.y = 1.0;
                let row_w = ui.available_width();
                if self.jabber_msg_heights.len() > 8_000 {
                    self.jabber_msg_heights.clear();
                }
                let origin = ui.cursor().top();
                let mut hist_drawn = false;
                let mut prev_sender: Option<&str> = None;
                let mut prev_time: i64 = 0;
                for (mi, m) in sel_msgs.iter().enumerate() {
                    if !hist_drawn && m.time >= session_start && m.time > 0 {
                        hist_drawn = true;
                        prev_sender = None;
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("— new —").weak());
                            ui.separator();
                        });
                    }
                    let sender = if m.outgoing { "\u{0}me" } else { m.from.as_str() };
                    let grouped = prev_sender == Some(sender)
                        && m.time >= prev_time
                        && m.time - prev_time < 300;
                    prev_sender = Some(sender);
                    prev_time = m.time;
                    let key = msg_row_key(m, grouped, row_w);
                    let top = ui.cursor().top() - origin;
                    if let Some(h) = self.jabber_msg_heights.get(&key).copied() {
                        if top + h < viewport.min.y - MSG_OVERDRAW
                            || top > viewport.max.y + MSG_OVERDRAW
                        {
                            ui.add_space(h);
                            continue;
                        }
                    }
                    if !grouped {
                        ui.add_space(5.0);
                        ui.label(
                            egui::RichText::new(eve_time_label(m.time, now)).weak().size(9.5),
                        );
                    }
                    let mentioned = !m.outgoing
                        && crate::jabber::mention_hit(&m.body, &names);
                    #[cfg(test)]
                    record_msg_row_built(ui.ctx());
                    let act = message_row(
                        ui,
                        ui.id().with(("msg", mi)),
                        mentioned,
                        MsgActions {
                            copy: true,
                            // A kicked room renders no composer, so there is
                            // nowhere for a mention to land.
                            mention: !m.outgoing && (!is_room || sel_accessible),
                            dm: is_room && !m.outgoing,
                        },
                        |ui| {
                            // Not `horizontal_wrapped`: that floors the row at `interact_size.y`,
                            // which is 11px of dead air per message on a row that only holds a
                            // nick, text and the occasional link.
                            let row = egui::Layout::left_to_right(egui::Align::Center)
                                .with_main_wrap(true);
                            let size = egui::vec2(ui.available_size_before_wrap().x, 0.0);
                            ui.allocate_ui_with_layout(size, row, |ui| {
                                if !grouped {
                                    if m.outgoing {
                                        ui.label(
                                            egui::RichText::new("me:")
                                                .color(me_col)
                                                .strong(),
                                        );
                                    } else {
                                        let n = m
                                            .from
                                            .split('@')
                                            .next()
                                            .unwrap_or(&m.from);
                                        let lbl = egui::Label::new(
                                            egui::RichText::new(format!("{n}:"))
                                                .strong()
                                                .color(accent),
                                        );
                                        let resp = if is_room {
                                            ui.add(lbl.sense(egui::Sense::click()))
                                                .on_hover_text("Message")
                                        } else {
                                            ui.add(lbl)
                                        };
                                        if resp.clicked() {
                                            dm_click = Some(n.to_owned());
                                        }
                                    }
                                }
                                render_message_body(ui, condense_attention_list(&m.body).as_ref());
                            });
                        },
                    );
                    if act != MsgRowAction::None {
                        // Grouped rows draw no nick, so the actions read `from`.
                        let n = m.from.split('@').next().unwrap_or(&m.from);
                        match act {
                            MsgRowAction::Copy => ui.ctx().copy_text(m.body.clone()),
                            MsgRowAction::Mention => msg_mention = Some(n.to_owned()),
                            MsgRowAction::GoToDm => msg_dm = Some(n.to_owned()),
                            MsgRowAction::None => {}
                        }
                    }
                    let h = ui.cursor().top() - origin - top;
                    if h > 0.0 {
                        self.jabber_msg_heights.insert(key, h);
                    }
                }
            });
        drop(guard);
        let mut focus_composer = false;
        if let Some(nick) = msg_mention {
            let d = self.jabber_drafts.entry(jid.clone()).or_default();
            if !d.is_empty() && !d.ends_with(char::is_whitespace) {
                d.push(' ');
            }
            d.push_str(&format!("{nick}: "));
            focus_composer = true;
        }
        if is_room && !sel_accessible {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "You're no longer in this channel, history only.",
                )
                .weak(),
            );
        } else {
        let shift_enter = egui::KeyboardShortcut::new(
            egui::Modifiers::SHIFT,
            egui::Key::Enter,
        );
        // The border belongs outside the scroll area, or the viewport clips it as it scrolls.
        let mut frame = composer_frame(ui).begin(ui);
        // `ScrollArea` pads its content clip by `clip_rect_margin`, which lands outside the
        // border and lets the next line bleed under it.
        frame.content_ui.visuals_mut().clip_rect_margin = 0.0;
        let resp = egui::ScrollArea::vertical()
            .id_salt("composer")
            .max_height(composer_h - COMPOSER_MARGIN.sum().y)
            .show(&mut frame.content_ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(
                        self.jabber_drafts.entry(jid.clone()).or_default(),
                    )
                    .frame(egui::Frame::NONE)
                    .hint_text("Message (Shift+Enter for a new line)")
                    .return_key(shift_enter)
                    .desired_rows(COMPOSER_MIN_ROWS as usize)
                    .desired_width(ui.available_width()),
                )
            })
            .inner;
        frame.frame.stroke = if resp.has_focus() {
            ui.visuals().selection.stroke
        } else {
            ui.style().interact(&resp).bg_stroke
        };
        frame.end(ui);
        if focus_composer {
            resp.request_focus();
        }
        let send = resp.has_focus()
            && ui.input(|i| {
                i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
            });
        let draft_empty = self
            .jabber_drafts
            .get(&jid)
            .map_or(true, |d| d.trim().is_empty());
        if send && !draft_empty {
            let body = self
                .jabber_drafts
                .get_mut(&jid)
                .map(std::mem::take)
                .unwrap_or_default();
            if let Some(tx) = &self.jabber_tx {
                let cmd = if is_room {
                    crate::jabber::Cmd::SendRoom { room: jid.clone(), body }
                } else {
                    crate::jabber::Cmd::Send { to: jid.clone(), body }
                };
                let _ = tx.send(cmd);
            }
        }
        }
        if let Some(nick) = dm_click.or(msg_dm) {
            out.push(TabAction::Open { jid: self.full_user_jid(&nick), prefer: win });
        }
    }

    pub(crate) fn apply_tab_actions(&mut self, actions: Vec<TabAction>) {
        for a in actions {
            match a {
                TabAction::Select { win, jid } => {
                    match &jid {
                        None => self.jabber.lock().unwrap().pings_unread = false,
                        Some(j) => self.jabber_mark_read(j),
                    }
                    self.win_set_active(win, jid);
                }
                TabAction::Close { jid, is_room } => {
                    self.close_jabber_tab(&jid, is_room);
                }
                TabAction::Promote { win, jid } => {
                    let list = match win {
                        ChatWinKey::Main => Some(&mut self.jabber_tabs),
                        ChatWinKey::Popout(id) => {
                            self.jabber_popouts.iter_mut().find(|w| w.id == id).map(|w| &mut w.tabs)
                        }
                    };
                    if let Some(list) = list {
                        if let Some(pos) = list.iter().position(|t| t == &jid) {
                            let t = list.remove(pos);
                            list.insert(0, t);
                        }
                    }
                }
                TabAction::Open { jid, prefer } => {
                    self.jabber_mark_read(&jid);
                    self.settings.jabber_closed_dms.retain(|j| j != &jid);
                    self.jabber_open(&jid, prefer);
                }
                TabAction::Move { jid, to, index } => {
                    if matches!(to, ChatWinKey::Popout(id) if self.popout(id).is_none()) {
                        continue;
                    }
                    let from = self.tab_set().owner(&jid);
                    self.tab_set().move_tab(&jid, to, index);
                    if let ChatWinKey::Popout(id) = to {
                        if from != Some(to) {
                            self.focus_window = Some(popout_viewport(id));
                        }
                    }
                }
                TabAction::MoveToNew { jid, at } => {
                    self.new_popout(&jid, at);
                }
                TabAction::CloseWindow(id) => self.return_popout_tabs(id),
            }
        }
    }
}
