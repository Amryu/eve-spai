//! The fleet dashboard tab: the fleet list, the start form, and a tracked fleet.

use super::*;

#[cfg(feature = "fleet")]
use crate::fleets::{
    backend::Action,
    model::{FleetRow, Perm, TagItem},
    state::{Cmd, Outcome, Page, Slot},
};

impl SpaiApp {
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_view(&mut self, ui: &mut egui::Ui) {
        self.fleet_body(ui);
    }

    #[cfg(not(feature = "fleet"))]
    pub(crate) fn fleet_view(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("This build has no fleet dashboard.").weak());
    }

    /// Hands one command to a worker. Off the UI thread even against the dry run, so the path the
    /// real backend will take is the one used every day.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_dispatch(&mut self, cmd: Cmd) {
        let (gen, backend, tx, ctx) = (
            self.fleet_gen,
            self.fleet_backend.clone(),
            self.fleet_tx.clone(),
            self.ui_ctx.clone(),
        );
        let seed = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.clone();
        std::thread::spawn(move || {
            // A malformed reply drops one command rather than the app, the way the watcher treats
            // its parser.
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::fleets::state::run(backend.as_ref(), &seed, cmd)
            }))
            .unwrap_or_else(|_| Outcome::Failed {
                what: "fleet request",
                why: "the worker panicked".to_owned(),
            });
            let _ = tx.send((gen, out));
            ctx.request_repaint();
        });
    }

    /// Takes whatever the workers finished. Never blocks: a frame that waits on a worker is a
    /// frozen app.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_collect(&mut self) {
        while let Ok(at) = self.fleet_mumble_rx.try_recv() {
            self.fleet_mumble_at = at;
        }
        let cur = self.fleet_gen;
        let mut done: Vec<Outcome> = Vec::new();
        while let Ok((gen, out)) = self.fleet_rx.try_recv() {
            if crate::fleets::state::accepts(cur, gen, &out) {
                done.push(out);
            }
        }
        if done.is_empty() {
            return;
        }
        // A close ends the page it was sent from: the in-game fleet is gone, the roster is the
        // dashboard's record of who was there, and every button on the live view now acts on
        // nothing. Move to the closed view rather than leaving the FC on a page that lies.
        let closed = done.iter().find_map(|o| match o {
            Outcome::Closed { id, .. } => Some(id.clone()),
            _ => None,
        });
        // The report is generated after the close, so the one read on the way in came back null
        // and the participant list was empty. This is the dashboard saying it is ready.
        let stats_ready = done.iter().find_map(|o| match o {
            Outcome::StatsReady { id } => Some(id.clone()),
            _ => None,
        });
        {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            for out in done {
                st.apply(out);
            }
        }
        if let Some(id) = closed {
            let page = Page::Historic(id.clone());
            self.fleet_gen.page += 1;
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page = page.clone();
            self.fleet_refresh_fresh(&page);
            // The dashboard has not written the report yet, so the open this just started will
            // come back with nobody in it. Ask again shortly, and keep asking for a while.
            self.fleet_reopen = Some((id, std::time::Instant::now() + REOPEN_WAIT, REOPEN_TRIES));
        }
        if let Some(id) = stats_ready {
            let page = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
            if page.fleet() == Some(&id) {
                self.fleet_reopen = None;
                self.fleet_dispatch(Cmd::Open(id));
            }
        }
    }

    /// Asks whether the open fleet is advertised, on opening it and then once a minute.
    ///
    /// The advert is toggled in game, so nothing on the dashboard says when it changes. Only a
    /// running fleet is asked about: a closed one has no advert, and ESI would only refuse.
    #[cfg(feature = "fleet")]
    fn fleet_advert_poll(&mut self, ctx: &egui::Context) {
        // Like the boost read: a headless render must not go asking ESI about a real fleet.
        if self.headless {
            return;
        }
        let fleet = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            match (&st.page, st.open.value.as_ref()) {
                (Page::Tracking(_), Some(o)) if o.fleet.closed_at.is_none() && o.fleet.esi_id > 0 => {
                    Some(o.fleet.clone())
                }
                _ => None,
            }
        };
        let Some(fleet) = fleet else {
            self.fleet_advert_at = None;
            return;
        };
        let due = match &self.fleet_advert_at {
            Some((id, at)) if *id == fleet.id => at.elapsed() >= ADVERT_POLL,
            _ => true,
        };
        if due {
            self.fleet_advert_at = Some((fleet.id.clone(), std::time::Instant::now()));
            self.fleet_dispatch(Cmd::CheckAdvert(Box::new(fleet)));
        }
        ctx.request_repaint_after(ADVERT_POLL);
    }

    /// Re-reads a fleet that opened with nothing in it.
    ///
    /// A fleet closed from this app is opened again immediately, and the dashboard generates its
    /// statistics after the close, so that first read finds a null report and builds an empty
    /// participant list. The hub says when the report is ready; this is the answer for a build or
    /// a session with no hub, and it stops either way once the roster has someone in it.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_reopen_poll(&mut self, ctx: &egui::Context) {
        let Some((id, due, left)) = self.fleet_reopen.clone() else { return };
        ctx.request_repaint_after(REOPEN_WAIT);
        let filled = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.page.fleet() != Some(&id)
                || st.open.value.as_ref().is_some_and(|o| o.composition.total() > 0)
        };
        if filled || left == 0 {
            self.fleet_reopen = None;
            return;
        }
        if std::time::Instant::now() < due {
            return;
        }
        self.fleet_reopen =
            Some((id.clone(), std::time::Instant::now() + REOPEN_WAIT, left - 1));
        self.fleet_dispatch(Cmd::Open(id));
    }

    /// Reloads a page the user has just navigated to, as opposed to asking for the same one
    /// again. Everything on screen belongs to where they were and has to go.
    #[cfg(feature = "fleet")]
    fn fleet_refresh_fresh(&mut self, page: &Page) {
        if let Page::Tracking(id) | Page::Historic(id) = page {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let same = st.open.value.as_ref().is_some_and(|o| o.fleet.id == *id);
            st.open.restart();
            if !same {
                // Both are keyed by pilot and mean nothing about a different fleet. Kept when it
                // is the same fleet, because the off-doctrine clock is what makes a long-standing
                // offender read differently from someone who just swapped.
                st.off_doctrine = Vec::new();
                st.locked = Default::default();
            }
        }
        self.fleet_refresh(page);
    }

    /// Loads what a page shows, on first sight and whenever it changes.
    #[cfg(feature = "fleet")]
    fn fleet_refresh(&mut self, page: &Page) {
        match page {
            Page::Fleets => {
                let (skip, search) = {
                    let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                    st.active_strat.begin();
                    st.active_pct.begin();
                    st.history.begin();
                    (st.history_skip, st.history_search.clone())
                };
                self.fleet_dispatch(Cmd::LoadActive { strategic: true });
                self.fleet_dispatch(Cmd::LoadActive { strategic: false });
                self.fleet_dispatch(Cmd::LoadHistory { skip, search });
            }
            Page::Tracking(id) | Page::Historic(id) => {
                {
                    let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                    st.open.begin();
                    // The previous fleet's charges are not this one's. Left in place they read as
                    // this fleet's coverage for as long as the open takes.
                    st.boosts = Vec::new();
                    st.boost_lines = Vec::new();
                    st.boosts_loading = true;
                }
                self.fleet_dispatch(Cmd::Open(id.clone()));
                self.fleet_boosts_read = None;
            }
            // The form renders its ping from the dashboard, and nothing else asks for one until
            // the first edit. Without this the pane fell back to the local template every time
            // the page opened, which is how a normal fleet came up showing the cap-save text.
            Page::Start => self.fleet_preview_now(),
        }
    }

    /// What the comms buttons on the open fleet point at.
    #[cfg(feature = "fleet")]
    fn fleet_comms_targets(&mut self) -> CommsTargets {
        use crate::fleets::comms;
        self.comms_remember_resolved();
        let (op_name, sector) = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let Some(open) = st.open.value.as_ref() else { return CommsTargets::default() };
            let tags: Vec<_> =
                open.fleet.tag_ids.iter().filter_map(|t| st.seed.tag(*t)).cloned().collect();
            (
                st.seed
                    .channel_name(&st.seed.mumble_channels, open.fleet.mumble_channel_id)
                    .map(str::to_owned),
                comms::sector(&tags),
            )
        };
        let Some(op_name) = op_name else { return CommsTargets::default() };
        let mut op = comms::links(
            &op_name,
            &self.settings.op_channel_links,
            &self.settings.comms_mumble_cache,
        );
        // Resolving ahead of the click, so a mumble:// link is usually there when it comes.
        if op.mumble.is_none() {
            if let Some(short) = op.short.clone() {
                op.mumble = self.comms_resolve(&short);
            }
        }
        let op = (op.mumble.is_some() || op.short.is_some()).then_some(op);
        let command = comms::command_url(sector, &op_name)
            .map(|u| comms::Links { mumble: Some(u), short: None });
        let unlinked = op.is_none().then(|| op_name.clone());
        CommsTargets { op, command, unlinked }
    }

    /// Copies what background fetches resolved into the settings, so the next run starts with it.
    #[cfg(feature = "fleet")]
    fn comms_remember_resolved(&mut self) {
        let fresh: Vec<(String, String)> = self
            .comms_resolved
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter_map(|(short, (m, _))| m.clone().map(|m| (short.clone(), m)))
            .collect();
        for (short, mumble) in fresh {
            if self.settings.comms_mumble_cache.get(&short) != Some(&mumble) {
                self.settings.comms_mumble_cache.insert(short, mumble);
                self.needs_save = true;
            }
        }
    }

    /// Joins a channel, Mumble first.
    ///
    /// A known `mumble://` link goes straight to Mumble. Without one the short link is resolved on
    /// a thread and the result handed to Mumble, so a click before the background fetch finished
    /// still ends in Mumble; only if that fails too does the short link go to the browser.
    #[cfg(feature = "fleet")]
    fn comms_join(&self, links: crate::fleets::comms::Links) {
        if let Some(m) = links.mumble {
            crate::mumble::open_url(&m);
            return;
        }
        let Some(short) = links.short else { return };
        let (map, ctx) = (self.comms_resolved.clone(), self.ui_ctx.clone());
        std::thread::spawn(move || {
            let page = crate::http::client(10)
                .ok()
                .and_then(|c| c.get(&short).send().ok())
                .and_then(|r| r.text().ok())
                .unwrap_or_default();
            match crate::fleets::comms::mumble_url_in(&page) {
                Some(m) => {
                    crate::mumble::open_url(&m);
                    map.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(short, (Some(m), std::time::Instant::now()));
                }
                None => {
                    let _ = open::that(&short);
                }
            }
            ctx.request_repaint();
        });
    }

    /// The short link for a comms channel, from pings, the built-in table or the user's settings.
    #[cfg(feature = "fleet")]
    pub(crate) fn comms_short_link(&self, channel_name: &str) -> Option<String> {
        crate::fleets::comms::short_link(channel_name, &self.settings.op_channel_links)
    }

    /// The `mumble://` link a short link resolves to, once it has been fetched. Starts the fetch
    /// the first time it is asked, so the answer is usually there by the time anyone clicks.
    #[cfg(feature = "fleet")]
    pub(crate) fn comms_resolve(&self, short: &str) -> Option<String> {
        if self.headless {
            return None;
        }
        let now = std::time::Instant::now();
        let mut map = self.comms_resolved.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(short) {
            Some((Some(url), _)) => return Some(url.clone()),
            Some((None, at)) if now.duration_since(*at) < COMMS_RETRY => return None,
            _ => {}
        }
        map.insert(short.to_owned(), (None, now));
        drop(map);
        let (short, map, ctx) =
            (short.to_owned(), self.comms_resolved.clone(), self.ui_ctx.clone());
        std::thread::spawn(move || {
            let page = crate::http::client(10)
                .ok()
                .and_then(|c| c.get(&short).send().ok())
                .and_then(|r| r.text().ok())
                .unwrap_or_default();
            let found = crate::fleets::comms::mumble_url_in(&page);
            map.lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(short, (found, std::time::Instant::now()));
            ctx.request_repaint();
        });
        None
    }

    /// Keeps the dashboard's push stream open for whatever fleet is on screen.
    ///
    /// The stream replaces polling: the site pushes the member tree on every change, and the same
    /// stream is how it learns the fleet closed. One connection per fleet, torn down by dropping
    /// the flag the thread watches, so leaving the page stops it.
    ///
    /// A backend with no hub (the spoof, and every test) says so once and is not asked again.
    #[cfg(feature = "fleet")]
    fn fleet_hub_once(&mut self) {
        use crate::fleets::hub::Event;

        if self.headless
            || self.fleet_hub_unavailable_flag.load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        let (page, closed) = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let closed = st.open.value.as_ref().is_some_and(|o| o.fleet.closed_at.is_some());
            (st.page.clone(), closed)
        };
        // Nothing left to push. The in-game fleet is gone and what remains is a record that only
        // changes when someone edits it, so the connection is closed rather than left open
        // repeating a tree the page must not show.
        let Some(id) = page.fleet().cloned().filter(|_| !closed) else {
            self.fleet_hub_stop();
            return;
        };
        if self.fleet_hub.as_ref().is_some_and(|(open, _)| *open == id) {
            return;
        }
        self.fleet_hub_stop();

        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        self.fleet_hub = Some((id.clone(), alive.clone()));
        let (gen, backend, tx, ctx) = (
            self.fleet_gen,
            self.fleet_backend.clone(),
            self.fleet_tx.clone(),
            self.ui_ctx.clone(),
        );
        let ships = self.fleet_ship_types();
        let unavailable = self.fleet_hub_unavailable_flag.clone();
        std::thread::spawn(move || {
            let mut feed = match backend.open_hub(&id) {
                Ok(f) => f,
                Err(e) => {
                    // Not an error banner: a backend without a hub is the normal offline case, and
                    // the page keeps polling.
                    crate::esilog::record("fleet hub", &e.to_string());
                    unavailable.store(true, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
            };
            while alive.load(std::sync::atomic::Ordering::Relaxed) {
                let Some(event) = feed.next() else { break };
                let out = match event {
                    Event::Data(d) => Some(Outcome::HubComposition {
                        id: id.clone(),
                        composition: d.composition.to_composition(&ships),
                    }),
                    Event::Fleet(v) => serde_json::from_value(*v)
                        .ok()
                        .map(|f| Outcome::HubFleet(Box::new(f))),
                    Event::Stats => Some(Outcome::StatsReady { id: id.clone() }),
                    Event::AuditLogs | Event::Idle => None,
                };
                let Some(out) = out else { continue };
                if tx.send((gen, out)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
    }

    #[cfg(feature = "fleet")]
    fn fleet_hub_stop(&mut self) {
        if let Some((_, alive)) = self.fleet_hub.take() {
            alive.store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// The SDE's hull names and groups, which the hub does not send.
    #[cfg(feature = "fleet")]
    fn fleet_ship_types(&self) -> std::collections::HashMap<i64, (String, String)> {
        match crate::store::Store::open() {
            Ok(s) => s.all_ships().into_iter().map(|(id, n, g)| (id, (n, g))).collect(),
            Err(_) => Default::default(),
        }
    }

    /// Re-reads the open fleet's boost channel off disk, at most every `BOOST_REREAD`.
    ///
    /// It waits for the fleet itself because the channel is never configured: the fleet carries the
    /// id, the reference table names it and that name is the log file's own prefix. A fleet with no
    /// boost channel, or one whose log has not reached this machine, reads nothing and says so.
    #[cfg(feature = "fleet")]
    fn fleet_read_boosts(&mut self) {
        // A screenshot must never read the machine's real chat logs.
        if self.headless {
            return;
        }
        let now = std::time::Instant::now();
        if self.fleet_boosts_read.is_some_and(|t| now.duration_since(t) < BOOST_REREAD) {
            return;
        }
        // Anything that means the scan will never happen has to end the wait, or the pane sits on
        // "reading" for the life of the page.
        let give_up = |app: &Self| {
            app.fleet.lock().unwrap_or_else(|e| e.into_inner()).boosts_loading = false;
        };
        let Some(dir) = crate::logpaths::chat_logs_dir(&self.settings.eve_logs_dir) else {
            give_up(self);
            return;
        };
        // Read before the fleet lock: both take their own, and the jabber one is held by workers.
        let pings = self.fleet_ping_history();
        let cmd = {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            // Still opening. That is a wait, not a dead end, so the pane keeps reading.
            let Some(open) = st.open.value.as_ref() else { return };
            let Some(channel) =
                st.seed.channel_name(&st.seed.boost_channels, open.fleet.boost_channel_id)
            else {
                st.boosts_loading = false;
                return;
            };
            let Some(started) = crate::fleets::model::parse_iso(&open.fleet.started_at) else {
                st.boosts_loading = false;
                return;
            };
            // From the ping, not from when tracking started: charges posted before anyone was
            // called belong to whatever ran before this fleet. Falls back to the start when the
            // ping is not in the history, which is any fleet older than the chat buffer.
            let comms = st
                .seed
                .channel_name(&st.seed.mumble_channels, open.fleet.mumble_channel_id)
                .unwrap_or_default();
            let to = open.fleet.closed_at.as_deref().and_then(crate::fleets::model::parse_iso);
            let from = boost_window_start(&pings, &open.fleet.name, comms, started, to);
            // Not raising `boosts_loading` here. Opening a fleet raises it, because that is the one
            // time there is nothing to show; this runs again every `BOOST_REREAD` on the same
            // fleet, and raising it each time collapsed the coverage to "Reading…" and back every
            // twenty seconds, jumping the settings pane underneath it up and down.
            Cmd::ReadBoosts { dir, channel: channel.to_owned(), from, to }
        };
        self.fleet_boosts_read = Some(now);
        self.fleet_dispatch(cmd);
    }

    /// Asks Mumble where it is, off the UI thread and no more than once every `MUMBLE_POLL`.
    ///
    /// A session-bus round trip is fast until the bus is busy or Mumble is wedged, and a frame
    /// that waits on one is a frame that stutters.
    #[cfg(feature = "fleet")]
    fn fleet_poll_mumble(&mut self, ctx: &egui::Context) {
        if self.headless {
            return;
        }
        let now = std::time::Instant::now();
        if self.fleet_mumble_asked.is_some_and(|t| now.duration_since(t) < MUMBLE_POLL) {
            return;
        }
        self.fleet_mumble_asked = Some(now);
        let (tx, ctx) = (self.fleet_mumble_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(crate::mumble::current_url());
            ctx.request_repaint();
        });
    }

    /// The systems the formup field offers, resolved out of the app's own map rather than the
    /// dashboard's reference list, which carries only a handful.
    #[cfg(feature = "fleet")]
    fn fleet_places(&self) -> Places {
        let named = |name: &str| {
            self.systems
                .as_ref()
                .and_then(|g| g.lookup(name))
                .map(|i| (i.id, i.name.clone()))
        };
        let staging = named(&self.settings.rescue_staging_system);
        let recent: Vec<(i64, String)> = self
            .settings
            .fleet_recent_formup
            .iter()
            .filter_map(|n| named(n))
            .filter(|(id, _)| Some(*id) != staging.as_ref().map(|(i, _)| *i))
            .take(3)
            .collect();
        let query: String = self
            .ui_ctx
            .data(|d| d.get_temp(egui::Id::new("fleet_formup_query")).unwrap_or_default());
        let hits = match self.store.as_ref().filter(|_| query.trim().len() >= 2) {
            Some(store) => store
                .search_systems(query.trim(), 8)
                .into_iter()
                .map(|(id, name, _)| (id, name))
                .collect(),
            None => Vec::new(),
        };
        Places { staging, recent, hits }
    }

    /// Everything the docked chat needs, read before the state lock the pages hold.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_chat_state(&self) -> ChatDock {
        let rooms = [
            crate::app::goon_jid(
                &self.settings.rescue_skirmish_jid,
                "skirmish_commanders@conference.goonfleet.com",
            ),
            crate::app::goon_jid(
                &self.settings.rescue_delve911_jid,
                "delve911@conference.goonfleet.com",
            ),
        ];
        ChatDock {
            open: self.fleet_chat_open,
            tab: self.fleet_chat_tab,
            connected: self.jabber_conn().0,
            tails: rooms.iter().map(|j| self.jabber_room_tail(j, 80)).collect(),
            rooms,
            drafts: self.fleet_chat_draft.clone(),
            send: None,
        }
    }

    /// Puts back what the dock changed, once the lock is gone.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_chat_apply(&mut self, dock: ChatDock) {
        self.fleet_chat_open = dock.open;
        self.fleet_chat_tab = dock.tab;
        self.fleet_chat_draft = dock.drafts;
        if let Some((room, body)) = dock.send {
            if let Some(tx) = &self.jabber_tx {
                let _ = tx.send(crate::jabber::Cmd::SendRoom { room, body });
            }
        }
    }

    /// Everything that could carry this fleet's ping: what we posted to skirmish_commanders, and
    /// what directorbot broadcast.
    #[cfg(feature = "fleet")]
    fn fleet_ping_history(&self) -> Vec<(String, String, bool, i64)> {
        let room = crate::app::goon_jid(
            &self.settings.rescue_skirmish_jid,
            "skirmish_commanders@conference.goonfleet.com",
        );
        let mut out = self.jabber_room_tail(&room, 400);
        out.extend(self.jabber_room_tail(crate::jabber::PING_FEED_KEY, 400));
        out
    }

    /// Fills the reference tables once, whatever view is on screen.
    ///
    /// NOTE: this gate is load-bearing. Without it the whole `fleet` module is referenced from a
    /// build that does not compile it.
    ///
    /// Not on first render of the fleet tab: the ping window and the rescue view read the same
    /// tables, and until this has run they show the invented placeholder names instead of the
    /// alliance's own. Boot is nine reads and happens once per run.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_boot_once(&mut self) {
        if self.fleet_booted || !self.settings.fleet_enabled {
            return;
        }
        self.fleet_booted = true;
        self.fleet_dispatch(Cmd::Bootstrap);
        let page = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
        self.fleet_refresh(&page);
    }

    #[cfg(feature = "fleet")]
    fn fleet_body(&mut self, ui: &mut egui::Ui) {
        self.fleet_collect();
        // A login started from settings can land while this tab is the one on screen.
        if self.fleet_apply_login() {
            self.needs_save = true;
        }
        self.fleet_boot_once();
        self.fleet_boss_poll(ui.ctx());
        self.fleet_channel_poll(ui.ctx());

        // Cloned before the lock, because the render closures cannot borrow `self` while the state
        // is held. Deferred work goes into the slots below and is applied once it is dropped.
        let page = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
        if matches!(page, Page::Tracking(_) | Page::Historic(_)) {
            self.fleet_read_boosts();
            self.fleet_poll_mumble(ui.ctx());
        }
        self.fleet_hub_once();
        self.fleet_reopen_poll(ui.ctx());
        self.fleet_advert_poll(ui.ctx());
        let mode = self.fleet_backend.mode();
        let seed_hint = crate::fleets::seed_path_hint();
        let presets = self.settings.fleet_presets.clone();
        let boost_rules = self.settings.fleet_boost_requirements.clone();
        let fleet_hulls = self.settings.fleet_hulls.clone();
        let fleet_tanks = self.settings.fleet_doctrine_tanks.clone();
        let fleet_strict = self.settings.fleet_doctrine_strict.clone();
        let mut open_boost_editor = false;
        let mut boost_detail: Option<String> = None;
        let mut edit_snowflakes = false;
        let mut open_migrate = false;
        let comms_targets = self.fleet_comms_targets();
        // Read before the state lock the pages hold, put back once it is gone.
        let mut chat = self.fleet_chat_state();
        let mut sidebar_open = self.fleet_sidebar_open;
        let here = self.fleet_mumble_at.clone();
        let mine = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let me = st.session.as_ref().map(|s| s.character_name.trim().to_lowercase());
            st.open
                .value
                .as_ref()
                .and_then(|o| o.fleet.commander.as_ref())
                .zip(me)
                .is_some_and(|(c, me)| c.label.trim().to_lowercase() == me)
        };
        let mut join_comms: Option<crate::fleets::comms::Links> = None;
        let places = self.fleet_places();
        let journal_open = self.fleet_journal_open;
        let detail_tab = self.fleet_detail_tab;
        let mut toggle_journal = false;
        let quick_open = self.fleet_quick_open;
        let mut toggle_quick = false;
        let mut set_tab: Option<DetailTab> = None;
        let mut act_on: Vec<Action> = Vec::new();
        let mut goto: Option<Page> = None;
        let mut cmd: Option<Cmd> = None;
        let mut refresh = false;
        let mut act = FormAct::default();
        // Before the lock below: rendering the fallback takes it too.
        let local_ping = self.fleet_local_ping();
        let skirmish_jid = crate::app::goon_jid(
            &self.settings.rescue_skirmish_jid,
            "skirmish_commanders@conference.goonfleet.com",
        );
        let can_jabber = self.jabber_conn().0 && !skirmish_jid.is_empty();

        egui::Panel::top("fleet_subnav").show_inside(ui, |ui| {
            ui.add_space(4.0);
            // Narrow, the status group drops below the tabs instead of landing on top of them.
            let roomy = ui.available_width() >= 900.0;
            ui.horizontal_wrapped(|ui| {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                let tab = page.tab();
                for (p, label) in [(Page::Fleets, "Fleets"), (Page::Start, "Start fleet")] {
                    if selectable_chip(ui, tab == p, label).clicked() && page != p {
                        goto = Some(p);
                    }
                }
                if selectable_chip(
                    ui,
                    quick_open,
                    format!("{}  Quick Fleet", egui_phosphor::regular::LIGHTNING),
                )
                .on_hover_text("Start from a saved preset")
                .clicked()
                {
                    toggle_quick = true;
                }
                if let Some(id) = page.fleet() {
                    ui.label(egui_phosphor::regular::CARET_RIGHT);
                    let name = st.open.value.as_ref().map(|o| o.fleet.name.clone());
                    ui.label(
                        egui::RichText::new(name.unwrap_or_else(|| id.short().to_owned())).strong(),
                    );
                }
                if !roomy {
                    ui.end_row();
                }
                let layout = if roomy {
                    egui::Layout::right_to_left(egui::Align::Center)
                } else {
                    egui::Layout::left_to_right(egui::Align::Center)
                };
                ui.with_layout(layout, |ui| {
                    if let Some((chip, why)) = mode_banner(mode) {
                        ui.label(
                            egui::RichText::new(chip)
                                .color(crate::theme::standing::WARNING)
                                .strong(),
                        )
                        .on_hover_text(why);
                    }
                    if st.seed.placeholder {
                        ui.label(egui::RichText::new("placeholder data").weak()).on_hover_text(
                            format!("Invented names. Drop a seed file at {seed_hint}."),
                        );
                    } else if st.seed.empty() {
                        // Empty tables and no invented ones to hide behind: say so, because every
                        // dropdown below this is about to be blank.
                        ui.label(
                            egui::RichText::new("no reference data")
                                .color(crate::theme::standing::WARNING),
                        )
                        .on_hover_text(format!(
                            "Setups, channels and tags have not loaded. Sign in on the settings \
                             page, or drop a seed file at {seed_hint}."
                        ));
                    }
                    if let Some(s) = &st.session {
                        ui.label(egui::RichText::new(s.identity.name.clone()).weak())
                            .on_hover_text(format!("Command group: {}", s.identity.command_group));
                    }
                    let n = st.journal.len();
                    if selectable_chip(ui, journal_open, format!("{n} recorded"))
                        .on_hover_text(journal_hint(mode))
                        .clicked()
                    {
                        toggle_journal = true;
                    }
                    // The lock is dropped at the end of the closure, before the deferred work runs.
                    let _ = &mut st;
                });
            });
            ui.add_space(4.0);
        });

        if journal_open {
            egui::Panel::right("fleet_journal")
                .resizable(true)
                .default_size(420.0)
                .size_range(280.0..=720.0)
                .show_inside(ui, |ui| {
                    let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                    journal_pane(ui, &st);
                });
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(e) = st.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(crate::theme::standing::HOSTILE, e);
                    if ui.small_button(egui_phosphor::regular::X).clicked() {
                        st.error = None;
                    }
                });
            }
            match &page {
                Page::Fleets => fleets_page(ui, &mut st, &mut goto, &mut cmd, &mut refresh),
                Page::Start => start_page(
                    ui,
                    &mut st,
                    &presets,
                    &places,
                    &mut act,
                    &local_ping,
                    can_jabber,
                    mode,
                    &mut chat,
                ),
                Page::Tracking(_) => {
                    tracking_page(ui, &mut st, false, detail_tab, &mut set_tab, &mut act_on, &boost_rules, &fleet_hulls, &fleet_tanks, &fleet_strict, &mut open_boost_editor, &mut sidebar_open, mine, here.clone(), &mut join_comms, &mut boost_detail, &mut edit_snowflakes, &mut open_migrate, &comms_targets, &mut chat)
                }
                Page::Historic(_) => {
                    tracking_page(ui, &mut st, true, detail_tab, &mut set_tab, &mut act_on, &boost_rules, &fleet_hulls, &fleet_tanks, &fleet_strict, &mut open_boost_editor, &mut sidebar_open, mine, here.clone(), &mut join_comms, &mut boost_detail, &mut edit_snowflakes, &mut open_migrate, &comms_targets, &mut chat)
                }
            }
        });

        if let Some(p) = goto {
            // A new page means results for the old one are no longer wanted.
            self.fleet_gen.page += 1;
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page = p.clone();
            self.fleet_refresh_fresh(&p);
        } else if refresh {
            self.fleet_refresh(&page);
        }
        if let Some(c) = cmd {
            self.fleet_dispatch(c);
        }
        if toggle_journal {
            self.fleet_journal_open = !self.fleet_journal_open;
        }
        if let Some(t) = set_tab {
            self.fleet_detail_tab = t;
        }
        for action in act_on {
            if let Some(id) = page.fleet().cloned() {
                // Anything that cannot be taken back asks first. The rest is one click.
                if let Some(question) = confirm_question(&action) {
                    let pilots = self
                        .fleet
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .open
                        .value
                        .as_ref()
                        .map(|o| o.composition.total())
                        .unwrap_or(0);
                    self.fleet_confirm = Some((id, action, question, pilots));
                } else {
                    self.fleet_dispatch(Cmd::Act(id, action));
                }
            }
        }
        if toggle_quick {
            self.fleet_quick_open = !self.fleet_quick_open;
        }
        if open_boost_editor {
            self.fleet_boost_editor = true;
        }
        if edit_snowflakes {
            self.fleet_snowflakes_open = Some(SnowflakeTarget::Fleet);
        }
        if open_migrate {
            self.fleet_migrate_open = true;
        }
        if let Some(what) = boost_detail {
            self.fleet_boost_detail = Some(what);
        }
        self.fleet_chat_apply(chat);
        self.fleet_sidebar_open = sidebar_open;
        if let Some(links) = join_comms {
            self.comms_join(links);
        }
        self.quick_fleet_window(ui.ctx(), &presets, &mut act);
        self.fleet_confirm_modal(ui.ctx());
        self.fleet_apply_form(act);
    }

    /// Applies what the start form asked for once the state lock is gone.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_apply_form(&mut self, mut act: FormAct) {
        if let Some(id) = act.open_fleet.take() {
            let page = Page::Tracking(id);
            self.fleet_gen.page += 1;
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page = page.clone();
            self.fleet_refresh_fresh(&page);
        }
        if let Some(i) = act.load_preset {
            if let Some(p) = self.settings.fleet_presets.get(i).cloned() {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                st.apply_preset(&p);
                st.preview.begin();
            }
            self.fleet_preview_now();
        }
        if let Some(i) = act.delete_preset {
            if i < self.settings.fleet_presets.len() {
                self.settings.fleet_presets.remove(i);
                self.needs_save = true;
            }
        }
        if let Some((label, folder)) = act.save_preset {
            let preset = self
                .fleet
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .preset_from_form(&label, &folder);
            // By folder and name together: the same name in another folder is another preset, so a
            // save never lifts one out of the folder it lives in.
            let key = preset.key();
            match self.settings.fleet_presets.iter_mut().find(|p| p.key() == key) {
                Some(slot) => *slot = preset,
                None => self.settings.fleet_presets.push(preset),
            }
            self.needs_save = true;
        }
        if act.auto_channels {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).auto_channels();
            self.fleet_preview_now();
        }
        if act.free_channels {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).free_channels();
            self.fleet_preview_now();
        }
        if act.force_channels {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).force_configured();
            self.fleet_preview_now();
        }
        if let Some(i) = act.quick_preset {
            let presets = self.settings.fleet_presets.clone();
            if let Some(p) = presets.get(i) {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                st.apply_preset(p);
                st.page = Page::Start;
            }
            self.fleet_quick_open = false;
            self.fleet_preview_now();
            // A quick fleet skips reading the form, so whether the FC can actually track it is
            // the one thing worth knowing before the button is pressed.
            act.check_boss = true;
        }
        if let Some(name) = act.search_character.clone() {
            if name.len() >= 3 {
                self.fleet.lock().unwrap_or_else(|e| e.into_inner()).found_characters.begin();
                self.fleet_dispatch(Cmd::Search {
                    kind: crate::fleets::backend::SearchKind::Character,
                    value: name,
                });
            }
        }
        // Remembered only when it is not the staging system: the button back to staging is
        // always there, so keeping it in the recents would waste one of three slots.
        {
            let chosen = self
                .fleet
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .draft
                .formup
                .as_ref()
                .map(|l| l.label.clone());
            if let Some(name) = chosen {
                let staging = self.settings.rescue_staging_system.trim().to_owned();
                let recents = &mut self.settings.fleet_recent_formup;
                if !name.eq_ignore_ascii_case(&staging)
                    && recents.first().map(|s| s.as_str()) != Some(name.as_str())
                {
                    recents.retain(|s| !s.eq_ignore_ascii_case(&name));
                    recents.insert(0, name);
                    recents.truncate(3);
                    self.needs_save = true;
                }
            }
        }
        if act.check_boss && self.fleet_boss_may_ask() {
            let (who, use_backup) = {
                let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                (st.fc(), st.draft.use_backup)
            };
            if let Some((character_id, _)) = who {
                self.fleet_dispatch(Cmd::CheckBoss { character_id, use_backup });
            }
        }
        if act.edited {
            // The site re-renders its preview on every change. Debounced, so typing a fleet name
            // does not spawn a worker per keystroke.
            self.fleet_preview_at = Some(std::time::Instant::now() + PREVIEW_DEBOUNCE);
        }
        if let Some(when) = self.fleet_preview_at {
            if std::time::Instant::now() >= when {
                self.fleet_preview_at = None;
                self.fleet_preview_now();
            } else {
                self.ui_ctx.request_repaint_after(PREVIEW_DEBOUNCE);
            }
        }
        if act.start {
            // Checked here as well as on the button: a click that lands in the frame the state
            // changed would otherwise get through.
            let req = {
                let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                if st.starting || st.already_tracking().is_some() {
                    None
                } else {
                    let req = st.start_request();
                    st.starting = req.is_some();
                    req
                }
            };
            if let Some(req) = req {
                self.fleet_dispatch(Cmd::Start(req));
            }
        }
        if let Some(group) = act.jabber_ping {
            self.fleet_send_ping(group);
        }
        if let Some(text) = act.boss_detail.take() {
            self.fleet_boss_detail = Some(text);
        }
        if act.open_snowflakes {
            self.fleet_snowflakes_open = Some(SnowflakeTarget::Draft);
        }
        // A drop on a row is also a drop on the folder under it; the row is the more precise
        // answer, so it wins and the folder drop is dropped.
        if let Some((src, dst)) = act.reorder_preset.take() {
            act.move_preset = None;
            self.fleet_preset_reorder(src, dst);
        }
        if let Some((dragged, before)) = act.reorder_folder.take() {
            reorder_folder(&mut self.settings.fleet_presets, &dragged, &before);
            self.needs_save = true;
        }
        if let Some((i, folder)) = act.move_preset.take() {
            let label = self.settings.fleet_presets.get(i).map(|p| p.label.clone());
            if let Some(label) = label {
                self.fleet_preset_relabel(i, &label, &folder);
            }
        }
        if let Some(i) = act.rename_preset {
            if let Some(p) = self.settings.fleet_presets.get(i) {
                self.fleet_preset_rename = Some((i, p.label.clone(), p.folder.clone()));
            }
        }
    }

    /// Whether enough time has passed to ask the dashboard again whether the FC is fleet boss.
    #[cfg(feature = "fleet")]
    fn fleet_boss_may_ask(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self.fleet_boss_asked.is_some_and(|t| now.duration_since(t) < BOSS_RECHECK) {
            return false;
        }
        self.fleet_boss_asked = Some(now);
        true
    }

    /// Re-asks on a timer while the start form is up.
    ///
    /// Whether a character is boss of a fleet changes in game without the app being told, so an
    /// answer from five minutes ago is not an answer. A manual refresh restarts the clock, so
    /// asking by hand does not put a second request right behind it.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_boss_poll(&mut self, ctx: &egui::Context) {
        let who = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).fc();
        let Some((character_id, _)) = who else { return };
        let due = self
            .fleet_boss_asked
            .is_none_or(|t| std::time::Instant::now().duration_since(t) >= BOSS_POLL);
        if !due {
            ctx.request_repaint_after(BOSS_POLL);
            return;
        }
        let use_backup =
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).draft.use_backup;
        self.fleet_boss_asked = Some(std::time::Instant::now());
        self.fleet_dispatch(Cmd::CheckBoss { character_id, use_backup });
        ctx.request_repaint_after(BOSS_POLL);
    }

    /// Re-reads the comms tables while either view that picks a channel is on screen.
    ///
    /// Someone else takes an op channel without this app hearing about it, so the `isInUse` flags
    /// boot() read are only good for a minute. The clock is the time of the last read, not the
    /// time the view opened, so a tab reopened after a long spell refreshes on its first frame
    /// rather than showing stale flags for another minute. Nothing polls while both views are
    /// closed, because nothing is reading the answer.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_channel_poll(&mut self, ctx: &egui::Context) {
        // Headless renders seed the channel list themselves; a refresh would replace it with the
        // backend's, the way the boost and advert polls are kept out for the same reason.
        if !self.fleet_booted || self.headless {
            return;
        }
        let due = self
            .fleet_channels_at
            .is_none_or(|t| std::time::Instant::now().duration_since(t) >= CHANNEL_POLL);
        if due {
            self.fleet_channels_at = Some(std::time::Instant::now());
            self.fleet_dispatch(Cmd::RefreshChannels);
        }
        ctx.request_repaint_after(CHANNEL_POLL);
    }

    /// Posts the ping to skirmish_commanders, the same way the rescue does.
    ///
    /// The dashboard's rendering when there is one, the local template when there is not, so this
    /// path never depends on being signed in.
    ///
    /// The same ping to the same group twice inside `PING_REPEAT` is a double click, not a second
    /// ping, and a ping that goes out twice is an FC's mistake broadcast to everyone.
    #[cfg(feature = "fleet")]
    fn fleet_send_ping(&mut self, group: &str) {
        let rendered = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.preview.value.as_ref().map(|p| p.ping.clone())
        };
        let body = match rendered.filter(|p| !p.trim().is_empty()) {
            Some(p) => p,
            None => self.fleet_local_ping(),
        };
        let now = std::time::Instant::now();
        let repeat = self.fleet_last_ping.as_ref().is_some_and(|(g, b, at)| {
            g == group && *b == body && now.duration_since(*at) < PING_REPEAT
        });
        if repeat {
            return;
        }
        self.fleet_last_ping = Some((group.to_owned(), body.clone(), now));
        let room = crate::app::goon_jid(
            &self.settings.rescue_skirmish_jid,
            "skirmish_commanders@conference.goonfleet.com",
        );
        if let Some(tx) = &self.jabber_tx {
            let _ = tx.send(crate::jabber::Cmd::SendRoom {
                room,
                body: crate::fleets::ping::bping(group, &body),
            });
        }
    }

    /// The ping rendered from the local template, for when the dashboard cannot render one.
    ///
    /// The same template the rescue uses: a rescue ping and a fleet ping say the same things, and
    /// an FC who cannot reach the dashboard still has to be able to call a fleet.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_local_ping(&self) -> String {
        let (setup_id, mumble_id, formup, fc) = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            (
                st.draft.form.setup_id,
                st.draft.form.mumble_channel_id,
                st.draft.formup.as_ref().map(|l| l.label.clone()),
                // The FC this fleet is being started for. The old setting named whoever it was
                // set to, even with someone else picked in the form.
                st.fc().map(|(_, name)| name).unwrap_or_else(|| self.active_character.clone()),
            )
        };
        let channel = self.fleet_channel_name(mumble_id);
        crate::fleets::ping::render(
            &self.settings.rescue_ping_template,
            &crate::fleets::ping::Vars {
                op: channel.clone(),
                doctrine: self.fleet_setup_name(setup_id),
                staging: formup
                    .unwrap_or_else(|| self.settings.rescue_staging_system.clone()),
                fc,
                // The short link, which is what the dashboard's own pings carry and what lands in
                // the right channel whatever it is called this week.
                mumble: self.comms_short_link(&channel).unwrap_or_default(),
                ..Default::default()
            },
        )
    }

    /// A mumble channel's name out of the fleet seed, for the comms line of a ping.
    #[cfg(feature = "fleet")]
    fn fleet_channel_name(&self, id: Option<crate::fleets::model::ChannelId>) -> String {
        let Some(id) = id else { return String::new() };
        let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
        st.seed
            .mumble_channels
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.trim().to_owned())
            .unwrap_or_default()
    }

    /// Keeps the `Doctrine:` line out of whatever preview last came back.
    ///
    /// It is the one piece of a ping this app cannot rebuild on its own, so the last one the
    /// dashboard rendered is what the local template falls back to.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_cache_doctrine_line(&mut self) {
        // The rescue lock is taken and dropped before the fleet one, never nested: two mutexes
        // taken in two orders is a hang.
        let want = self.rescue.lock().unwrap_or_else(|e| e.into_inner()).doctrine.clone();
        let Some(setup_id) =
            crate::settings::find_preset(&self.settings.fleet_presets, &want).map(|p| p.setup_id)
        else {
            return;
        };
        let found = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.rescue_preview.as_ref().and_then(|p| doctrine_line(&p.ping)).map(|l| (setup_id, l))
        };
        let Some((setup_id, line)) = found else { return };
        if setup_id == 0 || line.is_empty() {
            return;
        }
        let slot = self.settings.fleet_doctrine_lines.iter_mut().find(|(id, _)| *id == setup_id);
        match slot {
            Some((_, old)) if *old == line => {}
            Some((_, old)) => {
                *old = line;
                self.needs_save = true;
            }
            None => {
                self.settings.fleet_doctrine_lines.push((setup_id, line));
                self.needs_save = true;
            }
        }
    }

    /// Asks for a fresh preview, superseding any in flight.
    #[cfg(feature = "fleet")]
    fn fleet_preview_now(&mut self) {
        self.fleet_gen.preview += 1;
        let req = {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.preview.begin();
            st.ping_request()
        };
        self.fleet_dispatch(Cmd::Preview(req));
    }

    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = self.fleet_apply_login();
        ui.heading("Fleet dashboard");
        ui.label(
            egui::RichText::new(
                "Tracking, pings and composition for fleet commanders. Off by default.",
            )
            .weak(),
        );
        changed |= ui
            .checkbox(&mut self.settings.fleet_enabled, "Enable the fleet dashboard (FC only)")
            .changed();
        if !self.settings.fleet_enabled {
            return changed;
        }
        ui.add_space(4.0);
        egui::Grid::new("fleet_settings_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Staging system");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.settings.rescue_staging_system)
                        .desired_width(220.0),
                )
                .on_hover_text(
                    "Home staging. The default formup for a fleet, where a rescue measures titan \
                     range from, and what the map marks.",
                )
                .changed();
            ui.end_row();
        });
        ui.add_space(8.0);
        changed |= self.fleet_sign_in_ui(ui);
        ui.add_space(8.0);
        changed |= self.fleet_boost_rules(ui);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "Reference data comes from {}, and falls back to placeholder names when it is \
                 missing. Boost coverage is read from the EVE chat log of whichever boost channel \
                 the fleet uses, so there is nothing to set here.",
                crate::fleets::seed_path_hint()
            ))
            .weak(),
        );
        changed
    }

    /// Puts preset `src` just before preset `dst`, joining `dst`'s folder. Refused when that
    /// folder already has another preset by the same name. A folder change goes through the same
    /// path as a move, so the rescue still follows its preset.
    #[cfg(feature = "fleet")]
    fn fleet_preset_reorder(&mut self, src: usize, dst: usize) {
        let (Some(from), Some(to)) =
            (self.settings.fleet_presets.get(src), self.settings.fleet_presets.get(dst))
        else {
            return;
        };
        let (label, folder) = (from.label.clone(), to.folder.clone());
        if self.fleet_preset_taken(src, &label, &folder) {
            return;
        }
        if from.folder != folder {
            self.fleet_preset_relabel(src, &label, &folder);
        }
        reorder_preset(&mut self.settings.fleet_presets, src, dst);
        self.needs_save = true;
    }

    /// Whether another preset already has this folder and name, which is what identifies one.
    #[cfg(feature = "fleet")]
    fn fleet_preset_taken(&self, i: usize, label: &str, folder: &str) -> bool {
        let key = crate::settings::preset_key(folder, label);
        self.settings.fleet_presets.iter().enumerate().any(|(j, p)| j != i && p.key() == key)
    }

    /// Renames or moves a preset, unless another one already has that folder and name.
    ///
    /// The rescue remembers its preset by key, so it follows a preset that moves: otherwise moving
    /// the one it runs on would quietly switch it to whichever came first.
    #[cfg(feature = "fleet")]
    fn fleet_preset_relabel(&mut self, i: usize, label: &str, folder: &str) {
        if label.is_empty() || self.fleet_preset_taken(i, label, folder) {
            return;
        }
        let Some(p) = self.settings.fleet_presets.get_mut(i) else { return };
        let old = p.key();
        p.label = label.to_owned();
        p.folder = folder.to_owned();
        let new = p.key();
        if old == new {
            return;
        }
        self.needs_save = true;
        if self.settings.rescue_preset == old {
            self.settings.rescue_preset = new.clone();
        }
        let mut r = self.rescue.lock().unwrap_or_else(|e| e.into_inner());
        if r.doctrine == old {
            r.doctrine = new;
        }
    }

    /// Renames a saved fleet, or moves it to another folder without dragging.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_preset_rename_window(&mut self, ctx: &egui::Context) {
        let Some((i, mut label, mut folder)) = self.fleet_preset_rename.clone() else { return };
        let folders = preset_folders(&self.settings.fleet_presets);
        let taken_now = self.fleet_preset_taken(i, label.trim(), folder.trim());
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Rename saved fleet")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_size([360.0, 150.0])
            .show(ctx, |ui| {
                egui::Grid::new("preset_rename").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut label).desired_width(220.0));
                    ui.end_row();
                    ui.label("Folder");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut folder)
                                .hint_text("top level")
                                .desired_width(160.0),
                        );
                        egui::ComboBox::from_id_salt("preset_rename_folder")
                            .width(40.0)
                            .selected_text("")
                            .show_ui(ui, |ui| {
                                if ui.menu_label(folder.trim().is_empty(), "top level").clicked() {
                                    folder.clear();
                                }
                                for f in &folders {
                                    if ui.menu_label(folder.trim() == f, f).clicked() {
                                        folder = f.clone();
                                    }
                                }
                            });
                    });
                    ui.end_row();
                });
                ui.add_space(6.0);
                if taken_now {
                    ui.label(
                        egui::RichText::new("That folder already has a preset by that name.")
                            .color(crate::theme::standing::WARNING),
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !label.trim().is_empty() && !taken_now,
                            egui::Button::new("Save"),
                        )
                        .on_disabled_hover_text(if taken_now {
                            "That folder already has a preset by that name."
                        } else {
                            "A preset needs a name."
                        })
                        .clicked()
                    {
                        apply = true;
                    }
                    cancel |= ui.button("Cancel").clicked();
                });
            });
        let taken = self.fleet_preset_taken(i, label.trim(), folder.trim());
        if apply && !taken {
            self.fleet_preset_relabel(i, label.trim(), folder.trim());
            self.fleet_preset_rename = None;
        } else if !open || cancel {
            self.fleet_preset_rename = None;
        } else {
            self.fleet_preset_rename = Some((i, label, folder));
        }
    }

    /// Who is holding one boost, what they are flying, and what they posted to say so.
    ///
    /// The posts are the point: coverage is read out of chat, so the only way to tell a misread
    /// line from a pilot who posted the wrong charge is to see the line.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_boost_detail_window(&mut self, ctx: &egui::Context) {
        let Some(what) = self.fleet_boost_detail.clone() else { return };
        let (cover, lines, ships) = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let cover = st.boosts.iter().find(|c| c.what == what).cloned();
            let ships: std::collections::BTreeMap<String, String> = st
                .open
                .value
                .as_ref()
                .map(|o| {
                    o.composition
                        .members()
                        .map(|m| (m.name.to_lowercase(), m.ship_type_name.clone()))
                        .collect()
                })
                .unwrap_or_default();
            (cover, st.boost_lines.clone(), ships)
        };
        let mut open = true;
        let mut clear: Option<String> = None;
        let pick_id = egui::Id::new("fleet_boost_detail_pilot");
        egui::Window::new(format!("Boost: {what}"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([560.0, 380.0])
            .max_height((ctx.content_rect().height() - 80.0).max(240.0))
            .show(ctx, |ui| {
                let Some(c) = cover.as_ref() else {
                    ui.label(egui::RichText::new("Nobody is holding this one.").weak());
                    return;
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{} pilot(s)", c.pilots)).strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("{} with a mindlink", c.mindlinked)).weak(),
                    );
                    if c.generic {
                        ui.label(
                            egui::RichText::new("named by burst, not by charge")
                                .color(crate::theme::standing::WARNING),
                        );
                    }
                });
                ui.separator();
                let mut picked: String = ui.data(|d| d.get_temp(pick_id).unwrap_or_default());
                // The tree gets what is left after the posts below it, measured last frame.
                let chrome_id = ui.id().with("chrome");
                let chrome: f32 = ui.data(|d| d.get_temp(chrome_id).unwrap_or(200.0));
                let top_h = (ui.available_height() - chrome).max(70.0);
                let top = egui::ScrollArea::vertical()
                    .id_salt("boost_detail_pilots")
                    // Shrinks to the pilots it has, capped so a long list does not squeeze the
                    // posts out: two boosters should not leave half the window empty.
                    .auto_shrink([false, true])
                    .max_height(top_h)
                    .show(ui, |ui| {
                        for pilot in &c.who {
                            let ship = ships
                                .get(&pilot.to_lowercase())
                                .cloned()
                                .unwrap_or_else(|| "not in fleet".to_owned());
                            let on = picked.eq_ignore_ascii_case(pilot);
                            if ui
                                .menu_label(on, format!("{pilot}   {ship}"))
                                .on_hover_text("Show what they posted")
                                .clicked()
                            {
                                picked = if on { String::new() } else { pilot.clone() };
                            }
                        }
                    });
                ui.data_mut(|d| d.insert_temp(pick_id, picked.clone()));
                ui.separator();
                let posts: Vec<&crate::fleets::boosts::Line> = lines
                    .iter()
                    .filter(|l| {
                        picked.is_empty() || l.pilot.eq_ignore_ascii_case(&picked)
                    })
                    .collect();
                ui.label(
                    egui::RichText::new(if picked.is_empty() {
                        format!("Everything read from the channel ({})", posts.len())
                    } else {
                        format!("What {picked} posted ({})", posts.len())
                    })
                    .weak(),
                );
                let body = egui::ScrollArea::vertical()
                    .id_salt("boost_detail_posts")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for l in posts {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(crate::app::eve_time_label(
                                        l.at,
                                        chrono::Utc::now().timestamp(),
                                    ))
                                    .monospace()
                                    .weak(),
                                );
                                ui.label(egui::RichText::new(&l.pilot).strong());
                                ui.label(egui::RichText::new(&l.text).monospace());
                            });
                        }
                    });
                ui.add_space(4.0);
                if ui
                    .button(format!("{}  Mark as not covered", egui_phosphor::regular::X))
                    .on_hover_text("Drop it from coverage until someone posts again")
                    .clicked()
                {
                    clear = Some(what.clone());
                }
                let used = ui.min_rect().height();
                ui.data_mut(|d| {
                    d.insert_temp(
                        chrome_id,
                        (used - top.inner_rect.height() - body.inner_rect.height()).max(0.0),
                    )
                });
            });
        if let Some(w) = clear {
            let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.boosts_forced.insert(w, false);
            self.fleet_boost_detail = None;
        }
        if !open {
            self.fleet_boost_detail = None;
        }
    }

    /// Who gets named in the ping. Its own window, so the form does not reflow as pilots are added.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_snowflakes_window(&mut self, ctx: &egui::Context) {
        let Some(target) = self.fleet_snowflakes_open else { return };
        let mut open = true;
        let mut act = FormAct::default();
        egui::Window::new("Snowflakes")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([520.0, 320.0])
            .max_height((ctx.content_rect().height() - 80.0).max(240.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                    snowflake_rows(ui, &mut st, target, &mut act);
                });
            });
        if !open {
            self.fleet_snowflakes_open = None;
        }
        self.fleet_apply_form(act);
    }

    /// The full text of a failed fleet-boss check, which is too long for the form.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_boss_detail_window(&mut self, ctx: &egui::Context) {
        let Some(text) = self.fleet_boss_detail.clone() else { return };
        let mut open = true;
        egui::Window::new("Fleet boss check")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([460.0, 240.0])
            .max_height((ctx.content_rect().height() - 80.0).max(200.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("What the dashboard said:").weak());
                ui.add_space(4.0);
                // The scroll area is sized from what is left after the button below it, measured
                // last frame. Claiming `available_height` and then putting anything underneath
                // adds that thing's height again every frame, and the window grows for ever.
                let chrome_id = ui.id().with("chrome");
                let chrome: f32 = ui.data(|d| d.get_temp(chrome_id).unwrap_or(70.0));
                let body_h = (ui.available_height() - chrome).max(80.0);
                let body = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(body_h)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&text).monospace()).wrap(),
                        );
                    });
                ui.add_space(6.0);
                if ui.button(format!("{}  Copy", egui_phosphor::regular::COPY)).clicked() {
                    ui.ctx().copy_text(text.clone());
                }
                let used = ui.min_rect().height();
                ui.data_mut(|d| {
                    d.insert_temp(chrome_id, (used - body.inner_rect.height()).max(0.0))
                });
            });
        if !open {
            self.fleet_boss_detail = None;
        }
    }

    /// Signing in to the dashboard, and how much of what the tab does actually goes out.
    #[cfg(feature = "fleet")]
    fn fleet_sign_in_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::fleets::backend::Mode;
        use crate::fleets::login::LoginStatus;

        let mut changed = false;
        ui.label(egui::RichText::new("Sign-in").strong());
        let signed_in = crate::fleets::creds::has()
            || std::env::var(crate::fleets::COOKIE_ENV).is_ok_and(|t| !t.trim().is_empty());
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Sign in...", egui_phosphor::regular::SIGN_IN)).clicked() {
                crate::fleets::login::spawn_login(self.fleet_login.clone(), ui.ctx().clone());
            }
            if signed_in
                && ui.button(format!("{}  Sign out", egui_phosphor::regular::SIGN_OUT)).clicked()
            {
                crate::fleets::creds::forget();
                self.settings.fleet_live = false;
                self.settings.fleet_send_writes = false;
                self.fleet_set_backend(std::sync::Arc::new(
                    crate::fleets::spoof::SpoofBackend::seeded(),
                ));
                changed = true;
            }
            match crate::fleets::login::status(&self.fleet_login) {
                LoginStatus::Waiting => {
                    ui.spinner();
                    ui.label(egui::RichText::new("finish the login in the window").weak());
                }
                LoginStatus::Failed(why) => {
                    ui.label(egui::RichText::new(why).color(crate::theme::standing::HOSTILE));
                }
                _ if signed_in => {
                    ui.label(egui::RichText::new("session stored").weak());
                }
                _ => {
                    ui.label(egui::RichText::new("no session stored").weak());
                }
            }
        });

        let mode = self.fleet_backend.mode();
        ui.add_enabled_ui(signed_in, |ui| {
            if ui
                .checkbox(&mut self.settings.fleet_live, "Use the stored session")
                .on_hover_text("Off, the tab reads from the local seed file and sends nothing.")
                .changed()
            {
                changed = true;
            }
            if ui
                .checkbox(
                    &mut self.settings.fleet_send_writes,
                    "Send writes: start, ping, MOTD, invites and kicks",
                )
                .on_hover_text(
                    "Off, every action still records the request it would send. The preview is \
                     sent either way: it renders the ping and changes nothing upstream.",
                )
                .changed()
            {
                changed = true;
            }
        });
        if changed {
            self.fleet_reload_backend();
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(match mode {
                Mode::DryRun => "Dry run: the names come from the seed file and nothing is sent.",
                Mode::ReadOnly => "Live data. Writes are recorded, not sent.",
                Mode::Live => "Live. Actions go to the dashboard.",
            })
            .weak(),
        );
        changed
    }

    /// Rebuilds the backend from the current settings, after a toggle or a fresh session.
    #[cfg(feature = "fleet")]
    fn fleet_reload_backend(&mut self) {
        let next = crate::fleets::live_backend(&self.settings).unwrap_or_else(|| {
            std::sync::Arc::new(crate::fleets::spoof::SpoofBackend::seeded())
        });
        self.fleet_set_backend(next);
    }

    /// Picks up a finished login. Returns whether settings changed.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_apply_login(&mut self) -> bool {
        use crate::fleets::login::LoginStatus;
        let done = {
            let mut g = self.fleet_login.lock().unwrap_or_else(|e| e.into_inner());
            match &*g {
                LoginStatus::Done(_) => {
                    *g = LoginStatus::Idle;
                    true
                }
                _ => false,
            }
        };
        if !done {
            return false;
        }
        // A fresh session is only useful turned on, and the user just asked for it by signing in.
        self.settings.fleet_live = true;
        self.fleet_reload_backend();
        true
    }

    /// Which boosts each doctrine wants, and in what order to put them on.
    ///
    /// Its own window rather than a row in settings: there are two dozen doctrines and nine
    /// charges each, which is a page of its own however it is folded into this one.
    #[cfg(feature = "fleet")]
    fn fleet_boost_rules(&mut self, ui: &mut egui::Ui) -> bool {
        let n = self.settings.fleet_boost_requirements.len();
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "{}  Doctrines...",
                    egui_phosphor::regular::SLIDERS_HORIZONTAL
                ))
                .clicked()
            {
                self.fleet_boost_editor = true;
            }
            ui.label(
                egui::RichText::new(match n {
                    0 => "nothing set".to_owned(),
                    n => format!("{n} set"),
                })
                .weak(),
            );
        });
        false
    }

    /// Writes every doctrine's configuration to a file the user picks.
    #[cfg(feature = "fleet")]
    fn fleet_export_doctrines(&self, names: &dyn Fn(i32) -> Option<String>) -> Option<String> {
        let bundle = crate::fleets::config::export(&self.settings, names);
        let path = rfd::FileDialog::new()
            .set_file_name("doctrines.spaifleet.json")
            .add_filter("EVE Spai doctrines", &["json"])
            .save_file()?;
        Some(match std::fs::write(&path, crate::fleets::config::to_json(&bundle)) {
            Ok(()) => format!("Exported {} doctrines.", bundle.doctrines.len()),
            Err(e) => format!("Export failed: {e}"),
        })
    }

    /// Reads one back. A doctrine the file names replaces that doctrine and nothing else, so one
    /// can be shared without taking the rest of somebody's configuration with it.
    #[cfg(feature = "fleet")]
    fn fleet_import_doctrines(&mut self) -> Option<String> {
        let path = rfd::FileDialog::new()
            .add_filter("EVE Spai doctrines", &["json"])
            .pick_file()?;
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return Some(format!("Could not read it: {e}")),
        };
        let bundle = match crate::fleets::config::from_json(&text) {
            Ok(b) => b,
            Err(e) => return Some(format!("Not a doctrine file: {e}")),
        };
        let n = crate::fleets::config::import(&mut self.settings, &bundle, false);
        self.needs_save = true;
        Some(format!("Imported {} doctrines, {} hulls, {} boosts.", n.doctrines, n.hulls, n.boosts))
    }

    /// The editor window. Doctrines on the left, that doctrine's boosts on the right.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_boost_editor(&mut self, ctx: &egui::Context) -> bool {
        use crate::fleets::boosts::{self, Priority, CHARGES, COMBAT_BURSTS};
        if !self.fleet_boost_editor {
            return false;
        }
        let mut changed = false;
        let mut open = true;
        let mut add_custom: Option<String> = None;
        let mut io: Option<bool> = None;
        let status_id = egui::Id::new("fleet_doctrine_io_status");
        let status: Option<String> = ctx.data(|d| d.get_temp(status_id));
        // Every ship in the game, for the hull picker's type-ahead.
        let ships: Vec<(i64, String, String)> =
            self.store.as_ref().map(|s| s.all_ships()).unwrap_or_default();
        let mut setups = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.setups.clone();
        // Hand-added doctrines sit beside the dashboard's, with negative ids so the two id spaces
        // cannot collide however the setup list changes upstream.
        for (id, name) in &self.settings.fleet_custom_doctrines {
            setups.push(crate::fleets::model::SetupItem {
                id: crate::fleets::model::SetupId(*id),
                name: name.clone(),
                minimal_opsec_level_description: None,
                priority: 0,
                is_default: false,
            });
        }
        let (placeholder, no_data) = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            (st.seed.placeholder, st.seed.empty())
        };
        let pick_id = egui::Id::new("fleet_boost_editor_pick");
        let mut picked: i32 = ctx.data(|d| {
            d.get_temp(pick_id).unwrap_or_else(|| setups.first().map(|s| s.id.0).unwrap_or(0))
        });

        egui::Window::new("Doctrines")
            .open(&mut open)
            .default_size([780.0, 540.0])
            .min_size([560.0, 320.0])
            // A window grows to fit its content and never shrinks past it, so without a cap a
            // single frame of over-tall content is permanent.
            .max_height((ctx.content_rect().height() - 60.0).max(320.0))
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                if placeholder || no_data {
                    ui.label(
                        egui::RichText::new(if placeholder {
                            "These are placeholder doctrines. Drop a seed file in the profile \
                             directory to configure the real ones."
                        } else {
                            "No setups loaded. Sign in on the settings page, or drop a seed file \
                             in the profile directory."
                        })
                        .color(crate::theme::standing::WARNING),
                    );
                }
                let rules = &mut self.settings.fleet_boost_requirements;
                // The window sizes itself to its content, so a list that claims
                // `available_height` and then has anything below it adds that thing's height
                // again every frame: the export status did exactly that. Both columns fill the
                // height they were handed, with the chrome around their lists measured on the
                // previous frame rather than guessed at.
                let body_h = ui.available_height();
                let left_chrome_id = egui::Id::new("fleet_doctrine_left_chrome");
                let left_chrome: f32 = ui.data(|d| d.get_temp(left_chrome_id).unwrap_or(120.0));
                let left_list_h = (body_h - left_chrome).max(80.0);
                ui.horizontal_top(|ui| {
                    // Doctrines, with how many boosts each already has.
                    let left = ui.vertical(|ui| {
                        ui.set_width(240.0);
                        let search_id = ui.id().with("search");
                        let mut query: String =
                            ui.data(|d| d.get_temp(search_id).unwrap_or_default());
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut query)
                                    .hint_text("Search doctrines")
                                    .desired_width(f32::INFINITY),
                            )
                            .changed()
                        {
                            ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
                        }
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt("boost_setups")
                            .auto_shrink([false, false])
                            .max_height(left_list_h)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                for s in &setups {
                                    let name = s.name.trim();
                                    // FC Choice is the absence of a doctrine, so there is nothing
                                    // to configure for it.
                                    if !crate::fleets::doctrine::is_doctrine(s.id) {
                                        continue;
                                    }
                                    if !tag_matches(name, &query) {
                                        continue;
                                    }
                                    let n = rules.iter().filter(|r| r.setup_id == s.id.0).count();
                                    let label = if n == 0 {
                                        clip(name, 24)
                                    } else {
                                        format!("{}  ({n})", clip(name, 20))
                                    };
                                    if ui
                                        .menu_label(picked == s.id.0, label)
                                        .on_hover_text(name)
                                        .clicked()
                                    {
                                        picked = s.id.0;
                                    }
                                }
                            });
                        // A doctrine the dashboard does not list, so boosts can be set for one
                        // before it exists upstream.
                        let typed = query.trim().to_owned();
                        let known = setups.iter().any(|s| s.name.trim().eq_ignore_ascii_case(&typed));
                        if ui
                            .add_enabled(
                                !typed.is_empty() && !known,
                                egui::Button::new(format!(
                                    "{}  Add \"{}\"",
                                    egui_phosphor::regular::PLUS,
                                    clip(&typed, 14)
                                )),
                            )
                            .on_disabled_hover_text(if typed.is_empty() {
                                "Type a name above first."
                            } else {
                                "That doctrine is already listed."
                            })
                            .clicked()
                        {
                            add_custom = Some(typed);
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button(format!(
                                    "{}  Export",
                                    egui_phosphor::regular::UPLOAD_SIMPLE
                                ))
                                .on_hover_text("Save every doctrine's hulls and boosts to a file")
                                .clicked()
                            {
                                io = Some(false);
                            }
                            if ui
                                .button(format!(
                                    "{}  Import",
                                    egui_phosphor::regular::DOWNLOAD_SIMPLE
                                ))
                                .on_hover_text(
                                    "Read a file back. Doctrines it names are replaced, the rest \
                                     are left alone.",
                                )
                                .clicked()
                            {
                                io = Some(true);
                            }
                        });
                        if let Some(note) = &status {
                            ui.label(egui::RichText::new(note).weak());
                        }
                    });
                    ui.data_mut(|d| {
                        d.insert_temp(left_chrome_id, (left.response.rect.height() - left_list_h).max(0.0))
                    });
                    ui.separator();

                    ui.vertical(|ui| {
                        // The remaining width, not the content's: otherwise picking a doctrine
                        // with a long name widened the whole window.
                        ui.set_width(ui.available_width());
                        let name = setups
                            .iter()
                            .find(|s| s.id.0 == picked)
                            .map(|s| s.name.trim().to_owned())
                            .unwrap_or_else(|| "No doctrine".to_owned());
                        let mine = rules.iter().filter(|r| r.setup_id == picked).count();
                        let tab_id = egui::Id::new("fleet_doctrine_tab");
                        let mut tab: u8 = ui.data(|d| d.get_temp(tab_id).unwrap_or(0));
                        ui.horizontal(|ui| {
                            for (i, label) in
                                [(0u8, "Boosts"), (1, "Ships"), (2, "Always allowed")]
                            {
                                if selectable_chip(ui, tab == i, label).clicked() {
                                    tab = i;
                                }
                            }
                        });
                        ui.data_mut(|d| d.insert_temp(tab_id, tab));
                        ui.add_space(4.0);
                        if tab == 1 {
                            changed |= tank_row(ui, picked, &mut self.settings.fleet_doctrine_tanks);
                            changed |= doctrine_link_row(
                                ui,
                                picked,
                                &mut self.settings.fleet_doctrine_urls,
                            );
                            let strict = &mut self.settings.fleet_doctrine_strict;
                            let mut on = strict.contains(&picked);
                            if ui
                                .checkbox(&mut on, "Only these hulls")
                                .on_hover_text(
                                    "Nothing is waved through, not even the always-allowed ones. \
                                     For a fleet restricted enough that a bridging titan parked \
                                     in it is still the wrong ship.",
                                )
                                .changed()
                            {
                                strict.retain(|id| *id != picked);
                                if on {
                                    strict.push(picked);
                                }
                                changed = true;
                            }
                            ui.add_space(6.0);
                            changed |= hull_editor(
                                ui,
                                picked,
                                &name,
                                "flown by this doctrine",
                                &mut self.settings.fleet_hulls,
                                &ships,
                                body_h,
                            );
                            return;
                        }
                        if tab == 2 {
                            changed |=
                                always_allowed(ui, &mut self.settings.fleet_hulls, &ships, body_h);
                            return;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(clip(&name, 26)).strong(),
                            )
                            .on_hover_text(&name);
                            let armor = boosts::looks_like_armor(&name);
                            if ui
                                .add_enabled(
                                    mine == 0,
                                    egui::Button::new(format!(
                                        "{}  Fill ({})",
                                        egui_phosphor::regular::PLUS,
                                        if armor { "armor" } else { "shield" }
                                    )),
                                )
                                .on_disabled_hover_text("This doctrine already has boosts set.")
                                .on_hover_text(
                                    "Add every relevant charge at Medium, guessing the tank from \
                                     the name.",
                                )
                                .clicked()
                            {
                                rules.extend(boosts::default_rules(picked.into(), armor));
                                changed = true;
                            }
                            if ui
                                .add_enabled(
                                    mine == 0,
                                    egui::Button::new(if armor { "Fill (shield)" } else { "Fill (armor)" }),
                                )
                                .on_disabled_hover_text("This doctrine already has boosts set.")
                                .clicked()
                            {
                                rules.extend(boosts::default_rules(picked.into(), !armor));
                                changed = true;
                            }
                            let sources: Vec<(i32, String)> = setups
                                .iter()
                                .filter(|s| s.id.0 != picked)
                                .filter(|s| rules.iter().any(|r| r.setup_id == s.id.0))
                                .map(|s| (s.id.0, s.name.trim().to_owned()))
                                .collect();
                            let mut copy_from: Option<i32> = None;
                            ui.add_enabled_ui(mine == 0 && !sources.is_empty(), |ui| {
                                egui::ComboBox::from_id_salt("boost_copy_from")
                                    .selected_text(format!(
                                        "{}  Copy from",
                                        egui_phosphor::regular::COPY
                                    ))
                                    .width(150.0)
                                    .show_ui(ui, |ui| {
                                        for (id, name) in &sources {
                                            let n = rules
                                                .iter()
                                                .filter(|r| r.setup_id == *id)
                                                .count();
                                            if ui
                                                .menu_label(false, format!("{name}  ({n})"))
                                                .clicked()
                                            {
                                                copy_from = Some(*id);
                                            }
                                        }
                                    });
                            })
                            .response
                            .on_disabled_hover_text(if mine > 0 {
                                "This doctrine already has boosts set."
                            } else {
                                "No other doctrine has boosts set."
                            });
                            if let Some(from) = copy_from {
                                let copied: Vec<_> = rules
                                    .iter()
                                    .filter(|r| r.setup_id == from)
                                    .map(|r| crate::settings::FleetBoostRequirement {
                                        setup_id: picked,
                                        ..r.clone()
                                    })
                                    .collect();
                                rules.extend(copied);
                                changed = true;
                            }
                            if ui
                                .add_enabled(mine > 0, egui::Button::new("Clear"))
                                .on_disabled_hover_text("Nothing to clear.")
                                .clicked()
                            {
                                rules.retain(|r| r.setup_id != picked);
                                changed = true;
                            }
                        });
                        ui.add_space(4.0);

                        let mut remove: Option<usize> = None;
                        // Shield, armor, information, skirmish, each block together, so a glance
                        // says what is covered.
                        let mut order: Vec<usize> = rules
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| r.setup_id == picked)
                            .map(|(i, _)| i)
                            .collect();
                        order.sort_by_key(|i| {
                            let r = &rules[*i];
                            (
                                boosts::burst_of(&r.charge).map(|b| b as u8).unwrap_or(u8::MAX),
                                r.charge.clone(),
                            )
                        });
                        egui::ScrollArea::vertical().id_salt("boost_rules").show(ui, |ui| {
                            egui::Grid::new("boost_rule_rows")
                                .num_columns(3)
                                .striped(true)
                                .spacing([10.0, 4.0])
                                .show(ui, |ui| {
                                    for i in order {
                                        let rule = &mut rules[i];
                                        cell(ui, 220.0, |ui| {
                                            let here = boosts::burst_of(&rule.charge);
                                            let text = egui::RichText::new(rule.charge.clone())
                                                .color(match here {
                                                    Some(b) => burst_colour(ui, b),
                                                    None => ui.visuals().weak_text_color(),
                                                });
                                            egui::ComboBox::from_id_salt(("boost_charge", i))
                                                .selected_text(text)
                                                .width(210.0)
                                                .show_ui(ui, |ui| {
                                                    for (n, b) in CHARGES
                                                        .iter()
                                                        .filter(|(_, b)| COMBAT_BURSTS.contains(b))
                                                    {
                                                        changed |= ui
                                                            .menu_value(
                                                                &mut rule.charge,
                                                                (*n).to_owned(),
                                                                egui::RichText::new(*n)
                                                                    .color(burst_colour(ui, *b)),
                                                            )
                                                            .changed();
                                                    }
                                                });
                                        });
                                        cell(ui, 120.0, |ui| {
                                            let mut prio = Priority::parse(&rule.priority);
                                            egui::ComboBox::from_id_salt(("boost_prio", i))
                                                .selected_text(prio.label())
                                                .width(110.0)
                                                .show_ui(ui, |ui| {
                                                    for p in Priority::ALL {
                                                        if ui
                                                            .menu_value(
                                                                &mut prio,
                                                                p,
                                                                p.label(),
                                                            )
                                                            .changed()
                                                        {
                                                            rule.priority =
                                                                p.as_str().to_owned();
                                                            changed = true;
                                                        }
                                                    }
                                                });
                                        });
                                        if ui
                                            .button(egui_phosphor::regular::TRASH)
                                            .on_hover_text("Remove this boost")
                                            .clicked()
                                        {
                                            remove = Some(i);
                                        }
                                        ui.end_row();
                                    }
                                });
                            if mine == 0 {
                                ui.label(
                                    egui::RichText::new("No boosts set for this doctrine.").weak(),
                                );
                            }
                        });
                        if let Some(i) = remove {
                            rules.remove(i);
                            changed = true;
                        }
                        ui.add_space(4.0);
                        if ui
                            .button(format!("{}  Add a boost", egui_phosphor::regular::PLUS))
                            .clicked()
                        {
                            rules.push(crate::settings::FleetBoostRequirement {
                                setup_id: picked,
                                charge: boosts::CHARGES[0].0.to_owned(),
                                priority: Priority::Medium.as_str().to_owned(),
                            });
                            changed = true;
                        }
                    });
                });
            });

        if let Some(importing) = io {
            let names = {
                let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                let setups = st.seed.setups.clone();
                move |id: i32| {
                    setups.iter().find(|s| s.id.0 == id).map(|s| s.name.trim().to_owned())
                }
            };
            let note = if importing {
                self.fleet_import_doctrines()
            } else {
                self.fleet_export_doctrines(&names)
            };
            if let Some(note) = note {
                ctx.data_mut(|d| d.insert_temp(status_id, note));
                changed = true;
            }
        }
        if let Some(name) = add_custom {
            let next = self
                .settings
                .fleet_custom_doctrines
                .iter()
                .map(|(id, _)| *id)
                .min()
                .unwrap_or(0)
                - 1;
            self.settings.fleet_custom_doctrines.push((next, name));
            picked = next;
            changed = true;
        }
        ctx.data_mut(|d| d.insert_temp(pick_id, picked));
        if !open {
            self.fleet_boost_editor = false;
        }
        changed
    }

    #[cfg(not(feature = "fleet"))]
    pub(crate) fn fleet_boost_editor(&mut self, _ctx: &egui::Context) -> bool {
        false
    }

}

