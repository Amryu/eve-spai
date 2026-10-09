//! The delve911 capital rescue mode: its feed, settings, ping builder, chat helpers and window.

use super::*;

/// What an op's comms channel is doing, for the rescue's op picker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum OpUsage {
    Free,
    /// Another fleet is on it. Pinging a rescue onto it puts two fleets in one channel.
    InUse,
    /// The fleet this app is tracking is on it, which is expected.
    Ours,
    /// The dashboard has no channel by that name.
    NoChannel,
}

/// How long a failed dashboard ping render is left alone before asking again.
const PREVIEW_RETRY: std::time::Duration = std::time::Duration::from_secs(60);

impl SpaiApp {
    /// Parse new delve911 XMPP messages into rescue events. The in-game chat-log watcher covers the
    /// EVE channel of the same name; the real pings come through the MUC, so both feed `rescue`.
    pub(crate) fn ingest_delve911_jabber(&mut self) {
        if !self.rescue_on() {
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

        // A restart replays the last two minutes (below), and being yanked out of the game by a
        // ping that was already dealt with before the app started is not a rescue.
        let catching_up = self.delve911_cursor == 0;
        let fresh: Vec<(String, String, i64)> = {
            let j = self.jabber.lock().unwrap();
            let Some(msgs) = j.chats.get(&jid) else { return };
            // First sight: start two minutes back, so restarting replays the ping that just landed
            // without replaying days of loaded history.
            if self.delve911_cursor == 0 {
                self.delve911_cursor = crate::clock::utc().timestamp() - 120;
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
        // Anything that is not a plain stand-down is worth the FC's attention right now.
        let mut wake = false;
        for (from, body, time) in fresh {
            self.delve911_cursor = self.delve911_cursor.max(time);
            wake |= !crate::rescue::is_safe_call(&body);
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
        drop(r);
        if wake && !catching_up {
            self.view = nav::View::Rescue;
            self.raise_main = true;
        }
    }

    /// Move newly-parsed delve911 ping events into the fleet-ping feed exactly once each.
    /// Runs on the UI thread so it can take the jabber lock (the watcher only writes `rescue`).
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
                j.pings_new = j.pings_new.saturating_add(1);
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
    pub(crate) fn rescue_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.heading("FC / Rescue (delve911)");
        ui.label(
            egui::RichText::new(
                "Capital-rescue coordination, on top of the fleet dashboard: a rescue runs on a \
                 fleet preset tagged Capital Save and hands over to fleet tracking. Needs the \
                 fleet dashboard switched on above, plus the delve911 rooms.",
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
            ui.label("delve911 room").on_hover_text(
                "The Jabber room on conference.goonfleet.com where rescue requests come in.",
            );
            changed |= ui
                .add(egui::TextEdit::singleline(&mut self.settings.rescue_delve911_jid).hint_text("delve911").desired_width(220.0))
                .changed();
            ui.end_row();
            ui.label("Skirmish commanders room").on_hover_text("The Jabber room ping requests go to, on conference.goonfleet.com.");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.settings.rescue_skirmish_jid)
                        .hint_text("skirmish_commanders")
                        .desired_width(220.0),
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

        changed
    }

    /// Recompute the titan-range check only when staging or the target changes: it scans every
    /// system for the nearest in-range jump-off point, which is far too much for every frame.
    pub(crate) fn update_rescue_range(&mut self) {
        let target = self.rescue.lock().unwrap().capital_system;
        let (Some(systems), Some(coords), Some(target)) =
            (self.systems.clone(), self.map_coords.clone(), target)
        else {
            self.rescue_range = None;
            self.rescue_range_for = None;
            self.rescue_ly = None;
            return;
        };
        let Some(staging) =
            systems.lookup(&self.settings.rescue_staging_system).map(|i| i.id)
        else {
            self.rescue_range = None;
            self.rescue_range_for = None;
            self.rescue_ly = None;
            return;
        };
        if self.rescue_range_for == Some((staging, target)) {
            return;
        }
        self.rescue_range_for = Some((staging, target));
        self.rescue_range = None;
        self.rescue_ly = None;

        let at = |id: i64| coords.iter().find(|s| s.id == id);
        let (Some(stage_pos), Some(target_pos)) = (at(staging), at(target)) else { return };
        let ly_from_staging = crate::map::ly_distance(stage_pos, target_pos);
        self.rescue_ly = Some(ly_from_staging);
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

    /// The ping to send: the dashboard's own rendering when it could be had, the local template
    /// when it could not.
    ///
    /// The dashboard is the authority on what a ping looks like, and its format changes without
    /// telling us. The template is what keeps a rescue possible with no session or no dashboard.
    pub(crate) fn build_rescue_ping(&self) -> String {
        if let Some(p) = self
            .fleet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .rescue_preview
            .as_ref()
            .filter(|p| !p.ping.trim().is_empty())
        {
            return p.ping.clone();
        }
        self.rescue_ping_from_template()
    }

    /// Build the ping text from the template and current rescue state. `{doctrine}` expands to the
    /// selected doctrine's full description line.
    pub(crate) fn rescue_ping_from_template(&self) -> String {
        let r = self.rescue.lock().unwrap();
        let sys = r.capital_system_name.clone().unwrap_or_default();
        let pilot = r.capital_pilot.clone().unwrap_or_default();
        let cyno = r.cyno_pilot.clone().unwrap_or_default();
        let anom = r.anomaly.clone().unwrap_or_default();
        let op = r.op_channel.to_string();
        let mumble = op_comms_url(r.op_channel);
        let want = r.doctrine.clone();
        // Dropped before anything that takes it again: `rescue_preset` locks the same mutex, and a
        // std Mutex taken twice on one thread is a hang, not an error.
        drop(r);
        // The doctrine and the formup come from the fleet preset the rescue is running on, so
        // there is one place they are configured rather than two that drift apart.
        let all = self.rescue_presets();
        let preset = crate::settings::find_preset(&all, &want).or_else(|| all.first());
        // The dashboard's own line when one has ever been seen for this setup, the bare setup name
        // when it has not. A preset's doctrine notes come after it on their own line, which is
        // where the dashboard puts them.
        let doctrine = preset
            .map(|p| {
                let head = self
                    .settings
                    .fleet_doctrine_lines
                    .iter()
                    .find(|(id, _)| *id == p.setup_id)
                    .map(|(_, line)| line.clone())
                    .unwrap_or_else(|| self.fleet_setup_name(p.setup_id));
                match p.doctrine_notes.trim() {
                    "" => head,
                    notes => format!("{head}\n{notes}"),
                }
            })
            .unwrap_or_default();
        let staging = preset
            .and_then(|p| p.formup_location.as_ref().map(|(_, n)| n.clone()))
            .unwrap_or_else(|| self.settings.rescue_staging_system.clone());
        crate::fleets::ping::render(
            &self.settings.rescue_ping_template,
            &crate::fleets::ping::Vars {
                system: sys,
                pilot,
                cyno,
                anomaly: anom,
                op,
                doctrine,
                staging,
                fc: self.active_character.clone(),
                mumble,
            },
        )
    }

    /// Asks the dashboard to render the rescue's ping, no more often than the preset and op
    /// channel actually change.
    pub(crate) fn rescue_preview_poll(&mut self) {
        let (want, op) = {
            let r = self.rescue.lock().unwrap_or_else(|e| e.into_inner());
            (r.doctrine.clone(), r.op_channel)
        };
        let key = format!("{want}/{op}");
        if self.rescue_preview_key.as_deref() == Some(key.as_str()) {
            // One that came back empty means the dashboard could not be reached, and the window
            // has been showing the local template since. Ask again on a slow clock so the FC does
            // not have to touch the preset to recover from a blip.
            let held = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).rescue_preview.is_none();
            let stale = self
                .rescue_preview_at
                .is_none_or(|t| std::time::Instant::now().duration_since(t) >= PREVIEW_RETRY);
            if !held || !stale {
                return;
            }
        }
        let all = self.rescue_presets();
        let Some(preset) = crate::settings::find_preset(&all, &want).or_else(|| all.first()) else {
            return;
        };
        // A new op or preset makes the rendering already held wrong: it names the old comms. Drop
        // it now, so the ping falls back to the local template for the new op until the
        // dashboard answers, instead of showing the old channel until then.
        if self.rescue_preview_key.as_deref() != Some(key.as_str()) {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).rescue_preview = None;
        }
        self.rescue_preview_key = Some(key);
        self.rescue_preview_at = Some(std::time::Instant::now());
        self.fleet_gen.rescue += 1;
        let mut req = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.ping_request_from(preset)
        };
        req.mumble_channel_id = self.fleet_mumble_for_op(op);
        self.fleet_dispatch(crate::fleets::state::Cmd::RescuePreview(req));
    }

    /// Hands the rescue over to the fleet tab: the preset fills the start form, the comms channel
    /// follows whatever the rescue settled on, and the user lands on the form ready to track.
    pub(crate) fn rescue_start_tracking(&mut self) {
        let (want, op) = {
            let r = self.rescue.lock().unwrap();
            (r.doctrine.clone(), r.op_channel)
        };
        let all = self.rescue_presets();
        let Some(mut preset) =
            crate::settings::find_preset(&all, &want).or_else(|| all.first()).cloned()
        else {
            return;
        };
        // The rescue's own op channel wins: it is the one the FC has been telling people.
        preset.mumble_channel_id = self.fleet_mumble_for_op(op).map(|c| c.0);
        {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.apply_preset(&preset);
            st.page = crate::fleets::state::Page::Start;
        }
        self.view = nav::View::Fleet;
        self.raise_main = true;
    }

    /// The presets a rescue can run on: the ones tagged Capital Save, and nothing else.
    pub(crate) fn rescue_presets(&self) -> Vec<crate::settings::FleetPreset> {
        self.settings
            .fleet_presets
            .iter()
            .filter(|p| p.tag_ids.contains(&crate::settings::CAPITAL_SAVE_TAG))
            .cloned()
            .collect()
    }

    /// The comms channel an op number names, out of the dashboard's own list.
    pub(crate) fn fleet_mumble_for_op(
        &self,
        op: u8,
    ) -> Option<crate::fleets::model::ChannelId> {
        self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.mumble_channel_for_op(op)
    }

    /// What the dashboard calls the channel this op number lands on, for showing the FC before
    /// anything is sent.
    pub(crate) fn fleet_op_channel_name(&self, op: u8) -> Option<String> {
        let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
        let id = st.seed.mumble_channel_for_op(op)?;
        st.seed.channel_name(&st.seed.mumble_channels, Some(id)).map(|s| s.to_owned())
    }

    /// Whether an op's comms channel is free, per the dashboard's channel list.
    pub(crate) fn fleet_op_usage(&self, op: u8) -> OpUsage {
        let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
        let Some(id) = st.seed.mumble_channel_for_op(op) else { return OpUsage::NoChannel };
        let in_use = st.seed.mumble_channels.iter().any(|c| c.id == id && c.is_in_use);
        // The fleet this app is running holds its own channel, which is the point of it.
        let ours = st
            .open
            .value
            .as_ref()
            .is_some_and(|o| o.fleet.closed_at.is_none() && o.fleet.mumble_channel_id == Some(id));
        match (in_use, ours) {
            (_, true) => OpUsage::Ours,
            (true, false) => OpUsage::InUse,
            (false, false) => OpUsage::Free,
        }
    }

    /// A setup's name out of the fleet seed, for the doctrine line of a ping.
    pub(crate) fn fleet_setup_name(&self, setup_id: i32) -> String {
        let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
        st.seed
            .setups
            .iter()
            .find(|s| s.id.0 == setup_id)
            .map(|s| s.name.trim().to_owned())
            .unwrap_or_default()
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

    /// The rescue tab. Without the feature it is not in the rail at all, so this only says so for
    /// the case where someone reaches the view some other way.
    pub(crate) fn rescue_view(&mut self, ui: &mut egui::Ui) {
        if self.settings.rescue_popped {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Rescue is in its own window.").weak());
            if ui.button("Dock rescue").clicked() {
                self.settings.rescue_popped = false;
                self.needs_save = true;
            }
            return;
        }
        self.rescue_window_body(ui);
    }

    /// The rescue tab in its own window, compact: the ping and its buttons on top, the delve911
    /// and skirmish chat below, and a pin to keep it over EVE.
    #[allow(deprecated)]
    pub(crate) fn rescue_popout_window(&mut self, ctx: &egui::Context) {
        let mut builder = egui::ViewportBuilder::default()
            .with_icon(app_icon())
            .with_title("EVE Spai - Rescue")
            .with_min_inner_size([420.0, 360.0])
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        if !self.rescue_geom_applied {
            let (w, h) = self.settings.rescue_popout_size.unwrap_or((520.0, 640.0));
            builder = builder.with_inner_size([w, h]);
            if let Some((x, y)) = self.settings.rescue_popout_pos {
                builder = builder.with_position([x, y]);
            }
            self.rescue_pos_fix = super::alert_window::PosFix::new(self.settings.rescue_popout_pos);
            self.rescue_geom_applied = true;
        }
        let mut keep = true;
        let mut geom: Option<((f32, f32), Option<(f32, f32)>)> = None;
        let mut pos_fix = self.rescue_pos_fix;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("rescue_window"), builder, |ctx, _| {
            super::alert_window::apply_pos_fix(ctx, &mut pos_fix);
            egui::CentralPanel::default().show(ctx, |ui| self.rescue_body(ui, true));
            let sz = ctx.content_rect().size();
            if sz.x > 100.0 && sz.y > 100.0 {
                geom = Some(((sz.x, sz.y), ctx.input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y)))));
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                keep = false;
            }
        });
        self.rescue_pos_fix = pos_fix;
        if let Some((sz, pos)) = geom.filter(|_| pos_fix.is_none()) {
            if let Some(s) = super::alert_window::geometry_update(self.settings.rescue_popout_size, sz, 2.0) {
                self.settings.rescue_popout_size = Some(s);
                self.needs_save = true;
            }
            if let Some(p) = pos.and_then(|p| super::alert_window::geometry_update(self.settings.rescue_popout_pos, p, 1.0)) {
                self.settings.rescue_popout_pos = Some(p);
                self.needs_save = true;
            }
        }
        if !keep {
            self.settings.rescue_popped = false;
            self.rescue_geom_applied = false;
            self.needs_save = true;
        }
    }

    pub(crate) fn rescue_window_body(&mut self, ui: &mut egui::Ui) {
        self.rescue_body(ui, false);
    }

    /// The rescue view. `compact` is the popped-out window: the ops column becomes a band over
    /// the chat, with its buttons sharing rows.
    pub(crate) fn rescue_body(&mut self, ui: &mut egui::Ui, compact: bool) {
        // This viewport is the only one on screen while a rescue runs, and the fleet tab is where
        // worker results are normally taken off the channel. Without this the dashboard's ping
        // lands nowhere and the window keeps rendering the local template.
        self.fleet_collect();
        self.fleet_cache_doctrine_line();
        // The dashboard's rendering of the ping, refreshed when the preset or op channel change.
        self.rescue_preview_poll();
        // The same check the start form runs, on the same clock: a rescue ends in tracking a
        // fleet, and finding out there is nothing to track at that point is finding out too late.
        self.fleet_boss_poll(ui.ctx());
        // The rescue picks an op channel too, so it wants the same fresh `isInUse`.
        self.fleet_channel_poll(ui.ctx());
        // Clone what the columns need so the render closure never borrows `self` (it holds the
        // rescue lock). Deferred self-mutations go through flags applied after the lock drops.
        let ping = self.build_rescue_ping();
        // Resolved before the render closure, which holds the rescue lock the lookup would need.
        let op_channel_name = {
            let op = self.rescue.lock().unwrap_or_else(|e| e.into_inner()).op_channel;
            self.fleet_op_channel_name(op)
        };
        // Every op's state up front, so the picker can say which ones are taken before one is
        // chosen rather than after. Refreshed once a minute by the channel poll.
        let op_usage: Vec<(u8, OpUsage)> =
            (1u8..=12).filter(|n| *n != 8).map(|n| (n, self.fleet_op_usage(n))).collect();
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
        let presets = self.rescue_presets();
        // Each preset's op number, by its channel's name. The preset stores the dashboard's channel
        // id, and the id is not the op: channel 12 is Op 11.
        let preset_ops: Vec<Option<u8>> = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            presets
                .iter()
                .map(|p| {
                    let id = crate::fleets::model::ChannelId(p.mumble_channel_id?);
                    st.seed
                        .channel_name(&st.seed.mumble_channels, Some(id))
                        .and_then(crate::fleets::comms::op_number)
                })
                .collect()
        };
        // Every preset tagged Capital Save, from any folder. A name used in two folders shows its
        // folder, and only then: the usual list is one folder of distinct names.
        let preset_names: Vec<String> = presets
            .iter()
            .map(|p| {
                let shared = presets.iter().filter(|q| q.label == p.label).count() > 1;
                if shared {
                    crate::settings::preset_key_label(&p.key())
                } else {
                    p.label.clone()
                }
            })
            .collect();
        // Cloned out before the render closures, which cannot borrow `self` while state is held.
        let boss: Option<(bool, String)> = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.fc()
                .and_then(|(id, _)| st.boss.as_ref().filter(|(who, _)| *who == id))
                .map(|(_, c)| c.verdict())
        };
        let (jab_connected, jab_status, jab_retry_in, _) = self.jabber_conn();
        let jabber_set_up = !self.settings.jabber_jid.trim().is_empty();
        let mut open_jabber = false;
        let mut retry_click = false;
        let recheck = std::cell::Cell::new(false);
        // Who the route button sends to, and who else it may: worked out before the closures, which
        // hold the rescue lock and so cannot ask.
        let mut set_dest: Option<(i64, Vec<String>)> = None;
        let fc_char = self.rescue_fc_character();
        let route_choices = self.destination_choices();
        let mut start_tracking = false;
        let mut chat_dm: Option<String> = None;
        let in_range_ly = self.rescue_ly.filter(|_| self.rescue_range.is_none());
        let staging_name = self.settings.rescue_staging_system.clone();
        let mut pop_out = false;
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

        // Gathered before the rescue lock below, which the route's own lookups would need.
        let map = if compact { None } else { self.rescue_map_data() };
        let mut map_view = std::mem::take(&mut self.rescue_map_view);
        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            let mut r = self.rescue.lock().unwrap();
            let test_mode = r.test_mode;

            // Nothing sent from this window can leave the machine while XMPP is down, and an empty
            // chat pane looks identical to a quiet channel, so say so loudly.
            if !jab_connected && !jabber_set_up {
                // Never set up: retrying has nothing to connect with.
                ui.horizontal_wrapped(|ui| {
                    let why = "Jabber is not set up: rescue pings to delve911 and the skirmish channel go through it.";
                    ui.colored_label(
                        egui::Color32::from_rgb(0xE0, 0x3B, 0x2E),
                        format!("{}  {}", egui_phosphor::regular::PLUGS, if compact { "Jabber is not set up" } else { why }),
                    )
                    .on_hover_text(why);
                    if ui.button("Set up Jabber").clicked() {
                        open_jabber = true;
                    }
                });
                ui.separator();
            } else if !jab_connected {
                ui.horizontal_wrapped(|ui| {
                    let why = format!("Jabber disconnected — pings cannot be sent. {jab_status}");
                    ui.colored_label(
                        egui::Color32::from_rgb(0xE0, 0x3B, 0x2E),
                        format!("{}  {}", egui_phosphor::regular::PLUGS, if compact { "Jabber disconnected" } else { why.as_str() }),
                    )
                    .on_hover_text(&why);
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

            // The open pings, newest at the far right: beside the casualty, or on their own row in
            // the narrow pop-out.
            let mut chips = |ui: &mut egui::Ui| {
                if listed.is_empty() {
                    ui.label(egui::RichText::new("No delve911 pings yet").weak());
                    return;
                }
                let now = crate::clock::utc().timestamp();
                // Newest first: in a right-to-left layout the first widget lands furthest right.
                for (i, (seq, chip, sys, cyno, received)) in listed.iter().enumerate() {
                    if spai_ui::widgets::icon_button(ui, egui_phosphor::regular::CHECK)
                        .on_hover_text("Resolved, dismiss this ping")
                        .clicked()
                    {
                        resolve = Some(*seq);
                    }
                    let age = fmt_age_compact(now - received);
                    // The clock on the chip, not only in the tooltip: which ping has been
                    // running longest is the first thing to know with several open.
                    let mut text =
                        egui::RichText::new(format!("{chip}  {}", since_ping(now - received)));
                    if i == 0 {
                        text = text.strong().color(egui::Color32::from_rgb(0xE6, 0xA5, 0x1E));
                    }
                    let btn = egui::Button::new(text)
                        .selected(selected == Some(*seq))
                        .frame_when_inactive(selected == Some(*seq))
                        .stroke(egui::Stroke::NONE);
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
            };
            ui.horizontal(|ui| {
                let big = |t: egui::RichText| if compact { t.strong() } else { t.heading() };
                ui.label(big(egui::RichText::new(egui_phosphor::regular::WARNING_OCTAGON)));
                let pilot = r.capital_pilot.clone().unwrap_or_else(|| "unknown".into());
                let sys = r.capital_system_name.clone().unwrap_or_else(|| "?".into());
                ui.label(big(egui::RichText::new(pilot).strong()));
                ui.label("in");
                ui.label(big(egui::RichText::new(sys).strong()));
                if let Some(class) = r.cap_class {
                    ui.label(format!("[{}]", class.label()));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if compact {
                        super::ontop_pin_ui(ui, "rescue_window");
                        // The destination and range rows, folded into this one as icons.
                        let target = r.capital_system.zip(r.capital_system_name.clone());
                        if let Some(pick) = rescue_route_button(ui, egui_phosphor::regular::MAP_PIN_LINE.to_owned(), target.as_ref(), fc_char.as_deref(), &route_choices) {
                            set_dest = target.as_ref().map(|(id, _)| (*id, pick));
                        }
                        if let Some(ly) = in_range_ly {
                            ui.label(egui::RichText::new(egui_phosphor::regular::CHECK_CIRCLE).color(crate::theme::standing::FRIENDLY))
                                .on_hover_text(format!("In titan range, {ly:.1} ly from {staging_name}"));
                        }
                        if let Some((headline, detail)) = &range_warning {
                            ui.label(egui::RichText::new(format!("{}  out of range", egui_phosphor::regular::WARNING)).strong().color(egui::Color32::from_rgb(0xE0, 0x3B, 0x2E)))
                                .on_hover_text(format!("{headline}\n{detail}"));
                        }
                        if test_mode {
                            ui.label(egui::RichText::new(egui_phosphor::regular::FLASK).color(egui::Color32::from_rgb(0x40, 0xB0, 0xF0)))
                                .on_hover_text("Test: sending disabled");
                        }
                    } else if spai_ui::widgets::icon_button(ui, egui_phosphor::regular::ARROW_SQUARE_OUT)
                        .on_hover_text("Pop out into its own window, over EVE")
                        .clicked()
                    {
                        pop_out = true;
                    }
                    if !compact {
                        chips(ui);
                    }
                });
            });
            if compact {
                let cyno = [r.cyno_pilot.as_ref().map(|c| format!("cyno: {c}")), r.anomaly.as_ref().map(|a| format!("@ {a}"))]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ");
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        chips(ui);
                        if !cyno.is_empty() {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| ui.label(egui::RichText::new(cyno).weak()));
                        }
                    })
                });
            }
            if let Some(seq) = pick {
                r.select_ping(seq);
            }
            if let Some(seq) = resolve {
                r.resolve_ping(seq);
            }
            // The destination is auto-pushed when the selected ping changes, but never re-asserted,
            // so this is the way back after routing somewhere else mid-rescue. Routes to the
            // casualty's system, not staging.
            if !compact {
            ui.horizontal(|ui| {
                let target = r.capital_system.zip(r.capital_system_name.clone());
                let label = match &target {
                    Some((_, name)) => format!("{}  {name}", egui_phosphor::regular::MAP_PIN_LINE),
                    None => format!("{}  Set Destination", egui_phosphor::regular::MAP_PIN_LINE),
                };
                if let Some(pick) = rescue_route_button(ui, label, target.as_ref(), fc_char.as_deref(), &route_choices) {
                    set_dest = target.as_ref().map(|(id, _)| (*id, pick));
                }
                if let Some(ly) = in_range_ly {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  in titan range, {ly:.1} ly from {staging_name}",
                            egui_phosphor::regular::CHECK_CIRCLE
                        ))
                        .color(crate::theme::standing::FRIENDLY),
                    );
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
            if !compact && (r.cyno_pilot.is_some() || r.anomaly.is_some()) {
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
            }
            ui.separator();

            let mut ops_body = |ui: &mut egui::Ui| {
                    // Compact, the band takes its content's height and the chat the rest, so it
                    // is not scrolled.
                    let mut inner = |ui: &mut egui::Ui| {
                    // Actions outstanding for the ping being worked; the buttons pulse until done.
                    let sel_seq = r.selected_ping;
                    let op_now = r.op_channel;
                    let acts = sel_seq.map(|s| r.actions(s)).unwrap_or_default();
                    let cmd_pending = sel_seq.is_some() && acts.command_comms_op != Some(op_now);
                    let coord_pending = sel_seq.is_some() && !acts.coord_pinged;
                    let invite_pending = sel_seq.is_some() && acts.invited_op != Some(op_now);
                    let (mut mark_cmd, mut mark_coord, mut mark_invite) = (false, false, false);

                    // The rule belongs to the timer: with no ping selected the timer draws
                    // nothing and the line was left sitting at the top of the pane dividing
                    // nothing from the ping below it.
                    // Compact, the ping's button carries its clock.
                    if !compact && ping_timer_ui(ui, &r) {
                        ui.add_space(4.0);
                        ui.separator();
                    }
                    let regen = |ui: &mut egui::Ui, r: &mut crate::rescue::RescueState| {
                        if spai_ui::widgets::icon_button(ui, egui_phosphor::regular::ARROWS_CLOCKWISE)
                            .on_hover_text("Regenerate the ping from the template")
                            .clicked()
                        {
                            r.pending_ping = ping.clone();
                            r.ping_built_for = Some((r.op_channel, r.doctrine.clone(), r.selected_ping));
                            r.ping_edited = false;
                        }
                    };
                    if !compact {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Ping (editable, not auto-sent)").strong());
                            regen(ui, &mut r);
                        });
                    }
                    ui.horizontal_wrapped(|ui| {
                        if compact {
                            regen(ui, &mut r);
                        }
                        ui.label("Op");
                        egui::ComboBox::from_id_salt("rescue_op")
                            .width(56.0)
                            .selected_text(r.op_channel.to_string())
                            .show_ui(ui, |ui| {
                                // Op 8 command comms does not exist.
                                for (n, usage) in &op_usage {
                                    let text = match usage {
                                        OpUsage::InUse => egui::RichText::new(format!("{n}  in use"))
                                            .color(crate::theme::standing::HOSTILE),
                                        OpUsage::Ours => {
                                            egui::RichText::new(format!("{n}  your fleet")).weak()
                                        }
                                        _ => egui::RichText::new(n.to_string()),
                                    };
                                    ui.menu_value(&mut r.op_channel, *n, text);
                                }
                            });
                        // What the dashboard will actually be told. The op number is not the
                        // channel id, so the FC sees the name before anything goes out.
                        let usage = op_usage
                            .iter()
                            .find(|(n, _)| *n == r.op_channel)
                            .map(|(_, u)| *u)
                            .unwrap_or(OpUsage::NoChannel);
                        match &op_channel_name {
                            Some(name) => {
                                ui.label(egui::RichText::new(name).weak())
                                    .on_hover_text("The comms channel the ping will name.");
                                match usage {
                                    OpUsage::InUse => {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}  in use",
                                                egui_phosphor::regular::WARNING
                                            ))
                                            .strong()
                                            .color(crate::theme::standing::HOSTILE),
                                        )
                                        .on_hover_text(
                                            "Another fleet is on this channel. A rescue pinged \
                                             onto it puts two fleets in one channel: pick a free op \
                                             first.",
                                        );
                                    }
                                    OpUsage::Ours => {
                                        ui.label(egui::RichText::new("your fleet").weak())
                                            .on_hover_text(
                                                "The fleet you are tracking is on this channel.",
                                            );
                                    }
                                    OpUsage::Free | OpUsage::NoChannel => {}
                                }
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new("no such channel")
                                        .color(crate::theme::standing::HOSTILE),
                                )
                                .on_hover_text(
                                    "The dashboard has no channel by that name, so the ping \
                                     would not name one. Pick another op.",
                                );
                            }
                        }
                        ui.label("Preset");
                        let cur = r.doctrine.clone();
                        let shown = crate::settings::find_preset(&presets, &cur)
                            .and_then(|p| presets.iter().position(|q| q.key() == p.key()))
                            .map(|i| preset_names[i].clone())
                            .unwrap_or_else(|| crate::settings::preset_key_label(&cur));
                        // Compact, what is left of the row, so a narrow window wraps rather than widens.
                        let preset_w = if compact { (ui.available_size_before_wrap().x - 28.0).clamp(90.0, 170.0) } else { 170.0 };
                        egui::ComboBox::from_id_salt("rescue_preset")
                            .width(preset_w)
                            .selected_text(if cur.is_empty() { "—".into() } else { shown })
                            .show_ui(ui, |ui| {
                                if presets.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No preset tagged Capital Save").weak(),
                                    );
                                }
                                for (i, p) in presets.iter().enumerate() {
                                    if ui
                                        .menu_value(&mut r.doctrine, p.key(), &preset_names[i])
                                        .changed()
                                    {
                                        // A preset carries its own comms; the op stays editable
                                        // after, since a rescue often moves channel.
                                        if let Some(op) = preset_ops[i] {
                                            r.op_channel = op;
                                        }
                                    }
                                }
                            })
                            .response
                            .on_hover_text("A different doctrine means a different preset.");
                    });
                    let command_comms = |ui: &mut egui::Ui, wide: bool| -> bool {
                        let btn = egui::Button::new(format!(
                            "{}  {}",
                            egui_phosphor::regular::HEADSET,
                            if wide { "Command Comms" } else { "Comms" }
                        ));
                        let btn = match pulse_fill(ui, cmd_pending) {
                            Some(c) => btn.fill(c),
                            None => btn,
                        };
                        let resp = if wide { ui.add_sized([ui.available_width(), 24.0], btn) } else { ui.add(btn) };
                        resp
                            .on_hover_text("Open Command comms (Command Sector Alpha) for this op")
                            .clicked()
                    };
                    if !compact && command_comms(ui, true) {
                        let _ = open::that(command_mumble_url(op_now));
                        mark_cmd = true;
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
                    // The dashboard's rendering arrives a moment after the op or preset changed;
                    // an untouched draft takes it when it does, or it keeps the stop-gap for good.
                    let rerendered = ping != r.ping_built_from;
                    // A different ping means different pilot/system/cyno, so rebuild even over a
                    // hand-edited draft: sending the previous casualty's details would be worse.
                    if r.pending_ping.is_empty()
                        || switched_ping
                        || ((stale || rerendered) && !r.ping_edited)
                    {
                        r.pending_ping = ping.clone();
                        r.ping_built_from = ping.clone();
                        r.ping_built_for = Some(ping_key);
                        if switched_ping {
                            r.ping_edited = false;
                        }
                    }
                    if ui
                        .add(
                            egui::TextEdit::multiline(&mut r.pending_ping)
                                .desired_rows(if compact { 2 } else { 3 })
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        r.ping_edited = true;
                    }
                    // Pull the pilot who raised the ping into the op's REGULAR comms, addressed by
                    // their delve911 nick so it reads as a direct call-out in the channel.
                    let ping_author = r.ping_author.clone();
                    let op_for_invite = r.op_channel;
                    let mut invite_ui = |ui: &mut egui::Ui| {
                        match rescue_comms_invite(ping_author.as_deref(), op_for_invite) {
                            None => {
                                if !compact {
                                    ui.label(egui::RichText::new("No ping author to invite").weak());
                                }
                            }
                            Some(msg) => {
                                // Compact, the message is the button's tooltip rather than a box.
                                if !compact {
                                    egui::Frame::group(ui.style())
                                        .inner_margin(egui::Margin::symmetric(6, 4))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.label(egui::RichText::new(&msg).monospace());
                                        });
                                }
                                let can_invite =
                                    !test_mode && jab_connected && !delve911_jid.is_empty();
                                let btn = egui::Button::new(if compact {
                                    format!("{}  Invite", egui_phosphor::regular::USER_PLUS)
                                } else {
                                    format!("{}  Invite to Op {op_now} comms", egui_phosphor::regular::HEADSET)
                                });
                                let btn = match pulse_fill(ui, invite_pending) {
                                    Some(c) => btn.fill(c),
                                    None => btn,
                                };
                                let hover = if compact { format!("Invite to Op {op_now} comms, in delve911:\n{msg}") } else { "Post this in delve911".to_owned() };
                                if ui
                                    .add_enabled(can_invite, btn)
                                    .on_hover_text(hover)
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
                    };
                    // Whether the FC is actually boss of a fleet in game, on the same clock the
                    // start form uses. Nothing else tells you before you try to track.
                    let can_track = boss.as_ref().is_some_and(|(ok, _)| *ok);
                    let track_button = |ui: &mut egui::Ui, wide: bool| -> bool {
                        let b = egui::Button::new(format!("{}  {}", egui_phosphor::regular::ROCKET_LAUNCH, if wide { "Start tracking" } else { "Track" }));
                        let b = if wide { b.min_size(egui::vec2(ui.available_width(), 24.0)) } else { b };
                        ui.add_enabled(can_track, b)
                            .on_hover_text("Fill the start form from this preset and open the fleet tab")
                            .on_disabled_hover_text(
                                "That character is not the boss of a fleet in game, so there would be \
                                 nothing to track.",
                            )
                            .clicked()
                    };
                    // The fleet boss verdict: compact, its icon with the words on hover.
                    let verdict = |ui: &mut egui::Ui| {
                        let (glyph, colour, text) = match &boss {
                            Some((true, why)) => (
                                egui_phosphor::regular::CHECK_CIRCLE,
                                crate::theme::standing::FRIENDLY,
                                why.clone(),
                            ),
                            Some((false, why)) => (
                                egui_phosphor::regular::WARNING,
                                crate::theme::standing::WARNING,
                                why.clone(),
                            ),
                            None => (
                                egui_phosphor::regular::CLOCK_COUNTDOWN,
                                ui.visuals().weak_text_color(),
                                "Fleet boss not checked".to_owned(),
                            ),
                        };
                        let icon = ui.label(egui::RichText::new(glyph).color(colour));
                        if compact {
                            icon.on_hover_text(text);
                        } else {
                            ui.label(egui::RichText::new(text).color(colour));
                        }
                        if ui
                            .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                            .on_hover_text("Recheck the fleet boss and which op channels are free")
                            .clicked()
                        {
                            recheck.set(true);
                        }
                    };
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(format!("{}  Copy", egui_phosphor::regular::COPY)).clicked() {
                            if let Ok(mut clip) = arboard::Clipboard::new() {
                                let _ = clip.set_text(r.pending_ping.clone());
                            }
                        }
                        // coord and fc are directorbot ping GROUPS, posted to skirmish_commanders. Coord
                        // carries the ping; fc is the bare "!bping fc" backup that reaches more people,
                        // only after coord and never within 10s of the last ping. Off in test mode.
                        let can_send = !test_mode && !skirmish_jid.is_empty() && jab_connected;
                        if compact {
                            ui.label("Ping");
                        }
                        let now = crate::clock::utc().timestamp();
                        let fc_ok = crate::rescue::fc_ping_wait(r.coord_pinged_at, r.bpinged_at, now);
                        if let Err((_, Some(wait))) = &fc_ok {
                            ui.ctx().request_repaint_after(std::time::Duration::from_secs((*wait).max(1) as u64));
                        }
                        let send = |body: String| {
                            if let Some(tx) = &tx {
                                let _ = tx.send(crate::jabber::Cmd::SendRoom { room: skirmish_jid.clone(), body });
                            }
                        };
                        let coord = egui::Button::new(if compact { "coord" } else { "Ping coord" });
                        let coord = match pulse_fill(ui, coord_pending) {
                            Some(c) => coord.fill(c),
                            None => coord,
                        };
                        if ui.add_enabled(can_send, coord).on_hover_text("!bping coord with the ping").clicked() {
                            send(format!("!bping coord\n\n{}", r.pending_ping));
                            mark_coord = true;
                            r.coord_pinged_at = Some(now);
                            r.bpinged_at = Some(now);
                        }
                        let fc = egui::Button::new(if compact { "fc" } else { "Ping fc" });
                        let resp = ui.add_enabled(can_send && fc_ok.is_ok(), fc).on_hover_text("!bping fc alone: the backup after coord, reaching more people");
                        let resp = match &fc_ok {
                            Err((why, _)) => resp.on_disabled_hover_text(why),
                            Ok(()) => resp,
                        };
                        if resp.clicked() {
                            send("!bping fc".to_owned());
                            r.bpinged_at = Some(now);
                        }
                        // Compact, comms, tracking and the invite share the ping's row, and wrap
                        // below it only when the window is too narrow.
                        if compact {
                            if command_comms(ui, false) {
                                let _ = open::that(command_mumble_url(op_now));
                                mark_cmd = true;
                            }
                            if track_button(ui, false) {
                                start_tracking = true;
                            }
                            invite_ui(ui);
                            verdict(ui);
                        }
                    });
                    if !compact {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| verdict(ui));
                    }

                    // Handing over to the fleet tab: the preset fills the start form and the
                    // fleet it starts is the one being tracked from here on.
                    if !compact {
                        ui.add_space(6.0);
                        if track_button(ui, true) {
                            start_tracking = true;
                        }
                    }

                    if !compact {
                        ui.add_space(6.0);
                        ui.separator();
                        ui.label(egui::RichText::new("Comms invite").strong());
                        invite_ui(ui);
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
                    };
                    if compact {
                        inner(ui);
                    } else {
                        egui::ScrollArea::vertical().id_salt("rescue_ops").auto_shrink([false, false]).show(ui, inner);
                    }
            };
            if compact {
                // Its content's height, up to most of a short window, which then scrolls: the chat
                // keeps a usable share.
                let most = (ui.available_height() * 0.6).max(120.0);
                egui::ScrollArea::vertical().id_salt("rescue_ops_band").max_height(most).auto_shrink([false, true]).show(ui, |ui| ops_body(ui));
                ui.separator();
            } else {
                let ops_resp = egui::Panel::left("rescue_ops_panel")
                    .resizable(true)
                    .default_size(ops_w)
                    .size_range(180.0..=640.0)
                    .show_inside(ui, |ui| ops_body(ui));
                new_ops_w = ops_resp.response.rect.width();
            }

            egui::CentralPanel::default()
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 0)))
                .show_inside(ui, |ui| {
                // Docked and wide, the map takes half beside the chat; narrower, it is a chat tab.
                let beside = !compact && ui.available_width() >= 760.0;
                let map_tab = !compact && !beside && map.is_some();
                if r.chat_tab == 2 && !map_tab {
                    r.chat_tab = 0;
                }
                if let (true, Some(m)) = (beside, &map) {
                    let half = ui.available_width() / 2.0;
                    egui::Panel::right("rescue_map_panel")
                        .resizable(true)
                        .default_size(half)
                        .size_range(240.0..=ui.available_width() - 260.0)
                        .show_inside(ui, |ui| rescue_map_ui(ui, m, &mut map_view));
                }
                if r.chat_tab != 2 {
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
                        // Multiline with Enter rebound to Shift+Enter, the same deal the jabber
                        // page makes: Enter sends, Shift+Enter breaks the line.
                        let shift_enter =
                            egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter);
                        let resp = ui.add(
                            egui::TextEdit::multiline(&mut r.delve911_reply)
                                .return_key(shift_enter)
                                .desired_rows(1)
                                .hint_text("respond… (Shift+Enter for a new line)")
                                .margin(egui::Margin::same(2))
                                .desired_width(ui.available_width()),
                        );
                        let enter = resp.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
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
                }
                egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
                    let tabs: &[(u8, &str)] = if map_tab { &[(0, "delve911"), (1, "skirmish"), (2, "map")] } else { &[(0, "delve911"), (1, "skirmish")] };
                    ui.columns(tabs.len(), |c| {
                        for (col, (n, label)) in c.iter_mut().zip(tabs) {
                            let w = col.available_width();
                            if col.menu_label_sized([w, 22.0], r.chat_tab == *n, *label).clicked() {
                                r.chat_tab = *n;
                            }
                        }
                    });
                    ui.separator();
                    if r.chat_tab == 2 {
                        if let Some(m) = &map {
                            rescue_map_ui(ui, m, &mut map_view);
                        }
                        return;
                    }
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

        self.rescue_map_view = map_view;
        // Persist the ops column width once a resize drag ends (avoids a write storm mid-drag).
        if !ui.ctx().input(|i| i.pointer.any_down())
            && (new_ops_w - self.settings.rescue_col_ops_w).abs() > 1.0
        {
            self.settings.rescue_col_ops_w = new_ops_w;
            self.needs_save = true;
        }

        if open_jabber {
            self.view = nav::View::Jabber;
        }
        if retry_click {
            self.jabber_retry();
        }
        if recheck.get() {
            self.fleet_boss_asked = None;
            self.fleet_channels_at = None;
            self.fleet_boss_poll(ui.ctx());
            self.fleet_channel_poll(ui.ctx());
        }
        if let Some((sid, names)) = set_dest {
            self.rescue_send_route(&names, sid);
        }
        if let Some(nick) = chat_dm {
            let dm = self.full_user_jid(&nick);
            self.jabber_mark_read(&dm);
            self.settings.jabber_closed_dms.retain(|j| j != &dm);
            self.jabber_open(&dm, ChatWinKey::Main);
            self.view = nav::View::Jabber;
            self.raise_main = true;
        }

        if start_tracking {
            self.rescue_start_tracking();
            // From the popped-out window: the start form is in the main window, so bring it up.
            if ui.ctx().viewport_id() != egui::ViewportId::ROOT {
                self.raise_main = true;
            }
        }
        if pop_out {
            self.settings.rescue_popped = true;
            self.rescue_geom_applied = false;
            self.needs_save = true;
        }

        // Persist op and preset so the next rescue starts where we left off.
        let (op, preset) = {
            let r = self.rescue.lock().unwrap();
            (r.op_channel, r.doctrine.clone())
        };
        if op != self.settings.rescue_op_channel || preset != self.settings.rescue_preset {
            self.settings.rescue_op_channel = op;
            self.settings.rescue_preset = preset;
            self.needs_save = true;
        }
    }
}

