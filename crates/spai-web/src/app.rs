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
use crate::sync::{invite_link, Sync};

type Loading = Arc<Mutex<Option<Result<Universe, String>>>>;

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
        WebApp { loading, host: None, view: WhGraphView::default(), error: None, auth, sync: None, shown: u64::MAX, joined: false }
    }

    /// Signed in: keeps the group in step, and uses an invite link opened earlier.
    fn run_sync(&mut self, session: &crate::sso::Session, ctx: &egui::Context) {
        let sync = self.sync.get_or_insert_with(Sync::new);
        if !self.joined {
            self.joined = true;
            if let Some((id, secret)) = page::load::<(String, String)>(auth::INVITE) {
                page::forget(auth::INVITE);
                sync.send(Cmd::Join { link: invite_link(&id, &secret), char_id: session.character_id, prefs: SharePrefs::default() });
            }
        }
        sync.tick(session, ctx);
        let generation = sync.store.generation.get();
        if let (Some(host), true) = (&mut self.host, generation != self.shown) {
            host.holes = sync.store.wormholes();
            self.shown = generation;
        }
    }

    /// Nothing to show on the map yet: why, and what to do.
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
            if page::load::<(String, String)>(auth::INVITE).is_some() {
                say(ui, &["You were invited to a wormhole group".into(), "Sign in with EVE, as the character the invite is for, to ask to join.".into()]);
                return true;
            }
            return false;
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
                Some(Ok(u)) => self.host = Some(WebHost::new(Arc::new(u.systems()))),
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
                if let Some(g) = self.sync.as_ref().and_then(|s| s.store.share_groups().into_iter().next()) {
                    ui.label(egui::RichText::new(g.name).weak());
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
            if self.waiting(ui, &state) {
                return;
            }
            match (&mut self.host, &self.error) {
                (Some(host), _) => spai_ui::wh_tab::show(&mut self.view, host, ui),
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