/// Active fleets by kind, then the FC's own history.
#[cfg(feature = "fleet")]
fn fleets_page(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    goto: &mut Option<Page>,
    cmd: &mut Option<Cmd>,
    refresh: &mut bool,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Refresh", egui_phosphor::regular::ARROWS_CLOCKWISE)).clicked() {
            *refresh = true;
        }
        let can_start = st.can(Perm::StartFleet);
        if ui
            .add_enabled(
                can_start,
                egui::Button::new(format!(
                    "{}  Start a fleet",
                    egui_phosphor::regular::ROCKET_LAUNCH
                )),
            )
            .on_disabled_hover_text("Your account does not have the startFleet permission.")
            .clicked()
        {
            *goto = Some(Page::Start);
        }
    });
    ui.separator();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let strat = st.active_strat.clone();
        let pct = st.active_pct.clone();
        section(ui, "Strategic", &strat, goto);
        ui.add_space(8.0);
        section(ui, "Peacetime", &pct, goto);
        ui.add_space(8.0);

        let history = st.history.clone();
        let page_size = crate::fleets::backend::calls::HISTORY_PAGE;
        // Server side, so finding a fleet from last year is one request rather than paging back
        // through every month between here and it.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Find a fleet").strong());
            let mut q = st.history_search.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut q)
                    .hint_text("name, FC or doctrine")
                    .desired_width(240.0),
            );
            if resp.changed() {
                st.history_search = q;
                st.history_skip = 0;
                *cmd = Some(Cmd::LoadHistory {
                    skip: 0,
                    search: st.history_search.clone(),
                });
            }
            if !st.history_search.trim().is_empty()
                && ui.button(egui_phosphor::regular::X).on_hover_text("Clear").clicked()
            {
                st.history_search.clear();
                st.history_skip = 0;
                *cmd = Some(Cmd::LoadHistory { skip: 0, search: String::new() });
            }
        });
        ui.add_space(4.0);
        let heading = match history.value.as_ref() {
            Some(p) if p.total > 0 => format!(
                "{}   {}-{} of {}",
                if st.history_search.trim().is_empty() { "My history" } else { "Matches" },
                st.history_skip + 1,
                st.history_skip + p.items.len() as u32,
                p.total
            ),
            Some(_) if !st.history_search.trim().is_empty() => "No fleet matches".to_owned(),
            _ => "My history".to_owned(),
        };
        rows(ui, &heading, history.value.as_ref().map(|p| p.items.as_slice()), &history, goto);
        if let Some(p) = history.value.as_ref() {
            let more = (st.history_skip + page_size) < p.total as u32;
            if st.history_skip > 0 || more {
                ui.horizontal(|ui| {
                    if ui.add_enabled(st.history_skip > 0, egui::Button::new("Newer")).clicked() {
                        st.history_skip = st.history_skip.saturating_sub(page_size);
                        *cmd = Some(Cmd::LoadHistory {
                            skip: st.history_skip,
                            search: st.history_search.clone(),
                        });
                    }
                    if ui.add_enabled(more, egui::Button::new("Older")).clicked() {
                        st.history_skip += page_size;
                        *cmd = Some(Cmd::LoadHistory {
                            skip: st.history_skip,
                            search: st.history_search.clone(),
                        });
                    }
                });
            }
        }
    });
}