/// What the rescue map draws: the titan route from staging to the capital and the regions around
/// it, laid flat as the map tab lays New Eden.
pub(crate) struct RescueMap {
    graph: std::sync::Arc<crate::geo::Systems>,
    subset: Vec<crate::store::MapSystem>,
    hops: Vec<crate::web::route::Hop>,
    staging: Option<i64>,
    target: Option<i64>,
    note: String,
    /// The systems that set the view, so it refits when they change.
    ids: Vec<i64>,
}

/// Zoom, pan, and the systems the view was last fitted to.
pub(crate) type RescueMapView = (f32, egui::Vec2, Vec<i64>);

/// `[ label | ▾ ]` for the rescue route: the button sends it to the FC's character, the arrow to any
/// signed-in character. Returns who to send it to.
fn rescue_route_button(
    ui: &mut egui::Ui,
    label: String,
    target: Option<&(i64, String)>,
    fc: Option<&str>,
    choices: &[(String, Option<String>)],
) -> Option<Vec<String>> {
    let mut picked = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        let resp = ui.add_enabled(target.is_some() && fc.is_some(), egui::Button::new(label));
        let resp = match (target, fc) {
            (None, _) => resp.on_disabled_hover_text("This ping has no system to route to"),
            (_, None) => resp.on_disabled_hover_text("No signed-in character may set waypoints"),
            (Some((_, name)), Some(fc)) => resp.on_hover_text(format!(
                "Set Destination for {fc}: {name}, by the titan's landing system when it is out of range"
            )),
        };
        if resp.clicked() {
            picked = fc.map(|f| vec![f.to_owned()]);
        }
        ui.add_enabled_ui(target.is_some(), |ui| {
            ui.menu_button(egui_phosphor::regular::CARET_DOWN, |ui| {
                if let Some(p) = super::wormholes_ui::pick_characters_menu(ui, choices) {
                    picked = Some(p);
                }
            })
            .response
            .on_hover_text("Set it for other characters");
        });
    });
    picked
}

