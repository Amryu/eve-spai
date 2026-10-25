//! The rescuee's side of the delve911 rescue tab: a tackled Rorqual's ping, the checklist after
//! it, the FC's comms, and the delve911 chat. The FC's side is `rescue_ui`.

use std::collections::HashMap;

use spai_ui::i18n::Tr;

use crate::rescuee::{self, CommsTarget, FleetVerdict, Location, Panic};

use super::*;

/// How often the two pilots' fleets are asked about while the fleet item is open.
const FLEET_POLL: std::time::Duration = std::time::Duration::from_secs(30);
/// How often Mumble is asked which channel it is in.
const MUMBLE_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// What the panel asks the app to do once it has let go of the rescuee state.
#[derive(Default)]
struct Deferred {
    pop_out: bool,
    switch_mode: bool,
    open_jabber: bool,
    retry: bool,
    chat_dm: Option<String>,
    join: Option<CommsTarget>,
    panel_w: Option<f32>,
}

impl SpaiApp {
    /// Whether the rescue tab has two sides to offer: the FC's and the rescuee's.
    pub(crate) fn rescue_two_sided(&self) -> bool {
        self.rescue_on() && self.rescuee_on()
    }

    /// To the other side of the rescue tab. The FC's op and preset are saved first: they live in
    /// `rescue`, which outlives the switch, and the saved copy has to keep up with them.
    pub(crate) fn rescue_switch_mode(&mut self) {
        use crate::settings::RescueMode;
        self.rescue_persist_selectors();
        self.settings.rescue_mode = match self.settings.rescue_mode {
            RescueMode::Fc => RescueMode::Rescuee,
            RescueMode::Rescuee => RescueMode::Fc,
        };
        self.needs_save = true;
    }