/// One list of active fleets.
#[cfg(feature = "fleet")]
fn section(ui: &mut egui::Ui, title: &str, slot: &Slot<Vec<FleetRow>>, goto: &mut Option<Page>) {
    let count = slot.value.as_ref().map(Vec::len).unwrap_or(0);
    let heading = if count > 0 { format!("{title}   {count}") } else { title.to_owned() };
    rows(ui, &heading, slot.value.as_deref(), slot, goto);
}

/// A heading and its rows, with whatever the slot has to say about how fresh they are.
#[cfg(feature = "fleet")]
fn rows<T>(
    ui: &mut egui::Ui,
    heading: &str,
    items: Option<&[FleetRow]>,
    slot: &Slot<T>,
    goto: &mut Option<Page>,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(heading).strong());
        if slot.loading {
            ui.label(egui::RichText::new("loading").weak());
        }
        if slot.stale {
            ui.label(egui::RichText::new("stale").color(crate::theme::standing::WARNING))
                .on_hover_text("The last refresh failed, so this is the previous answer.");
        }
    });
    let Some(items) = items else { return };
    if items.is_empty() {
        ui.label(egui::RichText::new("Nothing here.").weak());
        return;
    }
    let now = chrono::Utc::now().timestamp();
    for row in items {
        if fleet_row(ui, row, now) {
            *goto = Some(if row.closed_at.is_some() {
                Page::Historic(row.id.clone())
            } else {
                Page::Tracking(row.id.clone())
            });
        }
    }
}

/// One fleet, as a clickable card. Returns whether it was clicked.
#[cfg(feature = "fleet")]
fn fleet_row(ui: &mut egui::Ui, row: &FleetRow, now: i64) -> bool {
    let resp = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&row.name).strong());
                if let Some(s) = &row.setup_name {
                    ui.label(egui::RichText::new(s.trim()).weak());
                }
                for t in &row.tags {
                    fleet_tag_chip(ui, t);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let age = crate::fleets::model::parse_iso(&row.started_at)
                        .map(|t| fmt_age_compact(now - t))
                        .unwrap_or_default();
                    ui.label(egui::RichText::new(age).weak());
                    if let Some(c) = row.commander.as_ref().or(row.started_by.as_ref()) {
                        ui.label(egui::RichText::new(c).weak());
                    }
                });
            });
        })
        .response
        .interact(egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// A tag in its own colour, mapping the server's class names onto the app's palette.
#[cfg(feature = "fleet")]
fn fleet_tag_chip(ui: &mut egui::Ui, tag: &TagItem) {
    ui.label(egui::RichText::new(tag.name.trim()).color(tag_colour(ui, tag)));
}

/// A tag's colour. The two that say what kind of fleet it is are fixed, because the seed carries
/// no colour class for them and grey is the wrong answer for both.
#[cfg(feature = "fleet")]
fn tag_colour(ui: &egui::Ui, tag: &TagItem) -> egui::Color32 {
    use crate::theme::standing;
    match tag.name.trim().to_uppercase().as_str() {
        "STRATEGIC" => return standing::HOSTILE,
        "PEACETIME" => return standing::WARNING,
        _ => {}
    }
    match tag.colour_class.as_str() {
        "red" => standing::HOSTILE,
        "green" => standing::FRIENDLY,
        "yellow" => standing::WARNING,
        "blue" => ui.visuals().hyperlink_color,
        _ => ui.visuals().weak_text_color(),
    }
}

/// What kind of fleet a preset starts, from its primary tag: S for the STRATEGIC tag, P for any
/// other primary tag, coloured the way those tags are. Nothing for a preset with no primary tag.
#[cfg(feature = "fleet")]
fn preset_kind(
    p: &crate::settings::FleetPreset,
    tags: &[TagItem],
) -> Option<(&'static str, egui::Color32, &'static str)> {
    use crate::theme::standing;
    let primary: Vec<&TagItem> =
        p.tag_ids.iter().filter_map(|id| tags.iter().find(|t| t.id.0 == *id)).filter(|t| t.is_primary).collect();
    if primary.is_empty() {
        return None;
    }
    // By name only. Other primary tags carry the dashboard's strategic flag too, and those count
    // as P until there is a reason to split them further.
    let strat = primary.iter().any(|t| t.name.trim().eq_ignore_ascii_case("STRATEGIC"));
    Some(if strat {
        ("S", standing::HOSTILE, "Strategic")
    } else {
        ("P", standing::WARNING, "Peacetime")
    })
}

/// The P or S in a fixed cell, so names line up whether or not a preset has one.
#[cfg(feature = "fleet")]
const KIND_W: f32 = 14.0;

#[cfg(feature = "fleet")]
fn preset_kind_cell(ui: &mut egui::Ui, kind: Option<(&'static str, egui::Color32, &'static str)>) {
    ui.allocate_ui_with_layout(
        egui::vec2(KIND_W, ui.spacing().interact_size.y),
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        |ui| {
            if let Some((letter, colour, what)) = kind {
                ui.add(
                    egui::Label::new(egui::RichText::new(letter).strong().color(colour))
                        .selectable(false),
                )
                .on_hover_text(what);
            }
        },
    );
}

/// How long the form waits after the last edit before rendering the ping again.
#[cfg(feature = "fleet")]
const PREVIEW_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);

/// How often Mumble is asked where it is. Someone moving channel mid-fleet is rare, and the
/// answer only drives a pulse.
#[cfg(feature = "fleet")]
const MUMBLE_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the boost channel is re-read. Boosters post once and then argue, so this is about
/// picking up a swap, not about latency.
#[cfg(feature = "fleet")]
const BOOST_REREAD: std::time::Duration = std::time::Duration::from_secs(20);
/// How long a short link that failed to resolve is left before it is fetched again.
#[cfg(feature = "fleet")]
const COMMS_RETRY: std::time::Duration = std::time::Duration::from_secs(30);
/// How long to leave the dashboard to write a freshly closed fleet's report before asking again.
#[cfg(feature = "fleet")]
const REOPEN_WAIT: std::time::Duration = std::time::Duration::from_secs(3);
/// How many times. A fleet that closed with nobody in it never fills, so this has to stop.
#[cfg(feature = "fleet")]
const REOPEN_TRIES: u8 = 6;
/// When a fleet's boost charges start counting.
///
/// A live fleet takes it from the ping, because charges posted before anyone was called belong to
/// whatever ran before it. A finished fleet has both ends recorded and its ping is long out of the
/// chat buffer, so hunting for one only ever lands on the fallback; the recorded start is the
/// better answer and costs nothing to read.
///
/// Either way the window opens `BOOST_GRACE` early, for the pilots who set their charges while the
/// FC was still making the fleet.
#[cfg(feature = "fleet")]
fn boost_window_start(
    pings: &[(String, String, bool, i64)],
    fleet_name: &str,
    comms: &str,
    started: i64,
    closed: Option<i64>,
) -> i64 {
    match closed {
        Some(_) => started - BOOST_GRACE,
        None => crate::fleets::ping::ping_time(pings, fleet_name, comms, started)
            .unwrap_or(started - BOOST_GRACE),
    }
}

/// How long before a fleet was created its boost charges still count. Pilots set them while the FC
/// is still making the fleet, so a window that opens at `startedAt` misses the ones who were ready.
#[cfg(feature = "fleet")]
const BOOST_GRACE: i64 = 5 * 60;

/// How long the same ping to the same group is treated as a double click rather than a new ping.
/// Long enough to cover an impatient re-click, short enough that a real second ping is not blocked.
#[cfg(feature = "fleet")]
const PING_REPEAT: std::time::Duration = std::time::Duration::from_secs(90);

/// The fleet-boss check is a round trip per click, so the refresh button has a floor.
#[cfg(feature = "fleet")]
const BOSS_RECHECK: std::time::Duration = std::time::Duration::from_secs(3);

/// How often the start form re-asks on its own. The answer goes stale the moment the FC forms up
/// in game, and nothing tells the app when that happens.
#[cfg(feature = "fleet")]
/// The text after `Doctrine:` on its own line, which is where the dashboard puts the hull priority
/// order. Anything it appends after that (the FC's notes) is on later lines and stays out.
#[cfg(feature = "fleet")]
fn doctrine_line(ping: &str) -> Option<String> {
    ping.lines()
        .find_map(|l| l.trim().strip_prefix("Doctrine:"))
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
}