/// How recent a ping must be for its route to go in the game by itself: an old one selected at
/// start-up is history, not a rescue to fly to.
const AUTO_ROUTE_FRESH_SECS: i64 = 600;

impl SpaiApp {
    /// The FC's character among those signed in here that may set waypoints: the dashboard's FC by
    /// name, else the active character.
    pub(crate) fn rescue_fc_character(&self) -> Option<String> {
        let chars = self.waypoint_characters();
        let fc = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).fc().map(|(_, name)| name);
        fc.filter(|n| chars.iter().any(|c| c.eq_ignore_ascii_case(n)))
            .and_then(|n| chars.iter().find(|c| c.eq_ignore_ascii_case(&n)).cloned())
            .or_else(|| chars.iter().find(|c| **c == self.active_character).cloned())
    }

    /// The waypoints that take `name` to the capital in `target`: out of the titan's range, its
    /// landing system first and then the gates on; staging ahead of it all when the character is
    /// not there yet. Just the capital's system when there is no titan route.
    pub(crate) fn rescue_waypoints(&mut self, name: &str, target: i64) -> Vec<i64> {
        let (Some(graph), Some(coords)) = (self.systems.clone(), self.map_coords.clone()) else { return vec![target] };
        let Some(staging) = self.rescue_staging_id(&graph) else { return vec![target] };
        let from = self.char_system(name);
        match self.fleet_map_route(&graph, &coords, staging, target) {
            Some(opt) => {
                let wp = crate::web::route::ingame_waypoints(&opt, from);
                if wp.is_empty() { vec![target] } else { wp }
            }
            None => vec![target],
        }
    }

    /// Puts the rescue route in the game for each of `names`, each from where they are.
    pub(crate) fn rescue_send_route(&mut self, names: &[String], target: i64) {
        let cid = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
        for n in names {
            let wp = self.rescue_waypoints(n, target);
            match wp.as_slice() {
                [only] => crate::esi::set_waypoint(cid.clone(), n.clone(), *only, true),
                _ => crate::esi::set_route(cid.clone(), n.clone(), wp),
            }
        }
        if names.contains(&self.active_character) {
            self.note_ingame_route();
        }
    }

    /// A fresh ping picked: its route goes to the FC's character once, without a click.
    /// A rescue ping came in within the last [`AUTO_ROUTE_FRESH_SECS`].
    pub(crate) fn rescue_ping_recent(&self) -> bool {
        let r = self.rescue.lock().unwrap_or_else(|e| e.into_inner());
        let newest = r.events.iter().map(|e| e.received).max().unwrap_or(0);
        crate::clock::utc().timestamp() - newest <= AUTO_ROUTE_FRESH_SECS
    }

    pub(crate) fn rescue_auto_route(&mut self) {
        let (seq, target, received, test) = {
            let r = self.rescue.lock().unwrap_or_else(|e| e.into_inner());
            let (Some(seq), Some(target)) = (r.selected_ping, r.capital_system) else { return };
            let received = r.events.iter().find(|e| e.seq == seq).map_or(0, |e| e.received);
            (seq, target, received, r.test_mode)
        };
        if self.rescue_routed_for == Some((seq, target)) || !self.rescue_on() {
            return;
        }
        self.rescue_routed_for = Some((seq, target));
        // A test scenario never reaches the game, and neither does an old ping.
        if test || crate::clock::utc().timestamp() - received > AUTO_ROUTE_FRESH_SECS {
            return;
        }
        if let Some(fc) = self.rescue_fc_character() {
            self.rescue_send_route(&[fc], target);
        }
    }

    pub(crate) fn rescue_map_data(&mut self) -> Option<RescueMap> {
        let graph = self.systems.clone()?;
        let coords = self.map_coords.clone()?;
        let target = self.rescue.lock().unwrap_or_else(|e| e.into_inner()).capital_system;
        let staging = self.rescue_staging_id(&graph);
        let route = match (staging, target) {
            (Some(from), Some(to)) => self.fleet_map_route(&graph, &coords, from, to),
            _ => None,
        };
        let name = |id: i64| graph.info_of(id).map_or_else(|| id.to_string(), |i| i.name.clone());
        let note = match (staging, target, &route) {
            (_, None, _) => "No capital pinged yet.".to_owned(),
            (None, _, _) => "No rescue staging system set.".to_owned(),
            (Some(s), Some(t), None) => format!("No titan route from {} to {}.", name(s), name(t)),
            (Some(s), _, Some(o)) => format!("Titan at {}: {}", name(s), o.note.clone().unwrap_or_else(|| o.label.clone())),
        };
        let hops = route.map(|o| o.hops).unwrap_or_default();
        let mut ids: Vec<i64> = hops.iter().map(|h| h.id).chain(staging).chain(target).collect();
        ids.sort_unstable();
        ids.dedup();
        let on_map = |id: i64| spai_ui::star_map::is_kspace(id) && graph.info_of(id).is_none_or(|i| !is_hidden_region(&i.region));
        let regions: std::collections::HashSet<i64> = coords.iter().filter(|s| ids.binary_search(&s.id).is_ok() && on_map(s.id)).map(|s| s.region_id).collect();
        let subset = coords
            .iter()
            .filter(|s| regions.contains(&s.region_id) && on_map(s.id))
            .map(|s| crate::store::MapSystem { x: s.x2d, z: s.z2d, ..s.clone() })
            .collect();
        Some(RescueMap { graph, subset, hops, staging, target, note, ids })
    }
}

