//! The page: the simulator with the browser answering for storage, the fitting data, the session
//! and the characters whose skills it may read.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use spai_fit_ui::{HostEvent, HostRequest, Profile, SimUi};
use spai_ui::{tr, trf};
use spai_webkit::accounts::{self, Account, SKILLS};
use spai_webkit::page;
use spai_webkit::sso::{Pending, Session, ACCOUNTS_KEY, PENDING_KEY, SESSION_KEY};

use crate::store::{Library, KEY};

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
        app.load_profiles();
        app
    }

    fn load_profiles(&mut self) {
        let profiles: Vec<Profile> = page::load::<Vec<(i64, String, Vec<(u32, u8)>)>>("spai.sim.skills")
            .unwrap_or_default()
            .into_iter()
            .map(|(id, name, skills)| Profile { id: id.to_string(), name, skills: spai_fit::skills::SkillSet::from_esi(skills) })
            .collect();
        self.sim.handle(HostEvent::Profiles(profiles));
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
            }
            HostRequest::DeleteFit(id) => {
                self.library.remove(&id);
                page::save_raw(KEY, &self.library.to_json());
                self.sim.handle(HostEvent::Fits(self.library.summaries()));
            }
            HostRequest::FetchSkills => self.fetch_skills(ctx),
            HostRequest::CopyText(t) => ctx.copy_text(t),
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
                }
                None => {
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