#[cfg(feature = "fleet")]
const BOSS_POLL: std::time::Duration = std::time::Duration::from_secs(60);
#[cfg(feature = "fleet")]
const ADVERT_POLL: std::time::Duration = std::time::Duration::from_secs(60);
#[cfg(feature = "fleet")]
const CHANNEL_POLL: std::time::Duration = std::time::Duration::from_secs(60);

/// What the start form asked for, applied once the state lock is gone.
#[cfg(feature = "fleet")]
#[derive(Default)]
pub(crate) struct FormAct {
    /// (dragged preset, the preset it was dropped on): it goes just before that one.
    pub reorder_preset: Option<(usize, usize)>,
    /// (dragged folder, the folder it was dropped on): the whole folder goes just before that one.
    pub reorder_folder: Option<(String, String)>,
    /// Go to a fleet that is already being tracked, instead of starting a second one.
    pub open_fleet: Option<crate::fleets::model::FleetId>,
    pub load_preset: Option<usize>,
    pub delete_preset: Option<usize>,
    /// (label, folder) for the preset to keep. An empty folder is the top level.
    pub save_preset: Option<(String, String)>,
    pub auto_channels: bool,
    pub free_channels: bool,
    pub force_channels: bool,
    /// Ask the dashboard whether the chosen FC is boss of a fleet in game.
    pub check_boss: bool,
    /// The full text of a failed check, to open in its own window.
    pub boss_detail: Option<String>,
    /// Open the snowflake editor.
    pub open_snowflakes: bool,
    /// `(preset index, folder)` for a preset dragged into a folder. An empty folder is the top.
    pub move_preset: Option<(usize, String)>,
    /// A preset to rename, by index.
    pub rename_preset: Option<usize>,
    /// A preset the Quick Fleet picker chose, which loads the form and goes to it.
    pub quick_preset: Option<usize>,
    /// A character name to look up for the snowflake row.
    pub search_character: Option<String>,
    /// A solar system name to look up for the formup field.
    pub search_system: Option<String>,
    pub edited: bool,
    pub start: bool,
    /// Post the ping request to skirmish_commanders, as the rescue does. The directorbot group.
    pub jabber_ping: Option<&'static str>,
}

/// The start form on the left, the ping it would send on the right.
#[cfg(feature = "fleet")]
#[allow(clippy::too_many_arguments)]
fn start_page(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    presets: &[crate::settings::FleetPreset],
    places: &Places,
    act: &mut FormAct,
    local_ping: &str,
    can_jabber: bool,
    mode: crate::fleets::backend::Mode,
    chat: &mut ChatDock,
) {
    let can_start = st.can(Perm::StartFleet);
    egui::Panel::right("fleet_preview")
        .resizable(true)
        .default_size(300.0)
        .size_range(250.0..=560.0)
        .show_inside(ui, |ui| side_pane(ui, st, presets, local_ping, act));
    // After the preview, so it docks to the left of it. The form keeps room for both columns,
    // which is what stops it growing tall enough to run under its own action bar.
    chat_dock(ui, chat, 2.0 * FORM_COL_W + 24.0);

    // The two buttons that do something live in their own strip rather than at the end of the
    // form: a form long enough to scroll would otherwise hide the thing it is for.
    egui::Panel::bottom("fleet_start_actions").frame(bar_frame(ui)).show_inside(ui, |ui| {
        // Said out loud rather than only on the disabled button's hover, where it is found by the
        // FC who has already clicked twice. Its own row, above the buttons: after the
        // right-to-left group below it would land under a child that has claimed the panel's
        // whole height, and grow the panel over the form.
        if let Some((id, name)) = st.already_tracking() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("This FC is already tracked as {name}."))
                        .color(crate::theme::standing::WARNING),
                );
                if ui.button("Open it").clicked() {
                    act.open_fleet = Some(id);
                }
            });
        }
        // Below this the two groups would sit on top of each other, so they stack instead.
        let roomy = ui.available_width() >= 560.0;
        ui.horizontal_wrapped(|ui| {
            save_preset_button(ui, &preset_folders(presets), act);
            // Right-aligned when there is room. Stacked below when there is not, where a
            // right-to-left run would come out back to front.
            if !roomy {
                ui.end_row();
            }
            // Wide, the group is right-aligned as one run. Narrow, it has to be free to wrap:
            // six controls do not fit on one line at 820px, and a right-to-left run would come
            // out back to front.
            let group = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
                if roomy {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| add(ui));
                } else {
                    ui.horizontal_wrapped(|ui| add(ui));
                }
            };
            {
                let note = |ui: &mut egui::Ui| {
                    if mode == crate::fleets::backend::Mode::Live {
                        return;
                    }
                    ui.label(
                        egui::RichText::new("Track fleet: not sent")
                            .color(crate::theme::standing::WARNING),
                    )
                    .on_hover_text(
                        "Tracking is recorded rather than issued until writes are on. The Ping \
                         buttons go straight to Jabber and are not affected.",
                    );
                };
                // Straight to skirmish_commanders, the way the rescue pings: it needs nothing
                // from the dashboard, so it still works with no session and while it is down.
                // Always `coord` here. Starting a fleet is a request to the coordinators; the
                // rescue tab is where a wider ping is a decision worth offering.
                let send = |ui: &mut egui::Ui, act: &mut FormAct| {
                    let resp = ui
                        .add_enabled(
                            can_jabber,
                            egui::Button::new(format!(
                                "{}  Request ping",
                                egui_phosphor::regular::PAPER_PLANE_TILT
                            )),
                        )
                        .on_hover_text(
                            "Post the ping to skirmish_commanders as !bping coord",
                        )
                        .on_disabled_hover_text("Jabber is not connected.");
                    if resp.clicked() {
                        act.jabber_ping = Some(crate::fleets::ping::COORD);
                    }
                };
                let ready = st.start_request().is_some();
                // The dashboard reads the fleet through the FC's ESI token, so tracking a
                // character who is not fleet boss produces a fleet with nothing in it.
                let boss_ok = st
                    .fc()
                    .and_then(|(id, _)| st.boss.as_ref().filter(|(who, _)| *who == id))
                    .is_some_and(|(_, c)| c.verdict().0);
                let starting = st.starting;
                let tracked = st.already_tracking();
                let track = |ui: &mut egui::Ui, act: &mut FormAct| {
                    let label = if starting { "Starting\u{2026}" } else { "Track fleet" };
                    let resp = ui
                        .add_enabled(
                            can_start && ready && boss_ok && !starting && tracked.is_none(),
                            egui::Button::new(format!(
                                "{}  {label}",
                                egui_phosphor::regular::ROCKET_LAUNCH
                            )),
                        )
                        .on_hover_text(
                            "Records the request this would send. Nothing leaves the app.",
                        );
                    let resp = if starting {
                        resp.on_disabled_hover_text("Waiting for the dashboard to answer.")
                    } else if let Some((_, name)) = &tracked {
                        resp.on_disabled_hover_text(format!(
                            "This FC is already boss of a tracked fleet: {name}. Tracking it \
                             again would split one fleet's pilots and participation across two."
                        ))
                    } else if !can_start {
                        resp.on_disabled_hover_text(
                            "Your account does not have the startFleet permission.",
                        )
                    } else if !ready {
                        resp.on_disabled_hover_text("Give the fleet a name first.")
                    } else {
                        resp.on_disabled_hover_text(
                            "That character is not the boss of a fleet in game. The dashboard \
                             reads the fleet through their token, so there would be nothing to \
                             read.",
                        )
                    };
                    if resp.clicked() {
                        act.start = true;
                    }
                };
                group(ui, &mut |ui| {
                    if roomy {
                        note(ui);
                        send(ui, act);
                        track(ui, act);
                    } else {
                        track(ui, act);
                        send(ui, act);
                        note(ui);
                    }
                });
            }
        });
    });

    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0);
            form_grid(ui, st, places, act);
            ui.add_space(6.0);
            tag_pickers(ui, st, act);
            ui.add_space(6.0);
            snowflake_summary(ui, st, act);
            ui.add_space(8.0);
        });
    });
}


/// An action strip sits against the edge of the view, so it keeps the panel's side margins and
/// trims the vertical ones to the gap the buttons already carry.
#[cfg(feature = "fleet")]
fn bar_frame(ui: &egui::Ui) -> egui::Frame {
    let f = egui::Frame::side_top_panel(ui.style());
    // More above than below: the panel draws its separator on the top edge, and flush against the
    // buttons it reads as an underline on the form rather than the top of a strip.
    egui::Frame { inner_margin: egui::Margin { top: 9, bottom: 5, ..f.inner_margin }, ..f }
}

/// Naming the current form and keeping it. Lives in the action strip so the form can scroll past
/// it without taking the controls along.
#[cfg(feature = "fleet")]
fn save_preset_button(ui: &mut egui::Ui, folders: &[String], act: &mut FormAct) {
    let id = ui.id().with("preset_name");
    let mut label: String = ui.data(|d| d.get_temp(id).unwrap_or_default());
    let folder_id = id.with("folder");
    let mut folder: String = ui.data(|d| d.get_temp(folder_id).unwrap_or_default());
    let open_id = id.with("open");
    let mut open: bool = ui.data(|d| d.get_temp(open_id).unwrap_or(false));
    if ui
        .add(egui::Button::new(format!(
            "{}  Save as preset",
            egui_phosphor::regular::BOOKMARK_SIMPLE
        )))
        .on_hover_text("Keep this form as a preset for next time.")
        .clicked()
    {
        open = !open;
    }
    if open {
        ui.add(
            egui::TextEdit::singleline(&mut label).hint_text("Preset name").desired_width(150.0),
        );
        // Typed, not picked: a new folder is made by naming it, and the existing ones are one
        // click away in the dropdown beside it.
        ui.add(
            egui::TextEdit::singleline(&mut folder)
                .hint_text("Folder (optional)")
                .desired_width(130.0),
        );
        let combo = egui::ComboBox::from_id_salt("preset_folder_pick").width(0.0);
        combo.show_ui(ui, |ui| {
            if ui.menu_label(folder.is_empty(), "Top level").clicked() {
                folder.clear();
            }
            for f in folders {
                if ui.menu_label(&folder == f, f).clicked() {
                    folder = f.clone();
                }
            }
        });
        let ready = !label.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("Save"))
            .on_disabled_hover_text("Name the preset first.")
            .clicked()
        {
            act.save_preset = Some((label.trim().to_owned(), folder.trim().to_owned()));
            label.clear();
            open = false;
        }
    }
    ui.data_mut(|d| {
        d.insert_temp(id, label);
        d.insert_temp(folder_id, folder);
        d.insert_temp(open_id, open);
    });
}

/// The folders presets are kept in, without repeats, in the order they first appear.
///
/// Not sorted: the preset list's own order is the order the FC arranged, folders included, so one
/// list holds it and nothing else has to be kept in step with it.
#[cfg(feature = "fleet")]
fn preset_folders(presets: &[crate::settings::FleetPreset]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in presets {
        let f = p.folder.trim();
        if !f.is_empty() && !out.iter().any(|x| x == f) {
            out.push(f.to_owned());
        }
    }
    out
}

/// Moves preset `src` to just before preset `dst`, in `dst`'s folder. The caller has already
/// checked the folder has no other preset by that name.
#[cfg(feature = "fleet")]
fn reorder_preset(presets: &mut Vec<crate::settings::FleetPreset>, src: usize, dst: usize) {
    if src == dst || src >= presets.len() || dst >= presets.len() {
        return;
    }
    let folder = presets[dst].folder.clone();
    let mut p = presets.remove(src);
    p.folder = folder;
    // Removing an earlier one shifts the target back by one.
    let at = if src < dst { dst - 1 } else { dst };
    presets.insert(at, p);
}

/// Moves every preset in folder `dragged` to just before the first preset in folder `before`,
/// keeping their order among themselves. That is the whole of a folder's position.
#[cfg(feature = "fleet")]
fn reorder_folder(presets: &mut Vec<crate::settings::FleetPreset>, dragged: &str, before: &str) {
    if dragged == before || dragged.is_empty() {
        return;
    }
    let (moving, mut rest): (Vec<_>, Vec<_>) =
        presets.drain(..).partition(|p| p.folder.trim() == dragged);
    let at = rest.iter().position(|p| p.folder.trim() == before).unwrap_or(rest.len());
    rest.splice(at..at, moving);
    *presets = rest;
}

/// Width every control in the form shares, so the column reads as one edge rather than a ragged
/// one.
#[cfg(feature = "fleet")]
const FIELD_W: f32 = 260.0;
/// Label gutter beside it.
#[cfg(feature = "fleet")]
const LABEL_W: f32 = 110.0;
/// What one labelled field costs across, grid spacing included.
#[cfg(feature = "fleet")]
const FORM_COL_W: f32 = LABEL_W + 8.0 + FIELD_W;

/// The fields themselves, in two columns when there is room for two.
///
/// The split is what the fleet is on the left and how it runs on the right, so a narrow window
/// stacking them still reads in a sensible order.
#[cfg(feature = "fleet")]
fn form_grid(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    places: &Places,
    act: &mut FormAct,
) {
    if ui.available_width() >= 2.0 * FORM_COL_W + 24.0 {
        // Two fixed columns hard against the left, not `ui.columns`: that divides the whole width
        // evenly, so on a wide window the two halves of one form end up a screen apart with the
        // fields stranded at the left edge of each half.
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(FORM_COL_W, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| form_identity(ui, st, places, act),
            );
            ui.add_space(24.0);
            ui.allocate_ui_with_layout(
                egui::vec2(FORM_COL_W, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| form_running(ui, st, act),
            );
        });
    } else {
        form_identity(ui, st, places, act);
        ui.add_space(6.0);
        form_running(ui, st, act);
    }
}

/// The field width a labelled row can actually afford here.
///
/// `FIELD_W` is what a field wants; in a pane narrower than a whole row it has to give way, or the
/// form draws past its panel and lands on whatever is beside it. Called from inside a grid cell,
/// where the label gutter has already been taken out of `available_width`.
#[cfg(feature = "fleet")]
fn field_w(ui: &egui::Ui) -> f32 {
    // Minus the room a combo box puts its arrow in: asking for the whole cell made those wider
    // than the cell and they ran into whatever was docked beside the form.
    (ui.available_width() - 26.0).clamp(90.0, FIELD_W)
}

/// The systems the formup field offers: the app's staging, the last few chosen instead of it, and
/// whatever the typed query turned up.
#[cfg(feature = "fleet")]
#[derive(Clone, Debug, Default)]
pub(crate) struct Places {
    pub staging: Option<(i64, String)>,
    pub recent: Vec<(i64, String)>,
    pub hits: Vec<(i64, String)>,
}

/// What the fleet is: its name, what it flies, who it is for, where it forms.
#[cfg(feature = "fleet")]
fn form_identity(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    places: &Places,
    act: &mut FormAct,
) {
    let seed = st.seed.clone();
    egui::Grid::new("fleet_form_identity")
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("FC");
            ui.vertical(|ui| {
                let signed_in = st.session.as_ref().map(|s| (s.character_id, s.character_name.clone()));
                let current = st
                    .fc()
                    .map(|(_, n)| n)
                    .or_else(|| signed_in.as_ref().map(|(_, n)| n.clone()))
                    .unwrap_or_else(|| "none".to_owned());
                let mut pick = st.draft.fc_character;
                egui::ComboBox::from_id_salt("fleet_fc")
                    .width(field_w(ui))
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if let Some((id, name)) = &signed_in {
                            ui.menu_value(&mut pick, None, format!("{name}  (signed in)"));
                            let _ = id;
                        }
                        for c in seed.characters.iter().filter(|c| !c.is_hidden) {
                            ui.menu_value(&mut pick, Some(c.id), &c.name);
                        }
                    });
                if pick != st.draft.fc_character {
                    st.draft.fc_character = pick;
                    act.check_boss = true;
                    act.edited = true;
                }
                boss_line(ui, st, act);
            });
            ui.end_row();

            ui.label("Name");
            let d = &mut st.draft;
            act.edited |= ui
                .add(egui::TextEdit::singleline(&mut d.form.name).desired_width(field_w(ui)))
                .changed();
            ui.end_row();

            ui.label("Description");
            act.edited |= ui
                .add(
                    egui::TextEdit::multiline(&mut d.form.description)
                        .desired_rows(2)
                        .desired_width(field_w(ui)),
                )
                .changed();
            ui.end_row();

            ui.label("Setup");
            let current = seed
                .setups
                .iter()
                .find(|s| s.id.0 == d.form.setup_id)
                .map(|s| s.name.trim().to_owned())
                .unwrap_or_else(|| "== Choose a setup ==".to_owned());
            egui::ComboBox::from_id_salt("fleet_setup")
                .width(field_w(ui))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    act.edited |= ui
                        .menu_value(&mut d.form.setup_id, 0, "== Choose a setup ==")
                        .changed();
                    for s in &seed.setups {
                        act.edited |= ui
                            .menu_value(&mut d.form.setup_id, s.id.0, s.name.trim())
                            .changed();
                    }
                });
            ui.end_row();

            ui.label("SIG");
            let sig = d
                .form
                .group_id
                .and_then(|g| seed.sigs.iter().find(|s| s.id == g.0 as i64))
                .map(|s| s.label.clone())
                .unwrap_or_else(|| "== None ==".to_owned());
            egui::ComboBox::from_id_salt("fleet_sig").width(field_w(ui)).selected_text(sig).show_ui(
                ui,
                |ui| {
                    act.edited |=
                        ui.menu_value(&mut d.form.group_id, None, "== None ==").changed();
                    for s in &seed.sigs {
                        let id = Some(crate::fleets::model::GroupId(s.id as i32));
                        act.edited |=
                            ui.menu_value(&mut d.form.group_id, id, &s.label).changed();
                    }
                },
            );
            let _ = d;
            ui.end_row();

            ui.label("Formup");
            act.edited |= formup_field(ui, &mut st.draft, places, act);
            ui.end_row();

            ui.label("Doctrine notes");
            let d = &mut st.draft;
            let mut notes = d.form.doctrine_notes.clone().unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut notes)
                        .hint_text("Optional, shown in the ping")
                        .desired_width(field_w(ui)),
                )
                .changed()
            {
                d.form.doctrine_notes = Some(notes).filter(|s| !s.trim().is_empty());
                act.edited = true;
            }
            ui.end_row();
        });
}

/// How it runs: comms, when it closes itself, and the switches.
#[cfg(feature = "fleet")]
fn form_running(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    let seed = st.seed.clone();
    let auto = st.draft.auto;
    let d = &mut st.draft;
    egui::Grid::new("fleet_form_running")
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            // A hand-picked channel is what "Force configured" puts back, so the choice is recorded
            // as configured and stops reading as a switch.
            if channel_row(ui, "Mumble", "fleet_mumble", &seed.mumble_channels,
                           &mut d.form.mumble_channel_id, auto.mumble) {
                d.configured.mumble = d.form.mumble_channel_id;
                d.auto.mumble = false;
                act.edited = true;
            }
            if channel_row(ui, "Logi", "fleet_logi", &seed.logi_channels,
                           &mut d.form.logi_channel_id, auto.logi) {
                d.configured.logi = d.form.logi_channel_id;
                d.auto.logi = false;
                act.edited = true;
            }
            if channel_row(ui, "Boost", "fleet_boost", &seed.boost_channels,
                           &mut d.form.boost_channel_id, auto.boost) {
                d.configured.boost = d.form.boost_channel_id;
                d.auto.boost = false;
                act.edited = true;
            }

            ui.label("Auto close");
            ui.horizontal(|ui| {
                let mut kind = d.form.auto_close_type.unwrap_or(1);
                act.edited |= selectable_chip(ui, kind == 0, "Start").clicked().then(|| {
                    kind = 0;
                }).is_some();
                act.edited |= selectable_chip(ui, kind == 1, "FC left").clicked().then(|| {
                    kind = 1;
                }).is_some();
                d.form.auto_close_type = Some(kind);
                let mut mins = d.form.auto_close_time.unwrap_or(30);
                act.edited |=
                    ui.add(egui::DragValue::new(&mut mins).range(1..=600).suffix(" min")).changed();
                d.form.auto_close_time = Some(mins);
            });
            ui.end_row();

            ui.label("Options");
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                act.edited |= ui.checkbox(&mut d.form.set_motd, "Set the fleet MOTD").changed();
                act.edited |=
                    ui.checkbox(&mut d.form.is_corporation_fleet, "Corporation fleet").changed();
                act.edited |= ui
                    .checkbox(&mut d.use_backup, "Use the backup key")
                    .on_hover_text("Tracks through the backup ESI key rather than this character.")
                    .changed();
                act.edited |= ui
                    .checkbox(
                        &mut d.form.ignore_participation_requirements,
                        "Ignore participation requirements",
                    )
                    .changed();
            });
            ui.end_row();
        });

    // Outside the grid: three buttons wrap freely at a narrow width, where a grid cell would
    // overlap the row beneath it.
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(format!("{}  Auto", egui_phosphor::regular::MAGIC_WAND))
            .on_hover_text("Keep the channels already chosen and replace only the ones that are taken.")
            .clicked()
        {
            act.auto_channels = true;
        }
        if ui
            .button(format!("{}  Pick free comms", egui_phosphor::regular::ARROWS_CLOCKWISE))
            .on_hover_text("Start again from the lowest free channel for all three.")
            .clicked()
        {
            act.free_channels = true;
        }
        let configured = st.draft.configured != crate::fleets::state::Configured::default();
        if ui
            .add_enabled(
                configured,
                egui::Button::new(format!("{}  Force configured", egui_phosphor::regular::LOCK)),
            )
            .on_disabled_hover_text("Nothing was configured to go back to.")
            .on_hover_text("Put back the channels this preset asked for, taken or not.")
            .clicked()
        {
            act.force_channels = true;
        }
    });
}

/// Whether the character the fleet would be started as is actually boss of a fleet.
///
/// Only the account's own characters can be checked, which is the same set the picker offers, so
/// there is nothing here to guard against.
#[cfg(feature = "fleet")]
fn boss_line(ui: &mut egui::Ui, st: &crate::fleets::FleetState, act: &mut FormAct) {
    let Some((id, _)) = st.fc() else { return };
    ui.horizontal(|ui| {
        match st.boss.as_ref().filter(|(who, _)| *who == id) {
            Some((_, check)) => {
                let (ok, why) = check.verdict();
                let (glyph, colour) = if ok {
                    (egui_phosphor::regular::CHECK_CIRCLE, crate::theme::standing::FRIENDLY)
                } else {
                    (egui_phosphor::regular::WARNING, crate::theme::standing::WARNING)
                };
                ui.label(egui::RichText::new(glyph).color(colour));
                match check.error() {
                    Some(full) => {
                        let resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(why).color(colour).underline(),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .sense(egui::Sense::click()),
                        );
                        if resp.on_hover_text("Show what the dashboard said").clicked() {
                            act.boss_detail = Some(full.to_owned());
                        }
                    }
                    None => {
                        ui.label(egui::RichText::new(why).color(colour));
                    }
                }
            }
            None => {
                ui.label(egui::RichText::new("Fleet boss not checked").weak());
            }
        }
        if ui
            .small_button(egui_phosphor::regular::ARROWS_CLOCKWISE)
            .on_hover_text("Ask again whether this character is boss of a fleet")
            .clicked()
        {
            act.check_boss = true;
        }
    });
}

/// Where the fleet forms up: any solar system, with the ones worth one click in front.
///
/// The field is a search box rather than a list: the dashboard takes a system id, and there are
/// five thousand of them.
#[cfg(feature = "fleet")]
fn formup_field(
    ui: &mut egui::Ui,
    draft: &mut crate::fleets::state::Draft,
    places: &Places,
    act: &mut FormAct,
) -> bool {
    use crate::fleets::model::Labelled;
    let mut changed = false;
    let query_id = egui::Id::new("fleet_formup_query");
    let mut query: String = ui.data(|d| d.get_temp(query_id).unwrap_or_default());
    let at_staging = draft
        .formup
        .as_ref()
        .zip(places.staging.as_ref())
        .is_some_and(|(l, (id, _))| l.id == *id);

    ui.horizontal(|ui| {
        // The chosen system reads as a badge rather than as text in the box, which is a search
        // box and has to stay typable.
        if let Some(l) = draft.formup.clone() {
            if ui
                .add(
                    egui::Button::new(format!("{}  {}", l.label, egui_phosphor::regular::X))
                        .fill(ui.visuals().selection.bg_fill)
                        .stroke(egui::Stroke::new(1.0, ui.visuals().selection.stroke.color)),
                )
                .on_hover_text("Clear the formup location")
                .clicked()
            {
                draft.formup = None;
                changed = true;
            }
        }
        let width = (ui.available_width() - 40.0).clamp(80.0, FIELD_W);
        let field = ui.add(
            egui::TextEdit::singleline(&mut query)
                .hint_text("Search systems")
                .desired_width(width),
        );
        if field.changed() {
            act.search_system = Some(query.trim().to_owned());
        }
        if let Some((id, name)) = places.staging.clone() {
            if ui
                .add_enabled(
                    !at_staging,
                    egui::Button::new(egui_phosphor::regular::HOUSE).frame(false),
                )
                .on_hover_text(format!("Form up at {name}, the staging set in settings"))
                .on_disabled_hover_text("Already forming up at staging.")
                .clicked()
            {
                draft.formup = Some(Labelled { id, label: name });
                changed = true;
            }
        }

        // Nothing typed yet: the staging system and the last few chosen instead of it.
        let offered: Vec<(i64, String)> = if query.trim().is_empty() {
            places.staging.iter().cloned().chain(places.recent.iter().cloned()).collect()
        } else {
            places.hits.clone()
        };
        let hov_id = egui::Id::new("fleet_formup_hover");
        let was_over: bool = ui.data(|d| d.get_temp(hov_id).unwrap_or(false));
        // Kept open while the pointer is over it, or clicking an item would take focus off the
        // field and close the popup before the click landed.
        let popup = egui::Popup::from_response(&field)
            .open(!offered.is_empty() && (field.has_focus() || was_over))
            .width(field_w(ui))
            .show(|ui| {
                for (id, name) in &offered {
                    let on = draft.formup.as_ref().is_some_and(|l| l.id == *id);
                    if ui.menu_label(on, name.as_str()).clicked() {
                        draft.formup = Some(Labelled { id: *id, label: name.clone() });
                        changed = true;
                        query.clear();
                    }
                }
            });
        let over = popup.as_ref().is_some_and(|r| r.response.contains_pointer());
        ui.data_mut(|d| d.insert_temp(hov_id, over));
    });
    ui.data_mut(|d| d.insert_temp(query_id, query));
    changed
}

/// One comms combo, marking what is taken and what was picked for you.
#[cfg(feature = "fleet")]
fn channel_row(
    ui: &mut egui::Ui,
    label: &str,
    salt: &str,
    list: &[crate::fleets::model::ChannelItem],
    slot: &mut Option<crate::fleets::model::ChannelId>,
    auto: bool,
) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        let current = slot
            .and_then(|id| list.iter().find(|c| c.id == id))
            .map(|c| c.name.trim().to_owned())
            .unwrap_or_else(|| "none".to_owned());
        egui::ComboBox::from_id_salt(salt).width(180.0).selected_text(current).show_ui(ui, |ui| {
            changed |= ui.menu_value(slot, None, "none").changed();
            for c in list {
                let text = if c.is_in_use {
                    egui::RichText::new(format!("{}  in use", c.name.trim())).weak()
                } else {
                    egui::RichText::new(c.name.trim().to_owned())
                };
                changed |= ui.menu_value(slot, Some(c.id), text).changed();
            }
        });
        if auto {
            ui.label(
                egui::RichText::new("switched").color(crate::theme::standing::WARNING),
            )
            .on_hover_text("The channel asked for was taken, so this free one was picked.");
        }
        if slot.is_some_and(|id| list.iter().any(|c| c.id == id && c.is_in_use)) {
            ui.label(
                egui::RichText::new("in use").color(crate::theme::standing::WARNING),
            )
            .on_hover_text("Another fleet has this channel.");
        }
    });
    ui.end_row();
    changed
}

/// Primary and secondary tags, as two rows of chips.
#[cfg(feature = "fleet")]
fn tag_pickers(ui: &mut egui::Ui, st: &mut crate::fleets::FleetState, act: &mut FormAct) {
    let tags = st.seed.tags.clone();
    egui::Grid::new("fleet_form_tags")
        .num_columns(2)
        .min_col_width(LABEL_W)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            for (title, salt, primary) in
                [("Primary tag", "fleet_tag_primary", true), ("Secondary tags", "fleet_tag_secondary", false)]
            {
                ui.label(title);
                let mut pool: Vec<_> =
                    tags.iter().filter(|t| t.is_primary == primary).cloned().collect();
                ui.vertical(|ui| {
                    if primary {
                        // The two that decide what kind of fleet this is come first, in that
                        // order, with their own pair of buttons above the field. Above rather
                        // than beside, or a narrow window pushes the field off the edge.
                        pool.sort_by_key(|t| (headline_rank(&t.name), t.id.0));
                        act.edited |= headline_tags(ui, &pool, &mut st.draft.tags);
                    }
                    ui.horizontal(|ui| {
                        act.edited |= tag_field(ui, salt, &pool, &mut st.draft.tags, primary);
                    });
                });
                ui.end_row();
            }
        });
}