/// The titan route to the tackled capital over the regions it crosses, with staging and the capital
/// marked. Drag to pan, scroll to zoom.
pub(crate) fn rescue_map_ui(ui: &mut egui::Ui, m: &RescueMap, view: &mut RescueMapView) {
    use std::collections::HashMap;
    let rect = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);
    let note_at = rect.left_top() + egui::vec2(8.0, 6.0);
    let Some(bounds) = crate::map::Bounds::of(&m.subset) else {
        painter.text(note_at, egui::Align2::LEFT_TOP, &m.note, egui::FontId::proportional(13.0), visuals.weak_text_color());
        return;
    };
    if view.0 <= 0.0 || view.2 != m.ids {
        *view = (1.0, egui::Vec2::ZERO, m.ids.clone());
    }
    if resp.dragged() {
        view.1 += resp.drag_delta();
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let old = view.0;
            view.0 = (old * (scroll * 0.003).exp()).clamp(0.5, 40.0);
            if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
                let rel = p - (rect.center() + view.1);
                view.1 += rel * (1.0 - view.0 / old);
            }
        }
    }
    let fit = rect.shrink2(egui::vec2(40.0, 28.0));
    let pos: HashMap<i64, egui::Pos2> = m.subset.iter().map(|s| (s.id, crate::map::project(s.x, s.z, &bounds, fit, view.0, view.1))).collect();
    let cull = rect.expand(8.0);
    spai_ui::star_map::paint_gates(&painter, &visuals, &m.graph, &m.subset, &pos, &HashMap::new(), cull);
    let dot = (2.0 * view.0.sqrt()).clamp(2.0, 6.0);
    for s in &m.subset {
        if let Some(p) = pos.get(&s.id).filter(|p| cull.contains(**p)) {
            painter.circle_filled(*p, dot, crate::app::security_color(s.security).gamma_multiply(0.75));
        }
    }
    if !m.hops.is_empty() {
        let phase = (ui.input(|i| i.time) * 28.0) as f32;
        spai_ui::star_map::paint_route_legs(&painter, &pos, &m.hops, phase, egui::Color32::from_rgb(0x4D, 0xD0, 0xC4), |_, _| false, |_, _, c| (c, c));
        ui.ctx().request_repaint();
    }
    let font = egui::FontId::proportional(12.0);
    for h in &m.hops {
        if let Some(p) = pos.get(&h.id) {
            painter.text(*p + egui::vec2(0.0, -dot - 3.0), egui::Align2::CENTER_BOTTOM, &h.name, font.clone(), visuals.text_color());
        }
    }
    if let Some(p) = m.staging.and_then(|s| pos.get(&s)) {
        painter.text(*p + egui::vec2(dot + 4.0, 0.0), egui::Align2::LEFT_CENTER, egui_phosphor::regular::STAR_FOUR, egui::FontId::proportional(16.0), egui::Color32::from_rgb(0xFF, 0x7A, 0x3D));
    }
    if let Some(p) = m.target.and_then(|t| pos.get(&t)) {
        let red = egui::Color32::from_rgb(0xE0, 0x3B, 0x2E);
        painter.circle_stroke(*p, dot + 7.0, egui::Stroke::new(2.5, red));
        if !m.hops.iter().any(|h| Some(h.id) == m.target) {
            let name = m.graph.info_of(m.target.unwrap_or_default()).map(|i| i.name.clone()).unwrap_or_default();
            painter.text(*p + egui::vec2(0.0, -dot - 9.0), egui::Align2::CENTER_BOTTOM, name, font.clone(), red);
        }
    }
    painter.text(note_at, egui::Align2::LEFT_TOP, &m.note, egui::FontId::proportional(13.0), visuals.weak_text_color());
}

