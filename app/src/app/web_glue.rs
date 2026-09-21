//! The web view from the app side: starting the server, its settings section, and the facts it publishes.

use super::*;

/// How long the typing has to stop before the socket is rebound.
pub(crate) const BIND_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(5);

/// What a typed bind address is.
#[derive(PartialEq, Clone, Copy, Debug)]
pub(crate) enum BindAddr {
    /// Nothing typed: the "reachable from the network" switch decides.
    Blank,
    /// An address this machine can be asked to listen on.
    Literal,
    /// Not an address at all.
    Invalid,
}

/// Reads a typed bind address without touching the resolver.
///
/// Literal addresses only, IPv6 in brackets included. A name would be a DNS lookup, and the bind
/// runs on the UI thread: a half-typed host name freezes the app for as long as the resolver takes.
pub(crate) fn bind_addr_state(text: &str) -> BindAddr {
    if text.trim().is_empty() {
        return BindAddr::Blank;
    }
    match crate::web::server::usable_bind_addr(text) {
        Some(_) => BindAddr::Literal,
        None => BindAddr::Invalid,
    }
}

impl SpaiApp {
    /// Start, stop or restart the web listener to match the settings.
    ///
    /// Called every frame and does nothing on almost all of them: the settings are hashed and
    /// compared, so only an actual change touches the socket.
    pub(crate) fn sync_web_server(&mut self) {
        if !self.web_allowed {
            return;
        }
        self.settle_bind_draft();
        let w = &self.settings.web;
        if w.enabled && w.token.is_empty() {
            match crate::web::auth::new_token() {
                Some(t) => {
                    self.settings.web.token = t;
                    self.needs_save = true;
                }
                None => {
                    // No entropy, no pairing secret, so the socket stays shut rather than opening
                    // with something weaker.
                    self.settings.web.enabled = false;
                    self.web_error = Some("could not generate a pairing token".to_owned());
                    self.needs_save = true;
                    return;
                }
            }
        }
        let w = self.settings.web.clone();
        let want = w.enabled.then(|| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            // Deliberately not the theme. It reaches the page through the published snapshot, and
            // hashing it here would restart the listener on every colour change.
            (w.port, w.bind_lan, w.allow_writeback, &w.token, &w.bind_addr, w.no_pairing)
                .hash(&mut h);
            h.finish()
        });
        self.collect_started_server();
        if want == self.web_started_for {
            return;
        }
        self.web_server = None;
        self.web_started_for = want;
        self.web_error = None;
        let Some(want_hash) = want else {
            self.web_starting = None;
            return;
        };
        let cfg = crate::web::server::Config {
            port: w.port,
            bind_lan: w.bind_lan,
            allow_writeback: w.allow_writeback,
            token: w.token.clone(),
            theme: self.settings.theme.clone(),
            bind_addr: w.bind_addr.clone(),
            no_pairing: w.no_pairing,
            map: self.web_map_geometry(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.web_starting = Some((want_hash, rx));
        let (web, detail, inbox) =
            (self.web.clone(), self.web_detail.clone(), self.web_inbox.clone());
        let ctx = self.ui_ctx.clone();
        // Off the UI thread: binding a socket waits on the resolver and on the last listener letting
        // go of the port, and a frame that waits for either is a frozen app.
        std::thread::spawn(move || {
            // The ship dialog reads hull stats out of the SDE. A second connection rather than the
            // app's: `Store` owns a rusqlite `Connection` and cannot be shared across threads, and
            // SQLite allows a second reader on the same file.
            {
                let mut d = detail.lock().unwrap_or_else(|e| e.into_inner());
                if d.store.is_none() {
                    match crate::store::Store::open() {
                        Ok(s) => d.store = Some(s),
                        Err(e) => eprintln!("[web] no store for the dialogs: {e}"),
                    }
                }
                // Coordinates for the jump maths, loaded once here rather than pushed each frame:
                // the app's own copy is behind the rescue feature, and `all_map_systems` is 5000
                // rows.
                if d.coords.is_none() {
                    if let Some(store) = d.store.as_ref() {
                        d.coords = Some(std::sync::Arc::new(store.all_map_systems()));
                    }
                }
            }
            let out = crate::web::server::start(cfg, web, detail, inbox);
            // Dropped on a failed send, which stops the listener: by then the settings have moved on
            // and something else is being bound.
            let _ = tx.send(out);
            ctx.request_repaint();
        });
    }

    /// Takes a listener a worker finished binding, or the reason it could not.
    fn collect_started_server(&mut self) {
        let Some((for_hash, rx)) = &self.web_starting else { return };
        let (for_hash, out) = match rx.try_recv() {
            Ok(out) => (*for_hash, out),
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.web_starting = None;
                self.web_error = Some("the listener could not be started".to_owned());
                return;
            }
        };
        self.web_starting = None;
        // The settings moved on while it was binding, so this listener is already the wrong one.
        if self.web_started_for != Some(for_hash) {
            return;
        }
        match out {
            Ok(h) => self.web_server = Some(h),
            Err(e) => {
                eprintln!("[web] {e}; web view disabled");
                self.web_error = Some(e);
            }
        }
    }