/// The two primary tags every fleet is one of, in the order they belong in.
#[cfg(feature = "fleet")]
const HEADLINE_TAGS: [&str; 2] = ["PEACETIME", "STRATEGIC"];

/// Sort key that floats the headline tags to the top of the primary list.
#[cfg(feature = "fleet")]
fn headline_rank(name: &str) -> usize {
    HEADLINE_TAGS.iter().position(|h| h.eq_ignore_ascii_case(name.trim())).unwrap_or(HEADLINE_TAGS.len())
}

/// Peacetime or strategic, one click apart, since it is the first thing an FC sets and the thing
/// most often set wrong.
#[cfg(feature = "fleet")]
fn headline_tags(
    ui: &mut egui::Ui,
    pool: &[TagItem],
    selected: &mut std::collections::BTreeSet<crate::fleets::model::TagId>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for name in HEADLINE_TAGS {
            let Some(t) = pool.iter().find(|t| t.name.trim().eq_ignore_ascii_case(name)) else {
                continue;
            };
            let on = selected.contains(&t.id);
            let text = egui::RichText::new(t.name.trim()).color(tag_colour(ui, t)).strong();
            if selectable_chip(ui, on, text).clicked() && !on {
                // One primary tag, so picking one drops whatever else was there.
                for other in pool {
                    selected.remove(&other.id);
                }
                selected.insert(t.id);
                changed = true;
            }
        }
    });
    changed
}

/// Whether a tag survives what has been typed into the search box.
///
/// Every word has to appear somewhere in the name, so "struct bash" finds "Structure Bash" without
/// the words being adjacent or in that order.
#[cfg(feature = "fleet")]
fn tag_matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    query.split_whitespace().all(|w| name.contains(&w.to_lowercase()))
}

/// A field of tag badges that opens a searchable list.
///
/// `single` is the primary row, where picking one replaces whatever was there: the dashboard takes
/// exactly one, so the field enforces it rather than letting the request be refused later.
#[cfg(feature = "fleet")]
fn tag_field(
    ui: &mut egui::Ui,
    salt: &str,
    pool: &[TagItem],
    selected: &mut std::collections::BTreeSet<crate::fleets::model::TagId>,
    single: bool,
) -> bool {
    let mut changed = false;
    let mut remove: Option<crate::fleets::model::TagId> = None;

    // The badges sit inside a frame the size of the other controls, so the row reads as a field
    // rather than as a loose run of chips.
    let frame = egui::Frame::new()
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .corner_radius(ui.visuals().widgets.inactive.corner_radius)
        .inner_margin(egui::Margin::symmetric(6, 4));
    let inner = frame.show(ui, |ui| {
        ui.set_width(FIELD_W - 12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let chosen: Vec<&TagItem> = pool.iter().filter(|t| selected.contains(&t.id)).collect();
            if chosen.is_empty() {
                ui.label(
                    egui::RichText::new(if single { "none" } else { "none selected" }).weak(),
                );
            }
            for t in chosen {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "{}  {}",
                                t.name.trim(),
                                egui_phosphor::regular::X
                            ))
                            .color(tag_colour(ui, t)),
                        )
                        .fill(ui.visuals().selection.bg_fill)
                        .stroke(egui::Stroke::new(1.0, ui.visuals().selection.stroke.color)),
                    )
                    .on_hover_text("Remove this tag")
                    .clicked()
                {
                    remove = Some(t.id);
                }
            }
        });
    });

    let open_button = ui
        .button(egui_phosphor::regular::CARET_DOWN)
        .on_hover_text(if single { "Choose the primary tag" } else { "Choose secondary tags" });
    let _ = inner;

    // As wide as the window allows: the tags are short, so a wide popup fits three or four to a
    // row instead of one, and the whole list is visible without scrolling at all.
    let screen = ui.ctx().content_rect();
    let width = (screen.width() - 80.0).clamp(280.0, 720.0);
    let height = (screen.height() * 0.5).clamp(180.0, 420.0);

    egui::Popup::from_toggle_button_response(&open_button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(width)
        .show(|ui| {
            ui.set_min_width(width);
            let search_id = ui.id().with((salt, "search"));
            let mut query: String = ui.data(|d| d.get_temp(search_id).unwrap_or_default());
            let edit = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text("Search tags")
                    .desired_width(f32::INFINITY),
            );
            if edit.changed() {
                ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    let mut any = false;
                    ui.horizontal_wrapped(|ui| {
                        for t in pool {
                            let name = t.name.trim();
                            if !tag_matches(name, &query) {
                                continue;
                            }
                            any = true;
                            let on = selected.contains(&t.id);
                            let text = egui::RichText::new(name).color(tag_colour(ui, t));
                            if selectable_chip(ui, on, text).clicked() {
                                if on {
                                    selected.remove(&t.id);
                                } else {
                                    if single {
                                        // Exactly one primary, so the others in this pool go.
                                        for other in pool {
                                            selected.remove(&other.id);
                                        }
                                    }
                                    selected.insert(t.id);
                                }
                                changed = true;
                            }
                        }
                    });
                    if !any {
                        ui.label(egui::RichText::new("Nothing matches.").weak());
                    }
                });
        });

    if let Some(id) = remove {
        selected.remove(&id);
        changed = true;
    }
    changed
}

/// The pilots called out in the ping.
#[cfg(feature = "fleet")]
fn snowflake_summary(ui: &mut egui::Ui, st: &crate::fleets::FleetState, act: &mut FormAct) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Snowflakes").strong());
        if ui
            .button(format!("{}  Manage...", egui_phosphor::regular::USER_LIST))
            .on_hover_text("Who gets named in the ping")
            .clicked()
        {
            act.open_snowflakes = true;
        }
        match st.draft.snowflakes.len() {
            0 => {
                ui.label(egui::RichText::new("none").weak());
            }
            _ => {
                for s in &st.draft.snowflakes {
                    ui.label(
                        egui::RichText::new(format!("{} {}", s.kind.label(), s.character_name))
                            .weak(),
                    );
                }
            }
        }
    });
}

/// Which list of snowflakes is being edited: the one on the start form, or the one on a fleet that
/// already exists. Both use the same editor and must not share widget ids.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SnowflakeTarget {
    #[default]
    Draft,
    Fleet,
}

#[cfg(feature = "fleet")]
impl SnowflakeTarget {
    fn salt(self) -> &'static str {
        match self {
            SnowflakeTarget::Draft => "draft",
            SnowflakeTarget::Fleet => "fleet",
        }
    }

    fn list(self, st: &mut crate::fleets::FleetState) -> &mut Vec<crate::fleets::model::Snowflake> {
        match self {
            SnowflakeTarget::Draft => &mut st.draft.snowflakes,
            SnowflakeTarget::Fleet => &mut st.edit.snowflakes,
        }
    }
}

/// The snowflake editor, in a window rather than in the form: it grows by a row per pilot plus a
/// suggestion strip, and a form that reflows while you are filling it in is a form you lose your
/// place in.
#[cfg(feature = "fleet")]
fn snowflake_rows(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    target: SnowflakeTarget,
    act: &mut FormAct,
) {
    use crate::fleets::model::{Snowflake, SnowflakeType};
    let can = st.can(Perm::ManageFleetSnowflakes);
    let hits = st.found_characters.clone();
    let salt = target.salt();

    let mut drop: Option<usize> = None;
    if target.list(st).is_empty() {
        ui.label(egui::RichText::new("None.").weak());
    }
    for (i, s) in target.list(st).iter_mut().enumerate() {
        ui.horizontal(|ui| {
            cell(ui, 110.0, |ui| {
                egui::ComboBox::from_id_salt(("snowflake", salt, i))
                    .width(100.0)
                    .selected_text(s.kind.label())
                    .show_ui(ui, |ui| {
                        for k in SnowflakeType::ALL {
                            act.edited |= ui.menu_value(&mut s.kind, k, k.label()).changed();
                        }
                    });
            });
            cell(ui, 180.0, |ui| {
                ui.label(&s.character_name);
            });
            if ui
                .add_enabled(can, egui::Button::new(egui_phosphor::regular::TRASH).frame(false))
                .on_disabled_hover_text(
                    "Your account does not have the manageFleetSnowflakes permission.",
                )
                .on_hover_text(format!("Drop {}", s.character_name))
                .clicked()
            {
                drop = Some(i);
            }
        });
    }
    if let Some(i) = drop {
        target.list(st).remove(i);
        act.edited = true;
    }

    // Typed, searched, then added: a snowflake names a real character in the ping, so a name
    // nobody has is worse than none at all.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let name_id = egui::Id::new("fleet_snowflake_name");
        let mut name: String = ui.data(|d| d.get_temp(name_id).unwrap_or_default());
        let kind_id = egui::Id::new(("fleet_snowflake_kind", salt));
        let mut kind: SnowflakeType = ui.data(|d| d.get_temp(kind_id).unwrap_or_default());

        cell(ui, 110.0, |ui| {
            egui::ComboBox::from_id_salt(("snowflake_new_kind", salt))
                .width(100.0)
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for k in SnowflakeType::ALL {
                        ui.menu_value(&mut kind, k, k.label());
                    }
                });
        });
        let edit = ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Character name")
                .desired_width(180.0),
        );
        if edit.changed() {
            act.search_character = Some(name.trim().to_owned());
        }
        if hits.loading {
            ui.label(egui::RichText::new("looking").weak());
        }
        ui.data_mut(|d| {
            d.insert_temp(name_id, name.clone());
            d.insert_temp(kind_id, kind);
        });

        // Exactly one match on the typed name is the only case that can be added without a pick.
        let exact = hits.value.as_ref().and_then(|v| {
            v.iter().find(|l| l.label.trim().eq_ignore_ascii_case(name.trim()))
        });
        let taken: Vec<i64> = target.list(st).iter().map(|s| s.character_id).collect();
        let already = |id: i64| taken.contains(&id);
        let ready = can && exact.is_some_and(|l| !already(l.id));
        let add = ui
            .add_enabled(ready, egui::Button::new(format!("{}  Add", egui_phosphor::regular::PLUS)))
            .on_disabled_hover_text(if !can {
                "Your account does not have the manageFleetSnowflakes permission."
            } else if name.trim().is_empty() {
                "Type a character name."
            } else if exact.is_none() {
                "No character by that name."
            } else {
                "Already a snowflake."
            });
        if add.clicked() {
            if let Some(l) = exact {
                target.list(st).push(Snowflake {
                    id: 0,
                    character_id: l.id,
                    character_name: l.label.clone(),
                    kind,
                });
                act.edited = true;
                ui.data_mut(|d| d.insert_temp(name_id, String::new()));
            }
        }
    });

    // Anything the search turned up that is not the exact name, one click away.
    if let Some(v) = hits.value.as_ref().filter(|v| v.len() > 1) {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Did you mean").weak());
            for l in v.iter().take(6) {
                if selectable_chip(ui, false, l.label.trim()).clicked() {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("fleet_snowflake_name"), l.label.clone())
                    });
                }
            }
        });
    }
}

/// What the roster needs across: the badge, its columns and the lock gutter, none of which wrap.
/// It scrolls sideways now, but scrolling to reach a kick button is not a layout, so anything
/// docked beside it still has to leave the table its width.
#[cfg(feature = "fleet")]
const MEMBER_TABLE_W: f32 = BADGE_W + COL[0] + COL[1] + COL[2] + COL[3] + 120.0;

/// Room for the scroll bar, which is drawn over the content rect rather than beside it.
#[cfg(feature = "fleet")]
const SCROLLBAR_W: f32 = 10.0;

/// What the docked chat asks for. Narrow enough that the form still gets two columns beside it
/// and the preview pane, which is what keeps the form short enough not to run under its own
/// action bar.
#[cfg(feature = "fleet")]
const DOCK_W: f32 = 270.0;

/// The least it is worth being: narrower than this and a message is one word per line.
#[cfg(feature = "fleet")]
const NARROW_DOCK_W: f32 = 190.0;

/// The two rooms a fleet is run from, docked beside the form.
#[cfg(feature = "fleet")]
pub(crate) struct ChatDock {
    pub open: bool,
    pub tab: u8,
    pub connected: bool,
    pub rooms: [String; 2],
    pub tails: Vec<Vec<(String, String, bool, i64)>>,
    pub drafts: [String; 2],
    pub send: Option<(String, String)>,
}

/// Collapsed it is a spine you click to open, so the space is there when wanted and gone when not.
///
/// The feed is the rescue tab's own renderer, so grouping, timestamps and per-name colours are the
/// same by construction rather than by being kept in step.
#[cfg(feature = "fleet")]
fn chat_dock(ui: &mut egui::Ui, d: &mut ChatDock, min_central: f32) {
    const LABELS: [&str; 2] = ["skirmish", "delve911"];
    // Expanded it needs its own width and whatever it docks beside. Below that it would take the
    // space that page needs and sit on its content, so it stays a spine and says why rather than
    // opening onto the page.
    let room = ui.available_width() - min_central;
    let fits = room >= NARROW_DOCK_W;
    if !d.open || !fits {
        egui::Panel::right("fleet_chat_dock").exact_size(26.0).show_inside(ui, |ui| {
            ui.add_space(6.0);
            let resp = ui.add_sized([20.0, 70.0], egui::Button::new("\u{00AB}"));
            if fits {
                if resp.on_hover_text("Show fleet chat").clicked() {
                    d.open = true;
                }
            } else {
                resp.on_hover_text(
                    "Not enough width for the chat beside this page. Widen the window, or \
                     collapse the panel on the right.",
                );
            }
        });
        return;
    }
    // What it may take without starving the page it docks beside.
    let room = room.clamp(NARROW_DOCK_W, DOCK_W);
    egui::Panel::right("fleet_chat_dock")
        .resizable(true)
        .default_size(room)
        .size_range(NARROW_DOCK_W..=520.0)
        .show_inside(ui, |ui| {
            // The composer is a panel of its own, pinned to the bottom, so the feed above simply
            // takes what is left. Measuring it and subtracting left it floating mid-pane.
            egui::Panel::bottom("fleet_chat_composer")
                .frame(egui::Frame::NONE)
                .show_inside(ui, |ui| {
                    let i = (d.tab as usize).min(LABELS.len() - 1);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let ready = d.connected && !d.drafts[i].trim().is_empty();
                        if ui.add_enabled(ready, egui::Button::new("Send")).clicked() {
                            d.send = Some((d.rooms[i].clone(), std::mem::take(&mut d.drafts[i])));
                        }
                        let w = (ui.available_width() - 4.0).max(60.0);
                        // Multiline with Enter rebound to Shift+Enter, the same deal the jabber
                        // page makes: Enter sends, Shift+Enter breaks the line.
                        let shift_enter = egui::KeyboardShortcut::new(
                            egui::Modifiers::SHIFT,
                            egui::Key::Enter,
                        );
                        let resp = ui.add_sized(
                            [w, 22.0],
                            egui::TextEdit::multiline(&mut d.drafts[i])
                                .return_key(shift_enter)
                                .desired_rows(1)
                                .hint_text(if d.connected {
                                    "message (Shift+Enter for a new line)"
                                } else {
                                    "jabber offline"
                                }),
                        );
                        if ready
                            && resp.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                        {
                            d.send = Some((d.rooms[i].clone(), std::mem::take(&mut d.drafts[i])));
                        }
                    });
                });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("\u{00BB}").on_hover_text("Hide fleet chat").clicked() {
                    d.open = false;
                }
                let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(40.0);
                for (i, label) in LABELS.iter().enumerate() {
                    // No stroke: on an unframed button egui adds one on hover and it counts
                    // toward the size, so hovering a tab nudged everything after it.
                    if ui.menu_label_sized([w, 22.0], d.tab as usize == i, *label).clicked() {
                        d.tab = i as u8;
                    }
                }
            });
            ui.separator();
            let i = (d.tab as usize).min(LABELS.len() - 1);
            // Not while the pointer is down, or an incoming message wipes a drag-select.
            let selecting = ui.input(|i| i.pointer.any_down());
            egui::ScrollArea::vertical()
                .id_salt(("fleet_chat", i))
                .auto_shrink([false, false])
                .stick_to_bottom(!selecting)
                .show(ui, |ui| {
                    if let Some((act, who, body)) =
                        crate::app::rescue_chat_feed(ui, &d.tails[i], LABELS[i])
                    {
                        match act {
                            crate::app::MsgRowAction::Copy => ui.ctx().copy_text(body),
                            crate::app::MsgRowAction::Mention => {
                                let t = &mut d.drafts[i];
                                if !t.is_empty() && !t.ends_with(char::is_whitespace) {
                                    t.push(' ');
                                }
                                t.push_str(&format!("{who}: "));
                            }
                            _ => {}
                        }
                    }
                });
        });
}

/// The start form's right-hand side: presets, a search over them, and the ping underneath.
#[cfg(feature = "fleet")]
fn side_pane(
    ui: &mut egui::Ui,
    st: &crate::fleets::FleetState,
    presets: &[crate::settings::FleetPreset],
    local_ping: &str,
    act: &mut FormAct,
) {
    let tab_id = egui::Id::new("fleet_side_tab");
    let mut tab: u8 = ui.data(|d| d.get_temp(tab_id).unwrap_or(0));
    ui.add_space(4.0);
    // Two panes, half the width each, not a pair of buttons: this is where the eye goes to pick a
    // fleet, so the target is the whole half rather than a label inside it.
    ui.horizontal(|ui| {
        let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        for (i, label) in [(0u8, "Presets"), (1, "Ping preview")] {
            if pane_tab(ui, w, tab == i, label).clicked() {
                tab = i;
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));
    ui.add_space(4.0);

    // One or the other, each with the whole pane. Sharing it vertically gave both halves too
    // little to be useful.
    if tab == 1 {
        preview_pane(ui, st, local_ping);
        return;
    }
    // The search belongs to the presets rather than being a view of its own: an FC filtering the
    // list still wants to see which folder each one is in.
    let q_id = egui::Id::new("fleet_preset_search");
    let mut query: String = ui.data(|d| d.get_temp(q_id).unwrap_or_default());
    // The row is allocated at an exact width rather than left to fill: a `horizontal` inside a
    // panel reports the panel's width as available and then draws its frame margins on top, which
    // is how the pane ended up wider than the window it lives in.
    let row_w = ui.available_width() - 12.0;
    ui.allocate_ui_with_layout(
        egui::vec2(row_w, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
        const CLEAR_W: f32 = 26.0;
        let w = (row_w - CLEAR_W - ui.spacing().item_spacing.x).max(60.0);
        ui.add_sized(
            [w, 22.0],
            egui::TextEdit::singleline(&mut query).hint_text("Search saved fleets"),
        );
        if ui
            .add_enabled_ui(!query.trim().is_empty(), |ui| {
                ui.add_sized([CLEAR_W, 22.0], egui::Button::new(egui_phosphor::regular::X))
                    .on_hover_text("Clear")
            })
            .inner
            .clicked()
        {
            query.clear();
        }
        },
    );
    ui.data_mut(|d| d.insert_temp(q_id, query.clone()));
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("fleet_side_list")
        .auto_shrink([false, false])
        .show(ui, |ui| preset_tree(ui, presets, &st.seed.tags, &query, act));
}

/// One of the two half-width panes at the top of the sidebar.
#[cfg(feature = "fleet")]
fn pane_tab(ui: &mut egui::Ui, w: f32, on: bool, label: &str) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(w, 30.0), egui::Sense::click());
    let v = ui.visuals();
    let fill = if on {
        v.selection.bg_fill
    } else if resp.hovered() {
        v.widgets.hovered.bg_fill
    } else {
        v.widgets.inactive.bg_fill
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    let colour = if on { v.selection.stroke.color } else { v.text_color() };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::TextStyle::Button.resolve(ui.style()),
        colour,
    );
    resp
}

/// Saved fleets by folder, one level deep, which is the whole depth a preset has.
///
/// Filtering hides rows, never folders that still have one: the folder is half of what tells an
/// FC which preset this is, so a filtered list without them is a list of ambiguous names.
#[cfg(feature = "fleet")]
fn preset_tree(
    ui: &mut egui::Ui,
    presets: &[crate::settings::FleetPreset],
    tags: &[TagItem],
    query: &str,
    act: &mut FormAct,
) {
    if presets.is_empty() {
        ui.label(egui::RichText::new("No saved fleets yet.").weak());
        return;
    }
    let shown = |p: &crate::settings::FleetPreset| {
        query.trim().is_empty()
            || tag_matches(&p.label, query)
            || tag_matches(&p.folder, query)
            || tag_matches(&p.name, query)
    };
    let mut groups: Vec<(String, Vec<usize>)> = vec![(String::new(), Vec::new())];
    for f in preset_folders(presets) {
        groups.push((f, Vec::new()));
    }
    for (i, p) in presets.iter().enumerate() {
        if !shown(p) {
            continue;
        }
        if let Some(g) = groups.iter_mut().find(|(name, _)| *name == p.folder.trim()) {
            g.1.push(i);
        }
    }
    if groups.iter().all(|(_, idx)| idx.is_empty()) {
        ui.label(egui::RichText::new("Nothing matches.").weak());
        return;
    }
    for (folder, idx) in groups.iter().filter(|(_, idx)| !idx.is_empty()) {
        if folder.is_empty() {
            // The top level takes drops too, which is how a preset comes back out of a folder.
            // The top level takes drops too, which is how a preset comes back out of a folder.
            let row_w = (ui.available_width() - SCROLLBAR_W).max(60.0);
            let (_, dropped) = ui.dnd_drop_zone::<usize, _>(egui::Frame::NONE, |ui| {
                for &i in idx {
                    preset_row(ui, presets, tags, i, row_w, act);
                }
            });
            if let Some(i) = dropped {
                act.move_preset = Some((*i, String::new()));
            }
        } else {
            let id = ui.make_persistent_id(("preset_folder", folder));
            let state =
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
            let (_, head, _) = state
                .show_header(ui, |ui| {
                    ui.dnd_drag_source(
                        egui::Id::new(("preset_folder_drag", folder)),
                        FolderDrag(folder.clone()),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(egui_phosphor::regular::DOTS_SIX_VERTICAL)
                                        .size(13.0)
                                        .weak(),
                                )
                                .selectable(false),
                            );
                        },
                    )
                    .response
                    .on_hover_text(format!("Drag {folder} onto another folder to go before it"));
                    ui.label(egui::RichText::new(folder).strong());
                })
                .body(|ui| {
                    let row_w = (ui.available_width() - SCROLLBAR_W).max(60.0);
                    let (_, dropped) = ui.dnd_drop_zone::<usize, _>(egui::Frame::NONE, |ui| {
                        ui.add_space(3.0);
                        for &i in idx {
                            preset_row(ui, presets, tags, i, row_w, act);
                        }
                        ui.add_space(3.0);
                        // A folder with everything filtered out of view still has to be a target,
                        // but claiming the full width here pushed the panel past the window.
                        ui.allocate_space(egui::vec2(40.0, 4.0));
                    });
                    if let Some(i) = dropped {
                        act.move_preset = Some((*i, folder.clone()));
                    }
                });
            let head = head.response;
            // Type first, because taking a payload of the wrong type still consumes it.
            if egui::DragAndDrop::has_payload_of_type::<FolderDrag>(ui.ctx()) {
                if head.dnd_hover_payload::<FolderDrag>().is_some_and(|f| f.0 != *folder) {
                    ui.painter().hline(
                        head.rect.x_range(),
                        head.rect.top() - 1.0,
                        egui::Stroke::new(2.0, ui.visuals().hyperlink_color),
                    );
                }
                if let Some(f) = head.dnd_release_payload::<FolderDrag>() {
                    if f.0 != *folder {
                        act.reorder_folder = Some((f.0.clone(), folder.clone()));
                    }
                }
            } else if egui::DragAndDrop::has_payload_of_type::<usize>(ui.ctx()) {
                // A preset dropped on the heading joins the folder, same as on its body.
                if let Some(i) = head.dnd_release_payload::<usize>() {
                    act.move_preset = Some((*i, folder.clone()));
                }
            }
        }
    }
}

/// What a folder heading carries while it is dragged, kept apart from a preset's `usize` so a drop
/// target can tell the two apart.
#[cfg(feature = "fleet")]
#[derive(Clone, Debug)]
struct FolderDrag(String);

/// One saved fleet: the whole row loads it, draggable into a folder, with rename and delete.
#[cfg(feature = "fleet")]
fn preset_row(
    ui: &mut egui::Ui,
    presets: &[crate::settings::FleetPreset],
    tags: &[TagItem],
    i: usize,
    row_w: f32,
    act: &mut FormAct,
) {
    const H: f32 = 24.0;
    const ICON_W: f32 = 26.0;
    const GRIP_W: f32 = 16.0;
    let p = &presets[i];
    // `row_w` is measured once for the whole container and handed down. Taking it from
    // `available_width()` per row made each row a few pixels wider than the last and the whole
    // column wider every frame: the name button overflowed its allocation, that widened the
    // container, and the next row read the wider number back.
    let row = ui.allocate_ui_with_layout(
        egui::vec2(row_w, H),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            // `min_size` on the button, not `add_sized` around it: the latter centres a button
            // that has already sized itself to its glyph, so two different icons came out two
            // different heights next to a name of a third.
            let icon = |g: &str| {
                egui::Button::new(egui::RichText::new(g).size(13.0))
                    .min_size(egui::vec2(ICON_W, H))
            };
            if ui
                .add(icon(egui_phosphor::regular::TRASH))
                .on_hover_text(format!("Forget the {} preset", p.label))
                .clicked()
            {
                act.delete_preset = Some(i);
            }
            if ui
                .add(icon(egui_phosphor::regular::PENCIL_SIMPLE))
                .on_hover_text(format!("Rename {} or move it to another folder", p.label))
                .clicked()
            {
                act.rename_preset = Some(i);
            }
            let gap = ui.spacing().item_spacing.x;
            let name_w = (ui.available_width() - GRIP_W - KIND_W - 2.0 * gap).max(40.0);
            // Justified, so the button fills exactly what it was given: `min_size` alone is a
            // floor a long name grows past, and `add_sized` alone leaves a short one adrift in
            // the middle of its cell.
            ui.allocate_ui_with_layout(
                egui::vec2(name_w, H),
                egui::Layout::top_down_justified(egui::Align::Center),
                |ui| {
                    if ui
                        .add(
                            egui::Button::new(&p.label)
                                .wrap_mode(egui::TextWrapMode::Truncate),
                        )
                        .on_hover_text(format!("Load {}", p.label))
                        .clicked()
                    {
                        act.load_preset = Some(i);
                    }
                },
            );
            // Right to left, so this lands between the grip and the name.
            preset_kind_cell(ui, preset_kind(p, tags));
            ui.dnd_drag_source(egui::Id::new(("preset_drag", i)), i, |ui| {
                ui.add_sized(
                    [GRIP_W, H],
                    egui::Label::new(
                        egui::RichText::new(egui_phosphor::regular::DOTS_SIX_VERTICAL)
                            .size(13.0)
                            .weak(),
                    )
                    .selectable(false),
                );
            })
            .response
            .on_hover_text(if p.folder.trim().is_empty() {
                format!("Drag {} into a folder, or onto another to go before it", p.label)
            } else {
                format!("Drag {} out of {}, or onto another to go before it", p.label, p.folder.trim())
            });
        },
    )
    .response;
    // Dropped on this row: the dragged preset goes just before it, in its folder. Checked for the
    // type first, because taking a payload of the wrong type still consumes it.
    if egui::DragAndDrop::has_payload_of_type::<usize>(ui.ctx()) {
        if row.dnd_hover_payload::<usize>().is_some_and(|src| *src != i) {
            let y = row.rect.top() - 1.0;
            ui.painter().hline(
                row.rect.x_range(),
                y,
                egui::Stroke::new(2.0, ui.visuals().hyperlink_color),
            );
        }
        if let Some(src) = row.dnd_release_payload::<usize>() {
            if *src != i {
                act.reorder_preset = Some((*src, i));
            }
        }
    }
}

