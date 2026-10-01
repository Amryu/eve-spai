//! The web app: signs in, joins a group through its invite link, waits to be let in, then shows
//! the group's wormhole map.

use std::sync::{Arc, Mutex};

use spai_core::universe::Universe;
use spai_share::engine::Cmd;
use spai_share::store::{SharePrefs, ShareStore as _};
use spai_ui::wh_graph::WhGraphView;

use crate::auth::{self, Auth};
use crate::host::WebHost;
use crate::page;
use crate::starmap::{MapData, Network, StarMap};
use crate::sync::{invite_link, Sync};

type Loading = Arc<Mutex<Option<Result<Universe, String>>>>;

const PREFS: &str = "spai.wh.prefs";
const LAYOUT: &str = "spai.wh.layout";
const ROUTE_PREFS: &str = "spai.route.prefs";
const DETECT: &str = "spai.detect";
const SCOUT: &str = "spai.scout";
const FILTER: &str = "spai.wh.filter";
/// The tab, the map's layers and its region, as they were left.
const VIEW: &str = "spai.view";

/// What the page looks like between loads.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct SavedView {
    tab: Tab,
    layers: crate::starmap::Layers,
    region: Option<i64>,
}
/// EVE-Scout's feed changes as its scouts report; five minutes, as the desktop polls it.
const SCOUT_EVERY: i64 = 300;