/// Seconds as a stopwatch, because a rescue is counted in minutes and the seconds matter.
pub(crate) fn since_ping(secs: i64) -> String {
    let s = secs.max(0);
    if s < 3600 {
        format!("{}:{:02}", s / 60, s % 60)
    } else {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    }
}

/// How long the ping being worked has been running.
///
/// The pilot calls PANIC and pings at the same moment, near enough, so this is also roughly how much
/// of the PANIC has gone. It counts up rather than down: the module's length depends on the hull and
/// the pilot's skills, and a countdown that is wrong is worse than a clock that is not.
/// Whether it drew anything, so a caller does not rule off an empty space.
fn ping_timer_ui(ui: &mut egui::Ui, r: &crate::rescue::RescueState) -> bool {
    let Some(at) = r.selected_ping.and_then(|seq| r.ping_time(seq)) else { return false };
    ping_timer_row(ui, crate::clock::utc().timestamp() - at);
    // A clock that only moves when something else redraws the window is not a clock.
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    ui.add_space(4.0);
    true
}

/// The row itself, given the seconds, so it can be rendered at a fixed time.
pub(crate) fn ping_timer_row(ui: &mut egui::Ui, secs: i64) {
    let secs = secs.max(0);
    // Amber at five minutes, red at ten: past that the PANIC is over on any hull and the question is
    // whether the fleet is already too late.
    let color = match secs {
        0..=299 => egui::Color32::from_rgb(0x4C, 0xC0, 0x6A),
        300..=599 => egui::Color32::from_rgb(0xE6, 0xA5, 0x1E),
        _ => egui::Color32::from_rgb(0xE0, 0x3B, 0x2E),
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("SINCE PING").strong().color(color));
        ui.label(egui::RichText::new(since_ping(secs)).heading().strong().color(color));
    })
    .response
    .on_hover_text(
        "Time since this delve911 ping. A PANIC is usually called as the ping goes out, so this is \
         about how much of it has run.",
    );
}

#[cfg(test)]
mod tests {
    use super::since_ping;

    /// Minutes and seconds, because a rescue is over in the time a wall clock would not have moved.
    #[test]
    fn the_clock_counts_in_minutes_and_seconds() {
        assert_eq!(since_ping(0), "0:00");
        assert_eq!(since_ping(7), "0:07");
        assert_eq!(since_ping(69), "1:09");
        assert_eq!(since_ping(599), "9:59");
        assert_eq!(since_ping(3600), "1:00:00", "an hour old ping is a forgotten one, but it reads");
        assert_eq!(since_ping(-5), "0:00", "a clock that ran backwards is a bug, not a negative time");
    }
}