    /// Commits a typed bind address once the typing has stopped for [`BIND_DEBOUNCE`].
    ///
    /// An address that is not one is never committed: the socket keeps what it has rather than
    /// falling back to every interface, which is the one mistake this field must not make.
    pub(crate) fn settle_bind_draft(&mut self) {
        let Some((draft, at)) = &self.web_bind_draft else { return };
        if bind_addr_state(draft) == BindAddr::Invalid || at.elapsed() < BIND_DEBOUNCE {
            return;
        }
        let addr = draft.trim().to_owned();
        self.web_bind_draft = None;
        if self.settings.web.bind_addr != addr {
            self.settings.web.bind_addr = addr;
            self.needs_save = true;
        }
    }

    /// What the bind address field has to say for itself: bound, refused, not an address, or waiting
    /// for the typing to stop.
    pub(crate) fn bind_status(&self) -> (String, egui::Color32) {
        use crate::theme::standing;
        let waiting = self.web_bind_draft.as_ref();
        if let Some((draft, at)) = waiting {
            if bind_addr_state(draft) == BindAddr::Invalid {
                return ("not an address".to_owned(), standing::HOSTILE);
            }
            let left = BIND_DEBOUNCE.saturating_sub(at.elapsed()).as_secs() + 1;
            return (format!("binding in {left}s"), standing::WARNING);
        }
        if let Some(e) = &self.web_error {
            return (e.clone(), standing::HOSTILE);
        }
        // A saved address that is not one is ignored rather than allowed to hold up the start, so
        // the field has to say so or the page looks bound to something it is not.
        if bind_addr_state(&self.settings.web.bind_addr) == BindAddr::Invalid {
            return ("not an address, ignored".to_owned(), standing::HOSTILE);
        }
        match &self.web_server {
            Some(h) => (format!("bound to {}", h.addr), standing::FRIENDLY),
            None if self.web_starting.is_some() => ("binding…".to_owned(), standing::WARNING),
            None => ("not bound".to_owned(), standing::WARNING),
        }
    }

    /// The remote web view's settings.
    ///
    /// Returns whether anything changed, which the caller folds into its own save flag.
    pub(crate) fn web_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        use egui_phosphor::regular as icon;
        let mut changed = false;

        ui.label(egui::RichText::new(format!("{}  Remote web view", icon::BROADCAST)).strong());
        changed |= ui
            .checkbox(&mut self.settings.web.enabled, "Serve the intel feed to a browser")
            .on_hover_text(
                "Opens a page on this machine showing the intel feed, alerts, fleet pings and the                  map. Pair a phone once and it stays paired.",
            )
            .changed();

        if !self.settings.web.enabled {
            ui.label(
                egui::RichText::new(
                    "LAN only, and not encrypted. Do not forward this port to the internet.",
                )
                .weak(),
            );
            return changed;
        }

