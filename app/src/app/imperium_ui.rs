//! The Imperium tab: the fitting simulator, with the desktop answering for storage (sqlite), the
//! fitting data (the blob next to the database, or the mirror's copy) and skills (ESI).

use std::sync::{Arc, Mutex};

use spai_fit_ui::{FitSummary, HostEvent, HostRequest, Profile};

use super::SpaiApp;
use crate::store::Store;

pub(crate) type SimEvents = Arc<Mutex<Vec<HostEvent>>>;

/// The mirror's prebaked fitting data, for a profile whose static-data build predates the blob.
const BLOB_MIRROR: &str = "https://eve-spai.com/sde";
/// Skills older than this are fetched again when the tab opens.
const SKILLS_FRESH_SECS: i64 = 24 * 3600;
const AFFILIATION_FRESH_SECS: i64 = 24 * 3600;

impl SpaiApp {
    /// Whether a signed-in character's alliance is in the Imperium, or the intel channels say so.
    pub(crate) fn is_imperium_member(&self) -> bool {
        self.is_imperium() || self.char_alliances.iter().any(|(_, a, _)| a.is_some_and(br_core::packs::is_imperium))
    }

    /// Looks up the alliances of the signed-in characters, when they have not been looked up today.
    pub(crate) fn refresh_affiliations(&mut self, ctx: &egui::Context) {
        let now = crate::clock::utc().timestamp();
        let stale: Vec<i64> = self
            .characters
            .iter()
            .map(|c| c.id)
            .filter(|id| !self.char_alliances.iter().any(|(c, _, at)| c == id && now - at < AFFILIATION_FRESH_SECS))
            .collect();
        if stale.is_empty() || self.char_alliances_requested {
            return;
        }
        self.char_alliances_requested = true;
        let events = self.sim_events.clone();
        crate::affiliation::fetch_own(
            stale,
            Box::new(move |rows| {
                if let Ok(store) = Store::open() {
                    let _ = store.affiliations_put(&rows);
                }
                events.lock().unwrap().push(HostEvent::Status(String::new()));
            }),
            ctx.clone(),
        );
    }

    pub(crate) fn imperium_view(&mut self, ui: &mut egui::Ui) {
        if !self.sim_opened {
            self.sim_opened = true;
            self.sim_list_fits();
            self.sim_load_profiles();
            self.sim_fetch_stale_skills(ui.ctx());
        }
        let events: Vec<HostEvent> = std::mem::take(&mut *self.sim_events.lock().unwrap());
        for e in events {
            match e {
                HostEvent::Status(s) if s.is_empty() => {
                    self.char_alliances = self.store.as_ref().map(|s| s.affiliations_get()).unwrap_or_default();
                    self.sim_load_profiles();
                }
                e => self.sim.handle(e),
            }
        }
        self.sim.show(ui);
        for r in self.sim.take_requests() {
            self.sim_request(ui.ctx(), r);
        }
    }

    fn sim_request(&mut self, ctx: &egui::Context, r: HostRequest) {
        match r {
            HostRequest::LoadDogma => self.sim_load_dogma(ctx),
            HostRequest::ListFits => self.sim_list_fits(),
            HostRequest::LoadFit(id) => {
                if let Some(doc) = self.store.as_ref().and_then(|s| s.fit_doc(&id)) {
                    self.sim.handle(HostEvent::Fit { id, doc });
                }
            }
            HostRequest::SaveFit { id, name, ship, doc } => {
                let ok = self.store.as_ref().is_some_and(|s| s.fit_put(&id, &name, i64::from(ship), &doc).is_ok());
                if ok {
                    self.sim.handle(HostEvent::Saved(id));
                } else {
                    self.sim.handle(HostEvent::Status(spai_ui::tr!("Could not save the fit").to_owned()));
                }
            }
            HostRequest::DeleteFit(id) => {
                if let Some(s) = &self.store {
                    let _ = s.fit_delete(&id);
                }
                self.sim_list_fits();
            }
            HostRequest::FetchSkills => self.sim_fetch_skills(ctx, true),
            HostRequest::CopyText(t) => ctx.copy_text(t),
        }
    }

