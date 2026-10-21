//! The page: the simulator with the browser answering for storage, the fitting data, the session
//! and the characters whose skills it may read.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use spai_fit_ui::{Doctrine, DoctrineSide, HostEvent, HostRequest, Profile, SimUi};

use crate::store::FleetEntry;
use spai_ui::{tr, trf};
use spai_webkit::accounts::{self, Account, SKILLS};
use spai_webkit::page;
use spai_webkit::sso::{Pending, Session, ACCOUNTS_KEY, PENDING_KEY, SESSION_KEY};

use crate::store::{Library, KEY};

/// Where the panel sizes and tabs shown are kept between visits.
const LAYOUT_KEY: &str = "spai.sim.layout";

/// Where the fitting data is published, next to the mirrored static data.
const BLOB_PATH: &str = "/sde/";

pub struct SimWebApp {
    sim: SimUi,
    library: Library,
    events: Arc<Mutex<Vec<HostEvent>>>,
    accounts: Rc<RefCell<Vec<Account>>>,
    session: Option<Session>,
    /// Skills fetched this visit, by character.
    fetching: Rc<RefCell<usize>>,
}

impl SimWebApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        spai_ui::theme::install_fonts_opts(&cc.egui_ctx, false);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        spai_ui::theme::Theme::default().apply(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let now = spai_core::clock::utc().timestamp();
        let mut app = SimWebApp {
            sim: SimUi::default(),
            library: page::load_raw(KEY).map(|t| Library::from_json(&t)).unwrap_or_default(),
            events: Default::default(),
            accounts: Rc::new(RefCell::new(page::load(ACCOUNTS_KEY).unwrap_or_default())),
            session: page::load::<Session>(SESSION_KEY).filter(|s| s.usable(now)),
            fetching: Default::default(),
        };
        app.sim.handle(HostEvent::Fits(app.library.summaries()));
        if let Some(layout) = page::load_raw(LAYOUT_KEY) {
            app.sim.handle(HostEvent::Layout(layout));
        }
        if let Some(loc) = web_sys::window().map(|w| w.location()) {
            let (origin, path) = (loc.origin().unwrap_or_default(), loc.pathname().unwrap_or_default());
            app.sim.link_base = Some(format!("{origin}{path}"));
            // A fit in the address, in the fragment or the query, comes in once the data is loaded;
            // the address is cleaned so a reload does not bring it again.
            let (hash, search) = (loc.hash().unwrap_or_default(), loc.search().unwrap_or_default());
            if let Some(text) = spai_fit::formats::link::param(&hash).or_else(|| spai_fit::formats::link::param(&search)).and_then(spai_fit::formats::link::decode) {
                app.sim.import_when_ready(text);
                if let Ok(history) = web_sys::window().expect("a window").history() {
                    let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
                }
            }
        }
        app.load_profiles();
        app.fetch_doctrines(&cc.egui_ctx);
        app.sync(&cc.egui_ctx);
        app
    }

    fn bearer(&self) -> Option<String> {
        self.session.as_ref().map(|s| s.token.clone())
    }

    fn api(path: &str) -> String {
        format!("{}/api/sim{path}", page::origin())
    }

    fn fetch_doctrines(&self, ctx: &egui::Context) {
        let (events, bearer, ctx) = (self.events.clone(), self.bearer(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let auth = |mut req: ehttp::Request| {
                if let Some(b) = &bearer {
                    req.headers.insert("Authorization", format!("Bearer {b}"));
                }
                req
            };
            let (mut imperium, mut admin) = (false, false);
            if bearer.is_some() {
                if let Ok(r) = ehttp::fetch_async(auth(ehttp::Request::get(Self::api("/me")))).await {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&r.bytes) {
                        imperium = v["imperium"].as_bool().unwrap_or(false);
                        admin = v["admin"].as_bool().unwrap_or(false);
                    }
                }
            }
            events.lock().unwrap().push(HostEvent::Access { signed_in: bearer.is_some(), imperium, admin });
            match ehttp::fetch_async(auth(ehttp::Request::get(Self::api("/doctrines")))).await {
                Ok(r) if r.ok => {
                    if let Ok(list) = serde_json::from_slice::<Vec<Doctrine>>(&r.bytes) {
                        events.lock().unwrap().push(HostEvent::Doctrines(list));
                    }
                }
                _ => {}
            }
            ctx.request_repaint();
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn publish(&self, ctx: &egui::Context, id: String, side: DoctrineSide, name: String, tags: Vec<String>, refit_keys: Vec<String>, doc: String) {
        let (events, bearer, ctx2) = (self.events.clone(), self.bearer(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let outcome: Result<(), String> = async {
                let b = bearer.ok_or("sign in first")?;
                let doc: serde_json::Value = serde_json::from_str(&doc).map_err(|e| e.to_string())?;
                let body = serde_json::json!({ "side": side, "name": name, "tags": tags, "refit_keys": refit_keys, "doc": doc });
                let mut req = ehttp::Request::post(Self::api(&format!("/doctrines/{id}")), body.to_string().into_bytes());
                req.method = ehttp::Method::PUT;
                req.headers = ehttp::Headers::new(&[("Content-Type", "application/json"), ("Authorization", &format!("Bearer {b}"))]);
                let r = ehttp::fetch_async(req).await?;
                if !r.ok {
                    return Err(serde_json::from_slice::<serde_json::Value>(&r.bytes).ok().and_then(|v| v["error"].as_str().map(str::to_owned)).unwrap_or_else(|| r.status.to_string()));
                }
                Ok(())
            }
            .await;
            events.lock().unwrap().push(HostEvent::Status(match outcome {
                Ok(()) => tr!("Published").to_owned(),
                Err(e) => trf!("Could not publish: {e}", e = e),
            }));
            ctx2.request_repaint();
        });
        self.fetch_doctrines(ctx);
    }

    fn delete_doctrine(&self, ctx: &egui::Context, id: String) {
        let (bearer, ctx) = (self.bearer(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(b) = bearer {
                let mut req = ehttp::Request::get(Self::api(&format!("/doctrines/{id}")));
                req.method = ehttp::Method::DELETE;
                req.headers.insert("Authorization", format!("Bearer {b}"));
                let _ = ehttp::fetch_async(req).await;
            }
            ctx.request_repaint();
        });
    }

    /// The browser's library against the server's: newer local saves go up, newer server copies
    /// come down, by revision.
    fn sync(&self, ctx: &egui::Context) {
        let Some(b) = self.bearer() else { return };
        let (events, ctx) = (self.events.clone(), ctx.clone());
        let mut lib = self.library.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let auth = |mut req: ehttp::Request| {
                req.headers.insert("Authorization", format!("Bearer {b}"));
                req
            };
            for (id, e) in lib.fits.clone() {
                if e.server_rev >= e.rev {
                    continue;
                }
                let Ok(doc) = serde_json::from_str::<serde_json::Value>(&e.doc) else { continue };
                let body = serde_json::json!({ "name": e.name, "rev": e.server_rev, "doc": doc });
                let mut req = ehttp::Request::post(Self::api(&format!("/fits/{id}")), body.to_string().into_bytes());
                req.method = ehttp::Method::PUT;
                req.headers = ehttp::Headers::new(&[("Content-Type", "application/json")]);
                if let Ok(r) = ehttp::fetch_async(auth(req)).await {
                    if r.ok {
                        if let Some(rev) = serde_json::from_slice::<serde_json::Value>(&r.bytes).ok().and_then(|v| v["rev"].as_i64()) {
                            if let Some(x) = lib.fits.get_mut(&id) {
                                x.server_rev = rev;
                                x.rev = rev;
                            }
                        }
                    }
                }
            }
            if let Ok(r) = ehttp::fetch_async(auth(ehttp::Request::get(Self::api("/fits")))).await {
                if let Ok(list) = serde_json::from_slice::<Vec<serde_json::Value>>(&r.bytes) {
                    for entry in list {
                        let (Some(id), Some(rev)) = (entry["id"].as_str(), entry["rev"].as_i64()) else { continue };
                        if lib.fits.get(id).is_some_and(|e| e.server_rev >= rev) {
                            continue;
                        }
                        let Ok(full) = ehttp::fetch_async(auth(ehttp::Request::get(Self::api(&format!("/fits/{id}"))))).await else { continue };
                        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&full.bytes) else { continue };
                        lib.fits.insert(id.to_owned(), crate::store::Entry {
                            name: v["name"].as_str().unwrap_or("").to_owned(),
                            ship: v["ship_type_id"].as_u64().unwrap_or(0) as u32,
                            updated_at: v["updated_at"].as_i64().unwrap_or(0),
                            doc: v["doc"].to_string(),
                            rev,
                            server_rev: rev,
                        });
                    }
                }
            }
            page::save_raw(KEY, &lib.to_json());
            events.lock().unwrap().push(HostEvent::Status("\u{0}reload".into()));
            ctx.request_repaint();
        });
    }

    fn load_profiles(&mut self) {
        let profiles: Vec<Profile> = page::load::<Vec<(i64, String, Vec<(u32, u8)>)>>("spai.sim.skills")
            .unwrap_or_default()
            .into_iter()
            .map(|(id, name, skills)| Profile { id: id.to_string(), name, skills: spai_fit::skills::SkillSet::from_esi(skills) })
            .collect();
        self.sim.handle(HostEvent::Profiles(profiles));
    }

    /// Off to EVE for an EVE Spai session, back through `/wh/callback` and then here.
    fn sign_in(&self) {
        let p = Pending { return_to: Some(format!("{}{}", page::origin(), page::base_of("sim"))), ..Pending::new() };
        page::save(PENDING_KEY, &p);
        page::go(&p.authorize_url(&format!("{}/wh/callback", page::origin())));
    }

    /// Off to EVE to add a character whose skills this page may read; back through `/wh/callback`.
    fn add_character(&self) {
        let p = Pending { scopes: vec![SKILLS.to_owned()], return_to: Some(format!("{}{}", page::origin(), page::base_of("sim"))), ..Pending::new() };
        page::save(PENDING_KEY, &p);
        page::go(&p.authorize_url(&format!("{}/wh/callback", page::origin())));
    }

    fn fetch_skills(&mut self, ctx: &egui::Context) {
        let list: Vec<Account> = self.accounts.borrow().iter().filter(|a| a.can(SKILLS)).cloned().collect();
        if list.is_empty() {
            self.sim.handle(HostEvent::Status(tr!("Add a character with the skills scope first").to_owned()));
            return;
        }
        *self.fetching.borrow_mut() = list.len();
        for a in list {
            let (events, accounts, fetching, ctx) = (self.events.clone(), self.accounts.clone(), self.fetching.clone(), ctx.clone());
            wasm_bindgen_futures::spawn_local(async move {
                let now = spai_core::clock::utc().timestamp();
                let result = async {
                    let (token, renewed) = accounts::fresh_token(&a, now).await?;
                    if let Some(r) = renewed {
                        let mut list = accounts.borrow_mut();
                        accounts::keep(&mut list, r);
                        page::save(ACCOUNTS_KEY, &*list);
                    }
                    let v = accounts::esi(&token, ehttp::Method::GET, &format!("/characters/{}/skills/", a.char_id)).await?;
                    Ok::<_, String>(accounts::skill_levels(&v))
                }
                .await;
                match result {
                    Ok(levels) => {
                        let mut stored: Vec<(i64, String, Vec<(u32, u8)>)> = page::load("spai.sim.skills").unwrap_or_default();
                        stored.retain(|(id, _, _)| *id != a.char_id);
                        stored.push((a.char_id, a.name.clone(), levels.into_iter().filter_map(|(id, l)| Some((u32::try_from(id).ok()?, l.min(5) as u8))).collect()));
                        page::save("spai.sim.skills", &stored);
                        events.lock().unwrap().push(HostEvent::Status(String::new()));
                    }
                    Err(e) => events.lock().unwrap().push(HostEvent::Status(trf!("{name}: {e}", name = a.name, e = e))),
                }
                let left = { let mut f = fetching.borrow_mut(); *f = f.saturating_sub(1); *f };
                if left == 0 {
                    ctx.request_repaint();
                }
            });
        }
    }

    fn load_dogma(&self, ctx: &egui::Context) {
        let events = self.events.clone();
        let ctx = ctx.clone();
        let url = format!("{}{BLOB_PATH}{}", page::origin(), spai_fit::dogma::blob::file_name());
        ehttp::fetch(ehttp::Request::get(url), move |r| {
            let event = match r {
                Ok(resp) if resp.ok => match spai_fit::dogma::blob::decode(&resp.bytes) {
                    Ok(d) => HostEvent::Dogma(Arc::new(d)),
                    Err(e) => HostEvent::DogmaFailed(format!("{e:#}")),
                },
                Ok(resp) => HostEvent::DogmaFailed(format!("{} {}", resp.status, resp.status_text)),
                Err(e) => HostEvent::DogmaFailed(e),
            };
            events.lock().unwrap().push(event);
            ctx.request_repaint();
        });
    }

    fn request(&mut self, ctx: &egui::Context, r: HostRequest) {
        let now = spai_core::clock::utc().timestamp();
        match r {
            HostRequest::LoadDogma => self.load_dogma(ctx),
            HostRequest::ListFits => self.sim.handle(HostEvent::Fits(self.library.summaries())),
            HostRequest::LoadFit(id) => {
                if let Some(e) = self.library.fits.get(&id) {
                    let doc = e.doc.clone();
                    self.sim.handle(HostEvent::Fit { id, doc });
                }
            }
            HostRequest::SaveFit { id, name, ship, doc } => {
                self.library.put(&id, &name, ship, &doc, now);
                page::save_raw(KEY, &self.library.to_json());
                self.sim.handle(HostEvent::Saved(id));
                self.sync(ctx);
            }
            HostRequest::DeleteFit(id) => {
                self.library.remove(&id);
                page::save_raw(KEY, &self.library.to_json());
                self.sim.handle(HostEvent::Fits(self.library.summaries()));
            }
            HostRequest::FetchSkills => self.fetch_skills(ctx),
            HostRequest::CopyText(t) => ctx.copy_text(t),
            HostRequest::SaveLayout(json) => page::save_raw(LAYOUT_KEY, &json),
            HostRequest::FetchPrices => {
                let (events, ctx) = (self.events.clone(), ctx.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(r) = ehttp::fetch_async(ehttp::Request::get(spai_fit_ui::PRICES_URL)).await {
                        if r.ok {
                            events.lock().unwrap().push(HostEvent::Prices(spai_fit_ui::parse_prices(&r.bytes)));
                            ctx.request_repaint();
                        }
                    }
                });
            }
            HostRequest::ReadClipboard => {
                let (events, ctx) = (self.events.clone(), ctx.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let text = match web_sys::window().map(|w| w.navigator().clipboard()) {
                        Some(c) => wasm_bindgen_futures::JsFuture::from(c.read_text()).await.ok().and_then(|v| v.as_string()).unwrap_or_default(),
                        None => String::new(),
                    };
                    events.lock().unwrap().push(HostEvent::Clipboard(text));
                    ctx.request_repaint();
                });
            }
            HostRequest::FetchDoctrines => self.fetch_doctrines(ctx),
            HostRequest::PublishDoctrine { id, side, name, tags, refit_keys, doc } => self.publish(ctx, id, side, name, tags, refit_keys, doc),
            HostRequest::DeleteDoctrine(id) => self.delete_doctrine(ctx, id),
            HostRequest::SyncLibrary => self.sync(ctx),
            HostRequest::FitDocs(ids) => {
                let docs = ids.iter().filter_map(|id| self.library.fits.get(id).map(|e| (id.clone(), e.doc.clone()))).collect();
                self.sim.handle(HostEvent::FitDocs(docs));
            }
            HostRequest::ListFleets => self.sim.handle(HostEvent::Fleets(self.library.fleet_summaries())),
            HostRequest::LoadFleet(id) => {
                if let Some(e) = self.library.fleets.get(&id) {
                    let doc = e.doc.clone();
                    self.sim.handle(HostEvent::Fleet { id, doc });
                }
            }
            HostRequest::LoadFleetDoc(id) => {
                if let Some(e) = self.library.fleets.get(&id) {
                    let doc = e.doc.clone();
                    self.sim.handle(HostEvent::FleetDoc { id, doc });
                }
            }
            HostRequest::SaveFleet { id, name, doc } => {
                self.library.fleets.insert(id.clone(), FleetEntry { name, updated_at: now, doc });
                page::save_raw(KEY, &self.library.to_json());
                self.sim.handle(HostEvent::FleetSaved(id));
            }
            HostRequest::DeleteFleet(id) => {
                self.library.fleets.remove(&id);
                page::save_raw(KEY, &self.library.to_json());
                self.sim.handle(HostEvent::Fleets(self.library.fleet_summaries()));
            }
        }
    }

    fn account_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr!("EVE Spai fitting")).strong());
            ui.separator();
            let names: Vec<String> = self.accounts.borrow().iter().filter(|a| a.can(SKILLS)).map(|a| a.name.clone()).collect();
            if names.is_empty() {
                ui.label(egui::RichText::new(tr!("No character added for skills")).weak());
            } else {
                ui.add(egui::Label::new(egui::RichText::new(trf!("Skills from: {names}", names = names.join(", "))).weak()).truncate());
            }
            if ui.small_button(tr!("Add character")).on_hover_text(tr!("Sign in to EVE once more with the skills scope; the login stays in this browser")).clicked() {
                self.add_character();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &self.session {
                Some(s) => {
                    ui.label(egui::RichText::new(&s.character_name).weak());
                    ui.label(egui::RichText::new(tr!("Fits sync with eve-spai.com as")).weak());
                }
                None => {
                    if ui.small_button(tr!("Sign in")).on_hover_text(tr!("Sign in to EVE Spai to keep fits on the server and see the Imperium doctrines")).clicked() {
                        self.sign_in();
                    }
                    ui.label(egui::RichText::new(tr!("Fits are kept in this browser")).weak());
                }
            });
        });
    }
}

impl eframe::App for SimWebApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let events: Vec<HostEvent> = std::mem::take(&mut *self.events.lock().unwrap());
        for e in events {
            match e {
                HostEvent::Status(s) if s.is_empty() => self.load_profiles(),
                HostEvent::Status(s) if s == "\u{0}reload" => {
                    self.library = page::load_raw(KEY).map(|t| Library::from_json(&t)).unwrap_or_default();
                    self.sim.handle(HostEvent::Fits(self.library.summaries()));
                }
                e => self.sim.handle(e),
            }
        }
        egui::Panel::top("sim_web_top").frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 5))).show_inside(ui, |ui| self.account_bar(ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| self.sim.show(ui));
        let ctx = ui.ctx().clone();
        for r in self.sim.take_requests() {
            self.request(&ctx, r);
        }
    }
}