    /// The rescuee's settings, for every Jabber user. True when anything changed.
    pub(crate) fn rescuee_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.heading(tr!("Rescuee (delve911)"));
        ui.label(
            egui::RichText::new(tr!("For the pilot being rescued: posts a tackled Rorqual's ping to delve911, walks through what to do next and joins the FC's comms. No fleet command needed."))
                .weak(),
        );
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut self.settings.rescuee_enabled, tr!("Enable delve911 Rescuee mode")).changed();
            help_icon(ui, tr!("Adds the Rescue tab and keeps the delve911 room joined. Someone else's cap-save ping then plays the Rescuee sound, so you know to get safe."));
        });
        if !self.settings.rescuee_enabled {
            return changed;
        }
        if self.settings.jabber_jid.trim().is_empty() {
            ui.label(egui::RichText::new(tr!("Needs Jabber: set it up above first.")).color(crate::theme::standing::WARNING));
        }
        ui.add_space(4.0);
        egui::Grid::new("rescuee_settings_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(tr!("delve911 room")).on_hover_text(tr!("The Jabber room on conference.goonfleet.com where rescue requests come in."));
            changed |= ui
                .add(egui::TextEdit::singleline(&mut self.settings.rescue_delve911_jid).hint_text(tr!("delve911")).desired_width(220.0))
                .changed();
            ui.end_row();
            ui.label(tr!("D-scans")).on_hover_text(tr!("While the Rescuee panel is open, a d-scan copied to the clipboard fills in the hostiles."));
            ui.vertical(|ui| {
                changed |= ui.checkbox(&mut self.settings.rescuee_read_dscan, tr!("Read d-scans from the clipboard")).changed();
                ui.add_enabled_ui(self.settings.rescuee_read_dscan, |ui| {
                    changed |= ui
                        .checkbox(&mut self.settings.rescuee_dscan_upload, tr!("Upload them to dscan.info for the FC"))
                        .on_hover_text(tr!("The scan goes to dscan.info, a third-party site, and its link is posted with the hostiles."))
                        .changed();
                });
            });
            ui.end_row();
        });
        changed
    }

    /// The Rescuee panel is on screen, docked or popped out.
    pub(crate) fn rescuee_panel_shown(&self) -> bool {
        self.rescuee_on()
            && self.rescue_mode() == crate::settings::RescueMode::Rescuee
            && (self.view == nav::View::Rescue || self.settings.rescue_popped)
    }

    /// A d-scan read off the clipboard while the panel shows: the hostiles, a preview, and when
    /// allowed its upload for the FC.
    pub(crate) fn rescuee_take_dscan(&mut self, ctx: &egui::Context, text: &str) {
        let counts = crate::rescue::parse_raw_dscan(text);
        if counts.is_empty() {
            return;
        }
        let now = crate::clock::utc().timestamp();
        let st = &mut self.rescuee;
        st.hostiles.count = Some(counts.iter().map(|(_, n)| n).sum());
        st.hostiles.ships = counts.clone();
        st.hostiles.dscan_link = None;
        st.dscan = Some(rescuee::DscanPreview { counts, link: None, at: now });
        if self.settings.rescuee_dscan_upload {
            *st.shared_dscan.lock().unwrap() = None;
            let (shared, ctx, text) = (st.shared_dscan.clone(), ctx.clone(), text.to_owned());
            std::thread::spawn(move || {
                let res = crate::dscan::upload(&text).map_err(|e| e.to_string());
                *shared.lock().unwrap() = Some(res);
                ctx.request_repaint();
            });
        }
    }

    /// Background answers and timed checks, once a frame while the panel shows.
    fn rescuee_polls(&mut self, ctx: &egui::Context) {
        let now = crate::clock::utc().timestamp();
        if self.rescuee.anomaly_i18n.is_none() {
            let map = self.store.as_ref().map(|s| s.anomaly_names()).unwrap_or_default();
            self.rescuee.anomaly_i18n = Some(std::sync::Arc::new(map));
        }
        // The d-scan upload came back.
        if let Some(res) = self.rescuee.shared_dscan.lock().unwrap().take() {
            match res {
                Ok(link) => {
                    if let Some(d) = &mut self.rescuee.dscan {
                        d.link = Some(link.clone());
                    }
                    self.rescuee.hostiles.dscan_link = Some(link);
                }
                Err(e) => eprintln!("[rescuee] dscan upload failed: {e}"),
            }
        }
        if let Some(res) = self.rescuee.shared_alliance.lock().unwrap().take() {
            self.rescuee.alliance_asking = false;
            match res {
                Ok(name) => self.rescuee.hostiles.alliance = Some(name),
                Err(e) => self.rescuee.alliance_error = Some(e),
            }
        }
        if let Some(f) = self.rescuee.shared_fleet.lock().unwrap().take() {
            self.rescuee.fleet = f;
            let own: Vec<i64> = self.characters.iter().map(|c| c.id).collect();
            if let Some(rorq) = &self.rescuee.fleet.rorqual {
                if rescuee::fleet_verdict(rorq, self.rescuee.fleet.cyno.as_ref(), &own) == FleetVerdict::Joined {
                    self.rescuee.checklist.fleet = true;
                }
            }
        }
        // The two pilots' fleets, while the fleet item is open.
        let wants_fleet = self.rescuee.sent.is_some() && !self.rescuee.checklist.fleet;
        if wants_fleet && self.rescuee.fleet_asked_at.is_none_or(|t| t.elapsed() >= FLEET_POLL) {
            self.rescuee.fleet_asked_at = Some(std::time::Instant::now());
            let (pilot, cyno) = self.rescuee.sent.as_ref().map(|s| (s.fields.pilot.clone(), s.fields.cyno.clone())).unwrap_or_default();
            let client_id = non_empty_or(&self.settings.sso_client_id, auth::DEFAULT_CLIENT_ID);
            let ids: Vec<Option<i64>> = [pilot, cyno].iter().map(|n| self.characters.iter().find(|c| c.name == *n).map(|c| c.id)).collect();
            let (shared, ctx) = (self.rescuee.shared_fleet.clone(), ctx.clone());
            std::thread::spawn(move || {
                let ask = |id: Option<i64>| -> Option<crate::fleets::model::EsiFleet> {
                    let id = id?;
                    let store = crate::store::Store::open().ok()?;
                    let client = crate::http::client(20).ok()?;
                    Some(crate::esi::character_fleet(&client, &store, &client_id, id))
                };
                let status = rescuee::FleetStatus {
                    rorqual: Some(ask(ids[0]).unwrap_or_else(|| crate::fleets::model::EsiFleet::Unknown("not logged in here".to_owned()))),
                    cyno: ask(ids[1]),
                    at: Some(now),
                };
                *shared.lock().unwrap() = Some(status);
                ctx.request_repaint();
            });
        }
        if self.rescuee.comms.is_some() && self.rescuee.mumble_asked_at.is_none_or(|t| t.elapsed() >= MUMBLE_POLL) {
            self.rescuee.mumble_asked_at = Some(std::time::Instant::now());
            self.rescuee.mumble_current = crate::mumble::current_url();
        }
        self.rescuee_scan_comms();
    }

    /// The comms the FC called this pilot into: the first line after the ping that names any,
    /// kept until the next rescue; later ones listed under it.
    fn rescuee_scan_comms(&mut self) {
        let Some(sent_at) = self.rescuee.sent.as_ref().map(|s| s.at) else { return };
        let jid = goon_jid(&self.settings.rescue_delve911_jid, "delve911@conference.goonfleet.com");
        let seen_at = self.rescuee.comms_also.last().or(self.rescuee.comms.as_ref()).map_or(sent_at, |c| c.at);
        let fresh: Vec<rescuee::CommsInvite> = {
            let j = self.jabber.lock().unwrap();
            let Some(msgs) = j.chats.get(&jid) else { return };
            msgs.iter()
                .filter(|m| m.time > seen_at && !m.outgoing)
                .filter(|m| !is_ping_bot(&m.from) && !m.body.contains("requests the attention of"))
                .filter_map(|m| rescuee::detect_comms(&m.body).map(|target| rescuee::CommsInvite { target, from: m.from.clone(), at: m.time }))
                .collect()
        };
        for inv in fresh {
            if self.rescuee.comms.is_none() {
                self.rescuee.comms = Some(inv);
            } else if self.rescuee.comms.as_ref().is_some_and(|c| c.target != inv.target) && !self.rescuee.comms_also.iter().any(|c| c.target == inv.target) {
                self.rescuee.comms_also.push(inv);
            }
        }
    }

    /// Sends the client to the comms, remembering the `mumble://` an op's short link stands for so
    /// the in-channel check can compare.
    fn rescuee_join(&mut self, target: &CommsTarget) {
        let direct = match target {
            CommsTarget::Beehive => crate::fleets::comms::builtin_named_mumble("beehive").map(str::to_owned),
            _ => None,
        };
        if let Some(url) = direct {
            crate::mumble::open_url(&url);
            return;
        }
        let link = match target {
            CommsTarget::Op(n) => op_comms_url(*n),
            CommsTarget::Link { link } => link.clone(),
            CommsTarget::Beehive => return,
        };
        if let Some(url) = self.rescuee.resolved_mumble.lock().unwrap().get(&link).cloned() {
            crate::mumble::open_url(&url);
            return;
        }
        let resolved = self.rescuee.resolved_mumble.clone();
        std::thread::spawn(move || {
            let url = crate::http::client(10).ok().and_then(|c| c.get(&link).send().ok()?.text().ok()).and_then(|b| crate::fleets::comms::mumble_url_in(&b));
            match url {
                Some(url) => {
                    resolved.lock().unwrap().insert(link, url.clone());
                    crate::mumble::open_url(&url);
                }
                None => {
                    if let Err(e) = crate::launch::that(&link) {
                        eprintln!("[rescuee] could not open {link}: {e}");
                    }
                }
            }
        });
    }

    /// The `mumble://` the pilot should be in, when known.
    fn rescuee_want_url(&self, target: &CommsTarget) -> Option<String> {
        match target {
            CommsTarget::Beehive => crate::fleets::comms::builtin_named_mumble("beehive").map(str::to_owned),
            CommsTarget::Op(n) => self.rescuee.resolved_mumble.lock().unwrap().get(&op_comms_url(*n)).cloned(),
            CommsTarget::Link { link } => self.rescuee.resolved_mumble.lock().unwrap().get(link).cloned(),
        }
    }

    /// The whole rescuee view. `compact` is the popped-out window.
    pub(crate) fn rescuee_body(&mut self, ui: &mut egui::Ui, compact: bool) {
        self.rescuee_polls(ui.ctx());
        let now = crate::clock::utc().timestamp();
        let now_ms = crate::clock::utc().timestamp_millis();
        let delve911_jid = goon_jid(&self.settings.rescue_delve911_jid, "delve911@conference.goonfleet.com");
        let msgs = self.jabber_room_tail(&delve911_jid, 60);
        let tx = self.jabber_tx.clone();
        let (jab_connected, jab_status, jab_retry_in, _) = self.jabber_conn();
        let jabber_set_up = !self.settings.jabber_jid.trim().is_empty();
        let systems = self.systems.clone();
        let ships = self.player.lock().unwrap().ships.clone();
        let empty = HashMap::new();
        let groups = self.ship_groups.clone();
        let detected = rescuee::detect(&ships, &self.ship_by_id, groups.as_deref().unwrap_or(&empty));
        let name_to_id: HashMap<String, i64> = self.ship_by_id.iter().map(|(id, n)| (n.to_lowercase(), *id)).collect();
        let own_ids: Vec<i64> = self.characters.iter().map(|c| c.id).collect();
        let i18n = self.rescuee.anomaly_i18n.clone().unwrap_or_default();
        let test_mode = self.rescue.lock().unwrap_or_else(|e| e.into_inner()).test_mode;
        let two_sided = self.rescue_two_sided();
        let rescue = self.rescue.clone();
        let panel_w = self.settings.rescuee_col_w.unwrap_or(360.0).clamp(260.0, 640.0);
        let comms_want = self.rescuee.comms.as_ref().and_then(|c| self.rescuee_want_url(&c.target));
        let mumble_current = self.rescuee.mumble_current.clone();
        let mut deferred = Deferred::default();

        let st = &mut self.rescuee;
        // The pilot's picks fall back to what ESI sees.
        if st.rorqual_pick.as_ref().is_none_or(|p| !detected.iter().any(|d| &d.rorqual == p)) {
            st.rorqual_pick = detected.first().map(|d| d.rorqual.clone());
        }
        let picked = detected.iter().find(|d| Some(&d.rorqual) == st.rorqual_pick.as_ref()).cloned();
        if let Some(d) = &picked {
            if st.cyno_pick.as_ref().is_none_or(|c| !d.cynos.contains(c)) {
                st.cyno_pick = d.cynos.first().cloned();
            }
        }
        let system_name = picked.as_ref().and_then(|d| systems.as_ref()?.info_of(d.system).map(|i| i.name.clone()));
        let fields = rescuee::PingFields {
            pilot: if st.pilot_override.trim().is_empty() { picked.as_ref().map(|d| d.rorqual.clone()).unwrap_or_default() } else { st.pilot_override.trim().to_owned() },
            cyno: if st.cyno_override.trim().is_empty() { st.cyno_pick.clone().unwrap_or_default() } else { st.cyno_override.trim().to_owned() },
            system: if st.system_override.trim().is_empty() { system_name.clone().unwrap_or_default() } else { st.system_override.trim().to_owned() },
            location: st.location.clone(),
            panic: st.panic,
            hostiles: st.hostiles.clone(),
        };

        let after_section = |ui: &mut egui::Ui, st: &mut rescuee::RescueeState, deferred: &mut Deferred, sent: &rescuee::Sent, open_after: bool| {
            {
                section(ui, "rescuee_after", tr!("After the ping"), tr!("What to do now, in order. The app ticks what it can see; tick the rest yourself."), open_after, compact, |ui| {
                    ui.label(egui::RichText::new(trf!("Sent {ago} ago", ago = rescue_ui::since_ping(now - sent.at))).weak());
                    let verdict = st.fleet.rorqual.as_ref().map(|r| rescuee::fleet_verdict(r, st.fleet.cyno.as_ref(), &own_ids));
                    ui.checkbox(&mut st.checklist.siege_red, tr!("Siege red: run the industrial core now")).on_hover_text(tr!("The moment the ping is out. Sieged, the Rorqual holds; the fleet needs the time."));
                    let location_now = st.location != Location::Unknown;
                    st.checklist.location |= location_now && sent.location_sent != Location::Unknown;
                    ui.horizontal(|ui| {
                        ui.add_enabled(false, egui::Checkbox::new(&mut st.checklist.location, tr!("Location in system given")));
                        if !st.checklist.location {
                            ui.colored_label(egui::Color32::from_rgb(0xE0, 0x3B, 0x2E), trf!("{icon}  CRITICAL", icon = egui_phosphor::regular::WARNING)).on_hover_text(tr!("The fleet cannot find you without it. Fill it in above and send the update."));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut st.checklist.fleet, tr!("Rorqual and cyno in the saving fleet")).on_hover_text(tr!("The FC invites both. Accept the invite in game; leave any fleet of your own first, or the invite cannot land."));
                        match &verdict {
                            Some(FleetVerdict::Joined) => {
                                let ago = st.fleet.at.map_or(String::new(), |t| trf!(", {ago} ago", ago = rescue_ui::since_ping(now - t)));
                                ui.label(egui::RichText::new(egui_phosphor::regular::CHECK_CIRCLE).color(crate::theme::standing::FRIENDLY)).on_hover_text(trf!("ESI sees both in one fleet{ago}", ago = ago));
                            }
                            Some(FleetVerdict::NotYet) => {
                                ui.label(egui::RichText::new(tr!("accept the invite in game")).weak());
                            }
                            Some(FleetVerdict::RorqualElsewhere { .. }) => {
                                ui.label(egui::RichText::new(tr!("the Rorqual is in its own fleet: leave it")).color(crate::theme::standing::WARNING));
                            }
                            Some(FleetVerdict::CynoElsewhere { .. }) => {
                                ui.label(egui::RichText::new(tr!("the cyno is in its own fleet: leave it")).color(crate::theme::standing::WARNING));
                            }
                            Some(FleetVerdict::Split) => {
                                ui.label(egui::RichText::new(tr!("Rorqual and cyno are in different fleets")).color(crate::theme::standing::WARNING));
                            }
                            Some(FleetVerdict::Unknown(why)) => {
                                ui.label(egui::RichText::new(egui_phosphor::regular::QUESTION).weak()).on_hover_text(why.as_str());
                            }
                            None => {
                                ui.spinner();
                            }
                        }
                    });
                    ui.checkbox(&mut st.checklist.directions, tr!("Follow the FC's directions")).on_hover_text(tr!("What the FC says in comms or delve911 beats this list."));
                    ui.horizontal(|ui| {
                        ui.add_enabled(false, egui::Checkbox::new(&mut st.checklist.hostiles, tr!("Hostiles reported")));
                        help_icon(ui, tr!("Number, hulls and alliance: fill them in above, then send the update."));
                    });
                    ui.add_space(4.0);
                    // Comms: the first the FC named after the ping.
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr!("Comms")).strong());
                        help_icon(ui, tr!("The voice channel the FC calls you into, read from delve911: Beehive, an op channel or a link. The first one posted after your ping counts."));
                    });
                    match &st.comms {
                        None => {
                            ui.label(egui::RichText::new(tr!("waiting for the FC to name a channel…")).weak());
                        }
                        Some(c) => {
                            ui.horizontal(|ui| {
                                let in_channel = comms_want.as_deref().zip(mumble_current.as_deref()).is_some_and(|(w, cur)| crate::mumble::in_channel(cur, w));
                                if in_channel {
                                    ui.label(egui::RichText::new(format!("{}  {}", egui_phosphor::regular::CHECK_CIRCLE, trf!("In {channel}", channel = c.target.label()))).color(crate::theme::standing::FRIENDLY));
                                } else {
                                    ui.label(egui::RichText::new(trf!("{channel}, from {who}", channel = c.target.label(), who = c.from)).strong());
                                    if ui.button(trf!("{icon}  Join", icon = egui_phosphor::regular::HEADSET)).on_hover_text(tr!("Sends Mumble there")).clicked() {
                                        deferred.join = Some(c.target.clone());
                                    }
                                    if cfg!(not(target_os = "linux")) {
                                        help_icon(ui, tr!("Whether you are in the channel cannot be read from Mumble on this system."));
                                    }
                                }
                            });
                            for also in &st.comms_also {
                                ui.label(egui::RichText::new(trf!("also named: {channel} ({who})", channel = also.target.label(), who = also.from)).weak());
                            }
                        }
                    }
                    ui.add_space(6.0);
                    if ui.button(tr!("Done: reset for the next rescue")).on_hover_text(tr!("Clears the ping, the list and the hostiles. Your ships stay.")).clicked() {
                        st.reset();
                    }
                });
            }
        };
        let panel = |ui: &mut egui::Ui, st: &mut rescuee::RescueeState, deferred: &mut Deferred| {
            // The title row, held to one height so the buttons never move it.
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{}  {}", egui_phosphor::regular::LIFEBUOY, tr!("Rescuee"))).strong());
                help_icon(ui, tr!("You are the one tackled. Work through the steps, click Send twice, then follow the list while the FC gets the fleet to you."));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if compact {
                        ontop_pin_ui(ui, "rescue_window");
                    } else if spai_ui::widgets::icon_button(ui, egui_phosphor::regular::ARROW_SQUARE_OUT).on_hover_text(tr!("Pop out into its own window, over EVE")).clicked() {
                        deferred.pop_out = true;
                    }
                    if two_sided && rescue_side_button(ui, tr!("FC"), tr!("Switch to the FC's side: build the fleet ping, call people into comms, track the fleet")) {
                        deferred.switch_mode = true;
                    }
                    if test_mode {
                        ui.label(egui::RichText::new(egui_phosphor::regular::FLASK).color(egui::Color32::from_rgb(0x40, 0xB0, 0xF0))).on_hover_text(tr!("Test: sending disabled"));
                    }
                });
            });
            if !jab_connected {
                ui.horizontal_wrapped(|ui| {
                    let why = if jabber_set_up { trf!("Jabber: {status}", status = jab_status) } else { tr!("Jabber is not set up: the ping goes to delve911 through it.").to_owned() };
                    ui.colored_label(egui::Color32::from_rgb(0xE0, 0x3B, 0x2E), format!("{}  {why}", egui_phosphor::regular::PLUGS));
                    if !jabber_set_up {
                        if ui.button(tr!("Set up Jabber")).clicked() {
                            deferred.open_jabber = true;
                        }
                    } else if ui.button(match jab_retry_in { Some(s) if s > 0 => trf!("Retry ({s}s)", s = s), _ => tr!("Retry").to_owned() }).clicked() {
                        deferred.retry = true;
                    }
                });
            }
            ui.add_space(4.0);

            let sent = st.sent.is_some();
            // Once it is out, what to do next comes first; the steps fold to their summaries.
            if let Some(sent_ping) = st.sent.clone() {
                after_section(ui, st, deferred, &sent_ping, true);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr!("The ping (sent)")).strong());
                    help_icon(ui, tr!("Edit a step to change what the next update says, then send the update below the steps."));
                });
            }

            // The steps as tabs, coloured by how ready each is; the first that is not ready is the
            // open one until the pilot picks another, and clicking the open one folds it.
            let ships_done = !fields.pilot.trim().is_empty() && !fields.system.trim().is_empty();
            let location_done = st.location != Location::Unknown;
            let system_wrong = !fields.system.trim().is_empty() && systems.as_ref().is_some_and(|s| s.lookup(fields.system.trim()).is_none());
            let cyno_wrong = !fields.cyno.trim().is_empty() && fields.cyno.trim().eq_ignore_ascii_case(fields.pilot.trim());
            let ships_state = if system_wrong || cyno_wrong { StepState::Wrong } else if ships_done { StepState::Good } else { StepState::Missing };
            let location_state = if location_done { StepState::Good } else { StepState::Missing };
            let hostiles_state = if st.hostiles.is_empty() { StepState::Missing } else { StepState::Good };
            let natural = if sent { 0 } else if ships_state != StepState::Good { 1 } else if !location_done { 2 } else { 0 };
            // 0 is none open, which is what folding a tab leaves.
            let open = st.step_open.unwrap_or(natural);
            let panic_label = |p: Panic| match p {
                Panic::Unknown => tr!("unknown"),
                Panic::Available => tr!("fitted and ready"),
                Panic::Activated => tr!("running now"),
                Panic::Used => tr!("used up"),
                Panic::None => tr!("none fitted"),
            };
            let ships_hover = if system_wrong {
                trf!("No system named {system}.", system = fields.system.trim())
            } else if cyno_wrong {
                tr!("The cyno is the Rorqual's own pilot.").to_owned()
            } else if ships_done {
                let cyno = if fields.cyno.trim().is_empty() { tr!("no cyno").to_owned() } else { fields.cyno.clone() };
                trf!("{pilot} + {cyno} in {system}", pilot = fields.pilot, cyno = cyno, system = fields.system)
            } else {
                tr!("No pilot or system yet: the ping cannot go out without them.").to_owned()
            };
            let location_hover = if location_done {
                trf!("{loc} · Panic: {panic}", loc = st.location.render(), panic = panic_label(st.panic))
            } else {
                tr!("No location yet: the FC cannot find you without one.").to_owned()
            };
            let hostiles_hover = if st.hostiles.is_empty() { tr!("None yet. Optional with the ping, usually sent as an update after it.").to_owned() } else { st.hostiles.render() };
            if let Some(n) = step_tabs(ui, open, &[
                (1, tr!("Your ships"), ships_state, ships_hover),
                (2, tr!("Location"), location_state, location_hover),
                (3, tr!("Hostiles"), hostiles_state, hostiles_hover),
            ]) {
                st.step_open = Some(if n == open { 0 } else { n });
            }
            let open = st.step_open.unwrap_or(natural);

            if open == 1 {
                step_body(ui, tr!("Every character logged in here that ESI sees undocked in a Rorqual, and the other characters undocked in the same system, one of which flies the cyno. ESI updates every 20 seconds. Type a name when an alt is not logged in here."), |ui| {
                if detected.is_empty() {
                    ui.label(egui::RichText::new(tr!("No undocked Rorqual seen among your characters.")).weak());
                }
                if detected.len() > 1 {
                    ui.horizontal(|ui| {
                        ui.label(tr!("Rorqual"));
                        let picked_name = st.rorqual_pick.clone().unwrap_or_default();
                        egui::ComboBox::from_id_salt("rescuee_rorqual").selected_text(picked_name).show_ui(ui, |ui| {
                            use spai_ui::widgets::SteadySelect as _;
                            for d in &detected {
                                ui.menu_value(&mut st.rorqual_pick, Some(d.rorqual.clone()), d.rorqual.clone());
                            }
                        });
                    });
                }
                if let Some(d) = &picked {
                    ui.horizontal(|ui| {
                        hull_badge(ui, rescuee::RORQUAL_TYPE_ID, 18.0);
                        ui.label(egui::RichText::new(&d.rorqual).strong());
                        ui.label(egui::RichText::new(trf!("in {system}", system = system_name.clone().unwrap_or_else(|| "?".to_owned()))).weak());
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr!("Cyno")).on_hover_text(tr!("Keep it on a perch 300 km off the Rorqual: the fleet lands on the cyno, not on the fight."));
                        if d.cynos.is_empty() {
                            ui.label(egui::RichText::new(tr!("no other ship of yours undocked there")).weak());
                        } else {
                            let picked_cyno = st.cyno_pick.clone().unwrap_or_default();
                            egui::ComboBox::from_id_salt("rescuee_cyno").selected_text(picked_cyno).show_ui(ui, |ui| {
                                use spai_ui::widgets::SteadySelect as _;
                                for c in &d.cynos {
                                    ui.menu_value(&mut st.cyno_pick, Some(c.clone()), c.clone());
                                }
                            });
                        }
                    });
                }
                egui::Grid::new("rescuee_overrides").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label(tr!("Pilot")).on_hover_text(tr!("The Rorqual pilot, as the FC should read it. Empty: the one ESI sees."));
                    ui.add(egui::TextEdit::singleline(&mut st.pilot_override).hint_text(picked.as_ref().map(|d| d.rorqual.clone()).unwrap_or_else(|| tr!("name").to_owned())).desired_width(f32::INFINITY));
                    ui.end_row();
                    ui.label(tr!("Cyno")).on_hover_text(tr!("Who flies the cyno. Empty: the pick above."));
                    ui.add(egui::TextEdit::singleline(&mut st.cyno_override).hint_text(st.cyno_pick.clone().unwrap_or_else(|| tr!("name").to_owned())).desired_width(f32::INFINITY));
                    ui.end_row();
                    ui.label(tr!("System")).on_hover_text(tr!("Where the Rorqual is. Empty: where ESI sees it."));
                    ui.add(egui::TextEdit::singleline(&mut st.system_override).hint_text(system_name.clone().unwrap_or_else(|| tr!("system").to_owned())).desired_width(f32::INFINITY));
                    ui.end_row();
                });
            });
            }
            if open == 2 {
                step_body(ui, tr!("Paste the probe scanner's row (right-click the site, Copy), type the anomaly's name and code, a moon like \"IV - Moon 2\", or anything else. Names in your client's language are turned into English. The FC warps the fleet to this spot, so it is what gets them to you."), |ui| {
                let before = st.location_input.clone();
                let resp = ui.add(egui::TextEdit::singleline(&mut st.location_input).hint_text(tr!("Angel Forsaken Hub ABC-123, or IV - Moon 2")).desired_width(f32::INFINITY));
                if resp.changed() || (before.is_empty() && !st.location_input.is_empty()) {
                    st.location = rescuee::parse_location(&st.location_input, &i18n);
                }
                let shown = st.location.render();
                if !shown.is_empty() && shown != st.location_input.trim() {
                    ui.label(egui::RichText::new(trf!("→ {loc}", loc = shown)).weak()).on_hover_text(tr!("As the ping will say it"));
                }
                if st.location == Location::Unknown {
                    ui.colored_label(crate::theme::standing::WARNING, trf!("{icon}  No location yet: the FC cannot find you without one.", icon = egui_phosphor::regular::WARNING));
                }
                ui.horizontal(|ui| {
                    ui.label(tr!("Panic")).on_hover_text(tr!("The Rorqual's PANIC module: it decides how long the FC has."));
                    egui::ComboBox::from_id_salt("rescuee_panic").selected_text(panic_label(st.panic)).show_ui(ui, |ui| {
                        use spai_ui::widgets::SteadySelect as _;
                        for p in [Panic::Unknown, Panic::Available, Panic::Activated, Panic::Used, Panic::None] {
                            ui.menu_value(&mut st.panic, p, panic_label(p));
                        }
                    });
                });
            });
            }
            if open == 3 {
                step_body(ui, tr!("How many, what they fly and whose they are. Copy a d-scan in game and it fills in by itself; or pick hulls here. Sent with the ping, and as an update afterwards with only what changed."), |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr!("Count"));
                    let mut n = st.hostiles.count.unwrap_or(0);
                    if ui.add(egui::DragValue::new(&mut n).range(0..=500)).on_hover_text(tr!("0: unknown")).changed() {
                        st.hostiles.count = (n > 0).then_some(n);
                    }
                    ui.label(tr!("Alliance"));
                    let mut alliance = st.hostiles.alliance.clone().unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut alliance).hint_text(tr!("name or ticker")).desired_width(120.0)).changed() {
                        st.hostiles.alliance = Some(alliance.trim().to_owned()).filter(|a| !a.is_empty());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr!("Look up")).on_hover_text(tr!("A hostile pilot's name: their alliance is looked up on ESI."));
                    ui.add(egui::TextEdit::singleline(&mut st.alliance_pilot).hint_text(tr!("pilot name")).desired_width(140.0));
                    let can = !st.alliance_pilot.trim().is_empty() && !st.alliance_asking;
                    if ui.add_enabled(can, egui::Button::new(egui_phosphor::regular::MAGNIFYING_GLASS)).clicked() {
                        st.alliance_asking = true;
                        st.alliance_error = None;
                        let (shared, name) = (st.shared_alliance.clone(), st.alliance_pilot.trim().to_owned());
                        let ctx = ui.ctx().clone();
                        std::thread::spawn(move || {
                            let res = (|| -> Result<String, String> {
                                let client = crate::http::client(15).map_err(|e| e.to_string())?;
                                let ids = crate::universe::character_ids(&client, &[name.clone()]).ok_or_else(|| tr!("no such pilot").to_owned())?;
                                let id = ids.get(&name).or_else(|| ids.values().next()).map(|(id, _)| *id).ok_or_else(|| tr!("no such pilot").to_owned())?;
                                let affil = crate::affiliation::lookup(&client, &[id]);
                                let (_, alliance) = affil.get(&id).copied().ok_or_else(|| tr!("no affiliation on ESI").to_owned())?;
                                let alliance = alliance.ok_or_else(|| tr!("no alliance").to_owned())?;
                                crate::universe::names(&client, &[alliance]).remove(&alliance).ok_or_else(|| tr!("alliance name unknown").to_owned())
                            })();
                            *shared.lock().unwrap() = Some(res);
                            ctx.request_repaint();
                        });
                    }
                    if st.alliance_asking {
                        ui.spinner();
                    }
                    if let Some(e) = &st.alliance_error {
                        ui.label(egui::RichText::new(e.as_str()).color(crate::theme::standing::WARNING));
                    }
                });
                // The hulls seen, each a chip that counts up; the x takes one off.
                ui.horizontal_wrapped(|ui| {
                    let mut drop: Option<usize> = None;
                    for (i, (ship, n)) in st.hostiles.ships.iter_mut().enumerate() {
                        let text = if *n > 1 { format!("{ship} ×{n}") } else { ship.clone() };
                        if ui.button(text).on_hover_text(tr!("Click: one more")).clicked() {
                            *n += 1;
                        }
                        if ui.small_button(egui_phosphor::regular::X).on_hover_text(tr!("One fewer")).clicked() {
                            *n = n.saturating_sub(1);
                            if *n == 0 {
                                drop = Some(i);
                            }
                        }
                    }
                    if let Some(i) = drop {
                        st.hostiles.ships.remove(i);
                    }
                });
                // The gang's kind, one click each; a hull by name for the specific.
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(tr!("What they fly")).weak()).on_hover_text(tr!("Pick every kind you see; click again to drop one. The chips below count them, and the d-scan names the exact hulls when there is time for one."));
                    for group in rescuee::HOSTILE_GROUPS {
                        let on = st.hostiles.ships.iter().any(|(s, _)| s.eq_ignore_ascii_case(group));
                        if super::selectable_chip(ui, on, *group).clicked() {
                            if on {
                                st.hostiles.ships.retain(|(s, _)| !s.eq_ignore_ascii_case(group));
                            } else {
                                add_ship(&mut st.hostiles, group);
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr!("Specific hull")).weak());
                    let resp = ui.add(egui::TextEdit::singleline(&mut st.ship_input).hint_text(tr!("Loki, Sabre… then Enter")).desired_width(160.0));
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !st.ship_input.trim().is_empty() {
                        let name = proper_hull_name(&st.ship_input, &name_to_id).unwrap_or_else(|| st.ship_input.trim().to_owned());
                        add_ship(&mut st.hostiles, &name);
                        st.ship_input.clear();
                    }
                });
                match &st.dscan {
                    Some(d) => dscan_preview(ui, d, &name_to_id, now),
                    None => {
                        ui.label(egui::RichText::new(trf!("{icon}  Or copy a d-scan in game: it fills in here by itself.", icon = egui_phosphor::regular::CLIPBOARD_TEXT)).weak())
                            .on_hover_text(tr!("Directional scanner, select all, Ctrl+C. Counted by hull, uploaded for the FC when allowed in settings."));
                    }
                }
            });
            }

        };

        // The preview and the button, pinned below the steps so no scroll ever hides them.
        let sender = |ui: &mut egui::Ui, st: &mut rescuee::RescueeState| {
            let sent = st.sent.is_some();
            let english = rescuee::render_ping(&fields);
            if !sent {
                // The preview in the user's language; what goes out is always English.
                let labels: [String; 7] = [
                    tr_noop!("Rorqual Name").tr().to_owned(),
                    tr_noop!("System").tr().to_owned(),
                    tr_noop!("Location In System (Anomaly or Moon)").tr().to_owned(),
                    tr_noop!("Name of Cyno (On perch, 300km away)").tr().to_owned(),
                    tr_noop!("Hostile Fleet Dscan").tr().to_owned(),
                    tr_noop!("Hostiles").tr().to_owned(),
                    tr_noop!("Panic").tr().to_owned(),
                ];
                let mut preview = rescuee::render_ping_with(&fields, tr_noop!("Rorqual Tackled").tr(), &labels);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr!("Send")).strong());
                    help_icon(ui, tr!("What goes to delve911. Shown in your language; posted in English, which is what every FC reads. Click the button twice within three seconds."));
                });
                ui.add(egui::TextEdit::multiline(&mut preview).interactive(false).desired_rows(5).desired_width(f32::INFINITY)).on_hover_text(&english);
                let missing = rescuee::missing(&fields);
                let blocked = missing.contains(&"pilot") || missing.contains(&"system");
                if blocked || fields.location == Location::Unknown {
                    let why = if blocked { tr!("Not ready: pilot and system are needed.") } else { tr!("Not ready: no location, so the FC cannot find you.") };
                    ui.colored_label(crate::theme::standing::WARNING, format!("{}  {why}", egui_phosphor::regular::WARNING));
                }
                let can_send = jab_connected && !test_mode && !blocked && tx.is_some();
                let armed = st.arm.left_ms(now_ms);
                let label = match armed {
                    Some(left) => trf!("Click again to send ({s}s)", s = (left + 999) / 1000),
                    None => tr!("Send ping to delve911").to_owned(),
                };
                let mut button = egui::Button::new(egui::RichText::new(label).strong());
                if let Some(fill) = pulse_fill(ui, armed.is_some()) {
                    button = button.fill(fill);
                }
                let resp = ui.add_enabled(can_send, button).on_hover_text(tr!("Click twice within three seconds, so it never goes out by accident")).on_disabled_hover_text(if !jab_connected { tr!("Jabber is not connected") } else { tr!("Fill in pilot and system first") });
                if armed.is_some() {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                if resp.clicked() && st.arm.press(now_ms) {
                    if let Some(tx) = &tx {
                        let _ = tx.send(crate::jabber::Cmd::SendRoom { room: delve911_jid.clone(), body: english.clone() });
                    }
                    st.checklist = rescuee::Checklist { location: fields.location != Location::Unknown, hostiles: !fields.hostiles.is_empty(), ..Default::default() };
                    st.comms = None;
                    st.comms_also.clear();
                    st.fleet_asked_at = None;
                    st.step_open = None;
                    st.sent = Some(rescuee::Sent { at: now, body: english.clone(), hostiles_sent: fields.hostiles.clone(), location_sent: fields.location.clone(), fields: fields.clone() });
                }
            } else if let Some(sent_ping) = st.sent.clone() {
                // What changed since the ping, right under the steps that change it.
                ui.add_space(4.0);
                let update = rescuee::followup(&sent_ping.hostiles_sent, &st.hostiles, &sent_ping.location_sent, &st.location);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr!("Update")).strong());
                    help_icon(ui, tr!("Only what changed since the last post: new hulls, a count, the alliance, the location, the d-scan link."));
                });
                match &update {
                    Some(text) => {
                        ui.add(egui::Label::new(egui::RichText::new(text.as_str()).weak()).wrap());
                    }
                    None => {
                        ui.label(egui::RichText::new(tr!("nothing new to tell")).weak());
                    }
                }
                let can = update.is_some() && jab_connected && !test_mode && tx.is_some();
                let armed = st.arm_update.left_ms(now_ms);
                let label = match armed {
                    Some(left) => trf!("Click again to send ({s}s)", s = (left + 999) / 1000),
                    None => tr!("Send update to delve911").to_owned(),
                };
                let mut button = egui::Button::new(egui::RichText::new(label).strong());
                if let Some(fill) = pulse_fill(ui, armed.is_some()) {
                    button = button.fill(fill);
                }
                if armed.is_some() {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                }
                let resp = ui.add_enabled(can, button).on_hover_text(tr!("Click twice within three seconds, so it never goes out by accident"));
                if resp.clicked() && st.arm_update.press(now_ms) {
                    if let (Some(text), Some(tx)) = (&update, &tx) {
                        let _ = tx.send(crate::jabber::Cmd::SendRoom { room: delve911_jid.clone(), body: text.clone() });
                        if let Some(s) = &mut st.sent {
                            s.hostiles_sent = st.hostiles.clone();
                            s.location_sent = st.location.clone();
                        }
                        st.checklist.hostiles |= !st.hostiles.is_empty();
                        st.checklist.location |= st.location != Location::Unknown;
                    }
                }
            }
        };

        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            if compact {
                // The sender's height last frame, so the band leaves room for it under a short window.
                let send_h: f32 = ui.data(|d| d.get_temp(egui::Id::new("rescuee_send_h")).unwrap_or(150.0));
                let most = (ui.available_height() * 0.6 - send_h).max(120.0);
                let out = egui::ScrollArea::vertical().id_salt("rescuee_band").max_height(most).auto_shrink([false, true]).show(ui, |ui| panel(ui, st, &mut deferred));
                spai_fit_ui::rows::note_scrolled(ui.ctx(), &out);
                let top = ui.cursor().top();
                ui.separator();
                sender(ui, st);
                let h = ui.cursor().top() - top;
                ui.data_mut(|d| d.insert_temp(egui::Id::new("rescuee_send_h"), h));
                ui.separator();
            } else {
                let resp = egui::Panel::left("rescuee_panel")
                    .resizable(true)
                    .default_size(panel_w)
                    .size_range(260.0..=640.0)
                    .show_inside(ui, |ui| {
                        egui::Panel::bottom("rescuee_send").show_inside(ui, |ui| {
                            ui.add_space(4.0);
                            sender(ui, st);
                            ui.add_space(4.0);
                        });
                        let out = egui::ScrollArea::vertical().id_salt("rescuee_panel_scroll").auto_shrink([false, false]).show(ui, |ui| panel(ui, st, &mut deferred));
                        spai_fit_ui::rows::note_scrolled(ui.ctx(), &out);
                    });
                let w = resp.response.rect.width();
                if (w - panel_w).abs() > 1.0 && !ui.ctx().input(|i| i.pointer.any_down()) {
                    deferred.panel_w = Some(w);
                }
            }
            // The delve911 chat alone: the reply row, then the feed.
            let mut r = rescue.lock().unwrap_or_else(|e| e.into_inner());
            egui::Panel::bottom("rescuee_chat_reply").show_inside(ui, |ui| {
                ui.add_space(2.0);
                if test_mode {
                    ui.label(egui::RichText::new(tr!("(reply disabled while testing)")).weak());
                } else if delve911_jid.is_empty() {
                    ui.label(egui::RichText::new(tr!("set this room's JID in settings to reply")).weak());
                }
                let can_send = !test_mode && !delve911_jid.is_empty() && jab_connected;
                rescue_ui::rescue_reply_row(ui, &mut r.delve911_reply, &delve911_jid, can_send, &tx);
            });
            egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
                ui.label(egui::RichText::new("delve911").weak());
                ui.separator();
                let selecting = ui.input(|i| i.pointer.any_down());
                let out = egui::ScrollArea::vertical().id_salt("rescuee_chat").auto_shrink([false, false]).stick_to_bottom(!selecting).show(ui, |ui| {
                    if let Some((act, who, body)) = rescue_chat_feed(ui, &msgs, "delve911") {
                        match act {
                            MsgRowAction::Copy => ui.ctx().copy_text(body),
                            MsgRowAction::Mention => {
                                let d = &mut r.delve911_reply;
                                if !d.is_empty() && !d.ends_with(char::is_whitespace) {
                                    d.push(' ');
                                }
                                d.push_str(&format!("{who}: "));
                            }
                            MsgRowAction::GoToDm => deferred.chat_dm = Some(who),
                            MsgRowAction::None => {}
                        }
                    }
                });
                spai_fit_ui::rows::note_scrolled(ui.ctx(), &out);
            });
        });

        if let Some(w) = deferred.panel_w {
            self.settings.rescuee_col_w = Some(w);
            self.needs_save = true;
        }
        if deferred.open_jabber {
            self.view = nav::View::Jabber;
        }
        if deferred.retry {
            self.jabber_retry();
        }
        if let Some(target) = deferred.join {
            self.rescuee_join(&target);
        }
        if let Some(nick) = deferred.chat_dm {
            let dm = self.full_user_jid(&nick);
            self.jabber_mark_read(&dm);
            self.settings.jabber_closed_dms.retain(|j| j != &dm);
            self.jabber_open(&dm, ChatWinKey::Main);
            self.view = nav::View::Jabber;
            self.raise_main = true;
        }
        if deferred.pop_out {
            self.settings.rescue_popped = true;
            self.rescue_geom_applied = false;
            self.needs_save = true;
        }
        if deferred.switch_mode {
            self.rescue_switch_mode();
        }
    }
}