    fn sim_list_fits(&mut self) {
        let list = self
            .store
            .as_ref()
            .map(|s| s.fits_list())
            .unwrap_or_default()
            .into_iter()
            .map(|r| FitSummary { id: r.id, name: r.name, ship: u32::try_from(r.ship_type_id).unwrap_or(0), updated_at: r.updated_at })
            .collect();
        self.sim.handle(HostEvent::Fits(list));
    }

    /// One profile per signed-in character whose skills are stored.
    fn sim_load_profiles(&mut self) {
        let Some(store) = &self.store else { return };
        let profiles: Vec<Profile> = self
            .characters
            .iter()
            .filter_map(|c| {
                let (skills, _) = store.skills_get(c.id)?;
                let set = spai_fit::skills::SkillSet::from_esi(skills.into_iter().filter_map(|(id, lvl)| Some((u32::try_from(id).ok()?, lvl.min(5) as u8))));
                Some(Profile { id: c.id.to_string(), name: c.name.clone(), skills: set })
            })
            .collect();
        self.sim.handle(HostEvent::Profiles(profiles));
    }

    fn sim_fetch_stale_skills(&mut self, ctx: &egui::Context) {
        self.sim_fetch_skills(ctx, false);
    }

    fn sim_fetch_skills(&mut self, ctx: &egui::Context, force: bool) {
        let now = crate::clock::utc().timestamp();
        let names: Vec<String> = self
            .characters
            .iter()
            .filter(|c| c.scopes.contains("esi-skills.read_skills"))
            .filter(|c| force || !self.store.as_ref().and_then(|s| s.skills_get(c.id)).is_some_and(|(_, at)| now - at < SKILLS_FRESH_SECS))
            .map(|c| c.name.clone())
            .collect();
        let client_id = super::non_empty_or(&self.settings.sso_client_id, crate::auth::DEFAULT_CLIENT_ID).to_owned();
        for name in names {
            let events = self.sim_events.clone();
            crate::esi::fetch_skills(
                client_id.clone(),
                name,
                Box::new(move |char_id, skills| {
                    if let Ok(mut store) = Store::open() {
                        let _ = store.skills_put(char_id, &skills);
                    }
                    events.lock().unwrap().push(HostEvent::Status(String::new()));
                }),
                ctx.clone(),
            );
        }
    }

    /// The blob next to the database, else the mirror's copy saved there, decoded on a thread.
    fn sim_load_dogma(&mut self, ctx: &egui::Context) {
        let events = self.sim_events.clone();
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("fit-dogma".into()).spawn(move || {
            let result = (|| -> anyhow::Result<spai_fit::Dogma> {
                let dir = crate::store::data_dir()?;
                let path = dir.join(spai_fit::dogma::blob::file_name());
                let bytes = match std::fs::read(&path) {
                    Ok(b) => b,
                    Err(_) => {
                        let url = format!("{BLOB_MIRROR}/{}", spai_fit::dogma::blob::file_name());
                        let client = crate::http::client(120)?;
                        let bytes = client.get(&url).send()?.error_for_status()?.bytes()?.to_vec();
                        spai_fit::dogma::blob::header(&bytes)?;
                        let part = dir.join(format!(".{}.part", spai_fit::dogma::blob::file_name()));
                        std::fs::write(&part, &bytes)?;
                        std::fs::rename(&part, &path)?;
                        bytes
                    }
                };
                spai_fit::dogma::blob::decode(&bytes)
            })();
            let event = match result {
                Ok(d) => HostEvent::Dogma(Arc::new(d)),
                Err(e) => HostEvent::DogmaFailed(format!("{e:#}")),
            };
            events.lock().unwrap().push(event);
            ctx.request_repaint();
        });
    }
}