        ui.horizontal_wrapped(|ui| {
            ui.label("Port");
            changed |= ui
                .add(egui::DragValue::new(&mut self.settings.web.port).range(1024..=65535))
                .changed();
            changed |= ui
                .checkbox(&mut self.settings.web.bind_lan, "Reachable from the network")
                .on_hover_text(
                    "Off binds this machine only, which needs a tunnel to reach from a phone.",
                )
                .changed();
            changed |= ui
                .checkbox(&mut self.settings.web.allow_writeback, "Allow changes from the page")
                .on_hover_text(
                    "Classifying an uncertain pilot and acknowledging an alert. Off makes the page                      read-only.",
                )
                .changed();
        });

        if let Some(err) = &self.web_error {
            ui.label(
                egui::RichText::new(format!("{}  {err}", icon::WARNING))
                    .color(crate::theme::standing::WARNING),
            );
        }

        let url = self.web_pairing_url();
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(format!("{}  Copy pairing link", icon::COPY))
                .on_hover_text("Carries the token. Treat it as a password.")
                .clicked()
            {
                ui.ctx().copy_text(url.clone());
            }
            if ui.button(format!("{}  Open in browser", icon::ARROW_SQUARE_OUT)).clicked() {
                let _ = open::that(&url);
            }
            if ui
                .button(format!("{}  Regenerate link", icon::ARROWS_CLOCKWISE))
                .on_hover_text("Invalidates every paired device. They have to open a new link.")
                .clicked()
            {
                self.settings.web.token.clear();
                changed = true;
            }
            let n = self.web_server.as_ref().map_or(0, |h| h.clients());
            ui.label(
                egui::RichText::new(match n {
                    0 => "no devices connected".to_owned(),
                    1 => "1 device connected".to_owned(),
                    n => format!("{n} devices connected"),
                })
                .weak(),
            );
        });

        // Neither the link nor the code is shown by default: both carry the token, and a settings
        // pane is the kind of screen people share or stream. Revealing either is a deliberate act,
        // and it does not persist.
        let label = if self.web_reveal { "Hide the pairing link" } else { "Show the pairing link and QR" };
        if ui.button(format!("{}  {label}", icon::MAGNIFYING_GLASS)).clicked() {
            self.web_reveal = !self.web_reveal;
        }
        if self.web_reveal {
            ui.label(egui::RichText::new(&url).monospace());
            ui.label(
                egui::RichText::new("Scan this from the phone. Anyone who reads it is paired.")
                    .weak(),
            );
            if let Some(tex) = self.web_qr_texture(ui.ctx(), &url) {
                ui.add(egui::Image::new(&tex).fit_to_original_size(1.0));
            }
        }

        changed |= self.web_advanced_section(ui);
        changed
    }

    /// The two switches that can put this on the open internet.
    ///
    /// Behind a collapsed header and behind a warning that has to be accepted once, because the
    /// difference between "a phone on my wifi" and "anyone" is one checkbox and the page carries
    /// alliance intel and private conversations. Both default off and stay off: nothing in here is
    /// reachable without opening the section, reading the warning and saying yes.
    pub(crate) fn web_advanced_section(&mut self, ui: &mut egui::Ui) -> bool {
        use egui_phosphor::regular as icon;
        let mut changed = false;
        // Collapsed until asked for, except when it is already holding something: a setting that
        // changes who can reach the page has no business being out of sight.
        let holds_something = !self.settings.web.bind_addr.is_empty()
            || self.settings.web.no_pairing
            || self.web_bind_draft.is_some();
        egui::CollapsingHeader::new(format!("{}  Advanced", icon::GEAR_SIX))
            .id_salt("web_advanced")
            .default_open(holds_something)
            .show(ui, |ui| {
                if !self.settings.web.advanced_ack {
                    ui.label(
                        egui::RichText::new(format!("{}  Read this first", icon::WARNING))
                            .color(crate::theme::standing::HOSTILE)
                            .strong(),
                    );
                    ui.label(
                        "These two settings can put this page on the public internet. Anyone who \
                         reaches it sees everything the app sees: the intel feed, fleet pings, your \
                         jabber conversations and any opsec channel you are in. There is no TLS and, \
                         with pairing off, no password either.",
                    );
                    ui.label(
                        egui::RichText::new(
                            "Only turn these on if you understand exactly what you are exposing and \
                             to whom.",
                        )
                        .strong(),
                    );
                    if ui.button("I understand, show the advanced settings").clicked() {
                        self.settings.web.advanced_ack = true;
                        changed = true;
                    }
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bind address");
                    let mut draft = match &self.web_bind_draft {
                        Some((t, _)) => t.clone(),
                        None => self.settings.web.bind_addr.clone(),
                    };
                    let edit = ui
                        .add(
                            egui::TextEdit::singleline(&mut draft)
                                .hint_text("blank = the choice above")
                                .desired_width(160.0),
                        )
                        .on_hover_text(
                            "An interface address to listen on instead. Blank uses the LAN setting \
                             above, which is what you want unless you are binding one specific \
                             interface, such as a VPN. The socket is rebound once you stop typing, \
                             since a half-typed address is a different one.",
                        );
                    if edit.changed() {
                        self.web_bind_draft = Some((draft, std::time::Instant::now()));
                    }
                    let (text, color) = self.bind_status();
                    ui.label(egui::RichText::new(text).color(color).size(11.0));
                    // The countdown has to run down on its own: nothing else repaints a settings
                    // window that is only being looked at.
                    if self.web_bind_draft.is_some() {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                    }
                });
                changed |= ui
                    .checkbox(&mut self.settings.web.no_pairing, "Serve without pairing")
                    .on_hover_text(
                        "Anyone who can reach the port gets in, with no token and no password. Only \
                         sensible behind something else that does the authenticating.",
                    )
                    .changed();
                if self.settings.web.no_pairing {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  This page is open to anyone who can reach it.",
                            icon::WARNING
                        ))
                        .color(crate::theme::standing::HOSTILE),
                    );
                }
            });
        changed
    }

    /// The pairing QR, uploaded once per distinct link.
    ///
    /// Keyed on the URL itself, so regenerating the token or changing the port replaces the texture
    /// rather than leaving a code that pairs nothing.
    pub(crate) fn web_qr_texture(
        &mut self,
        ctx: &egui::Context,
        url: &str,
    ) -> Option<egui::TextureHandle> {
        if self.web_qr.as_ref().is_none_or(|(for_url, _)| for_url != url) {
            let img = crate::web::auth::qr_image(url, 4)?;
            let tex = ctx.load_texture("web-pairing-qr", img, egui::TextureOptions::NEAREST);
            self.web_qr = Some((url.to_owned(), tex));
        }
        self.web_qr.as_ref().map(|(_, t)| t.clone())
    }

    /// The address a phone should open, token and all. Falls back to the bound address when the
    /// machine's LAN address cannot be worked out.
    pub(crate) fn web_pairing_url(&self) -> String {
        let host = self
            .web_server
            .as_ref()
            .map(|h| h.addr.clone())
            .unwrap_or_else(|| format!("0.0.0.0:{}", self.settings.web.port));
        // 0.0.0.0 is every interface, which is not an address anyone can type into a phone.
        //
        // Not probed headlessly: a harness build performs no side effects, and a render of this pane
        // gets committed to a ticket folder, where the machine's own address has no business being.
        let host = match host.strip_prefix("0.0.0.0") {
            Some(port) => match self.web_allowed.then(crate::web::server::lan_address).flatten() {
                Some(ip) => format!("{ip}{port}"),
                None => format!("<this machine's address>{port}"),
            },
            None => host,
        };
        format!("http://{host}/?t={}", self.settings.web.token)
    }

    /// Sov upgrades per system, classified exactly as the map classifies them.
    ///
    /// Kind, level and ore are worked out here rather than in the browser so the page draws the same
    /// icon for the same upgrade without carrying a copy of the keyword list.
    pub(crate) fn web_upgrade_marks(&self) -> Vec<(i64, Vec<(u8, u8, Option<i64>)>)> {
        let Some(g) = &self.systems else { return Vec::new() };
        let mut by_system: std::collections::BTreeMap<i64, Vec<(u8, u8, Option<i64>)>> =
            Default::default();
        for u in &self.settings.sov_upgrades {
            let Some(info) = g.lookup(&u.system) else { continue };
            let (icon, level) = upgrade_info(&u.upgrade);
            let (kind, ore) = match icon {
                UpgradeIcon::Mineral(id) => (UpgradeKind::Mining as u8, Some(id)),
                UpgradeIcon::Glyph(_) => (upgrade_kind(&u.upgrade) as u8, None),
            };
            by_system.entry(info.id).or_default().push((kind, level, ore));
        }
        by_system.into_iter().collect()
    }

    /// Alliance name to colour, resolved the way the map resolves it: the user's configured colour
    /// where there is one, otherwise the same hash-derived colour the app falls back to. Sent
    /// resolved so the page never has to know an alliance exists.
    pub(crate) fn web_sov_colors(&self) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
        let status = self.system_status.lock().unwrap_or_else(|e| e.into_inner());
        for f in status.values() {
            let Some(name) = &f.sov else { continue };
            if out.contains_key(name) {
                continue;
            }
            let c = self.alliance_color_of(name).unwrap_or_else(|| name_color(name));
            out.insert(name.clone(), format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()));
        }
        out
    }

    /// Alliance name to its coalition's colour, for the map's "by coalition" mode.
    pub(crate) fn web_coalition_colors(&self) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
        for c in &self.settings.coalitions {
            let col = c
                .color
                .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
                .unwrap_or_else(|| name_color(&c.name));
            let hex = format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b());
            for a in &c.alliances {
                out.insert(a.clone(), hex.clone());
            }
        }
        out
    }

    /// Map geometry for the page, built once from the SDE the app already has loaded.
    ///
    /// Built here rather than in the server because this is where both halves are to hand: the store
    /// holds the projected coordinates and `systems` holds the graph. Returns `None` before the SDE
    /// is ready, which the page renders as an empty map rather than as an error.
    pub(crate) fn web_map_geometry(&self) -> Option<std::sync::Arc<crate::web::map::Geometry>> {
        let store = self.store.as_ref()?;
        let graph = self.systems.as_ref()?;
        let systems = store.all_map_systems();
        if systems.is_empty() {
            return None;
        }
        Some(std::sync::Arc::new(crate::web::map::build(&systems, graph)))
    }

    pub(crate) fn publish_ui_facts(&self) {
        let mut f = self.web_facts.lock().unwrap_or_else(|e| e.into_inner());
        f.web_enabled = self.settings.web.enabled;
        // Everything below clones a roster, a rule list and a graph handle, once per frame. The web
        // view is off by default, so doing it anyway would be a cost paid by every user who never
        // turns it on. The publisher reads `web_enabled` and idles for the same reason.
        if !f.web_enabled {
            return;
        }
        f.systems = self.systems.clone();
        f.chars = self.characters.iter().map(|c| (c.name.clone(), c.id)).collect();
        f.active_character = self.active_character.clone();
        f.disabled = self.settings.intel_disabled_chars.clone();
        f.only_undocked = self.settings.alert_only_undocked;
        f.count_bridges = self.settings.intel_count_bridges;
        f.staging = self.staging_system().map(str::to_owned);
        f.notes = self.notes.clone();
        f.notes_view = self.notes_view.clone();
        f.intel_max_jumps = self.intel_max_jumps;
        f.intel_ttl_secs = self.settings.intel_ttl_secs;
        f.severity = self.settings.severity.clone();
        f.ping_rules = self.settings.jabber_ping_rules.clone();
        f.compact = self.settings.alerts.compact_mode;
        f.theme = self.settings.theme.clone();
        f.allow_writeback = self.settings.web.allow_writeback;
        f.sounds = self.settings.alerts.sounds.clone();
        f.camps = self.camped_cache.iter().map(|(id, _)| *id).collect();
        f.holes = self
            .wh_cache
            .iter()
            .filter_map(|w| Some((w.system_id, w.dest_system_id?)))
            .collect();
        f.upgrades = self.web_upgrade_marks();
        f.cyno = self.settings.cyno_generators.clone();
        f.route = self.travel_route.clone().unwrap_or_default();
        f.sov_colors = self.web_sov_colors();
        f.coal_colors = self.web_coalition_colors();
        f.jabber = self.web_jabber_side();
        f.rescue = self.web_rescue_side();
        f.avoid_gate = self.settings.route_avoid_gate.clone();
        f.avoid_jump = self.settings.route_avoid_jump.clone();
        drop(f);

        let mut d = self.web_detail.lock().unwrap_or_else(|e| e.into_inner());
        d.graph = self.systems.clone();
        d.player_sys = self.player_system();
        d.active_character = self.active_character.clone();
        d.staging = self.staging_system().map(str::to_owned);
        d.notes = self.notes.clone();
        d.count_bridges = self.settings.intel_count_bridges;
        // Cloned rather than shared: the status map is small and rewritten wholesale by its poller,
        // so holding its lock from a request thread would be the only way to block that poller.
        d.status = self.system_status.lock().unwrap_or_else(|e| e.into_inner()).clone();
        // Shared, unlike the above: one room's backlog is bigger than every pane put together, and
        // the page reads one conversation at a time.
        d.jabber = Some(self.jabber.clone());
        d.avoid_gate = self.settings.route_avoid_gate.clone();
        d.avoid_jump = self.settings.route_avoid_jump.clone();
        d.via_wormholes = self.settings.route_via_wormholes;
        // Pruned where they are read rather than on a timer: a route through a scanned hole is wrong
        // long before it is a day old, and a day is the point at which keeping it is worse than
        // losing it.
        let now = chrono::Utc::now().timestamp();
        d.saved_routes = self
            .settings
            .saved_map_routes
            .iter()
            .filter(|r| {
                !r.via_wormholes
                    || now - r.saved_at < crate::settings::WORMHOLE_ROUTE_TTL_SECS
            })
            .cloned()
            .collect();
        d.holes = if self.settings.route_via_wormholes { self.wh_adjacency() } else { Default::default() };
        d.type_names = Some(self.type_names.clone());
        d.wh_cache = self.wh_cache.clone();
        d.sov_upgrades = self.settings.sov_upgrades.clone();
        d.bookmarks = self.settings.bookmarks.clone();
        d.camps = Some(self.camps.clone());
    }

    /// The rescue pane, or `None` when this build has no rescue mode or the user has it off.
    ///
    /// Read-only by construction: the desktop's rescue window sends pings and pulls people into
    /// comms, and a socket on the LAN should not be able to broadcast to an alliance.
    #[cfg(feature = "fleet")]
    pub(crate) fn web_rescue_side(&self) -> Option<crate::web::rescue::RescueSide> {
        if !self.settings.fc_rescue_enabled {
            return None;
        }
        let r = self.rescue.lock().unwrap_or_else(|e| e.into_inner());
        let pings = r
            .recent_pings(8)
            .into_iter()
            .map(|e| crate::web::rescue::RescuePing {
                seq: e.seq,
                at: e.received,
                author: e.author.clone(),
                system: e.system_id,
                system_name: e.system_name.clone(),
                pilot: e.pilot.clone(),
                cyno: e.cyno.clone(),
                anomaly: e.anomaly.clone(),
                class: e.cap_class.map(|c| format!("{c:?}")),
                selected: r.selected_ping == Some(e.seq),
            })
            .collect();
        Some(crate::web::rescue::RescueSide {
            active: r.active,
            test_mode: r.test_mode,
            doctrine: r.doctrine.clone(),
            op_channel: r.op_channel,
            capital_system: r.capital_system_name.clone(),
            capital_pilot: r.capital_pilot.clone(),
            range: self.rescue_range.as_ref().map(|w| crate::web::rescue::RescueRange {
                ly: w.ly_from_staging,
                closest: w.closest_name.clone(),
                ansiblex_jumps: w.ansi_jumps,
                gate_jumps: w.gate_jumps,
                ly_to_target: w.ly_to_target,
            }),
            pings,
        })
    }

    /// Without the feature there is no rescue mode to report on.
    #[cfg(not(feature = "fleet"))]
    pub(crate) fn web_rescue_side(&self) -> Option<crate::web::rescue::RescueSide> {
        None
    }

    /// The Convos list as the page gets it.
    ///
    /// Built here rather than in the publisher because the rules read settings the publisher has no
    /// handle on: who is a contact, which conversations were closed, which were forgotten. The order
    /// is the app's own, so the two lists never disagree about what is at the top.
    pub(crate) fn web_jabber_side(&self) -> crate::web::jabber::JabberSide {
        use crate::web::jabber::WebConvo;
        let st = self.jabber.lock().unwrap_or_else(|e| e.into_inner());
        let forgotten: std::collections::HashSet<&String> =
            self.settings.jabber_forgotten.iter().collect();
        let contacts: std::collections::HashSet<&String> =
            self.settings.jabber_contacts.iter().collect();
        let closed_dms: std::collections::HashSet<&String> =
            self.settings.jabber_closed_dms.iter().collect();
        let closed_rooms: std::collections::HashSet<&String> =
            self.settings.jabber_closed_rooms.iter().collect();

        let mut keys: std::collections::BTreeSet<&String> = st.chats.keys().collect();
        keys.extend(st.roster.keys());
        keys.extend(st.rooms.iter());

        let mut convos: Vec<WebConvo> = keys
            .into_iter()
            .filter(|jid| !forgotten.contains(*jid))
            .filter(|jid| jid.as_str() != crate::jabber::PING_FEED_KEY)
            .map(|jid| {
                let room = st.rooms.contains(jid) || st.rooms_left.contains(jid);
                let last_at =
                    st.chats.get(jid).and_then(|c| c.last()).map(|m| m.time).unwrap_or(0);
                let name = st
                    .roster
                    .get(jid)
                    .and_then(|c| c.name.clone())
                    .unwrap_or_else(|| jid.split('@').next().unwrap_or(jid).to_owned());
                let presence = (!room).then(|| {
                    let p = st
                        .roster
                        .get(jid)
                        .map(|c| c.presence)
                        .or_else(|| st.presences.get(jid).map(|(p, _)| *p))
                        .unwrap_or_default();
                    let (r, g, b) = p.color();
                    format!("#{r:02x}{g:02x}{b:02x}")
                });
                let listed = if room {
                    !closed_rooms.contains(jid) && !st.rooms_left.contains(jid)
                } else {
                    !closed_dms.contains(jid)
                        && (contacts.contains(jid) || st.chats.contains_key(jid))
                };
                WebConvo {
                    jid: jid.clone(),
                    name,
                    room,
                    listed,
                    unread: st.unread_counts.get(jid).copied().unwrap_or(0),
                    mention: st.mentions.contains(jid),
                    last_at,
                    presence,
                    motd: if room {
                        st.room_subjects.get(jid).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    },
                }
            })
            .collect();
        // DMs above rooms, unread above read, then by recency. The app's rule, stated once: an
        // unread conversation with no history yet would sort to the bottom on recency alone, which
        // is the one place it must not be.
        convos.sort_by(|a, b| {
            a.room
                .cmp(&b.room)
                .then((b.unread > 0).cmp(&(a.unread > 0)))
                .then(b.last_at.cmp(&a.last_at))
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        crate::web::jabber::JabberSide {
            configured: !self.settings.jabber_jid.trim().is_empty(),
            connected: st.connected,
            convos,
            mention_names: self.mention_names(),
        }
    }
}