/// The ping and MOTD the dashboard would render, refreshed as the form changes.
#[cfg(feature = "fleet")]
fn preview_pane(ui: &mut egui::Ui, st: &crate::fleets::FleetState, local_ping: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Ping preview").strong());
        if st.preview.loading {
            ui.label(egui::RichText::new("rendering").weak());
        }
    });
    ui.separator();
    let local;
    let p = match st.preview.value.as_ref() {
        Some(p) => p,
        // No session, no dashboard, or a dry run: the local template still produces a ping, and an
        // FC who cannot ping cannot call a fleet.
        None => {
            ui.label(
                egui::RichText::new("From the local template: the dashboard did not render one.")
                    .color(crate::theme::standing::WARNING),
            );
            ui.add_space(4.0);
            local = crate::fleets::model::PingPreview {
                ping: local_ping.to_owned(),
                motd: String::new(),
            };
            &local
        }
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (title, text) in [("Ping", &p.ping), ("MOTD", &p.motd)] {
            if text.trim().is_empty() {
                continue;
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong());
                if ui.small_button(egui_phosphor::regular::COPY).on_hover_text("Copy").clicked() {
                    ui.ctx().copy_text(text.clone());
                }
            });
            ui.add(
                egui::Label::new(
                    // Smaller than body text on purpose: a ping is a dozen lines of fixed-width
                    // text in a 360px pane, and wrapping every one of them reads worse than a
                    // step down in size.
                    egui::RichText::new(text.clone()).monospace().size(11.0),
                )
                .wrap(),
            );
            ui.add_space(6.0);
        }
    });
}

/// What this tab would have sent, newest first.
#[cfg(feature = "fleet")]
fn journal_pane(ui: &mut egui::Ui, st: &crate::fleets::FleetState) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Recorded requests").strong());
    ui.label(
        egui::RichText::new("Built and kept, never sent. This is what the real client would post.")
            .weak(),
    );
    ui.separator();
    if st.journal.is_empty() {
        ui.label(egui::RichText::new("Nothing yet.").weak());
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, rec) in st.journal.iter().enumerate().rev() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(rec.line()).monospace());
                if ui.small_button(egui_phosphor::regular::COPY).on_hover_text("Copy").clicked() {
                    let text = match rec.pretty_body() {
                        Some(b) => format!("{}\n{b}", rec.line()),
                        None => rec.line(),
                    };
                    ui.ctx().copy_text(text);
                }
            });
            if let Some(body) = rec.pretty_body() {
                egui::CollapsingHeader::new("body").id_salt(("journal", i)).show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(body).monospace()).wrap());
                });
            }
            ui.add_space(4.0);
        }
    });
}

/// The question a destructive action asks before it is recorded, or nothing when it is harmless.
#[cfg(feature = "fleet")]
fn confirm_question(action: &Action) -> Option<&'static str> {
    Some(match action {
        Action::Close => "Close this fleet?",
        Action::KickAll => "Kick everyone out of the fleet?",
        Action::KickCapsules => "Kick every pod out of the fleet?",
        Action::KickMany { .. } => "Kick everyone in the wrong ship?",
        Action::Kick { .. } => "Kick them out of the fleet?",
        _ => return None,
    })
}

/// How much damage an action does if it was not meant.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Danger {
    None,
    /// Undoable, but somebody has to be re-invited.
    Caution,
    /// Takes the whole fleet with it.
    Severe,
}

#[cfg(feature = "fleet")]
fn danger(action: &Action) -> Danger {
    match action {
        Action::Close | Action::KickAll | Action::KickMany { .. } => Danger::Severe,
        Action::KickCapsules | Action::Kick { .. } => Danger::Caution,
        _ => Danger::None,
    }
}

/// What a severe action costs, spelled out rather than implied.
#[cfg(feature = "fleet")]
fn confirm_consequence(action: &Action, pilots: usize) -> Option<String> {
    match action {
        Action::Close => Some(format!(
            "The fleet stops being tracked and {pilots} pilots stop earning PAPs for it."
        )),
        Action::KickAll => {
            Some(format!("All {pilots} pilots are removed. They have to be invited back one by one."))
        }
        Action::KickCapsules => Some("Every pilot in a pod is removed.".to_owned()),
        Action::KickMany { character_ids, .. } => Some(format!(
            "{} pilots are removed. Bridges and the FC are left where they are.",
            character_ids.len()
        )),
        _ => None,
    }
}

#[cfg(all(test, feature = "fleet"))]
mod confirm_tests {
    use super::*;

    /// Folders come out of the presets themselves, once each, sorted, with the top level implied.
    #[test]
    fn preset_folders_are_listed_once_each() {
        let p = |label: &str, folder: &str| crate::settings::FleetPreset {
            label: label.to_owned(),
            folder: folder.to_owned(),
            ..Default::default()
        };
        let presets = vec![
            p("Home Defence", "Home"),
            p("Roam", ""),
            p("Tower bash", "Structures"),
            p("Entosis", " Home "),
            p("Move op", "  "),
        ];
        assert_eq!(preset_folders(&presets), vec!["Home".to_owned(), "Structures".to_owned()]);
        assert!(preset_folders(&[]).is_empty());
    }

    /// Peacetime and strategic sort ahead of everything else, in that order.
    #[test]
    fn the_headline_tags_come_first() {
        let mut names = vec!["Corp", "STRATEGIC", "SIG/SQUAD", "PEACETIME", "Incursion-VG"];
        names.sort_by_key(|n| headline_rank(n));
        assert_eq!(&names[..2], &["PEACETIME", "STRATEGIC"]);
        // Case and stray spaces come from the API, not from the user.
        assert_eq!(headline_rank(" peacetime "), 0);
        assert_eq!(headline_rank("Corp"), 2);
    }

    /// What the tag search keeps and what it drops.
    #[test]
    fn the_tag_search_matches_every_word_anywhere() {
        assert!(tag_matches("Structure Bash", ""));
        assert!(tag_matches("Structure Bash", "  "));
        assert!(tag_matches("Structure Bash", "bash"));
        assert!(tag_matches("Structure Bash", "STRUCT"));
        // Out of order and not adjacent.
        assert!(tag_matches("Structure Bash", "bash struct"));
        assert!(!tag_matches("Structure Bash", "roam"));
        assert!(!tag_matches("Structure Bash", "bash roam"));
    }

    /// How loud each action is, and what its dialog says it costs.
    #[test]
    fn the_worst_actions_say_what_they_cost() {
        assert_eq!(danger(&Action::Close), Danger::Severe);
        assert_eq!(danger(&Action::KickAll), Danger::Severe);
        assert_eq!(danger(&Action::KickCapsules), Danger::Caution);
        assert_eq!(
            danger(&Action::KickMany { character_ids: vec![1, 2], exclude: false }),
            Danger::Severe
        );
        // A sweep says how many it takes.
        let line = confirm_consequence(
            &Action::KickMany { character_ids: vec![1, 2, 3], exclude: false },
            42,
        )
        .expect("a consequence");
        assert!(line.contains('3'), "{line}");
        assert_eq!(danger(&Action::SetMotd), Danger::None);
        assert_eq!(danger(&Action::AddWing), Danger::None);

        for a in [Action::Close, Action::KickAll] {
            let line = confirm_consequence(&a, 42).expect("a consequence");
            assert!(line.contains("42"), "{line}");
        }
        assert!(confirm_consequence(&Action::SetMotd, 42).is_none());
    }

    /// Anything that asks a question has a danger level, and anything dangerous asks.
    #[test]
    fn a_dangerous_action_always_asks_first() {
        for a in [
            Action::Close,
            Action::KickAll,
            Action::KickCapsules,
            Action::Kick { character_id: 1, exclude: false },
        ] {
            assert_ne!(danger(&a), Danger::None, "{a:?}");
            assert!(confirm_question(&a).is_some(), "{a:?}");
        }
        assert!(confirm_question(&Action::SetMotd).is_none());
    }
}

/// Which half of a fleet's page is showing.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum DetailTab {
    /// Who is in the fleet, by wing and squad, the way the dashboard lists them.
    #[default]
    Members,
    /// What they are flying, grouped by hull and judged against the doctrine.
    Composition,
}

/// A tracked fleet, or a closed one read back.
#[cfg(feature = "fleet")]
fn tracking_page(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    // Whether the page was reached through the history. The fleet's own `closedAt` overrides it
    // below: a fleet can close on the dashboard's auto-close timer while this tab is still on the
    // tracking page, and every button there would then act on a fleet that no longer exists.
    from_history: bool,
    tab: DetailTab,
    set_tab: &mut Option<DetailTab>,
    act_on: &mut Vec<Action>,
    boost_rules: &[crate::settings::FleetBoostRequirement],
    hulls: &[crate::settings::FleetHull],
    tanks: &[(i32, String)],
    strict: &[i32],
    open_editor: &mut bool,
    sidebar: &mut bool,
    mine: bool,
    here: Option<String>,
    join: &mut Option<crate::fleets::comms::Links>,
    boost_detail: &mut Option<String>,
    edit_snowflakes: &mut bool,
    open_migrate: &mut bool,
    comms: &CommsTargets,
    chat: &mut ChatDock,
) {
    let Some(open) = st.open.value.clone() else {
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(if st.open.loading {
                "Loading the fleet."
            } else {
                "No fleet open."
            })
            .weak())
        });
        return;
    };

    // The fleet's own record decides, not the route taken to it. A fleet that closed on the
    // dashboard's timer while this page was open is just as closed as one read back out of the
    // history, and the live view's buttons would act on nothing.
    let read_only = from_history || open.fleet.closed_at.is_some();

    let seed = st.seed.clone();
    let boosts = st.boosts.clone();
    let boosts_loading = st.boosts_loading;
    let off_doctrine = st.off_doctrine.clone();
    // Rebuilt from settings rather than taken from the snapshot, so editing the hull list shows
    // up without waiting for the next poll.
    let open = crate::fleets::state::OpenFleet {
        doctrine: crate::fleets::doctrine::configured(
            open.fleet.setup_id,
            seed.setup_name(open.fleet.setup_id).unwrap_or_default(),
            open.doctrine.clone(),
            hulls,
            tanks
                .iter()
                .find(|(id, _)| *id == open.fleet.setup_id.0)
                .and_then(|(_, t)| crate::fleets::doctrine::Tank::parse(t))
                .or_else(|| {
                    wanted_tank(&crate::fleets::boosts::wanted_for(
                        open.fleet.setup_id.0.into(),
                        boost_rules,
                    ))
                }),
            strict.contains(&open.fleet.setup_id.0),
        ),
        ..open
    };
    // Against the doctrine above, not the one the snapshot carried. The dashboard has no hull
    // list to give, so the backend's doctrine is whatever the seed file holds, which is nothing:
    // classifying against that marked every mainline hull as off-doctrine.
    let mut off_doctrine = crate::fleets::doctrine::track_off_doctrine(
        &off_doctrine,
        &open.composition,
        open.doctrine.as_ref(),
        open.at,
    );
    // A pilot the FC has confirmed has stopped being a question.
    off_doctrine.retain(|o| !st.locked.contains(&o.character_id));
    // Kept, so the next frame carries each pilot's clock forward instead of restarting it.
    st.off_doctrine = off_doctrine.clone();
    let wanted = crate::fleets::boosts::wanted_for(open.fleet.setup_id.0.into(), boost_rules);
    egui::Panel::top("fleet_header").show_inside(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.heading(&open.fleet.name);
            if let Some(name) = seed.setup_name(open.fleet.setup_id) {
                ui.label(egui::RichText::new(name).weak());
            }
            for t in open.fleet.tag_ids.iter().filter_map(|t| seed.tag(*t)) {
                fleet_tag_chip(ui, t);
            }
            // The fleet's own record, not the route to the page: a fleet that closed under the
            // tracking page has to say so there too.
            if let Some(at) = open.fleet.closed_at.as_deref() {
                ui.label(egui::RichText::new("closed").color(crate::theme::standing::WARNING))
                    .on_hover_text(format!("Closed {at}"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let now = chrono::Utc::now().timestamp();
                if let Some(t) = crate::fleets::model::parse_iso(&open.fleet.started_at) {
                    let end = open
                        .fleet
                        .closed_at
                        .as_deref()
                        .and_then(crate::fleets::model::parse_iso)
                        .unwrap_or(now);
                    ui.label(egui::RichText::new(fmt_age(end - t)).strong())
                        .on_hover_text("How long the fleet has been up.");
                }
                if let Some(c) = &open.fleet.commander {
                    ui.label(egui::RichText::new(&c.label).weak());
                }
                // Beside the boss whose advert it is, and in this right-to-left run so it can
                // never wrap the title onto a second line. Nothing at all when it cannot be read,
                // which is any fleet whose boss is not one of this machine's characters.
                let advert = st
                    .advert
                    .as_ref()
                    .filter(|(id, _)| *id == open.fleet.id && open.fleet.closed_at.is_none())
                    .map(|(_, up)| *up);
                match advert {
                    Some(true) => {
                        ui.label(
                            egui::RichText::new("advert up").color(crate::theme::standing::FRIENDLY),
                        )
                        .on_hover_text("The fleet is listed in the Fleet Finder.");
                    }
                    Some(false) => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  advert off",
                                egui_phosphor::regular::WARNING
                            ))
                            .color(crate::theme::standing::WARNING),
                        )
                        .on_hover_text(
                            "The fleet is not listed in the Fleet Finder, so nobody can find it \
                             there. Advertise it in game.",
                        );
                    }
                    None => {}
                }
            });
        });
        if let Some(f) = &open.fleet.formup_location {
            ui.label(egui::RichText::new(format!("Formup: {}", f.label)).weak());
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (t, label) in
                [(DetailTab::Members, "Members"), (DetailTab::Composition, "Composition")]
            {
                if selectable_chip(ui, tab == t, label).clicked() {
                    *set_tab = Some(t);
                }
            }
            ui.label(
                egui::RichText::new(format!("{} pilots", open.composition.total())).weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if selectable_chip(
                    ui,
                    *sidebar,
                    format!("{}  Settings", egui_phosphor::regular::SLIDERS_HORIZONTAL),
                )
                .on_hover_text("Change the doctrine or the comms of this fleet")
                .clicked()
                {
                    *sidebar = !*sidebar;
                }
                // Right-to-left, so writing these after Settings puts them before it.
                comms_buttons(ui, &seed, &open, mine, here.as_deref(), comms, join);
            });
        });
        ui.add_space(4.0);
    });

    // Nothing on that bar can act on a fleet that is already closed, so the bar is not there.
    if !read_only {
        egui::Panel::bottom("fleet_actions").frame(bar_frame(ui)).show_inside(ui, |ui| {
            action_bar(ui, st, act_on, open_migrate);
        });
    }

    if *sidebar {
        egui::Panel::right("fleet_sidebar")
            .resizable(true)
            .default_size(330.0)
            .size_range(270.0..=540.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    readiness_pane(
                        ui,
                        &open,
                        &boosts,
                        boosts_loading,
                        &wanted,
                        &mut st.boosts_forced,
                        open_editor,
                        boost_detail,
                    );
                    ui.separator();
                    fleet_sidebar(ui, st, &open, read_only, act_on, edit_snowflakes);
                });
            });
    }
    // A fleet is run out of these two rooms, so they are beside it here as well as on the form.
    // After the readiness sidebar, so it docks to the left of it. The member tree's columns are
    // fixed, so it needs their full width before anything may take space beside it.
    chat_dock(ui, chat, MEMBER_TABLE_W);

    egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
        let (can_move, can_kick) = (
            // A roster-only tree has sentinel wing and squad ids, so a move would post a seat the
            // server cannot address.
            !read_only && !open.composition.flat && st.can(Perm::MoveMember),
            !read_only && st.can(Perm::KickMember),
        );
        let mut toggle_lock: Option<i64> = None;
        // Both ways for the roster: its columns are a fixed width and do not wrap, so in a pane
        // narrower than the table the kick buttons used to end up past the edge unreachable.
        let area = match tab {
            DetailTab::Members => egui::ScrollArea::both(),
            _ => egui::ScrollArea::vertical(),
        };
        area.auto_shrink([false, false]).show(ui, |ui| match tab {
            DetailTab::Members => members_view(
                ui,
                &open,
                &st.locked,
                can_move,
                can_kick,
                read_only,
                act_on,
                &mut toggle_lock,
            ),
            DetailTab::Composition => {
                composition_view(ui, &open, &off_doctrine)
            }
        });
        if let Some(id) = toggle_lock {
            if !st.locked.insert(id) {
                st.locked.remove(&id);
            }
        }
    });
}

/// Where a comms button sends the FC, worked out where the app's links are in reach.
#[cfg(feature = "fleet")]
#[derive(Clone, Debug, Default)]
pub(crate) struct CommsTargets {
    /// The fleet's own channel, both ways in.
    pub op: Option<crate::fleets::comms::Links>,
    /// The command channel for the fleet's sector.
    pub command: Option<crate::fleets::comms::Links>,
    /// The fleet's channel, when no link for it is known. Said on the button rather than guessed.
    pub unlinked: Option<String>,
}

/// Join the fleet's own comms and the command channel its FC belongs in.
///
/// On the FC's own fleet the buttons also say whether Mumble is actually there: a fleet running
/// with its commander in the wrong channel is a fleet nobody can reach, and it is the kind of
/// mistake that goes unnoticed until it matters.
#[cfg(feature = "fleet")]
fn comms_buttons(
    ui: &mut egui::Ui,
    seed: &crate::fleets::seed::Seed,
    open: &crate::fleets::state::OpenFleet,
    mine: bool,
    here: Option<&str>,
    targets: &CommsTargets,
    join: &mut Option<crate::fleets::comms::Links>,
) {
    use crate::fleets::comms;
    let tags: Vec<_> = open.fleet.tag_ids.iter().filter_map(|t| seed.tag(*t)).cloned().collect();
    let sector = comms::sector(&tags);

    // Drawn in a right-to-left strip, so the order here is the reverse of how it reads: the
    // fleet's own comms end up first.
    for (label, target) in [
        (format!("Join {} command", sector.label()), &targets.command),
        ("Join comms".to_owned(), &targets.op),
    ] {
        let Some(links) = target else {
            if label == "Join comms" {
                if let Some(name) = &targets.unlinked {
                    ui.add_enabled(
                        false,
                        egui::Button::new(format!(
                            "{}  {label}",
                            egui_phosphor::regular::HEADPHONES
                        )),
                    )
                    .on_disabled_hover_text(format!("No comms link is known for {name}."));
                }
            }
            continue;
        };
        // Only a fleet this account is running is worth nagging about, and only against a real
        // mumble:// link: a built path would say "away" to an FC sitting in the right channel
        // under its vanity name.
        let away = mine
            && match (here, links.mumble.as_deref()) {
                (Some(h), Some(m)) => !crate::mumble::in_channel(h, m),
                _ => false,
            };
        let text = egui::RichText::new(format!("{}  {label}", egui_phosphor::regular::HEADPHONES));
        let button = match pulse_fill(ui, away) {
            Some(c) => egui::Button::new(text).fill(c),
            None => egui::Button::new(text),
        };
        let tip = match (mine, here) {
            (true, Some(_)) if away => "Your fleet, and Mumble is somewhere else.".to_owned(),
            (true, Some(_)) if links.mumble.is_some() => "You are in this channel.".to_owned(),
            (true, None) => "Mumble is not running, so there is nothing to check against."
                .to_owned(),
            _ => match links.mumble.as_deref().and_then(crate::mumble::channel_path) {
                Some(p) => format!("Opens {p}"),
                None => format!(
                    "Opens {}",
                    links.short.clone().unwrap_or_default()
                ),
            },
        };
        if ui.add(button).on_hover_text(tip).clicked() {
            *join = Some(links.clone());
        }
    }
}

/// Which hulls a doctrine flies, and which are welcome in any fleet.
///
/// The dashboard hands over no hulls at all, so without this list every ship in the fleet reads as
/// out of doctrine.
#[cfg(feature = "fleet")]
fn hull_editor(
    ui: &mut egui::Ui,
    setup_id: i32,
    setup_name: &str,
    hint: &str,
    hulls: &mut Vec<crate::settings::FleetHull>,
    ships: &[(i64, String, String)],
    body_h: f32,
) -> bool {
    let mut changed = false;

    for (owner, title, hint) in [(setup_id, setup_name, hint)] {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(clip(title, 26)).strong()).on_hover_text(title);
            if !hint.is_empty() {
                ui.label(egui::RichText::new(hint).weak());
            }
        });

        let mut remove: Option<usize> = None;
        // Bounded by what is left after the add row below it, and it takes all of that: shrinking
        // to content left a doctrine with four hulls using a fifth of a window the FC had opened
        // to work in.
        let chrome_id = ui.id().with(("hull_chrome", owner));
        let chrome: f32 = ui.data(|d| d.get_temp(chrome_id).unwrap_or(60.0));
        // From the window body's height less what this column already used and what this editor
        // puts below the list, not from `available_height`: inside a `horizontal_top` a child is
        // handed no vertical bound, so that reads as a fraction of the window and the list ends
        // up a few rows tall in a pane the FC opened to work in.
        let before = ui.min_rect().height();
        let list_h = (body_h - before - chrome).max(90.0);
        let list = egui::ScrollArea::vertical()
            .id_salt(("hull_rows_scroll", owner))
            .auto_shrink([false, false])
            .max_height(list_h)
            .show(ui, |ui| {
                egui::Grid::new(("hull_rows", owner))
                    .num_columns(if owner == 0 { 2 } else { 3 })
                    .striped(true)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                        for (i, h) in hulls.iter_mut().enumerate() {
                            if h.setup_id != owner {
                                continue;
                            }
                            cell(ui, 230.0, |ui| {
                                ui.label(&h.name);
                            });
                            // Only for a doctrine's own hulls: the always-allowed list is not a
                            // doctrine and has no damage of its own.
                            if owner != 0 {
                                cell(ui, 70.0, |ui| {
                                    changed |= ui
                                        .checkbox(&mut h.main, "Main")
                                        .on_hover_text(
                                            "This is what the fleet is built around. A Flycatcher \
                                             is an Interdictor, which reads as support in every \
                                             fleet except the one flying them as the damage.",
                                        )
                                        .changed();
                                });
                            }
                            if ui
                                .button(egui_phosphor::regular::TRASH)
                                .on_hover_text(format!("Drop {}", h.name))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                    });
            });
        let list_used = list.inner_rect.height();
        if let Some(i) = remove {
            hulls.remove(i);
            changed = true;
        }

        // Typed and picked, so the name matches what the composition will carry.
        let q_id = ui.id().with(("hull_query", owner));
        let mut query: String = ui.data(|d| d.get_temp(q_id).unwrap_or_default());
        // As wide as the pane, so a hull name and its group fit on one line.
        let width = ui.available_width().max(260.0);
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut query).hint_text("Add a hull").desired_width(200.0),
            );
            let hits: Vec<&(i64, String, String)> = if query.trim().len() >= 2 {
                ships
                    .iter()
                    .filter(|(_, n, _)| tag_matches(n, &query))
                    .filter(|(id, n, _)| {
                        !hulls.iter().any(|h| {
                            h.setup_id == owner
                                && (h.type_id == *id || h.name.eq_ignore_ascii_case(n))
                        })
                    })
                    .take(10)
                    .collect()
            } else {
                Vec::new()
            };
            let hov_id = ui.id().with(("hull_popup_hover", owner));
            let was_over: bool = ui.data(|d| d.get_temp(hov_id).unwrap_or(false));
            // Kept open while the pointer is over it: clicking an item takes focus off the field,
            // and a popup that closes on the press never sees the click.
            let popup = egui::Popup::from_response(&field)
                .open(!hits.is_empty() && (field.has_focus() || was_over))
                .width(width)
                .show(|ui| {
                    for (id, n, g) in &hits {
                        if ui.menu_label(false, format!("{n}   {g}")).clicked() {
                            hulls.push(crate::settings::FleetHull {
                                setup_id: owner,
                                type_id: *id,
                                name: n.clone(),
                                main: false,
                            });
                            changed = true;
                            query.clear();
                        }
                    }
                });
            let over = popup.as_ref().is_some_and(|r| r.response.contains_pointer());
            ui.data_mut(|d| d.insert_temp(hov_id, over));
            if ships.is_empty() {
                ui.label(egui::RichText::new("no ship data loaded").weak());
            }
        });
        ui.data_mut(|d| d.insert_temp(q_id, query));
        ui.add_space(8.0);
        // Only what this editor adds below the list. Measuring the whole column instead counted
        // the rows above the list twice and left the list a third of its pane.
        let used = ui.min_rect().height();
        ui.data_mut(|d| d.insert_temp(chrome_id, (used - before - list_used).max(0.0)));
    }
    changed
}

/// How a doctrine tanks. One answer for the whole fleet, which is what decides whether its logi
/// can actually rep it.
#[cfg(feature = "fleet")]
fn tank_row(ui: &mut egui::Ui, setup_id: i32, tanks: &mut Vec<(i32, String)>) -> bool {
    use crate::fleets::doctrine::Tank;
    let mut changed = false;
    let mut tank = tanks
        .iter()
        .find(|(id, _)| *id == setup_id)
        .and_then(|(_, t)| Tank::parse(t));
    ui.horizontal(|ui| {
        ui.label("Tanks with");
        egui::ComboBox::from_id_salt("doctrine_tank")
            .selected_text(tank.map(|t| t.label()).unwrap_or("not set"))
            .width(130.0)
            .show_ui(ui, |ui| {
                for t in [None, Some(Tank::Shield), Some(Tank::Armor)] {
                    let label = t.map(|t| t.label()).unwrap_or("not set");
                    if ui.menu_value(&mut tank, t, label).changed() {
                        tanks.retain(|(id, _)| *id != setup_id);
                        if let Some(t) = t {
                            tanks.push((setup_id, t.label().to_owned()));
                        }
                        changed = true;
                    }
                }
            });
        ui.label(egui::RichText::new("logi that reps the other way does not count").weak());
    });
    changed
}

/// Where a doctrine is written up, ready to paste into a ping.
#[cfg(feature = "fleet")]
fn doctrine_link_row(ui: &mut egui::Ui, setup_id: i32, urls: &mut Vec<(i32, String)>) -> bool {
    let mut changed = false;
    let mut url = urls
        .iter()
        .find(|(id, _)| *id == setup_id)
        .map(|(_, u)| u.clone())
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Forum link");
        if ui
            .add(
                egui::TextEdit::singleline(&mut url)
                    .hint_text("https://...")
                    .desired_width(300.0),
            )
            .changed()
        {
            urls.retain(|(id, _)| *id != setup_id);
            if !url.trim().is_empty() {
                urls.push((setup_id, url.trim().to_owned()));
            }
            changed = true;
        }
        let has = !url.trim().is_empty();
        if ui
            .add_enabled(has, egui::Button::new(egui_phosphor::regular::COPY).frame(false))
            .on_disabled_hover_text("Nothing to copy.")
            .on_hover_text("Copy the link")
            .clicked()
        {
            ui.ctx().copy_text(url.trim().to_owned());
        }
        if ui
            .add_enabled(has, egui::Button::new(egui_phosphor::regular::ARROW_SQUARE_OUT).frame(false))
            .on_disabled_hover_text("Nothing to open.")
            .on_hover_text("Open it")
            .clicked()
        {
            let _ = open::that(url.trim());
        }
    });
    changed
}

/// The hulls every fleet takes, whatever it is flying.
///
/// The groups are built in and not worth a list to maintain; anything else is named here, and a
/// hull that only suits one kind of fleet carries its tank.
#[cfg(feature = "fleet")]
fn always_allowed(
    ui: &mut egui::Ui,
    hulls: &mut Vec<crate::settings::FleetHull>,
    ships: &[(i64, String, String)],
    body_h: f32,
) -> bool {
    ui.label(
        egui::RichText::new(
            "Never out of doctrine, in any fleet. The fleet commander's own ship is exempt too.",
        )
        .weak(),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Always:").strong());
        for g in crate::fleets::doctrine::SUPPORT_GROUPS {
            ui.label(egui::RichText::new(*g).color(ui.visuals().hyperlink_color));
        }
    });
    ui.add_space(8.0);
    hull_editor(ui, 0, "Hulls", "", hulls, ships, body_h)
}

/// Shortens a label so a long one cannot widen the panel it sits in.
#[cfg(feature = "fleet")]
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}\u{2026}", kept.trim_end())
}

/// One colour per burst, so shield, armor, information and skirmish read apart at a glance.
#[cfg(feature = "fleet")]
fn burst_colour(ui: &egui::Ui, burst: crate::fleets::boosts::Burst) -> egui::Color32 {
    use crate::fleets::boosts::Burst;
    use crate::theme::{chip, standing};
    match burst {
        Burst::Shield => ui.visuals().hyperlink_color,
        Burst::Armor => chip::ISK,
        Burst::Information => chip::STRUCTURE,
        Burst::Skirmish => standing::FRIENDLY,
        Burst::Mining => ui.visuals().weak_text_color(),
    }
}