/// The button to the other side of the rescue tab, in a header row.
pub(crate) fn rescue_side_button(ui: &mut egui::Ui, other: &str, hover: &str) -> bool {
    ui.add(egui::Button::new(format!("{}  {other}", egui_phosphor::regular::ARROWS_LEFT_RIGHT)).min_size(egui::vec2(0.0, 22.0)))
        .on_hover_text(hover)
        .clicked()
}

/// A titled block: a collapsing header in the pop-out, where height is short, a plain heading
/// docked.
fn section(ui: &mut egui::Ui, id: &str, title: &str, help: &str, open: bool, compact: bool, body: impl FnOnce(&mut egui::Ui)) {
    if compact {
        let header = egui::CollapsingHeader::new(egui::RichText::new(title).strong()).id_salt(id).default_open(open).show(ui, body);
        header.header_response.on_hover_text(help);
    } else {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).strong());
            help_icon(ui, help);
        });
        ui.separator();
        body(ui);
    }
}

/// How far along a step is: its tab's colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StepState {
    Missing,
    Wrong,
    Good,
}

/// The three steps as tabs of one width, numbered, coloured green, yellow or red. The open one
/// is outlined. Returns the one clicked.
fn step_tabs(ui: &mut egui::Ui, open: u8, tabs: &[(u8, &str, StepState, String)]) -> Option<u8> {
    let mut clicked = None;
    let gap = ui.spacing().item_spacing.x;
    let w = ((ui.available_width() - gap * (tabs.len() as f32 - 1.0)) / tabs.len() as f32).max(60.0);
    ui.horizontal(|ui| {
        for (n, title, state, hover) in tabs {
            let (colour, icon) = match state {
                StepState::Good => (crate::theme::standing::FRIENDLY, egui_phosphor::regular::CHECK_CIRCLE),
                StepState::Missing => (crate::theme::standing::WARNING, egui_phosphor::regular::WARNING),
                StepState::Wrong => (crate::theme::standing::HOSTILE, egui_phosphor::regular::X_CIRCLE),
            };
            let text = egui::RichText::new(format!("{n}  {title}  {icon}")).color(colour).strong();
            let stroke = if *n == open { egui::Stroke::new(2.0, colour) } else { egui::Stroke::new(1.0, colour.gamma_multiply(0.4)) };
            let button = egui::Button::new(text).fill(colour.gamma_multiply(0.12)).stroke(stroke).truncate();
            if ui.add_sized([w, 28.0], button).on_hover_text(hover.as_str()).clicked() {
                clicked = Some(*n);
            }
        }
    });
    clicked
}