pub struct WebApp {
    loading: Loading,
    host: Option<WebHost>,
    view: WhGraphView,
    error: Option<String>,
    auth: auth::Shared,
    sync: Option<Sync>,
    /// The store's generation the map last took its holes at.
    shown: u64,
    joined: bool,
    /// "Open in EVE Spai" was pressed, so say what to do if nothing opened.
    tried_app: bool,
    tab: Tab,
    map: StarMap,
    map_data: Option<MapData>,
    network: Arc<Mutex<Option<Network>>>,
    asked_network: bool,
    group: crate::group::GroupTab,
    accounts: crate::accounts::web::Shared,
    added: auth::Added,
    /// What to ask for when adding a character: location, waypoints, skills.
    add_scopes: (bool, bool, bool),
    route_note: std::rc::Rc<std::cell::RefCell<Option<Result<String, String>>>>,
    skills: std::rc::Rc<std::cell::RefCell<Option<Result<(u32, u32), String>>>>,
    /// Ask about the holes the added characters' jumps look like they went through.
    detect: bool,
    /// Thera and Turnur holes from EVE-Scout: whether to use them, what last came in, and when it
    /// was asked for. They stay in this browser, never shared to the group.
    scout_on: bool,
    scout_in: Arc<Mutex<Option<Vec<spai_core::wormholes::Wormhole>>>>,
    scout: Vec<spai_core::wormholes::Wormhole>,
    scout_at: i64,
    scout_changed: bool,
    /// The view as last saved, to save it again only when it changes.
    view_saved: Option<SavedView>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
enum Tab {
    #[default]
    Wormholes,
    Map,
    Group,
}

impl WebApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        spai_ui::theme::install_fonts_opts(&cc.egui_ctx, false);
        // The theme is EVE Spai's own, not the browser's light or dark preference.
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        spai_ui::theme::Theme::default().apply(&cc.egui_ctx);
        let loading: Loading = Default::default();
        let (slot, ctx) = (loading.clone(), cc.egui_ctx.clone());
        ehttp::fetch(ehttp::Request::get(format!("{}universe.json.gz", page::base())), move |r| {
            let result = match r {
                Ok(resp) if resp.ok => Universe::from_gz(&resp.bytes),
                Ok(resp) => Err(format!("the universe file answered {} {}", resp.status, resp.status_text)),
                Err(e) => Err(e),
            };
            *slot.lock().unwrap() = Some(result);
            ctx.request_repaint();
        });
        let accounts = crate::accounts::web::load();
        let added: auth::Added = Default::default();
        let auth = auth::start(&cc.egui_ctx, &accounts, &added);
        WebApp { loading, host: None, view: WhGraphView::default(), error: None, auth, sync: None,
            shown: u64::MAX,
            joined: false,
            tried_app: false,
            tab: page::load::<SavedView>(VIEW).map_or_else(Tab::default, |v| v.tab),
            map: {
                let mut m = StarMap::default();
                if let Some(v) = page::load::<SavedView>(VIEW) {
                    m.layers = v.layers;
                    m.region = v.region;
                }
                m.layers.scout = page::load(SCOUT).unwrap_or(m.layers.scout);
                m
            },
            map_data: None,
            network: Default::default(),
            asked_network: false,
            group: Default::default(),
            accounts,
            added,
            add_scopes: (true, true, true),
            detect: page::load(DETECT).unwrap_or(true),
            scout_on: page::load(SCOUT).unwrap_or(true),
            scout_in: Default::default(),
            scout: Vec::new(),
            scout_at: 0,
            scout_changed: false,
            view_saved: None,
            route_note: Default::default(),
            skills: Default::default(),
        }
    }

    /// The added characters: where they are, into both maps and the route planner, and what the
    /// planner asked of ESI.
    fn run_accounts(&mut self, ctx: &egui::Context) {
        use crate::accounts::{web, SKILLS, WAYPOINT};
        if !self.accounts.borrow().list.is_empty() {
            web::poll(&self.accounts, ctx);
        }
        // Jumps that look like holes, for the corner card; only for someone who may add holes.
        let moves = std::mem::take(&mut self.accounts.borrow_mut().moves);
        if let Some(h) = self.host.as_mut().filter(|h| self.detect && h.can_edit) {
            let now = spai_core::clock::utc().timestamp();
            h.not_holes.retain(|_, t| now - *t < 3600);
            for m in moves {
                if let Some((certain, cands)) = crate::accounts::judge(&h.geo, &m, &h.holes, &h.not_holes) {
                    h.detected(&m.name, m.from, m.to, m.at, certain, cands);
                }
            }
        }
        let (chars, pilots, here) = {
            let a = self.accounts.borrow();
            let chars: std::collections::HashMap<String, (i64, bool)> =
                a.list.iter().filter_map(|x| a.live.get(&x.char_id).map(|l| (x.name.clone(), *l))).collect();
            let pilots: Vec<crate::planner::Pilot> = a
                .list
                .iter()
                .map(|x| crate::planner::Pilot {
                    char_id: x.char_id,
                    name: x.name.clone(),
                    system: a.live.get(&x.char_id).map(|l| l.0),
                    waypoints: x.can(WAYPOINT),
                    skills: x.can(SKILLS),
                })
                .collect();
            let mut here: std::collections::HashMap<i64, (usize, bool)> = std::collections::HashMap::new();
            for (sys, online) in a.live.values() {
                let e = here.entry(*sys).or_default();
                e.0 += 1;
                e.1 |= *online;
            }
            (chars, pilots, here)
        };
        if let Some(h) = &mut self.host {
            h.chars = chars;
        }
        self.map.here = here;
        self.map.plan.pilots = pilots;
        match self.map.plan.request.take() {
            Some(crate::planner::PlanRequest::SetRoute(id, path)) => {
                let (slot, ctx2, n) = (self.route_note.clone(), ctx.clone(), path.len());
                web::set_route(&self.accounts, id, path, move |r| {
                    *slot.borrow_mut() = Some(r.map(|()| format!("Set in game: {n} waypoint{}", if n == 1 { "" } else { "s" })));
                    ctx2.request_repaint();
                });
            }
            Some(crate::planner::PlanRequest::Skills(id)) => {
                let (slot, ctx2) = (self.skills.clone(), ctx.clone());
                web::fetch_skills(&self.accounts, id, move |r| {
                    *slot.borrow_mut() = Some(r);
                    ctx2.request_repaint();
                });
            }
            None => {}
        }
        if let Some(r) = self.route_note.borrow_mut().take() {
            self.map.plan.note = Some(r);
        }
        if let Some(r) = self.skills.borrow_mut().take() {
            match r {
                Ok((jdc, jfc)) => self.map.plan.set_skills(jdc, jfc),
                Err(e) => self.map.plan.note = Some(Err(e)),
            }
        }
    }

    /// The Characters menu: who is added, where they are, and adding another.
    fn characters_menu(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let n = self.accounts.borrow().list.len();
        let note = self.added.borrow().clone();
        ui.menu_button(format!("{}  Characters{}", icon::USERS, if n > 0 { format!(" ({n})") } else { String::new() }), |ui| {
            ui.set_min_width(320.0);
            if let Some(r) = &note {
                match r {
                    Ok(name) => ui.label(egui::RichText::new(format!("{name} added.")).color(spai_ui::theme::standing::FRIENDLY)),
                    Err(e) => ui.label(egui::RichText::new(e).color(ui.visuals().error_fg_color)),
                };
            }
            let (list, live, errors) = {
                let a = self.accounts.borrow();
                (a.list.clone(), a.live.clone(), a.errors.clone())
            };
            let geo = self.host.as_ref().map(|h| h.geo.clone());
            let mut remove = None;
            for acc in &list {
                ui.horizontal(|ui| {
                    let where_ = live.get(&acc.char_id).and_then(|(sys, _)| geo.as_ref()?.info_of(*sys)).map(|i| i.name.clone());
                    let online = live.get(&acc.char_id).is_some_and(|l| l.1);
                    let dot = if online { icon::CIRCLE } else { icon::CIRCLE_DASHED };
                    ui.label(egui::RichText::new(dot).color(if online { spai_ui::theme::standing::FRIENDLY } else { ui.visuals().weak_text_color() }));
                    ui.label(egui::RichText::new(&acc.name).strong());
                    if let Some(w) = where_ {
                        ui.label(egui::RichText::new(w).weak());
                    } else if let Some(e) = errors.get(&acc.char_id) {
                        ui.label(egui::RichText::new(icon::WARNING).color(spai_ui::theme::standing::WARNING)).on_hover_text(e);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::X).on_hover_text("Remove from this browser").clicked() {
                            remove = Some(acc.char_id);
                        }
                        let grants: Vec<&str> = [
                            (crate::accounts::LOCATION, "location"),
                            (crate::accounts::WAYPOINT, "waypoints"),
                            (crate::accounts::SKILLS, "skills"),
                        ]
                        .into_iter()
                        .filter(|(s, _)| acc.can(s))
                        .map(|(_, l)| l)
                        .collect();
                        ui.label(egui::RichText::new(grants.join(", ")).weak());
                    });
                });
            }
            if let Some(id) = remove {
                crate::accounts::web::remove(&self.accounts, id);
            }
            if list.iter().any(|a| a.can(crate::accounts::LOCATION)) {
                if ui
                    .checkbox(&mut self.detect, "Detect wormholes from their jumps")
                    .on_hover_text("A jump no gate explains opens a small card to fill the hole in")
                    .changed()
                {
                    page::save(DETECT, &self.detect);
                }
            }
            if !list.is_empty() {
                ui.separator();
            }
            ui.label(egui::RichText::new("Add a character").strong());
            ui.label(egui::RichText::new("Its EVE login stays in this browser and is used with ESI only, never sent to EVE Spai's server.").weak());
            ui.checkbox(&mut self.add_scopes.0, "Location and online status").on_hover_text("Shows where it is on both maps, starts routes there, and notices the holes it jumps through");
            ui.checkbox(&mut self.add_scopes.1, "Set waypoints").on_hover_text("Set in game from the route planner");
            ui.checkbox(&mut self.add_scopes.2, "Jump skills").on_hover_text("JDC and JFC for jump routes");
            let (l, w, k) = self.add_scopes;
            let scopes = crate::accounts::scopes(l, w, k);
            if ui.add_enabled(!scopes.is_empty(), egui::Button::new("Sign in with EVE")).clicked() {
                *self.added.borrow_mut() = None;
                auth::add_character(scopes);
            }
        });
    }

    /// Signed in: keeps the group in step, and uses an invite link opened earlier.
    fn run_sync(&mut self, session: &crate::sso::Session, ctx: &egui::Context) {
        let sync = self.sync.get_or_insert_with(Sync::new);
        // An invite the user chose to use here, before or after signing in.
        if !self.joined && page::load::<bool>(auth::INVITE_HERE) == Some(true) {
            self.joined = true;
            if let Some((id, secret)) = page::load::<(String, String)>(auth::INVITE) {
                page::forget(auth::INVITE);
                page::forget(auth::INVITE_HERE);
                sync.send(Cmd::Join { link: invite_link(&id, &secret), char_id: session.character_id, prefs: SharePrefs::default() });
            }
        }
        sync.tick(session, ctx);
        if let Some(host) = &mut self.host {
            let now = spai_core::clock::utc().timestamp();
            for e in std::mem::take(&mut host.edits) {
                match e {
                    crate::host::Edit::Save(w) => sync.store.edit_hole(&w, now),
                    crate::host::Edit::Dead(uid) => sync.store.kill_hole(&uid),
                    crate::host::Edit::Sigs { system, scan, full } => {
                        let (added, updated, removed) = sync.store.paste_sigs(system, &scan, &session.character_name, now, full);
                        host.sig_note = Some(format!("{added} new, {updated} updated, {removed} removed"));
                    }
                }
                sync.poke();
            }
            host.can_edit = sync
                .store
                .share_groups()
                .iter()
                .any(|g| g.char_id == session.character_id && g.role.can_write() && sync.store.share_key(&g.id, g.epoch).is_some());
            // EVE-Scout's holes, asked for every few minutes while they are wanted. The switch is on
            // the map's layers; switching it redoes the holes at once.
            let now_s = spai_core::clock::utc().timestamp();
            if self.map.layers.scout != self.scout_on {
                self.scout_on = self.map.layers.scout;
                page::save(SCOUT, &self.scout_on);
                self.scout_changed = true;
            }
            if self.scout_on && now_s - self.scout_at >= SCOUT_EVERY {
                self.scout_at = now_s;
                let (slot, ctx) = (self.scout_in.clone(), ctx.clone());
                ehttp::fetch(ehttp::Request::get(spai_core::wormholes::SCOUT_URL), move |r| {
                    let Some(sigs) = r.ok().filter(|r| r.ok).and_then(|r| serde_json::from_slice::<Vec<spai_core::wormholes::ScoutSig>>(&r.bytes).ok()) else { return };
                    let now = spai_core::clock::utc().timestamp();
                    *slot.lock().unwrap() = Some(scout_holes(&sigs, now));
                    ctx.request_repaint();
                });
            }
            if let Some(list) = self.scout_in.lock().unwrap().take() {
                self.scout = list;
                self.scout_changed = true;
            }
            if sync.store.generation.get() != self.shown || std::mem::take(&mut self.scout_changed) {
                host.holes = sync.store.wormholes();
                if self.scout_on {
                    // A connection the group shares already is the group's entry, kept as it is.
                    let joined = |w: &spai_core::wormholes::Wormhole, h: &spai_core::wormholes::Wormhole| {
                        let a = (w.system_id, w.dest_system_id);
                        a == (h.system_id, h.dest_system_id) || a == (h.dest_system_id.unwrap_or(0), Some(h.system_id))
                    };
                    let extra: Vec<_> = self.scout.iter().filter(|w| !w.is_expired(now_s) && !host.holes.iter().any(|h| joined(w, h))).cloned().collect();
                    host.holes.extend(extra);
                }
                host.sigs = sync.store.all_sigs();
                if let Some(d) = &mut self.map_data {
                    d.set_holes(&host.holes);
                }
                self.map.plan.holes_changed();
                self.shown = sync.store.generation.get();
            }
            // The group's Ansiblex network, once in the group; routes on both tabs use it.
            if !self.asked_network && sync.store.share_groups().iter().any(|g| sync.store.share_key(&g.id, g.epoch).is_some()) {
                self.asked_network = true;
                let (slot, ctx) = (self.network.clone(), ctx.clone());
                let mut req = ehttp::Request::get(format!("{}/api/wh/v2/bridges", page::origin()));
                req.headers.insert("Authorization", format!("Bearer {}", session.token));
                ehttp::fetch(req, move |r| {
                    if let Some(n) = r.ok().filter(|r| r.ok).and_then(|r| serde_json::from_slice::<Option<Network>>(&r.bytes).ok()).flatten() {
                        *slot.lock().unwrap() = Some(n);
                        ctx.request_repaint();
                    }
                });
            }
            if let Some(n) = self.network.lock().unwrap().take() {
                let mut geo = host.geo.gates_only();
                let key = spai_core::ansiblex::BridgeKey { bridges: n.list(), capital: n.capital.clone(), max_zone: n.max_zone.max(1) };
                spai_core::ansiblex::feed(key, &mut geo);
                host.geo = Arc::new(geo);
                if let Some(d) = &mut self.map_data {
                    d.set_network(n, &host.geo);
                }
            }
        }
    }

    /// Nothing to show on the map yet: why, and what to do.
    /// An invite opened here and not yet used: in EVE Spai, or in the browser.
    fn invite_choice(&mut self, ui: &mut egui::Ui, state: &Auth) -> bool {
        let Some((id, secret)) = page::load::<(String, String)>(auth::INVITE) else { return false };
        if page::load::<bool>(auth::INVITE_HERE) == Some(true) || matches!(state, Auth::Working(_)) {
            return false;
        }
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.25);
            ui.heading("You were invited to a wormhole group");
            ui.label("Open the invite in EVE Spai if you use it, or use the wormhole map here in the browser.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                // Centred: the two buttons as one row in the middle.
                let w = 2.0 * 190.0 + ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                if ui.add_sized([190.0, 32.0], egui::Button::new("Open in EVE Spai")).clicked() {
                    self.tried_app = true;
                    page::go(&invite_link(&id, &secret));
                }
                if ui.add_sized([190.0, 32.0], egui::Button::new("Use it here")).clicked() {
                    page::save(auth::INVITE_HERE, &true);
                    self.joined = false;
                    if !matches!(state, Auth::SignedIn(_)) {
                        auth::sign_in();
                    }
                }
            });
            if self.tried_app {
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Nothing opened? EVE Spai 0.13.1 or later opens these links. In an older one, paste the link into the Join field of Wormhole sharing:").weak());
                let link = format!("{}{}join/{id}#{secret}", page::origin(), page::base());
                ui.add(egui::TextEdit::singleline(&mut link.clone()).desired_width(560.0));
                ui.label(egui::RichText::new("Sign in as the character the invite is for.").weak());
            }
        });
        true
    }

    fn waiting(&self, ui: &mut egui::Ui, state: &Auth) -> bool {
        let say = |ui: &mut egui::Ui, lines: &[String]| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.3);
                for (i, l) in lines.iter().enumerate() {
                    if i == 0 {
                        ui.heading(l);
                    } else {
                        ui.label(l);
                    }
                }
            });
        };
        let Auth::SignedIn(session) = state else {
            if matches!(state, Auth::Working(_)) {
                return false;
            }
            if page::load::<(String, String)>(auth::INVITE).is_some() {
                say(ui, &["You were invited to a wormhole group".into(), "Sign in with EVE, as the character the invite is for, to ask to join.".into()]);
            } else {
                say(
                    ui,
                    &[
                        "EVE Spai wormhole map".into(),
                        "The wormholes a group shares in EVE Spai, for its members without the app.".into(),
                        "Open the invite link an admin of your group sent you, or sign in if this browser has joined before.".into(),
                    ],
                );
            }
            return true;
        };
        let Some(sync) = &self.sync else { return false };
        let status = sync.status.lock().unwrap().clone();
        let groups = sync.store.share_groups();
        let fingerprint = status.fingerprint.clone().unwrap_or_default();
        if let Some(g) = groups.iter().find(|g| g.char_id == session.character_id && sync.store.share_key(&g.id, g.epoch).is_none()) {
            let mut lines = vec![
                format!("Waiting to be let into {}", g.name),
                "The invite lets you in as soon as the EVE Spai that made it next syncs: it has to be running.".into(),
                format!("Fingerprint: {fingerprint}"),
            ];
            lines.extend(status.error.clone());
            say(ui, &lines);
            return true;
        }
        if groups.is_empty() {
            let mut lines = vec![
                "No wormhole group yet".into(),
                "Open the invite link an admin of your group gave you, while signed in as the character it is for.".into(),
            ];
            if status.busy {
                lines.push("Asking to join\u{2026}".into());
            }
            lines.extend(status.error.clone());
            say(ui, &lines);
            return true;
        }
        false
    }
}