/// Whether the fleet can fight, and why it reads that way.
///
/// The numbers are the point: "Logi danger" on its own is an argument, "3 of 40, two Guardians in
/// a shield fleet" is something to act on.
#[cfg(feature = "fleet")]
#[allow(clippy::too_many_arguments)]
fn readiness_pane(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    coverage: &[crate::fleets::boosts::Coverage],
    // The boost channel has not been read yet. Saying "not covered" about a channel nobody has
    // looked at is a false alarm, and a fleet's worth of them teaches the FC to ignore the pane.
    boosts_loading: bool,
    wanted: &[crate::fleets::boosts::Wanted],
    marks: &mut crate::fleets::boosts::Forced,
    open_editor: &mut bool,
    detail: &mut Option<String>,
) {
    use crate::fleets::{checks, logi};
    let comp = &open.composition;
    if comp.total() == 0 {
        return;
    }
    let tank = open.doctrine.as_ref().and_then(|d| d.tank).or_else(|| wanted_tank(wanted));
    let report = logi::report(comp, open.doctrine.as_ref(), tank);

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Readiness").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(egui_phosphor::regular::SLIDERS_HORIZONTAL).frame(false))
                .on_hover_text("Set which boosts this doctrine wants")
                .clicked()
            {
                *open_editor = true;
            }
        });
    });
    ui.add_space(2.0);

    status_line(ui, &checks::logi_from(&report));
    detail_line(
        ui,
        format!(
            "{} brought, {} usable, wants {} {}",
            report.brought(),
            report.counted,
            report.size.label(),
            tank.map(|t| t.label()).unwrap_or("logi"),
        ),
        None,
    );
    // One row per hull and reason. Per pilot, a fleet with a dozen wrong-sized logi pushed the
    // boosts and the settings off the bottom of the sidebar; the names are on the hover.
    for g in report.rejected_groups() {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(format!("{} {}", g.pilots.len(), g.ship))
                    .color(crate::theme::standing::HOSTILE),
            );
            ui.label(egui::RichText::new(g.why.label()).weak());
        })
        .response
        .on_hover_text(g.pilots.join("\n"));
    }
    ui.add_space(6.0);

    // The pane's own headline, short: the gaps are listed as chips under it rather than run
    // together into a sentence that wraps three lines in a sidebar.
    let long = checks::boosts(wanted, coverage);
    let gaps = crate::fleets::boosts::gaps_with(wanted, coverage, marks);
    let level = match gaps.first().map(|g| g.priority) {
        Some(crate::fleets::boosts::Priority::High) => checks::Level::Danger,
        Some(crate::fleets::boosts::Priority::Medium) => checks::Level::Warning,
        _ => checks::Level::Fine,
    };
    let (short, level) = match (boosts_loading, wanted.is_empty(), gaps.len()) {
        (true, ..) => ("Reading the boost channel\u{2026}".to_owned(), checks::Level::Fine),
        (_, true, _) => ("Nothing set for this doctrine.".to_owned(), checks::Level::Fine),
        (_, false, 0) => (format!("All {} covered.", wanted.len()), level),
        (_, false, n) => (format!("{n} of {} not covered.", wanted.len()), level),
    };
    status_line(ui, &checks::Check { detail: short, level, ..long });
    if !gaps.is_empty() && !boosts_loading {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            ui.label(egui::RichText::new("Not covered").weak());
            for g in &gaps {
                if gap_chip(ui, g).clicked() {
                    marks.insert(g.what.clone(), true);
                }
            }
        });
    }
    // Anything the FC judged for themselves, and a way back.
    let by_hand: Vec<(String, bool)> = marks.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let mut undo: Option<String> = None;
    if !by_hand.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(14.0);
            ui.label(egui::RichText::new("By hand").weak());
            for (what, on) in &by_hand {
                let text = egui::RichText::new(format!(
                    "{what} {}",
                    if *on { "covered" } else { "not covered" }
                ));
                if ui
                    .add(
                        egui::Button::new(text)
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .stroke(egui::Stroke::new(1.0, ui.visuals().weak_text_color())),
                    )
                    .on_hover_text("Go back to what the channel says")
                    .clicked()
                {
                    undo = Some(what.clone());
                }
            }
        });
    }
    if let Some(what) = undo {
        marks.remove(&what);
    }
    detail_line(ui, checks::booster_line(comp, open.doctrine.as_ref()), None);
    if coverage.is_empty() {
        detail_line(ui, "Nobody has posted in the boost channel.".to_owned(), None);
    }
    for c in coverage {
        let colour = burst_colour(ui, c.burst);
        // Count and mindlink lead, because "two of them and one is linked" is what an FC acts on;
        // which charge it is only matters once that reads badly.
        let ml = match (c.mindlinked, c.pilots) {
            (0, _) => "no ML".to_owned(),
            (m, p) if m >= p => "ML".to_owned(),
            (m, _) => format!("{m} ML"),
        };
        let ml_colour = if c.mindlinked > 0 {
            crate::theme::standing::FRIENDLY
        } else {
            ui.visuals().weak_text_color()
        };
        // Both halves take the click, not just the last one laid out.
        let resp = ui
            .horizontal_wrapped(|ui| {
                ui.add_space(14.0);
                // Fixed cell, so the names line up however many pilots each boost has.
                let lead = cell(ui, 76.0, |ui| {
                    let n = ui.add(
                        egui::Label::new(egui::RichText::new(format!("{}x", c.pilots)).strong())
                            .sense(egui::Sense::click()),
                    );
                    let m = ui.add(
                        egui::Label::new(egui::RichText::new(ml).color(ml_colour))
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .sense(egui::Sense::click()),
                    );
                    n.union(m)
                });
                let name = ui.add(
                    egui::Label::new(egui::RichText::new(&c.what).color(colour))
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .sense(egui::Sense::click()),
                );
                lead.union(name)
            })
            .inner;
        // Opens the breakdown rather than toggling: the interesting question about a covered
        // boost is who is holding it and what they posted, and dropping it is one click inside.
        if resp.on_hover_text("Who is holding this, and what they posted").clicked() {
            *detail = Some(c.what.clone());
        }
    }
    ui.add_space(6.0);

    for check in [checks::interdiction(comp), checks::tackle(comp)] {
        status_line(ui, &check);
    }
    ui.add_space(4.0);
}

/// One boost nobody is on. The important ones carry a filled background, since a colour alone on
/// a red or amber theme is not much of a difference.
#[cfg(feature = "fleet")]
fn gap_chip(ui: &mut egui::Ui, g: &crate::fleets::boosts::Wanted) -> egui::Response {
    use crate::fleets::boosts::Priority;
    let colour = priority_colour(g.priority);
    let text = egui::RichText::new(&g.what).color(colour).strong();
    let button = egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend);
    let button = match g.priority {
        Priority::High => button
            .fill(colour.gamma_multiply(0.25))
            .stroke(egui::Stroke::new(1.0, colour)),
        Priority::Medium => button.stroke(egui::Stroke::new(1.0, colour.gamma_multiply(0.6))),
        Priority::Low => button.frame(false),
    };
    ui.add(button).on_hover_text(format!(
        "{} priority. Click to mark it covered.",
        g.priority.label()
    ))
}

/// One check as a coloured headline plus its reason.
#[cfg(feature = "fleet")]
fn status_line(ui: &mut egui::Ui, check: &crate::fleets::checks::Check) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(level_icon(check.level)).color(level_colour(check.level)));
        ui.label(egui::RichText::new(&check.what).strong().color(level_colour(check.level)));
        ui.label(&check.detail);
    });
}

/// An indented line under a status, optionally led by a coloured name.
#[cfg(feature = "fleet")]
fn detail_line(ui: &mut egui::Ui, text: String, lead: Option<(String, egui::Color32)>) {
    ui.horizontal_wrapped(|ui| {
        ui.add_space(14.0);
        if let Some((name, colour)) = lead {
            ui.add(
                egui::Label::new(egui::RichText::new(name).color(colour))
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        }
        ui.label(egui::RichText::new(text).weak());
    });
}

/// Which way the fleet tanks, taken from the boosts its doctrine asks for. The user maintains that
/// list, so it beats guessing from the doctrine's name.
#[cfg(feature = "fleet")]
fn wanted_tank(wanted: &[crate::fleets::boosts::Wanted]) -> Option<crate::fleets::logi::Tank> {
    use crate::fleets::boosts::{burst_of, Burst};
    use crate::fleets::logi::Tank;
    let mut shield = 0usize;
    let mut armor = 0usize;
    for w in wanted {
        match burst_of(&w.what).or_else(|| Burst::parse(&w.what)) {
            Some(Burst::Shield) => shield += 1,
            Some(Burst::Armor) => armor += 1,
            _ => {}
        }
    }
    match shield.cmp(&armor) {
        std::cmp::Ordering::Greater => Some(Tank::Shield),
        std::cmp::Ordering::Less => Some(Tank::Armor),
        std::cmp::Ordering::Equal => None,
    }
}

/// What can still be changed about a fleet that is already up: what it flies and where it talks.
///
/// Staged rather than live, because every field here is a request: changing the setup three times
/// while making up your mind would be three PUTs and three MOTDs.
#[cfg(feature = "fleet")]
fn fleet_sidebar(
    ui: &mut egui::Ui,
    st: &mut crate::fleets::FleetState,
    open: &crate::fleets::state::OpenFleet,
    read_only: bool,
    act_on: &mut Vec<Action>,
    edit_snowflakes: &mut bool,
) {
    let seed = st.seed.clone();
    let fleet = open.fleet.clone();
    let st_can_access = st.can(Perm::AccessFleet);
    let can = st_can_access && !read_only;
    let st_can_snowflakes = st.can(Perm::ManageFleetSnowflakes);
    let mut open_snowflakes = false;
    let e = &mut st.edit;

    ui.add_space(4.0);
    ui.label(egui::RichText::new("Fleet settings").strong());
    ui.add_space(4.0);

    // Setup and comms describe a fleet that is running. On a closed one they are shown but not
    // offered: editable controls whose change Apply then refuses read as a broken button.
    ui.add_enabled_ui(!read_only, |ui| {
    egui::Grid::new("fleet_sidebar_grid")
        .num_columns(2)
        .min_col_width(60.0)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Setup");
            let current = seed
                .setups
                .iter()
                .find(|s| s.id == e.setup_id)
                .map(|s| s.name.trim().to_owned())
                .unwrap_or_else(|| "== Choose a setup ==".to_owned());
            egui::ComboBox::from_id_salt("sidebar_setup")
                .width(190.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for s in &seed.setups {
                        ui.menu_value(&mut e.setup_id, s.id, s.name.trim());
                    }
                });
            ui.end_row();

            for (label, salt, list, slot) in [
                ("Comms", "sidebar_mumble", &seed.mumble_channels, &mut e.mumble),
                ("Logi", "sidebar_logi", &seed.logi_channels, &mut e.logi),
                ("Boost", "sidebar_boost", &seed.boost_channels, &mut e.boost),
            ] {
                ui.label(label);
                let current = slot
                    .and_then(|id| list.iter().find(|c| c.id == id))
                    .map(|c| c.name.trim().to_owned())
                    .unwrap_or_else(|| "none".to_owned());
                egui::ComboBox::from_id_salt(salt).width(190.0).selected_text(current).show_ui(
                    ui,
                    |ui| {
                        ui.menu_value(slot, None, "none");
                        for c in list {
                            let text = if c.is_in_use {
                                egui::RichText::new(format!("{}  in use", c.name.trim())).weak()
                            } else {
                                egui::RichText::new(c.name.trim().to_owned())
                            };
                            ui.menu_value(slot, Some(c.id), text);
                        }
                    },
                );
                ui.end_row();
            }
        });
    });

    ui.add_space(6.0);
    ui.label("Tags");
    let tags = seed.tags.clone();
    for (salt, primary) in [("track_tag_primary", true), ("track_tag_secondary", false)] {
        let mut pool: Vec<_> = tags.iter().filter(|t| t.is_primary == primary).cloned().collect();
        if primary {
            pool.sort_by_key(|t| (headline_rank(&t.name), t.id.0));
            headline_tags(ui, &pool, &mut e.tags);
        }
        ui.horizontal(|ui| {
            tag_field(ui, salt, &pool, &mut e.tags, primary);
        });
    }

    // The FCs, backseats, logi anchors and hunters. They belong to the fleet rather than to the
    // ping that started it, so a fleet already running can gain a backseat and a closed one can
    // be read back to see who was on it.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Snowflakes");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Not gated on `can`: who was on the fleet is corrected after it ends, which is the
            // whole point of the record.
            if ui
                .add_enabled(st_can_snowflakes, egui::Button::new("Edit"))
                .on_disabled_hover_text(
                    "Your account does not have the manageFleetSnowflakes permission.",
                )
                .clicked()
            {
                open_snowflakes = true;
            }
        });
    });
    if e.snowflakes.is_empty() {
        ui.label(egui::RichText::new("None.").weak());
    }
    for s in &e.snowflakes {
        ui.horizontal(|ui| {
            cell(ui, 70.0, |ui| {
                ui.label(egui::RichText::new(s.kind.label()).color(ui.visuals().hyperlink_color));
            });
            ui.label(&s.character_name);
        });
    }

    // There is no in-game fleet left to carry a MOTD once it has closed.
    if !read_only {
        ui.add_space(6.0);
        ui.checkbox(&mut e.set_motd, "Re-set the MOTD")
            .on_hover_text("The MOTD names the channels, so it goes stale when they change.");
    }
    ui.add_space(6.0);

    let dirty = e.differs(&fleet);
    // A closed fleet takes a snowflake correction and nothing else.
    let sendable = if read_only {
        let (tags, flakes) = e.record_changes(&fleet);
        e.only_record_differs(&fleet)
            && (!tags || st_can_access)
            && (!flakes || st_can_snowflakes)
    } else {
        can && dirty
    };
    ui.horizontal(|ui| {
        let apply = ui
            .add_enabled(
                sendable,
                egui::Button::new(format!("{}  Apply", egui_phosphor::regular::CHECK)),
            )
            .on_disabled_hover_text(if !dirty {
                "Nothing changed."
            } else if read_only {
                "A closed fleet takes changes to its tags and snowflakes, nothing else."
            } else {
                "Your account does not have the accessFleet permission."
            });
        if apply.clicked() {
            act_on.push(Action::Update(Box::new(e.applied(&fleet))));
            if e.set_motd && !read_only {
                act_on.push(Action::SetMotd);
            }
        }
        if ui
            .add_enabled(dirty, egui::Button::new("Discard"))
            .on_disabled_hover_text("Nothing changed.")
            .clicked()
        {
            e.of = None;
            e.seed(&fleet);
        }
    });
    if dirty {
        ui.label(
            egui::RichText::new("Not applied yet.").color(crate::theme::standing::WARNING),
        );
    }
    if open_snowflakes {
        *edit_snowflakes = true;
    }
}

#[cfg(feature = "fleet")]
fn level_colour(level: crate::fleets::checks::Level) -> egui::Color32 {
    use crate::fleets::checks::Level;
    match level {
        Level::Fine => crate::theme::standing::FRIENDLY,
        Level::Warning => crate::theme::standing::WARNING,
        Level::Danger => crate::theme::chip::TACKLED,
        Level::Critical => crate::theme::standing::HOSTILE,
    }
}

#[cfg(feature = "fleet")]
fn level_icon(level: crate::fleets::checks::Level) -> &'static str {
    use crate::fleets::checks::Level;
    use egui_phosphor::regular as icon;
    match level {
        Level::Fine => icon::CHECK_CIRCLE,
        Level::Warning | Level::Danger => icon::WARNING,
        Level::Critical => icon::WARNING_OCTAGON,
    }
}

#[cfg(feature = "fleet")]
fn priority_colour(p: crate::fleets::boosts::Priority) -> egui::Color32 {
    use crate::fleets::boosts::Priority;
    match p {
        Priority::High => crate::theme::standing::HOSTILE,
        Priority::Medium => crate::theme::standing::WARNING,
        Priority::Low => crate::theme::standing::NEUTRAL,
    }
}

/// What can be done to the fleet, and what this account may not do.
#[cfg(feature = "fleet")]
fn action_bar(
    ui: &mut egui::Ui,
    st: &crate::fleets::FleetState,
    act_on: &mut Vec<Action>,
    open_migrate: &mut bool,
) {
    use egui_phosphor::regular as icon;
    ui.horizontal_wrapped(|ui| {
        // The sweep is built from the roster, so it cannot live in the static table below.
        if let Some(open) = st.open.value.as_ref() {
            let victims =
                crate::fleets::doctrine::off_doctrine_kickable(
                    &open.composition,
                    open.doctrine.as_ref(),
                    &st.locked,
                );
            let n = victims.len();
            let allowed = st.can(Perm::KickMember);
            let text = egui::RichText::new(format!("{}  Kick off-doctrine  {n}", icon::BROOM))
                .color(crate::theme::standing::HOSTILE);
            let resp = ui
                .add_enabled(
                    allowed && n > 0,
                    egui::Button::new(text)
                        .stroke(egui::Stroke::new(1.0, crate::theme::standing::HOSTILE)),
                )
                .on_disabled_hover_text(if !allowed {
                    "Your account does not have the kickMember permission.".to_owned()
                } else {
                    "Nobody is in a hull the doctrine refuses.".to_owned()
                });
            let resp = if n > 0 {
                let who: Vec<String> = victims
                    .iter()
                    .take(8)
                    .map(|m| format!("{} in a {}", m.name, m.ship_type_name))
                    .collect();
                let more = n.saturating_sub(who.len());
                resp.on_hover_text(if more > 0 {
                    format!("{}\nand {more} more", who.join("\n"))
                } else {
                    who.join("\n")
                })
            } else {
                resp
            };
            if resp.clicked() {
                act_on.push(Action::KickMany {
                    character_ids: victims.iter().map(|m| m.character_id).collect(),
                    exclude: false,
                });
            }
        }

        // Inviting needs somebody to invite, which is a picker this page does not have yet.
        let actions: [(&str, &str, Action); 5] = [
            (icon::MEGAPHONE, "Set MOTD", Action::SetMotd),
            (icon::STACK, "Add wing", Action::AddWing),
            (icon::PROHIBIT, "Kick pods", Action::KickCapsules),
            (icon::SIGN_OUT, "Kick everyone", Action::KickAll),
            (icon::X_CIRCLE, "Close fleet", Action::Close),
        ];
        for (glyph, label, action) in actions {
            let perm = action.perm();
            let allowed = st.can(perm);
            let text = egui::RichText::new(format!("{glyph}  {label}"));
            let button = match danger(&action) {
                Danger::Severe => egui::Button::new(text.color(crate::theme::standing::HOSTILE))
                    .stroke(egui::Stroke::new(1.0, crate::theme::standing::HOSTILE)),
                Danger::Caution => egui::Button::new(text.color(crate::theme::standing::WARNING)),
                Danger::None => egui::Button::new(text),
            };
            let resp = ui.add_enabled(allowed, button).on_disabled_hover_text(format!(
                "Your account does not have the {} permission.",
                perm.as_str()
            ));
            let resp = if allowed {
                resp.on_hover_text("Records the request this would send. Nothing leaves the app.")
            } else {
                resp
            };
            if resp.clicked() {
                act_on.push(action);
            }
        }
        // Not in the table above: it needs a character picked and boss-checked first, so it opens
        // a dialog rather than sending anything.
        let allowed = st.can(Perm::AccessFleet);
        if ui
            .add_enabled(
                allowed,
                egui::Button::new(egui::RichText::new(format!(
                    "{}  Migrate fleet",
                    egui_phosphor::regular::USER_SWITCH
                ))
                .color(crate::theme::standing::WARNING)),
            )
            .on_disabled_hover_text("Your account does not have the accessFleet permission.")
            .on_hover_text("Hand this fleet to another FC who is already boss of a fleet.")
            .clicked()
        {
            *open_migrate = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("recorded, not sent").color(crate::theme::standing::WARNING),
            );
        });
    });
}

/// Who is in the fleet, by wing and squad.
#[cfg(feature = "fleet")]
#[allow(clippy::too_many_arguments)]
fn members_view(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    locked: &crate::fleets::doctrine::Locked,
    can_move: bool,
    can_kick: bool,
    read_only: bool,
    act_on: &mut Vec<Action>,
    toggle_lock: &mut Option<i64>,
) {
    use crate::fleets::model::Seat;
    let comp = &open.composition;
    if comp.total() == 0 {
        // A fleet closed a moment ago has no report yet: the dashboard writes its statistics after
        // the close, and until it has, the participant list it serves is empty. Saying "nobody"
        // about a fleet that just had fifty people in it reads as a bug, which is what it was.
        let text = if open.fleet.closed_at.is_some() {
            "The dashboard is still writing this fleet's report."
        } else {
            "Nobody in the fleet."
        };
        ui.label(egui::RichText::new(text).weak());
        return;
    }
    let mut drop_on: Option<(i64, Seat)> = None;
    // A roster reads as a table, so the rows sit closer together than the app's default rhythm.
    ui.spacing_mut().item_spacing.y = 2.0;
    // Every column but the name is anchored to this edge, so the tree's indentation eats into the
    // name and nothing else steps right as it nests.
    let left = ui.max_rect().left();

    let mut ctx = RowCtx { locked, can_move, can_kick, read_only, toggle_lock };

    // A closed fleet has no tree to show: the dashboard keeps the participants, the in-game fleet
    // is gone, and the wing and squad ids are the `-1` sentinel. Drawing "Fleet / Roster" around a
    // flat list claims a structure that is not there.
    if comp.flat {
        // Who got paid, most first. A closed fleet is read to settle participation, and the
        // dashboard's own order is the roster's, which answers nothing.
        let mut rows: Vec<&crate::fleets::model::Member> = comp.members().collect();
        rows.sort_by(|a, b| b.pap_count.cmp(&a.pap_count).then_with(|| a.name.cmp(&b.name)));
        for m in rows {
            member_row(
                ui,
                open,
                left,
                m,
                Seat::Squad(crate::fleets::model::WingId(-1), crate::fleets::model::SquadId(-1)),
                None,
                &mut ctx,
                act_on,
            );
        }
        return;
    }

    // Indented like a wing, so the fleet commander has the same left rail as everything under it.
    ui.indent("fleet_boss", |ui| {
        commander_seat(ui, open, left, Seat::Boss, comp.commander.as_ref(), &mut ctx, act_on,
                       &mut drop_on);
    });

    for wing in &comp.wings {
        let pilots: usize = wing.squads.iter().map(|s| s.members.len() + usize::from(s.commander.is_some())).sum::<usize>()
            + usize::from(wing.commander.is_some());
        egui::CollapsingHeader::new(headcount(&wing.name, pilots))
            .id_salt(("wing", wing.id.0))
            .default_open(true)
            .show(ui, |ui| {
                commander_seat(ui, open, left, Seat::WingCommander(wing.id),
                               wing.commander.as_ref(), &mut ctx, act_on, &mut drop_on);
                for squad in &wing.squads {
                    let n = squad.members.len() + usize::from(squad.commander.is_some());
                    egui::CollapsingHeader::new(headcount(&squad.name, n))
                        .id_salt(("squad", wing.id.0, squad.id.0))
                        .default_open(true)
                        .show(ui, |ui| {
                            commander_seat(
                                ui,
                                open,
                                left,
                                Seat::SquadCommander(wing.id, squad.id),
                                squad.commander.as_ref(),
                                &mut ctx,
                                act_on,
                                &mut drop_on,
                            );
                            // The whole squad body takes a drop, so a pilot can be dragged onto an
                            // empty squad as well as onto one with rows in it.
                            let (_, dropped) =
                                ui.dnd_drop_zone::<DragPilot, _>(egui::Frame::NONE, |ui| {
                                    ui.set_min_size(egui::vec2(ui.available_width(), 16.0));
                                    if squad.members.is_empty() {
                                        ui.label(egui::RichText::new("Empty").weak());
                                    }
                                    for m in &squad.members {
                                        member_row(ui, open, left, m,
                                                   Seat::Squad(wing.id, squad.id), None,
                                                   &mut ctx, act_on);
                                    }
                                });
                            if let Some(p) = dropped {
                                drop_on =
                                    Some((p.character_id, Seat::Squad(wing.id, squad.id)));
                            }
                        });
                    ui.add_space(2.0);
                }
            });
        ui.add_space(2.0);
    }

    // A move that would change nothing is not a request worth recording.
    if let Some((character_id, seat)) = drop_on {
        if ctx.can_move && comp.seat_of(character_id) != Some(seat) {
            let (wing, squad) = seat.ids();
            act_on.push(Action::Move { character_id, wing, squad });
        }
    }
}

/// One commander seat. Holds at most one pilot, which is what makes it a seat rather than a list.
#[cfg(feature = "fleet")]
#[allow(clippy::too_many_arguments)]
fn commander_seat(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    left: f32,
    seat: crate::fleets::model::Seat,
    holder: Option<&crate::fleets::model::Member>,
    ctx: &mut RowCtx<'_>,
    act_on: &mut Vec<Action>,
    drop_on: &mut Option<(i64, crate::fleets::model::Seat)>,
) {
    let title = match seat {
        crate::fleets::model::Seat::Boss => "FC",
        crate::fleets::model::Seat::WingCommander(_) => "WC",
        _ => "SC",
    };
    let (_, dropped) = ui.dnd_drop_zone::<DragPilot, _>(egui::Frame::NONE, |ui| {
        ui.set_min_width(ui.available_width());
        match holder {
            Some(m) => member_row(ui, open, left, m, seat, Some(title), ctx, act_on),
            None => {
                ui.horizontal(|ui| {
                    seat_badge(ui, title, seat);
                    ui.label(egui::RichText::new("empty").weak());
                });
            }
        }
    });
    if let Some(p) = dropped {
        // One commander per seat: the holder has to be moved out before anyone else moves in, so
        // the drop is refused rather than silently sending a request the server will reject.
        if holder.is_none() {
            *drop_on = Some((p.character_id, seat));
        }
    }
}

/// A pilot in flight between two squads.
#[cfg(feature = "fleet")]
#[derive(Clone, Copy, PartialEq, Debug)]
struct DragPilot {
    character_id: i64,
    seat: crate::fleets::model::Seat,
}

/// Column widths every fleet table shares, so the member list and the composition line up.
/// The ship column carries an icon and a hull name: "Heavy Interdiction Cruiser" does not fit in
/// what a name alone needs, and a cell that overflows shoves every button after it out of line.
///
/// The group column is sized to that same name, which measures 150px: at 120 it overflowed by 29
/// and every kick button on a hictor row sat out of line with the rest. Measured, not guessed, by
/// `uitest_closed_fleet_rows_keep_their_columns`.
#[cfg(feature = "fleet")]
const COL: [f32; 4] = [190.0, 210.0, 160.0, 90.0];
/// The participation column, wide enough for "no PAP" and a two-digit count.
#[cfg(feature = "fleet")]
const PAP_W: f32 = 70.0;
/// The FC / WC / SC column, present on every roster row so the names align.
#[cfg(feature = "fleet")]
const BADGE_W: f32 = 36.0;
/// The name column at the top of the tree. Nesting comes out of this one.
#[cfg(feature = "fleet")]
const NAME_W: f32 = 230.0;
/// Ship icon beside a hull name.
#[cfg(feature = "fleet")]
const SHIP_ICON: f32 = 18.0;
/// The padlock column, held open on every row so the kick buttons stay in a line.
#[cfg(feature = "fleet")]
const LOCK_W: f32 = 30.0;

/// A wing or squad heading. The count is spelled out: "Squad 1  17" reads as a name with a number
/// stuck to it, which is not what it is.
#[cfg(feature = "fleet")]
fn headcount(name: &str, n: usize) -> String {
    match n {
        0 => format!("{name}  ·  empty"),
        1 => format!("{name}  ·  1 pilot"),
        n => format!("{name}  ·  {n} pilots"),
    }
}

/// Which seat a roster row is, when it is one.
#[cfg(feature = "fleet")]
fn seat_badge(ui: &mut egui::Ui, title: &str, seat: crate::fleets::model::Seat) {
    cell(ui, BADGE_W, |ui| {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(title).strong().color(ui.visuals().hyperlink_color))
            .on_hover_text(seat.label());
    });
}

/// Lays out one cell of a fleet table at a fixed width.
#[cfg(feature = "fleet")]
fn cell<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            add(ui)
        },
    )
    .inner
}

/// One pilot: draggable by the name, with a kick of their own.
#[cfg(feature = "fleet")]
/// What every roster row needs besides the pilot: what the account may do, who is confirmed, and
/// somewhere to put a confirmation.
#[cfg(feature = "fleet")]
struct RowCtx<'a> {
    locked: &'a crate::fleets::doctrine::Locked,
    can_move: bool,
    can_kick: bool,
    /// A closed fleet cannot be acted on at all, so the kick column comes out rather than sitting
    /// greyed in every row. On a live fleet a disabled kick still says why it is disabled.
    read_only: bool,
    toggle_lock: &'a mut Option<i64>,
}