/// The open step's fields under the tabs, with its help.
fn step_body(ui: &mut egui::Ui, help: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            help_icon(ui, help);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), body);
        });
    });
}

fn add_ship(h: &mut rescuee::Hostiles, name: &str) {
    match h.ships.iter_mut().find(|(s, _)| s.eq_ignore_ascii_case(name)) {
        Some((_, n)) => *n += 1,
        None => h.ships.push((name.to_owned(), 1)),
    }
}

/// The hull's name as the data spells it, for a typed one.
fn proper_hull_name(typed: &str, name_to_id: &HashMap<String, i64>) -> Option<String> {
    let t = typed.trim().to_lowercase();
    name_to_id.contains_key(&t).then(|| {
        let mut c = typed.trim().chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    })
}

/// The d-scan as read: the hulls with their badges and counts, and the link once uploaded.
fn dscan_preview(ui: &mut egui::Ui, d: &rescuee::DscanPreview, name_to_id: &HashMap<String, i64>, now: i64) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(trf!("D-scan, {ago} ago", ago = rescue_ui::since_ping(now - d.at))).weak());
        help_icon(ui, tr!("Read off the clipboard. Check it is the right scan: this is what the FC gets."));
    });
    const SHOWN: usize = 8;
    for (ship, n) in d.counts.iter().take(SHOWN) {
        ui.horizontal(|ui| {
            if let Some(id) = name_to_id.get(&ship.to_lowercase()) {
                hull_badge(ui, *id, 16.0);
            }
            ui.label(format!("{ship}  ×{n}"));
        });
    }
    if d.counts.len() > SHOWN {
        ui.label(egui::RichText::new(trf!("+{n} more", n = d.counts.len() - SHOWN)).weak());
    }
    match &d.link {
        Some(l) => {
            ui.hyperlink_to(l, l);
        }
        None => {
            ui.label(egui::RichText::new(tr!("uploading…")).weak());
        }
    }
}