impl eframe::App for WebApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.host.is_none() {
            match self.loading.lock().unwrap().take() {
                Some(Ok(u)) => {
                    let mut host = WebHost::new(Arc::new(u.systems()));
                    match page::load::<spai_ui::wh_tab::WhPrefs>(PREFS) {
                        // 10 was this page's own default before it took the desktop's.
                        Some(p) if p.pin_jumps == 10 => host.prefs = spai_ui::wh_tab::WhPrefs { pin_jumps: crate::host::PIN_JUMPS, ..p },
                        Some(p) => host.prefs = p,
                        // First visit: the systems most of the group stages in.
                        None => {
                            host.prefs.route_pins = crate::host::DEFAULT_PINS.iter().map(|p| p.to_string()).collect();
                            host.dirty = true;
                        }
                    }
                    host.filter = page::load(FILTER).unwrap_or_default();
                    let layout: Vec<(i64, f32, f32)> = page::load(LAYOUT).unwrap_or_default();
                    host.layout = layout.into_iter().map(|(id, x, y)| (id, egui::pos2(x, y))).collect();
                    self.host = Some(host);
                    // Flat, as the desktop's map lays New Eden out; k-space only.
                    let flat = u
                        .map_systems()
                        .into_iter()
                        .filter(|s| s.id < 31_000_000)
                        // The unreachable regions (A821-A, J7HZ-F, UUA-F4) are left off, as the desktop does.
                        .filter(|s| !u.region_name(s.region_id).chars().any(|c| c.is_ascii_digit()))
                        .map(|s| spai_core::map::MapSystem { x: s.x2d, z: s.z2d, ..s })
                        .collect();
                    self.map_data = Some(MapData::new(flat, u.map_systems(), u.regions.clone()));
                    if let Some(p) = page::load(ROUTE_PREFS) {
                        self.map.plan.prefs = p;
                    }
                }
                Some(Err(e)) => self.error = Some(e),
                None => {}
            }
        }
        let state = self.auth.lock().unwrap().clone();
        match &state {
            Auth::SignedIn(s) => {
                self.run_sync(s, ui.ctx());
                self.run_accounts(ui.ctx());
            }
            _ => self.sync = None,
        }
        let view = SavedView { tab: self.tab, layers: self.map.layers, region: self.map.region };
        if self.view_saved.as_ref() != Some(&view) {
            page::save(VIEW, &view);
            self.view_saved = Some(view);
        }
        let top = egui::Panel::top("web_top").frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 5))).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("EVE Spai");
                if self.host.is_some() && matches!(state, Auth::SignedIn(_)) {
                    use spai_ui::widgets::SteadySelect as _;
                    ui.separator();
                    ui.menu_value(&mut self.tab, Tab::Wormholes, "Wormholes");
                    ui.menu_value(&mut self.tab, Tab::Map, "Map");
                    ui.menu_value(&mut self.tab, Tab::Group, "Group");
                    ui.separator();
                    if let Some(h) = self.host.as_mut().filter(|h| h.can_edit) {
                        if ui.button(format!("{}  Add a wormhole", egui_phosphor::regular::PLUS)).clicked() {
                            let sel = if self.tab == Tab::Wormholes { self.view.selected } else { self.map.selected };
                            h.add_form(sel);
                        }
                        ui.separator();
                    }
                }
                if let Some(sync) = &self.sync {
                    let groups = sync.store.share_groups();
                    let names: Vec<String> = groups.iter().map(|g| format!("{} ({})", g.name, g.role.label())).collect();
                    if !names.is_empty() {
                        ui.label(egui::RichText::new(names.join(", ")).weak()).on_hover_text("Your groups, and your role in each: a viewer sees the map and shares nothing");
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &state {
                    Auth::SignedIn(s) => {
                        if ui.button("Sign out").clicked() {
                            auth::sign_out(&self.auth);
                        }
                        ui.label(&s.character_name);
                        ui.separator();
                        self.characters_menu(ui);
                    }
                    Auth::Working(what) => {
                        ui.label(egui::RichText::new(what).weak());
                    }
                    Auth::SignedOut | Auth::Failed(_) => {
                        if ui.button("Sign in with EVE").clicked() {
                            auth::sign_in();
                        }
                        if let Auth::Failed(e) = &state {
                            ui.label(egui::RichText::new(e).color(ui.visuals().error_fg_color));
                        }
                    }
                });
            });
        });
        if let Some(h) = &mut self.host {
            h.side_w = 0.0;
        }
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.invite_choice(ui, &state) || self.waiting(ui, &state) {
                return;
            }
            if let (Tab::Group, Some(sync), Auth::SignedIn(session)) = (self.tab, &mut self.sync, &state) {
                let status = sync.status.lock().unwrap().clone();
                for cmd in self.group.show(ui, &*sync.store, &status, session.character_id, &page::origin()) {
                    sync.send(cmd);
                }
                return;
            }
            match (&mut self.host, &self.error) {
                (Some(host), _) if self.tab == Tab::Map => {
                    if let Some(d) = &self.map_data {
                        self.map.show(ui, &host.geo, d);
                    }
                    if std::mem::take(&mut self.map.plan.dirty) {
                        page::save(ROUTE_PREFS, &self.map.plan.prefs);
                    }
                }
                (Some(host), _) => {
                    host.detect = self.detect;
                    spai_ui::wh_tab::show(&mut self.view, host, ui);
                    // The settings menu's switches, back into the app that owns them.
                    if host.detect != self.detect {
                        self.detect = host.detect;
                        page::save(DETECT, &self.detect);
                    }
                    if std::mem::take(&mut host.open_group) {
                        self.tab = Tab::Group;
                    }
                    if std::mem::take(&mut host.dirty) {
                        page::save(PREFS, &host.prefs);
                        page::save(FILTER, &host.filter);
                        let layout: Vec<(i64, f32, f32)> = host.layout.iter().map(|(id, p)| (*id, p.x, p.y)).collect();
                        page::save(LAYOUT, &layout);
                    }
                }
                (None, Some(e)) => {
                    ui.label(egui::RichText::new(format!("New Eden did not load: {e}")).color(ui.visuals().error_fg_color));
                }
                (None, None) => {
                    ui.centered_and_justified(|ui| ui.label(egui::RichText::new("Loading New Eden\u{2026}").weak()));
                }
            }
        });
        if let (Some(h), Auth::SignedIn(_)) = (&mut self.host, &state) {
            h.form_window(ui.ctx(), top.response.rect.bottom());
            h.corner(ui.ctx(), top.response.rect.bottom());
        }
    }
}

/// EVE-Scout's entries as holes, each with an id stable across polls so the map keeps its place.
fn scout_holes(sigs: &[spai_core::wormholes::ScoutSig], now: i64) -> Vec<spai_core::wormholes::Wormhole> {
    sigs.iter()
        .filter_map(|s| spai_core::wormholes::scout_to_wormhole(s, now))
        .map(|w| {
            let uid = format!("evescout:{}:{}", w.system_id, w.dest_system_id.unwrap_or(0));
            let id = uid.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211)) as i64 & i64::MAX;
            spai_core::wormholes::Wormhole { uid, id, ..w }
        })
        .collect()
}