#[cfg(feature = "fleet")]
#[allow(clippy::too_many_arguments)]
fn member_row(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    left: f32,
    m: &crate::fleets::model::Member,
    seat: crate::fleets::model::Seat,
    badge: Option<&str>,
    ctx: &mut RowCtx<'_>,
    act_on: &mut Vec<Action>,
) {
    use crate::fleets::doctrine::Standing;
    let standing =
        crate::fleets::doctrine::classify_in(&open.composition, m, open.doctrine.as_ref());
    // A tint rather than coloured text: on a red or orange theme a hostile-coloured ship name is
    // barely a shade away from a normal one.
    let locked = ctx.locked.contains(&m.character_id);
    // A confirmed pilot loses the tint: the FC has already looked at it.
    let tint = match standing {
        Standing::Unexpected if !locked => {
            Some(crate::theme::standing::HOSTILE.gamma_multiply(0.18))
        }
        _ => None,
    };
    let frame = match tint {
        Some(c) => egui::Frame::NONE.fill(c).inner_margin(egui::Margin::symmetric(0, 1)),
        None => egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 1)),
    };
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            let id = egui::Id::new(("fleet_pilot", m.character_id));
            // Every row carries the badge column, filled or not, so a commander's name starts at
            // the same edge as the pilots under them.
            match badge {
                Some(b) => seat_badge(ui, b, seat),
                None => cell(ui, BADGE_W, |_| {}),
            }
            // The nesting is paid for out of the name column, so the ship, group, role and kick
            // sit on the same edge whatever depth the row is at.
            let indent = (ui.max_rect().left() - left).max(0.0);
            cell(ui, (NAME_W - indent).max(60.0), |ui| {
                if ctx.can_move {
                    let payload = DragPilot { character_id: m.character_id, seat };
                    ui.dnd_drag_source(id, payload, |ui| {
                        ui.label(format!(
                            "{}  {}",
                            egui_phosphor::regular::DOTS_SIX_VERTICAL,
                            m.name
                        ));
                    })
                    .response
                    .on_hover_text("Drag onto another squad to move this pilot.");
                } else {
                    ui.label(&m.name);
                }
            });
            cell(ui, COL[1], |ui| {
                ui.add(
                    egui::Image::new(eve_type_icon_url(m.ship_type_id, SHIP_ICON))
                        .fit_to_exact_size(egui::Vec2::splat(SHIP_ICON)),
                );
                // Truncated, not wrapped or overflowing: the cell is a column in a table and a
                // long hull name must not move the rows beside it.
                ui.add(
                    egui::Label::new(&m.ship_type_name)
                        .wrap_mode(egui::TextWrapMode::Truncate),
                )
                .on_hover_text(format!("{} ({})", m.ship_type_name, standing.label()));
            });
            // Truncated for the same reason as the hull: a table column that grows to its
            // content is not a column.
            cell(ui, COL[2], |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(&m.ship_group).weak())
                        .wrap_mode(egui::TextWrapMode::Truncate),
                )
                .on_hover_text(&m.ship_group);
            });
            cell(ui, COL[3], |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(&m.role).weak())
                        .wrap_mode(egui::TextWrapMode::Truncate),
                )
                .on_hover_text(&m.role);
            });
            // Only a closed fleet's report carries PAPs. On a live roster the column would be a
            // row of zeroes, which reads as "nobody got paid" rather than "not known yet".
            if open.composition.flat {
                cell(ui, PAP_W, |ui| {
                    let text = match m.pap_count {
                        0 => egui::RichText::new("no PAP").weak(),
                        n => egui::RichText::new(format!("{n} PAP")).strong(),
                    };
                    ui.label(text).on_hover_text(
                        "Participation credits the dashboard recorded for this pilot in this \
                         fleet.",
                    );
                });
            }
            // A fixed column whether or not the row has a lock in it, or every row that does
            // pushes its kick button out of line with the rows that do not.
            cell(ui, LOCK_W, |ui| {
                // Only offered where it means something: a hull the doctrine refuses, which the
                // FC can confirm is meant to be there.
                if !(standing.odd() || locked) {
                    return;
                }
                let glyph = if locked {
                    egui_phosphor::regular::LOCK
                } else {
                    egui_phosphor::regular::LOCK_SIMPLE_OPEN
                };
                let text = egui::RichText::new(glyph).color(if locked {
                    crate::theme::standing::FRIENDLY
                } else {
                    ui.visuals().weak_text_color()
                });
                if ui
                    .add(egui::Button::new(text).frame(false))
                    .on_hover_text(if locked {
                        "Confirmed as meant to be here. Click to take it back."
                    } else {
                        "Confirm this ship is meant to be here"
                    })
                    .clicked()
                {
                    *ctx.toggle_lock = Some(m.character_id);
                }
            });
            if ctx.read_only {
                return;
            }
            if ui
                .add_enabled(
                    ctx.can_kick,
                    egui::Button::new(egui_phosphor::regular::SIGN_OUT).frame(false),
                )
                .on_disabled_hover_text("Your account does not have the kickMember permission.")
                .on_hover_text(format!("Kick {}", m.name))
                .clicked()
            {
                act_on.push(Action::Kick { character_id: m.character_id, exclude: false });
            }
        });
    });
}

/// What the fleet is flying, and whether the doctrine asked for it.
#[cfg(feature = "fleet")]
fn composition_view(
    ui: &mut egui::Ui,
    open: &crate::fleets::state::OpenFleet,
    off_doctrine: &[crate::fleets::doctrine::OffDoctrine],
) {
    use crate::fleets::doctrine::{by_ship, unexpected_pilots, Standing};
    let doctrine = open.doctrine.as_ref();
    let lines = by_ship(&open.composition, doctrine);
    if lines.is_empty() {
        ui.label(egui::RichText::new("Nobody in the fleet.").weak());
        return;
    }
    // Everything above the tables lives in the readiness pane now, so the tab is the tables.
    let total = open.composition.total().max(1);

    // The doctrine's hulls and the always-allowed ones together, split by the job each does: an FC
    // reads a fleet as four questions, and a cyno or a spare logi answers one of them whatever list
    // it came off. The always-allowed rows say so in their last column.
    let core: Vec<_> = lines
        .iter()
        .filter(|l| matches!(l.standing, Standing::Doctrine | Standing::Support))
        .collect();
    let command_ships = core
        .iter()
        .any(|l| l.group.trim().eq_ignore_ascii_case("Command Ship"));
    for role in crate::fleets::doctrine::Role::ALL {
        let in_role: Vec<_> = core
            .iter()
            .filter(|l| composition_role(l, doctrine, command_ships) == role)
            .collect();
        if in_role.is_empty() {
            continue;
        }
        section_head(ui, role.label(), in_role.iter().map(|l| l.count).sum(), Standing::Doctrine);
        egui::Grid::new(("comp_doctrine", role.label()))
            .num_columns(4)
            .striped(true)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                for l in &in_role {
                    let note = (l.standing == Standing::Support)
                        .then(|| "always allowed".to_owned());
                    share_row(ui, l.type_id, &l.name, l.count, total, note);
                }
            });
        ui.add_space(6.0);
    }

    let odd_lines: Vec<_> = lines.iter().filter(|l| l.standing.odd()).collect();
    if !odd_lines.is_empty() {
        section_head(
            ui,
            "Not in doctrine",
            odd_lines.iter().map(|l| l.count).sum(),
            Standing::Unexpected,
        );
        egui::Grid::new("comp_odd").num_columns(4).striped(true).spacing([12.0, 3.0]).show(
            ui,
            |ui| {
                for l in &odd_lines {
                    share_row(ui, l.type_id, &l.name, l.count, total, Some(l.group.clone()));
                }
            },
        );
        ui.add_space(6.0);
    }

    let odd = unexpected_pilots(&lines);
    off_doctrine_report(ui, off_doctrine, open.at, odd);
}

/// Which of the four sections a hull is counted under.
///
/// A capital that is only in the fleet because capitals are always allowed is there to bridge or
/// light a cyno, not to shoot, so it counts as support rather than as the DPS a doctrine capital
/// would be. Everything else goes by what the hull does.
#[cfg(feature = "fleet")]
fn composition_role(
    l: &crate::fleets::doctrine::ShipLine,
    doctrine: Option<&crate::fleets::doctrine::Doctrine>,
    command_ships: bool,
) -> crate::fleets::doctrine::Role {
    use crate::fleets::doctrine::{Category, Role, Standing};
    if l.standing == Standing::Support && l.category == Category::Capital {
        return Role::Support;
    }
    Role::of(l, doctrine, command_ships)
}

/// The heading over one composition table.
#[cfg(feature = "fleet")]
fn section_head(
    ui: &mut egui::Ui,
    title: &str,
    pilots: usize,
    standing: crate::fleets::doctrine::Standing,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().color(standing_colour(ui, standing)));
        ui.label(egui::RichText::new(format!("{pilots}")).weak());
    });
}

/// One row of a composition table: what, how many, what share, and what it is made of.
#[cfg(feature = "fleet")]
fn share_row(
    ui: &mut egui::Ui,
    type_id: i64,
    name: &str,
    count: usize,
    total: usize,
    detail: Option<String>,
) {
    const ICON: f32 = 24.0;
    let share = count as f32 / total as f32;
    cell(ui, COL[0], |ui| {
        // A fixed cell either way, so a row with no hull behind it still lines up.
        cell(ui, ICON + 4.0, |ui| {
            if type_id > 0 {
                ui.add(
                    egui::Image::new(crate::app::eve_type_icon_url(type_id, ICON))
                        .fit_to_exact_size(egui::vec2(ICON, ICON)),
                );
            }
        });
        ui.label(name);
    });
    cell(ui, 40.0, |ui| {
        ui.label(egui::RichText::new(format!("{count}")).strong());
    });
    cell(ui, 110.0, |ui| {
        ui.label(egui::RichText::new(format!("{:.0}%", share * 100.0)).weak());
        ui.add(egui::ProgressBar::new(share).desired_width(70.0).desired_height(6.0));
    });
    cell(ui, COL[1] + COL[2], |ui| {
        if let Some(d) = detail {
            ui.label(egui::RichText::new(d).weak());
        }
    });
    ui.end_row();
}

/// Who has been in the wrong ship long enough that it was not a mistake on undock.
#[cfg(feature = "fleet")]
fn off_doctrine_report(
    ui: &mut egui::Ui,
    rows: &[crate::fleets::doctrine::OffDoctrine],
    now: i64,
    odd: usize,
) {
    use crate::fleets::doctrine::OFF_DOCTRINE_GRACE;
    if rows.is_empty() {
        return;
    }
    let late = rows.iter().filter(|r| now - r.since >= OFF_DOCTRINE_GRACE).count();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Who is off doctrine")
                .strong()
                .color(crate::theme::standing::HOSTILE),
        );
        let _ = odd;
        ui.label(
            egui::RichText::new(match late {
                0 => format!("{}", rows.len()),
                n => format!("{}, {n} for over {} minutes", rows.len(), OFF_DOCTRINE_GRACE / 60),
            })
            .weak(),
        );
    });
    // One table's worth of width, so two fit side by side only where there is genuinely room.
    const TABLE_W: f32 = COL[0] + 150.0 + COL[1] + COL[2] + 36.0;
    // A wide window leaves this list scrolling down a mostly empty page. Splitting it in two is
    // reactive rather than a setting: the column count follows what fits.
    let cols = if ui.available_width() >= 2.0 * TABLE_W { 2 } else { 1 };
    let per = rows.len().div_ceil(cols);
    ui.horizontal_top(|ui| {
        for (c, chunk) in rows.chunks(per.max(1)).enumerate() {
            egui::Grid::new(("comp_off_doctrine", c))
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 3.0])
                .show(ui, |ui| {
                    for r in chunk {
                        let age = now - r.since;
                        // Long enough that it was not a mistake on undock.
                        let loud = age >= OFF_DOCTRINE_GRACE;
                        cell(ui, COL[0], |ui| {
                            ui.label(&r.name);
                        });
                        cell(ui, 150.0, |ui| {
                            // The clock only counts time this app was watching the fleet, so on a
                            // page just opened it is zero for everyone. A row of "0s" says
                            // nothing; say what is actually known instead.
                            let text = if age <= 0 {
                                egui::RichText::new("just seen").weak()
                            } else if loud {
                                egui::RichText::new(fmt_age(age))
                                    .strong()
                                    .color(crate::theme::standing::HOSTILE)
                            } else {
                                egui::RichText::new(fmt_age(age)).weak()
                            };
                            ui.label(text).on_hover_text(
                                "Counted from when this app first saw the pilot in this hull, \
                                 not from when they got into it.",
                            );
                        });
                        cell(ui, COL[1] + COL[2], |ui| {
                            ui.label(
                                egui::RichText::new(&r.ship)
                                    .color(crate::theme::standing::HOSTILE),
                            );
                        });
                        ui.end_row();
                    }
                });
            if c + 1 < cols {
                ui.add_space(16.0);
            }
        }
    });
    ui.add_space(6.0);
}

/// Doctrine reads as normal, support as a quiet aside, anything else as a problem.
#[cfg(feature = "fleet")]
fn standing_colour(ui: &egui::Ui, standing: crate::fleets::doctrine::Standing) -> egui::Color32 {
    use crate::fleets::doctrine::Standing;
    match standing {
        Standing::Doctrine => ui.visuals().text_color(),
        Standing::Support => ui.visuals().hyperlink_color,
        Standing::Unexpected => crate::theme::standing::HOSTILE,
    }
}

impl SpaiApp {
    /// The preset picker: type, pick, and the start form comes up filled in.
    ///
    /// A window rather than a menu, because there are enough presets across enough folders that
    /// the search is the point.
    #[cfg(feature = "fleet")]
    pub(crate) fn quick_fleet_window(
        &mut self,
        ctx: &egui::Context,
        presets: &[crate::settings::FleetPreset],
        act: &mut FormAct,
    ) {
        if !self.fleet_quick_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Quick Fleet")
            .open(&mut open)
            .default_size([340.0, 420.0])
            .collapsible(false)
            .show(ctx, |ui| {
                if presets.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "No presets yet. Fill the start form and save it as one.",
                        )
                        .weak(),
                    );
                    return;
                }
                let search_id = ui.id().with("quick_search");
                let mut query: String = ui.data(|d| d.get_temp(search_id).unwrap_or_default());
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut query)
                        .hint_text("Search presets")
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();
                if edit.changed() {
                    ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
                }
                ui.separator();

                let tags = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.tags.clone();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut any = false;
                    let mut groups: Vec<String> = vec![String::new()];
                    groups.extend(preset_folders(presets));
                    for folder in groups {
                        let hits: Vec<usize> = presets
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| p.folder.trim() == folder)
                            // The folder name counts as part of what a preset is called, so
                            // typing the folder finds everything in it.
                            .filter(|(_, p)| {
                                tag_matches(&format!("{} {}", p.folder, p.label), &query)
                            })
                            .map(|(i, _)| i)
                            .collect();
                        if hits.is_empty() {
                            continue;
                        }
                        any = true;
                        if !folder.is_empty() {
                            ui.label(egui::RichText::new(&folder).strong());
                        }
                        for i in hits {
                            let p = &presets[i];
                            // The description first, then the fleet name, and neither when it
                            // only repeats the label the button already carries.
                            let sub = [p.description.as_str(), p.name.as_str()]
                                .into_iter()
                                .map(str::trim)
                                .find(|s| !s.is_empty() && !s.eq_ignore_ascii_case(p.label.trim()))
                                .unwrap_or_default();
                            let resp = ui
                                .horizontal(|ui| {
                                    preset_kind_cell(ui, preset_kind(p, &tags));
                                    ui.add(
                                        egui::Button::new(format!("{}   {sub}", p.label))
                                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                                    )
                                })
                                .inner;
                            if resp.clicked() {
                                act.quick_preset = Some(i);
                            }
                        }
                        ui.add_space(4.0);
                    }
                    if !any {
                        ui.label(egui::RichText::new("Nothing matches.").weak());
                    }
                });
            });
        if !open {
            self.fleet_quick_open = false;
        }
    }

    /// Asks before anything that cannot be taken back.
    #[cfg(feature = "fleet")]
    pub(crate) fn fleet_confirm_modal(&mut self, ctx: &egui::Context) {
        let Some((id, action, question, pilots)) = self.fleet_confirm.clone() else { return };
        let mut decided: Option<bool> = None;
        egui::Modal::new(egui::Id::new("fleet_confirm")).show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.heading(egui::RichText::new(question).color(match danger(&action) {
                Danger::Severe => crate::theme::standing::HOSTILE,
                _ => ui.visuals().text_color(),
            }));
            if let Some(line) = confirm_consequence(&action, pilots) {
                ui.label(line);
            }
            ui.label(
                egui::RichText::new("This build records the request and sends nothing.").weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    decided = Some(false);
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Do it").color(crate::theme::standing::HOSTILE),
                        )
                        .stroke(egui::Stroke::new(1.0, crate::theme::standing::HOSTILE)),
                    )
                    .clicked()
                {
                    decided = Some(true);
                }
            });
        });
        match decided {
            Some(true) => {
                self.fleet_confirm = None;
                self.fleet_dispatch(Cmd::Act(id, action));
            }
            Some(false) => self.fleet_confirm = None,
            None => {}
        }
    }
}

/// The chip the sub-nav shows for a backend that is not fully live, and why.
#[cfg(feature = "fleet")]
fn mode_banner(mode: crate::fleets::backend::Mode) -> Option<(&'static str, &'static str)> {
    use crate::fleets::backend::Mode;
    match mode {
        Mode::DryRun => Some((
            "DRY RUN - nothing is sent",
            "Every action records the request it would send and sends none of it.",
        )),
        Mode::ReadOnly => Some((
            "READ ONLY - writes are held",
            "The data is live. Actions record the request they would send and send none of it.",
        )),
        Mode::Live => None,
    }
}

#[cfg(feature = "fleet")]
fn journal_hint(mode: crate::fleets::backend::Mode) -> &'static str {
    use crate::fleets::backend::Mode;
    match mode {
        Mode::DryRun | Mode::ReadOnly => "Requests this tab would have sent. Nothing was.",
        Mode::Live => "Requests this tab has sent.",
    }
}

#[cfg(all(test, feature = "fleet"))]
mod doctrine_line_tests {
    use super::doctrine_line;

    #[test]
    fn takes_the_priority_order_and_leaves_the_notes() {
        let ping = "CAP Save\n\nFC Name: Someone\nComms: Op 1 https://example.invalid/x.html\n\
                    Doctrine: Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)\n\
                    Bring a spare probe launcher";
        assert_eq!(
            doctrine_line(ping).as_deref(),
            Some("Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)")
        );
    }

    #[test]
    fn a_ping_without_one_caches_nothing() {
        assert_eq!(doctrine_line("FC Name: Someone\nComms: Op 4"), None);
        assert_eq!(doctrine_line("Doctrine:   "), None);
    }
}

#[cfg(all(test, feature = "fleet"))]
mod boost_window_tests {
    use super::{boost_window_start, BOOST_GRACE};

    const START: i64 = 1_700_000_000;

    fn ping(at: i64) -> Vec<(String, String, bool, i64)> {
        vec![(
            "directorbot".to_owned(),
            "Hammer Fleet forming, Op 1, get in".to_owned(),
            false,
            at,
        )]
    }

    /// A closed fleet never consults the chat buffer: its own record is better and always there.
    #[test]
    fn a_finished_fleet_counts_from_its_recorded_start() {
        let from =
            boost_window_start(&ping(START + 600), "Hammer Fleet", "Op 1", START, Some(START + 3600));
        assert_eq!(from, START - BOOST_GRACE);
    }

    #[test]
    fn a_live_fleet_counts_from_the_ping() {
        let from = boost_window_start(&ping(START + 600), "Hammer Fleet", "Op 1", START, None);
        assert_eq!(from, START + 600);
    }

    /// The ping is out of the buffer, which is every fleet older than the chat history. Counting
    /// from the start alone missed everyone who was ready before the FC made the fleet.
    #[test]
    fn no_ping_in_the_buffer_still_gets_the_grace() {
        assert_eq!(
            boost_window_start(&[], "Hammer Fleet", "Op 1", START, None),
            START - BOOST_GRACE
        );
    }
}

#[cfg(feature = "fleet")]
impl SpaiApp {
    /// Hands the fleet to another FC.
    ///
    /// The same shape the dashboard uses: pick a character, ask whether they are the boss of an
    /// in-game fleet, and only then offer the button. Handing a fleet to someone who is not
    /// already boss of a fleet leaves the record pointing at nothing.
    pub(crate) fn fleet_migrate_window(&mut self, ctx: &egui::Context) {
        if !self.fleet_migrate_open {
            return;
        }
        const NAME: &str = "fleet_migrate_name";
        let mut open = true;
        let mut search: Option<String> = None;
        let mut check: Option<i64> = None;
        let mut migrate: Option<i64> = None;
        egui::Window::new("Migrate fleet")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
                let hits = st.found_characters.clone();
                let boss = st.migrate_boss.clone();
                // Without startFleetOther the site limits the picker to the account's own
                // characters, so the same list is offered here.
                let mine: Vec<crate::fleets::model::Labelled> = st
                    .seed
                    .characters
                    .iter()
                    .map(|c| crate::fleets::model::Labelled {
                        id: c.id,
                        label: c.name.clone(),
                    })
                    .collect();
                let any = st.can(Perm::StartFleetOther);
                drop(st);

                ui.label("Boss of the new fleet");
                ui.add_space(4.0);
                let name_id = egui::Id::new(NAME);
                let mut name: String = ui.data(|d| d.get_temp(name_id).unwrap_or_default());
                if any {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("Character name")
                            .desired_width(240.0),
                    );
                    if edit.changed() {
                        search = Some(name.trim().to_owned());
                    }
                    ui.data_mut(|d| d.insert_temp(name_id, name.clone()));
                    if hits.loading {
                        ui.label(egui::RichText::new("looking").weak());
                    }
                } else {
                    ui.label(
                        egui::RichText::new(
                            "Your account may only hand a fleet to its own characters.",
                        )
                        .weak(),
                    );
                }

                let pool: Vec<crate::fleets::model::Labelled> = if any {
                    hits.value.clone().unwrap_or_default()
                } else {
                    mine
                };
                let picked: Option<crate::fleets::model::Labelled> =
                    ui.data(|d| d.get_temp(egui::Id::new("fleet_migrate_pick")));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for l in pool.iter().take(8) {
                        let on = picked.as_ref().is_some_and(|p| p.id == l.id);
                        if selectable_chip(ui, on, l.label.trim()).clicked() {
                            ui.data_mut(|d| {
                                d.insert_temp(egui::Id::new("fleet_migrate_pick"), l.clone())
                            });
                            check = Some(l.id);
                        }
                    }
                });

                ui.add_space(6.0);
                let Some(who) = picked else {
                    ui.label(egui::RichText::new("Pick a character.").weak());
                    return;
                };
                let answer = boss.as_ref().filter(|(id, _)| *id == who.id).map(|(_, c)| c);
                let ready = match answer {
                    None => {
                        ui.label(egui::RichText::new("Checking the fleet boss\u{2026}").weak());
                        false
                    }
                    Some(c) if c.is_fleet_boss => {
                        ui.label(
                            egui::RichText::new(format!("{} is boss of a fleet.", who.label))
                                .color(crate::theme::standing::FRIENDLY),
                        );
                        true
                    }
                    Some(c) => {
                        ui.label(
                            egui::RichText::new(match c.error_message.as_deref() {
                                Some(m) if !m.trim().is_empty() => m.trim().to_owned(),
                                _ => format!(
                                    "{} is not online, or not boss of a fleet.",
                                    who.label
                                ),
                            })
                            .color(crate::theme::standing::WARNING),
                        );
                        false
                    }
                };

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            ready,
                            egui::Button::new(format!(
                                "{}  Migrate",
                                egui_phosphor::regular::HAND_ARROW_DOWN
                            )),
                        )
                        .on_disabled_hover_text(
                            "The character has to be boss of an in-game fleet first.",
                        )
                        .clicked()
                    {
                        migrate = Some(who.id);
                    }
                });
            });

        if let Some(v) = search {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).found_characters.begin();
            self.fleet_dispatch(Cmd::Search { kind: crate::fleets::backend::SearchKind::Character, value: v });
        }
        if let Some(id) = check {
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).migrate_boss = None;
            self.fleet_dispatch(Cmd::CheckMigrateBoss { character_id: id });
        }
        if let Some(character_id) = migrate {
            let page = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
            if let Some(fleet) = page.fleet().cloned() {
                self.fleet_dispatch(Cmd::Act(fleet, Action::Migrate { character_id }));
            }
            open = false;
        }
        if !open {
            self.fleet_migrate_open = false;
            self.fleet.lock().unwrap_or_else(|e| e.into_inner()).migrate_boss = None;
            ctx.data_mut(|d| {
                d.remove::<crate::fleets::model::Labelled>(egui::Id::new("fleet_migrate_pick"));
                d.remove::<String>(egui::Id::new(NAME));
            });
        }
    }
}

#[cfg(all(test, feature = "fleet"))]
mod composition_role_tests {
    use super::composition_role;
    use crate::fleets::doctrine::{Category, Role, ShipLine, Standing};

    fn line(name: &str, group: &str, standing: Standing) -> ShipLine {
        ShipLine {
            type_id: 1,
            name: name.to_owned(),
            group: group.to_owned(),
            category: Category::of(group),
            count: 1,
            standing,
        }
    }

    /// Always-allowed hulls sit in the section for the job they do, next to the doctrine's own.
    #[test]
    fn always_allowed_hulls_join_the_role_sections() {
        let support = Standing::Support;
        assert_eq!(composition_role(&line("Falcon", "Force Recon Ship", support), None, false),
                   Role::Support);
        assert_eq!(composition_role(&line("Sabre", "Interdictor", support), None, false),
                   Role::Support);
        // Not always support: a logi or a line hull on the list is counted as what it is.
        assert_eq!(composition_role(&line("Scimitar", "Logistics", support), None, false),
                   Role::Logi);
        assert_eq!(composition_role(&line("Ferox Navy Issue", "Combat Battlecruiser", support),
                                    None, false),
                   Role::Dps);
    }

    /// A titan on the always-allowed list is there to bridge. Counted as DPS it would inflate the
    /// number an FC reads first, by a hull that will never shoot.
    #[test]
    fn a_bridging_titan_is_support_not_dps() {
        assert_eq!(composition_role(&line("Avatar", "Titan", Standing::Support), None, false),
                   Role::Support);
        // A doctrine that flies capitals keeps them where they are.
        assert_eq!(composition_role(&line("Avatar", "Titan", Standing::Doctrine), None, false),
                   Role::Dps);
    }
}

#[cfg(all(test, feature = "fleet"))]
mod preset_kind_tests {
    use super::preset_kind;

    fn preset(tags: &[i32]) -> crate::settings::FleetPreset {
        crate::settings::FleetPreset { tag_ids: tags.to_vec(), ..Default::default() }
    }

    /// S for STRATEGIC, P for every other primary tag, nothing without one.
    #[test]
    fn a_preset_is_strat_or_pct_by_its_primary_tag() {
        let tags = crate::fleets::seed::invented().tags;
        let letter = |ids: &[i32]| preset_kind(&preset(ids), &tags).map(|(l, _, _)| l);
        assert_eq!(letter(&[1, 12]), Some("S"), "STRATEGIC");
        assert_eq!(letter(&[2, 33]), Some("P"), "PEACETIME");
        // Primary and flagged strategic by the dashboard, but not the STRATEGIC tag: P for now.
        assert_eq!(letter(&[20]), Some("P"), "another primary tag");
        // Only secondary tags, or none: no claim either way.
        assert_eq!(letter(&[12, 33]), None);
        assert_eq!(letter(&[]), None);
    }
}

#[cfg(all(test, feature = "fleet"))]
mod preset_order_tests {
    use super::{preset_folders, reorder_folder, reorder_preset};
    use crate::settings::FleetPreset;

    fn p(folder: &str, label: &str) -> FleetPreset {
        FleetPreset { label: label.to_owned(), folder: folder.to_owned(), ..Default::default() }
    }

    fn keys(all: &[FleetPreset]) -> Vec<String> {
        all.iter().map(|p| format!("{}/{}", p.folder, p.label)).collect()
    }

    #[test]
    fn a_preset_goes_before_the_one_it_was_dropped_on() {
        let mut all = vec![p("", "A"), p("", "B"), p("", "C")];
        reorder_preset(&mut all, 2, 0);
        assert_eq!(keys(&all), ["/C", "/A", "/B"]);
        // Down the list: the target shifts back once the dragged one is out of the way.
        reorder_preset(&mut all, 0, 2);
        assert_eq!(keys(&all), ["/A", "/C", "/B"]);
        // Onto a row in another folder: it joins that folder.
        let mut all = vec![p("", "A"), p("F", "B")];
        reorder_preset(&mut all, 0, 1);
        assert_eq!(keys(&all), ["F/A", "F/B"]);
    }

    /// A folder moves as a block and keeps its presets' order, and the list order is the folder
    /// order: nothing sorts them any more.
    #[test]
    fn a_folder_goes_before_the_one_it_was_dropped_on() {
        let mut all = vec![p("", "top"), p("F1", "a"), p("F2", "x"), p("F1", "b"), p("F2", "y")];
        assert_eq!(preset_folders(&all), ["F1", "F2"]);
        reorder_folder(&mut all, "F2", "F1");
        assert_eq!(preset_folders(&all), ["F2", "F1"]);
        assert_eq!(keys(&all), ["/top", "F2/x", "F2/y", "F1/a", "F1/b"]);
        // Not alphabetical: a folder named later can come first.
        let all = vec![p("Zulu", "z"), p("Alpha", "a")];
        assert_eq!(preset_folders(&all), ["Zulu", "Alpha"]);
    }
}
