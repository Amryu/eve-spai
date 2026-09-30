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
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
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
        let auth = auth::start(&cc.egui_ctx);
        WebApp { loading, host: None, view: WhGraphView::default(), error: None, auth, sync: None,
            shown: u64::MAX,
            joined: false,
            tried_app: false,
            tab: Tab::default(),
            map: StarMap::default(),
            map_data: None,
            network: Default::default(),
            asked_network: false,
            group: Default::default(),
        }
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
                }
                sync.poke();
            }
            host.can_edit = sync
                .store
                .share_groups()
                .iter()
                .any(|g| g.char_id == session.character_id && g.role.can_write() && sync.store.share_key(&g.id, g.epoch).is_some());
            if sync.store.generation.get() != self.shown {
                host.holes = sync.store.wormholes();
                host.sigs = sync.store.all_sigs();
                if let Some(d) = &mut self.map_data {
                    d.set_holes(&host.holes);
                }
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
                "An admin of the group approves this browser. Read them its fingerprint:".into(),
                fingerprint,
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
                    if let Some(p) = page::load(PREFS) {
                        host.prefs = p;
                    }
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
            Auth::SignedIn(s) => self.run_sync(s, ui.ctx()),
            _ => self.sync = None,
        }
        egui::Panel::top("web_top").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("EVE Spai");
                if self.host.is_some() && matches!(state, Auth::SignedIn(_)) {
                    use spai_ui::widgets::SteadySelect as _;
                    ui.separator();
                    ui.menu_value(&mut self.tab, Tab::Wormholes, "Wormholes");
                    ui.menu_value(&mut self.tab, Tab::Map, "Map");
                    ui.menu_value(&mut self.tab, Tab::Group, "Group");
                    ui.separator();
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
                    spai_ui::wh_tab::show(&mut self.view, host, ui);
                    if std::mem::take(&mut host.dirty) {
                        page::save(PREFS, &host.prefs);
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
    }
}
