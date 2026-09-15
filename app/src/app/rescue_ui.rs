//! The delve911 capital rescue mode: its feed, settings, ping builder, chat helpers and window.

use super::*;

impl SpaiApp {
    /// Parse new delve911 XMPP messages into rescue events. The in-game chat-log watcher covers the
    /// EVE channel of the same name; the real pings come through the MUC, so both feed `rescue`.
    #[cfg(feature = "fc-rescue")]
    pub(crate) fn ingest_delve911_jabber(&mut self) {
        if !self.settings.fc_rescue_enabled {
            return;
        }
        let (Some(systems), Some(ships)) = (self.systems.clone(), self.ship_groups.clone()) else {
            return;
        };
        let jid =
            goon_jid(&self.settings.rescue_delve911_jid, "delve911@conference.goonfleet.com");
        if jid.is_empty() {
            return;
        }

        let fresh: Vec<(String, String, i64)> = {
            let j = self.jabber.lock().unwrap();
            let Some(msgs) = j.chats.get(&jid) else { return };
            // First sight: start two minutes back, so restarting replays the ping that just landed
            // without replaying days of loaded history.
            if self.delve911_cursor == 0 {
                self.delve911_cursor = chrono::Utc::now().timestamp() - 120;
            }
            msgs.iter()
                .filter(|m| m.time > self.delve911_cursor && !m.outgoing)
                // The room bot echoes every ping back as a multi-kilobyte attention roll-call.
                .filter(|m| !is_ping_bot(&m.from) && !m.body.contains("requests the attention of"))
                .map(|m| (m.from.clone(), m.body.clone(), m.time))
                .collect()
        };
        if fresh.is_empty() {
            return;
        }

        let mut events = Vec::new();
        for (from, body, time) in fresh {
            self.delve911_cursor = self.delve911_cursor.max(time);
            // Pure parser under catch_unwind: a bad line drops one event, never the app.
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::rescue::parse_event(&from, &body, time, &systems, &ships)
            })) {
                Ok(ev) => events.push(ev),
                Err(_) => eprintln!("[rescue] parser panicked on delve911 message, skipping"),
            }
        }
        let mut r = self.rescue.lock().unwrap();
        for ev in events {
            r.push_event(ev);
        }
    }

    /// Move newly-parsed delve911 ping events into the fleet-ping feed exactly once each.
    /// Runs on the UI thread so it can take the jabber lock (the watcher only writes `rescue`).
    #[cfg(feature = "fc-rescue")]
    pub(crate) fn drain_rescue_feed(&mut self, ctx: &egui::Context) {
        let mut fresh: Vec<crate::rescue::RescueEvent> = Vec::new();
        {
            let r = self.rescue.lock().unwrap();
            for ev in &r.events {
                if ev.seq > self.rescue_feed_cursor && ev.is_ping {
                    fresh.push(ev.clone());
                }
                self.rescue_feed_cursor = self.rescue_feed_cursor.max(ev.seq);
            }
        }
        if fresh.is_empty() {
            return;
        }
        let mut arrived = false;
        {
            let mut j = self.jabber.lock().unwrap();
            for ev in &fresh {
                let mut text = String::from("delve911");
                if let Some(sys) = &ev.system_name {
                    text.push_str(" — ");
                    text.push_str(sys);
                }
                if let Some(pilot) = &ev.pilot {
                    text.push_str(&format!(" — {pilot} tackled"));
                }
                if let Some(cyno) = &ev.cyno {
                    text.push_str(&format!(" (cyno {cyno})"));
                }
                if ev.system_name.is_none() && ev.pilot.is_none() {
                    text = format!("delve911: {}", ev.raw);
                }
                j.pings.push(crate::pings::Ping::Plain {
                    timestamp: ev.received,
                    text,
                    sender: Some(ev.author.clone()),
                    target: Some("delve911".to_owned()),
                    raw: ev.raw.clone(),
                });
                arrived = true;
            }
            let n = j.pings.len();
            if n > 2000 {
                j.pings.drain(0..n - 2000);
            }
            if arrived {
                j.pings_unread = true;
                j.notify.push((crate::jabber::PING_FEED_KEY.to_owned(), true));
            }
        }
        if arrived {
            // Arm rescue mode: the banner/top-bar button offers 1-click entry (no auto-hijack).
            self.rescue_armed = true;
            ctx.request_repaint();
        }
    }

    /// FC-only rescue settings. Returns true if anything changed (caller sets needs_save).
    #[cfg(feature = "fc-rescue")]
    pub(crate) fn rescue_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.heading("FC / Rescue (delve911)");
        ui.label(
            egui::RichText::new(
                "Capital-rescue coordination for fleet commanders. Off by default.",
            )
            .weak(),
        );
        changed |= ui
            .checkbox(
                &mut self.settings.fc_rescue_enabled,
                "Enable delve911 Rescue Mode (FC only)",
            )
            .changed();
        if !self.settings.fc_rescue_enabled {
            return changed;
        }
        ui.add_space(4.0);
        egui::Grid::new("rescue_settings_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("delve911 channel");
            changed |= ui
                .add(egui::TextEdit::singleline(&mut self.settings.rescue_channel).desired_width(220.0))
                .changed();
            ui.end_row();
            ui.label("Home staging system");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.settings.rescue_staging_system)
                        .desired_width(220.0),
                )
                .changed();
            ui.end_row();
            ui.label("skirmish_commanders JID");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.settings.rescue_skirmish_jid)
                        .hint_text("empty = skirmish_commanders@conference.goonfleet.com")
                        .desired_width(280.0),
                )
                .changed();
            ui.end_row();
            ui.label("delve911 room JID");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.settings.rescue_delve911_jid)
                        .hint_text("empty = delve911@conference.goonfleet.com")
                        .desired_width(280.0),
                )
                .changed();
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.label("Ping template");
        ui.label(
            egui::RichText::new(
                "Placeholders: {fc} {pilot} {system} {cyno} {anom} {op} {mumble} {doctrine} {staging}",
            )
            .weak(),
        );
        changed |= ui
            .add(
                egui::TextEdit::multiline(&mut self.settings.rescue_ping_template)
                    .desired_rows(5)
                    .desired_width(f32::INFINITY),
            )
            .changed();

        ui.add_space(6.0);
        if ui.button(format!("{}  Configure doctrines…", egui_phosphor::regular::LIST_BULLETS)).clicked() {
            self.rescue_doctrines_open = true;
        }
        changed
    }

    /// Recompute the titan-range check only when staging or the target changes: it scans every
    /// system for the nearest in-range jump-off point, which is far too much for every frame.
    #[cfg(feature = "fc-rescue")]
    pub(crate) fn update_rescue_range(&mut self) {
        let target = self.rescue.lock().unwrap().capital_system;
        let (Some(systems), Some(coords), Some(target)) =
            (self.systems.clone(), self.map_coords.clone(), target)
        else {
            self.rescue_range = None;
            self.rescue_range_for = None;
            return;
        };
        let Some(staging) =
            systems.lookup(&self.settings.rescue_staging_system).map(|i| i.id)
        else {
            self.rescue_range = None;
            self.rescue_range_for = None;
            return;
        };
        if self.rescue_range_for == Some((staging, target)) {
            return;
        }
        self.rescue_range_for = Some((staging, target));
        self.rescue_range = None;

        let at = |id: i64| coords.iter().find(|s| s.id == id);
        let (Some(stage_pos), Some(target_pos)) = (at(staging), at(target)) else { return };
        let ly_from_staging = crate::map::ly_distance(stage_pos, target_pos);
        if ly_from_staging <= DELVE911_RANGE_LY {
            return;
        }

        const MAX_JUMPS: u32 = 40;
        let Some(hop) = best_jump_off(&systems, &coords, stage_pos, target, target_pos, MAX_JUMPS)
        else {
            return;
        };
        self.rescue_range = Some(RangeWarning {
            ly_from_staging,
            closest_name: hop.name.clone(),
            ansi_jumps: systems.jumps(hop.id, target, MAX_JUMPS),
            gate_jumps: systems.jumps_gates_only(hop.id, target, MAX_JUMPS),
            ly_to_target: crate::map::ly_distance(hop, target_pos),
        });
    }

    /// Build the ready-to-send ping text from the template and current rescue state. `{doctrine}`
    /// expands to the selected doctrine's full description line.
    #[cfg(feature = "fc-rescue")]
    pub(crate) fn build_rescue_ping(&self) -> String {
        let r = self.rescue.lock().unwrap();
        let sys = r.capital_system_name.clone().unwrap_or_else(|| "?".into());
        let pilot = r.capital_pilot.clone().unwrap_or_else(|| "?".into());
        let cyno = r.cyno_pilot.clone().unwrap_or_else(|| "?".into());
        let anom = r.anomaly.clone().unwrap_or_else(|| "?".into());
        let op = r.op_channel.to_string();
        let doctrine = self
            .settings
            .rescue_doctrines
            .iter()
            .find(|d| d.name == r.doctrine)
            .map(|d| if d.description.is_empty() { d.name.clone() } else { d.description.clone() })
            .unwrap_or_else(|| if r.doctrine.is_empty() { "?".into() } else { r.doctrine.clone() });
        let staging = self.settings.rescue_staging_system.clone();
        let fc = if self.active_character.is_empty() { "?".into() } else { self.active_character.clone() };
        let mumble = op_comms_url(r.op_channel);
        self.settings
            .rescue_ping_template
            .replace("{system}", &sys)
            .replace("{pilot}", &pilot)
            .replace("{cyno}", &cyno)
            .replace("{anom}", &anom)
            .replace("{op}", &op)
            .replace("{doctrine}", &doctrine)
            .replace("{staging}", &staging)
            .replace("{fc}", &fc)
            .replace("{mumble}", &mumble)
    }

    /// (connected, status text, seconds until the next automatic retry, a worker thread is alive).
    pub(crate) fn jabber_conn(&self) -> (bool, String, Option<i64>, bool) {
        let s = self.jabber.lock().unwrap();
        let retry_in = s
            .retry_at
            .map(|t| t.saturating_duration_since(std::time::Instant::now()).as_secs() as i64);
        (s.connected, s.status.clone(), retry_in, s.running)
    }

    /// Retry a lost XMPP connection: skip the backoff when a worker is still alive, otherwise clear
    /// the fatal error so `maybe_start_jabber` spawns a fresh one.
    pub(crate) fn jabber_retry(&mut self) {
        if self.jabber.lock().unwrap().running {
            if let Some(tx) = &self.jabber_tx {
                let _ = tx.send(crate::jabber::Cmd::RetryNow);
            }
            return;
        }
        {
            let mut s = self.jabber.lock().unwrap();
            s.fatal = None;
            s.status = "Connecting…".to_owned();
        }
        self.settings.jabber_enabled = true;
        self.needs_save = true;
    }

    pub(crate) fn jabber_retry_button(&mut self, ui: &mut egui::Ui) {
        let (_, _, retry_in, _) = self.jabber_conn();
        if ui
            .button(format!("{}  Retry now", egui_phosphor::regular::ARROWS_CLOCKWISE))
            .clicked()
        {
            self.jabber_retry();
        }
        if let Some(secs) = retry_in.filter(|s| *s > 0) {
            ui.label(
                egui::RichText::new(format!("retrying in {}:{:02}", secs / 60, secs % 60)).weak(),
            );
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
    }

    /// Last `n` messages of a jabber room/conversation as (sender, body, outgoing, time), oldest
    /// first. Only the rescue window reads this today.
    #[cfg(feature = "fc-rescue")]
    pub(crate) fn jabber_room_tail(&self, jid: &str, n: usize) -> Vec<(String, String, bool, i64)> {
        if jid.is_empty() {
            return Vec::new();
        }
        let j = self.jabber.lock().unwrap();
        j.chats
            .get(jid)
            .map(|msgs| {
                let start = msgs.len().saturating_sub(n);
                msgs[start..]
                    .iter()
                    .map(|m| {
                        let who = m.from.split('/').next_back().unwrap_or(&m.from).to_string();
                        (who, m.body.clone(), m.outgoing, m.time)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(feature = "fc-rescue")]
    /// The rescue tab. Without the feature it is not in the rail at all, so this only says so for
    /// the case where someone reaches the view some other way.
    #[cfg(feature = "fc-rescue")]
    pub(crate) fn rescue_view(&mut self, ui: &mut egui::Ui) {
        self.rescue_window_body(ui);
    }

    #[cfg(not(feature = "fc-rescue"))]
    pub(crate) fn rescue_view(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("This build has no rescue mode.").weak());
    }

    #[cfg(feature = "fc-rescue")]
    pub(crate) fn rescue_window_body(&mut self, ui: &mut egui::Ui) {
        // Clone what the columns need so the render closure never borrows `self` (it holds the
        // rescue lock). Deferred self-mutations go through flags applied after the lock drops.
        let ping = self.build_rescue_ping();
        let skirmish_jid = goon_jid(
            &self.settings.rescue_skirmish_jid,
            "skirmish_commanders@conference.goonfleet.com",
        );
        let delve911_jid =
            goon_jid(&self.settings.rescue_delve911_jid, "delve911@conference.goonfleet.com");
        let skirmish_msgs = self.jabber_room_tail(&skirmish_jid, 60);
        let delve911_msgs = self.jabber_room_tail(&delve911_jid, 60);
        let tx = self.jabber_tx.clone();
        let ops_w = self.settings.rescue_col_ops_w.clamp(180.0, 640.0);
        let mut new_ops_w = ops_w;
        let doctrines = self.settings.rescue_doctrines.clone();
        let (jab_connected, jab_status, jab_retry_in, _) = self.jabber_conn();
        let mut retry_click = false;
        let mut set_dest: Option<i64> = None;
        let mut chat_dm: Option<String> = None;
        let has_char = self.active_character != "No character";
        let range_warning = self.rescue_range.as_ref().map(|w| {
            let jumps = |n: Option<u32>, unit: &str| match n {
                Some(j) => format!("{j} {unit}"),
                None => format!("no {unit} route"),
            };
            (
                format!(
                    "{}  OUT OF TITAN RANGE — {:.1} ly from staging",
                    egui_phosphor::regular::WARNING,
                    w.ly_from_staging
                ),
                format!(
                    "Closest reachable: {}  →  {} · {} · {:.1} ly",
                    w.closest_name,
                    jumps(w.ansi_jumps, "Ansiblex jumps"),
                    jumps(w.gate_jumps, "stargate jumps"),
                    w.ly_to_target
                ),
            )
        });

        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            let mut r = self.rescue.lock().unwrap();
            let test_mode = r.test_mode;

            // Nothing sent from this window can leave the machine while XMPP is down, and an empty
            // chat pane looks identical to a quiet channel, so say so loudly.
            if !jab_connected {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xE0, 0x3B, 0x2E),
                        format!(
                            "{}  Jabber disconnected — pings cannot be sent. {jab_status}",
                            egui_phosphor::regular::PLUGS
                        ),
                    );
                    if ui.button("Retry now").clicked() {
                        retry_click = true;
                    }
                    if let Some(secs) = jab_retry_in.filter(|s| *s > 0) {
                        ui.label(
                            egui::RichText::new(format!(
                                "retrying in {}:{:02}",
                                secs / 60,
                                secs % 60
                            ))
                            .weak(),
                        );
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                    }
                });
                ui.separator();
            }

            // The five most recent unresolved pings, right-aligned in the title bar, newest at the
            // far right. Picking one drives everything below it: the summary line, the ping
            // template, the checklist and the ESI destination.
            let listed: Vec<(u64, String, String, String, i64)> = r
                .recent_pings(5)
                .into_iter()
                .map(|e| {
                    (
                        e.seq,
                        // Chips stay narrow so five fit next to the summary; the rest is on hover.
                        e.pilot
                            .clone()
                            .or_else(|| e.system_name.clone())
                            .unwrap_or_else(|| "?".into()),
                        e.system_name.clone().unwrap_or_else(|| "?".into()),
                        e.cyno.clone().unwrap_or_else(|| "?".into()),
                        e.received,
                    )
                })
                .collect();
            let selected = r.selected_ping;
            let op_channel = r.op_channel;
            let actions_done: std::collections::HashSet<u64> = listed
                .iter()
                .map(|(seq, ..)| *seq)
                .filter(|seq| r.actions(*seq).done(op_channel))
                .collect();
            let mut pick: Option<u64> = None;
            let mut resolve: Option<u64> = None;

            ui.horizontal(|ui| {
                ui.heading(egui_phosphor::regular::WARNING_OCTAGON.to_string());
                let pilot = r.capital_pilot.clone().unwrap_or_else(|| "unknown".into());
                let sys = r.capital_system_name.clone().unwrap_or_else(|| "?".into());
                ui.heading(egui::RichText::new(pilot).strong());
                ui.label("in");
                ui.heading(egui::RichText::new(sys).strong());
                if let Some(class) = r.cap_class {
                    ui.label(format!("[{}]", class.label()));
                }
                if r.fleet.stale {
                    ui.colored_label(egui::Color32::from_rgb(0xE0, 0xA0, 0x30), "· stale");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if listed.is_empty() {
                        ui.label(egui::RichText::new("No delve911 pings yet").weak());
                        return;
                    }
                    let now = chrono::Utc::now().timestamp();
                    // Newest first: in a right-to-left layout the first widget lands furthest right.
                    for (i, (seq, chip, sys, cyno, received)) in listed.iter().enumerate() {
                        if ui
                            .small_button(egui_phosphor::regular::CHECK)
                            .on_hover_text("Resolved, dismiss this ping")
                            .clicked()
                        {
                            resolve = Some(*seq);
                        }
                        let age = fmt_age_compact(now - received);
                        let mut text = egui::RichText::new(chip);
                        if i == 0 {
                            text = text.strong().color(egui::Color32::from_rgb(0xE6, 0xA5, 0x1E));
                        }
                        let btn = egui::Button::selectable(selected == Some(*seq), text);
                        // Same pulse as the buttons: this ping still needs coord/comms/invite.
                        let btn = match pulse_fill(ui, !actions_done.contains(seq)) {
                            Some(c) => btn.fill(c),
                            None => btn,
                        };
                        let tip = if actions_done.contains(seq) {
                            format!("{sys}  ·  cyno {cyno}  ·  {age} ago")
                        } else {
                            format!("{sys}  ·  cyno {cyno}  ·  {age} ago\nAction outstanding")
                        };
                        if ui.add(btn).on_hover_text(tip).clicked() {
                            pick = Some(*seq);
                        }
                        ui.add_space(8.0);
                    }
                });
            });
            if let Some(seq) = pick {
                r.select_ping(seq);
            }
            if let Some(seq) = resolve {
                r.resolve_ping(seq);
            }
            // The destination is auto-pushed when the selected ping changes, but never re-asserted,
            // so this is the way back after routing somewhere else mid-rescue. Routes to the
            // casualty's system, not staging.
            ui.horizontal(|ui| {
                let target = r.capital_system.zip(r.capital_system_name.clone());
                let label = match &target {
                    Some((_, name)) => {
                        format!("{}  Set Destination: {name}", egui_phosphor::regular::MAP_PIN_LINE)
                    }
                    None => format!("{}  Set Destination", egui_phosphor::regular::MAP_PIN_LINE),
                };
                let resp = ui.add_enabled(
                    target.is_some() && has_char,
                    egui::Button::new(label),
                );
                let resp = if target.is_none() {
                    resp.on_disabled_hover_text("This ping has no system to route to")
                } else if !has_char {
                    resp.on_disabled_hover_text("No active character to route")
                } else {
                    resp.on_hover_text("Route this character to the tackled capital")
                };
                if resp.clicked() {
                    set_dest = target.map(|(id, _)| id);
                }
            });
            // Only rendered in the rare out-of-range case, and wrapped so a long system name can't
            // widen the window or push the chat panel around.
            if let Some((headline, detail)) = &range_warning {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(0xE0, 0x3B, 0x2E), headline);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(0xE6, 0xA5, 0x1E), detail);
                });
            }
            // Only render the cyno/anomaly line when there's something to show (avoids an empty gap).
            if r.cyno_pilot.is_some() || r.anomaly.is_some() {
                ui.horizontal_wrapped(|ui| {
                    if let Some(cyno) = &r.cyno_pilot {
                        ui.label(format!("cyno: {cyno}"));
                    }
                    if let Some(anom) = &r.anomaly {
                        ui.label(format!("· @ {anom}"));
                    }
                });
            }
            if test_mode {
                ui.colored_label(
                    egui::Color32::from_rgb(0x40, 0xB0, 0xF0),
                    format!("{}  TEST — sending disabled", egui_phosphor::regular::FLASK),
                );
            }
            ui.separator();

            let ops_resp = egui::Panel::left("rescue_ops_panel")
                .resizable(true)
                .default_size(ops_w)
                .size_range(180.0..=640.0)
                .show_inside(ui, |ui| {
                    // Scrolled, and auto_shrink off on the vertical axis: without this the column's
                    // content sets a minimum height on the whole window, so a short window pushes
                    // the chat panel off the bottom instead of clipping this column.
                    egui::ScrollArea::vertical()
                        .id_salt("rescue_ops")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                    // Actions outstanding for the ping being worked; the buttons pulse until done.
                    let sel_seq = r.selected_ping;
                    let op_now = r.op_channel;
                    let acts = sel_seq.map(|s| r.actions(s)).unwrap_or_default();
                    let cmd_pending = sel_seq.is_some() && acts.command_comms_op != Some(op_now);
                    let coord_pending = sel_seq.is_some() && !acts.coord_pinged;
                    let invite_pending = sel_seq.is_some() && acts.invited_op != Some(op_now);
                    let (mut mark_cmd, mut mark_coord, mut mark_invite) = (false, false, false);

                    rescue_checklist_ui(ui, &mut *r);
                    ui.add_space(6.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Ping (editable, not auto-sent)").strong());
                        if ui
                            .small_button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                            .on_hover_text("Regenerate from template")
                            .clicked()
                        {
                            r.pending_ping = ping.clone();
                            r.ping_built_for =
                                Some((r.op_channel, r.doctrine.clone(), r.selected_ping));
                            r.ping_edited = false;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Op");
                        egui::ComboBox::from_id_salt("rescue_op")
                            .width(56.0)
                            .selected_text(r.op_channel.to_string())
                            .show_ui(ui, |ui| {
                                // Op 8 command comms does not exist.
                                for n in (1u8..=12).filter(|n| *n != 8) {
                                    ui.selectable_value(&mut r.op_channel, n, n.to_string());
                                }
                            });
                        ui.label("Doctrine");
                        let cur = r.doctrine.clone();
                        egui::ComboBox::from_id_salt("rescue_doc")
                            .width(150.0)
                            .selected_text(if cur.is_empty() { "—".into() } else { cur })
                            .show_ui(ui, |ui| {
                                for d in &doctrines {
                                    ui.selectable_value(&mut r.doctrine, d.name.clone(), &d.name);
                                }
                            });
                    });
                    {
                        let btn = egui::Button::new(format!(
                            "{}  Command Comms",
                            egui_phosphor::regular::HEADSET
                        ));
                        let btn = match pulse_fill(ui, cmd_pending) {
                            Some(c) => btn.fill(c),
                            None => btn,
                        };
                        if ui
                            .add_sized([ui.available_width(), 24.0], btn)
                            .on_hover_text("Open Command comms (Command Sector Alpha) for this op")
                            .clicked()
                        {
                            let _ = open::that(command_mumble_url(op_now));
                            mark_cmd = true;
                        }
                    }
                    // Rebuild from the template on first show and on an op/doctrine change, but not
                    // once the FC has typed into the box: the combos sit right above it, so silently
                    // discarding their edit is too easy to trigger. The refresh button still forces
                    // a rebuild.
                    let ping_key = (r.op_channel, r.doctrine.clone(), r.selected_ping);
                    let switched_ping = r
                        .ping_built_for
                        .as_ref()
                        .is_some_and(|(_, _, seq)| *seq != r.selected_ping);
                    let stale = r.ping_built_for.as_ref() != Some(&ping_key);
                    // A different ping means different pilot/system/cyno, so rebuild even over a
                    // hand-edited draft: sending the previous casualty's details would be worse.
                    if r.pending_ping.is_empty() || switched_ping || (stale && !r.ping_edited) {
                        r.pending_ping = ping.clone();
                        r.ping_built_for = Some(ping_key);
                        if switched_ping {
                            r.ping_edited = false;
                        }
                    }
                    if ui
                        .add(
                            egui::TextEdit::multiline(&mut r.pending_ping)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        r.ping_edited = true;
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(format!("{}  Copy", egui_phosphor::regular::COPY)).clicked() {
                            if let Ok(mut clip) = arboard::Clipboard::new() {
                                let _ = clip.set_text(r.pending_ping.clone());
                            }
                        }
                        // coord/fc/all are directorbot ping GROUPS: prefix "!bping <group>" onto the
                        // ping and post it to skirmish_commanders. Off in test mode.
                        let can_send = !test_mode && !skirmish_jid.is_empty() && jab_connected;
                        for group in ["coord", "fc", "all"] {
                            let btn = egui::Button::new(format!("Ping {group}"));
                            let btn = match pulse_fill(ui, coord_pending && group == "coord") {
                                Some(c) => btn.fill(c),
                                None => btn,
                            };
                            if ui.add_enabled(can_send, btn).clicked() {
                                if let Some(tx) = &tx {
                                    let _ = tx.send(crate::jabber::Cmd::SendRoom {
                                        room: skirmish_jid.clone(),
                                        body: format!("!bping {group}\n\n{}", r.pending_ping),
                                    });
                                }
                                if group == "coord" {
                                    mark_coord = true;
                                }
                            }
                        }
                    });

                    // Pull the pilot who raised the ping into the op's REGULAR comms, addressed by
                    // their delve911 nick so it reads as a direct call-out in the channel.
                    ui.add_space(6.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Comms invite").strong());
                    match rescue_comms_invite(r.ping_author.as_deref(), r.op_channel) {
                        None => {
                            ui.label(
                                egui::RichText::new("No ping author to invite").weak(),
                            );
                        }
                        Some(msg) => {
                            egui::Frame::group(ui.style())
                                .inner_margin(egui::Margin::symmetric(6, 4))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(egui::RichText::new(&msg).monospace());
                                });
                            let can_invite =
                                !test_mode && jab_connected && !delve911_jid.is_empty();
                            let btn = egui::Button::new(format!(
                                "{}  Invite to Op {op_now} comms",
                                egui_phosphor::regular::HEADSET
                            ));
                            let btn = match pulse_fill(ui, invite_pending) {
                                Some(c) => btn.fill(c),
                                None => btn,
                            };
                            if ui
                                .add_enabled(can_invite, btn)
                                .on_hover_text("Post this in delve911")
                                .clicked()
                            {
                                if let Some(tx) = &tx {
                                    let _ = tx.send(crate::jabber::Cmd::SendRoom {
                                        room: delve911_jid.clone(),
                                        body: msg,
                                    });
                                }
                                mark_invite = true;
                            }
                        }
                    }

                    if let Some(seq) = sel_seq {
                        if mark_cmd {
                            r.actions_mut(seq).command_comms_op = Some(op_now);
                        }
                        if mark_coord {
                            r.actions_mut(seq).coord_pinged = true;
                        }
                        if mark_invite {
                            r.actions_mut(seq).invited_op = Some(op_now);
                        }
                    }
                        });
                });
            new_ops_w = ops_resp.response.rect.width();

            egui::CentralPanel::default()
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 0)))
                .show_inside(ui, |ui| {
                egui::Panel::bottom("rescue_chat_reply").show_inside(ui, |ui| {
                    let (room, room_set) = if r.chat_tab == 1 {
                        (skirmish_jid.clone(), !skirmish_jid.is_empty())
                    } else {
                        (delve911_jid.clone(), !delve911_jid.is_empty())
                    };
                    ui.add_space(2.0);
                    if test_mode {
                        ui.label(egui::RichText::new("(reply disabled while testing)").weak());
                    } else if !room_set {
                        ui.label(egui::RichText::new("set this room's JID in settings to reply").weak());
                    }
                    ui.horizontal(|ui| {
                        let clicked = ui.button("Send").clicked();
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut r.delve911_reply)
                                .hint_text("respond…")
                                .margin(egui::Margin::same(2))
                                .desired_width(ui.available_width()),
                        );
                        let enter =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let can_send =
                            !test_mode && room_set && jab_connected && !r.delve911_reply.trim().is_empty();
                        if (enter || clicked) && can_send {
                            if let Some(tx) = &tx {
                                let _ = tx.send(crate::jabber::Cmd::SendRoom {
                                    room,
                                    body: r.delve911_reply.clone(),
                                });
                            }
                            r.delve911_reply.clear();
                        }
                    });
                });
                egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
                    ui.columns(2, |c| {
                        let w0 = c[0].available_width();
                        if c[0]
                            .add_sized([w0, 22.0], egui::Button::selectable(r.chat_tab == 0, "delve911"))
                            .clicked()
                        {
                            r.chat_tab = 0;
                        }
                        let w1 = c[1].available_width();
                        if c[1]
                            .add_sized([w1, 22.0], egui::Button::selectable(r.chat_tab == 1, "skirmish"))
                            .clicked()
                        {
                            r.chat_tab = 1;
                        }
                    });
                    ui.separator();
                    // Don't snap to the bottom while the pointer is held, so a drag-select isn't wiped
                    // by an incoming message (parity with the main chat).
                    let selecting = ui.input(|i| i.pointer.any_down());
                    egui::ScrollArea::vertical()
                        .id_salt("rescue_chat")
                        .auto_shrink([false, false])
                        .stick_to_bottom(!selecting)
                        .show(ui, |ui| {
                            let hit = if r.chat_tab == 1 {
                                rescue_chat_feed(ui, &skirmish_msgs, "skirmish")
                            } else {
                                rescue_chat_feed(ui, &delve911_msgs, "delve911")
                            };
                            if let Some((act, who, body)) = hit {
                                match act {
                                    MsgRowAction::Copy => ui.ctx().copy_text(body),
                                    MsgRowAction::Mention => {
                                        let d = &mut r.delve911_reply;
                                        if !d.is_empty() && !d.ends_with(char::is_whitespace) {
                                            d.push(' ');
                                        }
                                        d.push_str(&format!("{who}: "));
                                    }
                                    MsgRowAction::GoToDm => chat_dm = Some(who),
                                    MsgRowAction::None => {}
                                }
                            }
                        });
                });
            });

        });

        // Persist the ops column width once a resize drag ends (avoids a write storm mid-drag).
        if !ui.ctx().input(|i| i.pointer.any_down())
            && (new_ops_w - self.settings.rescue_col_ops_w).abs() > 1.0
        {
            self.settings.rescue_col_ops_w = new_ops_w;
            self.needs_save = true;
        }

        if retry_click {
            self.jabber_retry();
        }
        if let Some(sid) = set_dest {
            self.rescue_push_destination(sid);
        }
        if let Some(nick) = chat_dm {
            let dm = self.full_user_jid(&nick);
            self.jabber_mark_read(&dm);
            self.settings.jabber_closed_dms.retain(|j| j != &dm);
            self.jabber_open(&dm, ChatWinKey::Main);
            self.view = nav::View::Jabber;
            self.raise_main = true;
        }

        // Persist op/doctrine so the next rescue starts where we left off.
        let (op, doc) = {
            let r = self.rescue.lock().unwrap();
            (r.op_channel, r.doctrine.clone())
        };
        if op != self.settings.rescue_op_channel || doc != self.settings.rescue_doctrine {
            self.settings.rescue_op_channel = op;
            self.settings.rescue_doctrine = doc;
            self.needs_save = true;
        }
    }
}
