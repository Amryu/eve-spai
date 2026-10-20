//! The Imperium tab: the fitting simulator, with the desktop answering for storage (sqlite), the
//! fitting data (the blob next to the database, or the mirror's copy) and skills (ESI).

use std::sync::{Arc, Mutex};

use spai_fit_ui::{Doctrine, DoctrineSide, FitSummary, FleetSummary, HostEvent, HostRequest, Profile};

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
            self.sim_fetch_doctrines(ui.ctx());
            self.sim_sync(ui.ctx());
        }
        let events: Vec<HostEvent> = std::mem::take(&mut *self.sim_events.lock().unwrap());
        for e in events {
            match e {
                HostEvent::Status(s) if s.is_empty() => {
                    self.char_alliances = self.store.as_ref().map(|s| s.affiliations_get()).unwrap_or_default();
                    self.sim_load_profiles();
                }
                HostEvent::Status(s) if s == "\u{0}list" => {
                    let ids: Vec<String> = self.sim.fit_docs.keys().cloned().collect();
                    for id in ids {
                        self.sim.forget_fit_doc(&id);
                    }
                    self.sim_list_fits();
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
                    self.sim_sync(ctx);
                } else {
                    self.sim.handle(HostEvent::Status(spai_ui::tr!("Could not save the fit").to_owned()));
                }
            }
            HostRequest::DeleteFit(id) => {
                if let Some(s) = &self.store {
                    let _ = s.fit_delete(&id);
                }
                self.sim_list_fits();
                self.sim_sync(ctx);
            }
            HostRequest::FetchSkills => self.sim_fetch_skills(ctx, true),
            HostRequest::CopyText(t) => ctx.copy_text(t),
            HostRequest::FetchDoctrines => self.sim_fetch_doctrines(ctx),
            HostRequest::PublishDoctrine { id, side, name, tags, refit_keys, doc } => self.sim_publish(ctx, id, side, name, tags, refit_keys, doc),
            HostRequest::DeleteDoctrine(id) => self.sim_delete_doctrine(ctx, id),
            HostRequest::SyncLibrary => self.sim_sync(ctx),
            HostRequest::FitDocs(ids) => {
                let docs = self.store.as_ref().map(|s| s.fit_docs(&ids)).unwrap_or_default();
                self.sim.handle(HostEvent::FitDocs(docs));
            }
            HostRequest::ListFleets => self.sim_list_fleets(),
            HostRequest::LoadFleet(id) => {
                if let Some(doc) = self.store.as_ref().and_then(|s| s.fleet_doc(&id)) {
                    self.sim.handle(HostEvent::Fleet { id, doc });
                }
            }
            HostRequest::LoadFleetDoc(id) => {
                if let Some(doc) = self.store.as_ref().and_then(|s| s.fleet_doc(&id)) {
                    self.sim.handle(HostEvent::FleetDoc { id, doc });
                }
            }
            HostRequest::SaveFleet { id, name, doc } => {
                if self.store.as_ref().is_some_and(|s| s.fleet_put(&id, &name, &doc).is_ok()) {
                    self.sim.handle(HostEvent::FleetSaved(id));
                } else {
                    self.sim.handle(HostEvent::Status(spai_ui::tr!("Could not save the fleet").to_owned()));
                }
            }
            HostRequest::DeleteFleet(id) => {
                if let Some(s) = &self.store {
                    let _ = s.fleet_delete(&id);
                }
                self.sim_list_fleets();
            }
        }
    }

    fn sim_list_fleets(&mut self) {
        let list = self.store.as_ref().map(|s| s.fleets_list()).unwrap_or_default().into_iter().map(|(id, name, updated_at)| FleetSummary { id, name, updated_at }).collect();
        self.sim.handle(HostEvent::Fleets(list));
    }

    /// The signed-in character the server work runs as: the active one, else the first.
    fn sim_character(&self) -> Option<i64> {
        self.characters.iter().find(|c| c.name == self.active_character).or(self.characters.first()).map(|c| c.id)
    }

    fn sim_fetch_doctrines(&mut self, ctx: &egui::Context) {
        let events = self.sim_events.clone();
        let (char_id, path) = (self.sim_character(), self.store.as_ref().map(|s| s.path().to_path_buf()));
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("sim-doctrines".into()).spawn(move || {
            let base = crate::brshare::api_base();
            let bearer = match (char_id, path) {
                (Some(c), Some(p)) => crate::brshare::valid_session(&p, c),
                _ => None,
            };
            let Ok(client) = crate::http::client(30) else { return };
            let access = match &bearer {
                Some(b) => client.get(format!("{base}/api/sim/me")).bearer_auth(b).send().ok().filter(|r| r.status().is_success()).and_then(|r| r.json::<serde_json::Value>().ok()),
                None => None,
            };
            let (imperium, admin) = access.as_ref().map_or((false, false), |v| (v["imperium"].as_bool().unwrap_or(false), v["admin"].as_bool().unwrap_or(false)));
            events.lock().unwrap().push(HostEvent::Access { signed_in: bearer.is_some(), imperium, admin });
            let mut req = client.get(format!("{base}/api/sim/doctrines"));
            if let Some(b) = &bearer {
                req = req.bearer_auth(b);
            }
            match req.send().and_then(|r| r.error_for_status()).and_then(|r| r.json::<Vec<Doctrine>>()) {
                Ok(list) => events.lock().unwrap().push(HostEvent::Doctrines(list)),
                Err(e) => crate::esilog::record("sim doctrines", &format!("{e}")),
            }
            ctx.request_repaint();
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn sim_publish(&mut self, ctx: &egui::Context, id: String, side: DoctrineSide, name: String, tags: Vec<String>, refit_keys: Vec<String>, doc: String) {
        let events = self.sim_events.clone();
        let (char_id, path) = (self.sim_character(), self.store.as_ref().map(|s| s.path().to_path_buf()));
        let ctx2 = ctx.clone();
        let _ = std::thread::Builder::new().name("sim-publish".into()).spawn(move || {
            let outcome = (|| -> Result<(), String> {
                let (c, p) = char_id.zip(path).ok_or("sign in first")?;
                let bearer = crate::brshare::valid_session(&p, c).ok_or("the sign-in with eve-spai.com failed")?;
                let client = crate::http::client(30).map_err(|e| e.to_string())?;
                let doc: serde_json::Value = serde_json::from_str(&doc).map_err(|e| e.to_string())?;
                let body = serde_json::json!({ "side": side, "name": name, "tags": tags, "refit_keys": refit_keys, "doc": doc });
                let resp = client.put(format!("{}/api/sim/doctrines/{id}", crate::brshare::api_base())).bearer_auth(&bearer).json(&body).send().map_err(|e| e.to_string())?;
                if !resp.status().is_success() {
                    return Err(resp.json::<serde_json::Value>().ok().and_then(|v| v["error"].as_str().map(str::to_owned)).unwrap_or_else(|| "refused".into()));
                }
                Ok(())
            })();
            let mut ev = events.lock().unwrap();
            match outcome {
                Ok(()) => ev.push(HostEvent::Status(spai_ui::tr!("Published").to_owned())),
                Err(e) => ev.push(HostEvent::Status(spai_ui::trf!("Could not publish: {e}", e = e))),
            }
            ctx2.request_repaint();
        });
        self.sim_fetch_doctrines(ctx);
    }

    fn sim_delete_doctrine(&mut self, ctx: &egui::Context, id: String) {
        let events = self.sim_events.clone();
        let (char_id, path) = (self.sim_character(), self.store.as_ref().map(|s| s.path().to_path_buf()));
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("sim-doctrine-delete".into()).spawn(move || {
            if let (Some(c), Some(p)) = (char_id, path) {
                if let (Some(b), Ok(client)) = (crate::brshare::valid_session(&p, c), crate::http::client(30)) {
                    if let Err(e) = client.delete(format!("{}/api/sim/doctrines/{id}", crate::brshare::api_base())).bearer_auth(&b).send().and_then(|r| r.error_for_status()) {
                        events.lock().unwrap().push(HostEvent::Status(spai_ui::trf!("Could not delete: {e}", e = e)));
                    }
                }
            }
            ctx.request_repaint();
        });
    }

    /// Pushes what changed here, takes what the server has newer, then lists the library again.
    fn sim_sync(&mut self, ctx: &egui::Context) {
        let events = self.sim_events.clone();
        let (char_id, path) = (self.sim_character(), self.store.as_ref().map(|s| s.path().to_path_buf()));
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("sim-sync".into()).spawn(move || {
            let (Some(c), Some(p)) = (char_id, path) else { return };
            let Some(bearer) = crate::brshare::valid_session(&p, c) else { return };
            let Ok(client) = crate::http::client(30) else { return };
            let Ok(store) = Store::open() else { return };
            let base = crate::brshare::api_base();
            for (id, name, ship, doc, _rev, server_rev, deleted) in store.fits_unsynced() {
                if deleted {
                    if client.delete(format!("{base}/api/sim/fits/{id}")).bearer_auth(&bearer).send().is_ok_and(|r| r.status().is_success() || r.status().as_u16() == 404) {
                        let _ = store.fit_purge(&id);
                    }
                    continue;
                }
                let Ok(doc_json) = serde_json::from_str::<serde_json::Value>(&doc) else { continue };
                let body = serde_json::json!({ "name": name, "rev": server_rev, "doc": doc_json, "ship_type_id": ship });
                if let Ok(r) = client.put(format!("{base}/api/sim/fits/{id}")).bearer_auth(&bearer).json(&body).send() {
                    // Saved elsewhere since: the server's copy comes down below, this one stays.
                    if r.status().is_success() {
                        if let Some(rev) = r.json::<serde_json::Value>().ok().and_then(|v| v["rev"].as_i64()) {
                            let _ = store.fit_mark_synced(&id, rev);
                        }
                    }
                }
            }
            let Ok(list) = client.get(format!("{base}/api/sim/fits")).bearer_auth(&bearer).send().and_then(|r| r.error_for_status()).and_then(|r| r.json::<Vec<serde_json::Value>>()) else { return };
            let local: std::collections::HashMap<String, (i64, bool)> = store.fits_revisions().into_iter().map(|(id, rev, deleted)| (id, (rev, deleted))).collect();
            for entry in list {
                let (Some(id), Some(rev)) = (entry["id"].as_str(), entry["rev"].as_i64()) else { continue };
                if local.get(id).is_some_and(|(r, d)| *r >= rev || *d) {
                    continue;
                }
                let Ok(full) = client.get(format!("{base}/api/sim/fits/{id}")).bearer_auth(&bearer).send().and_then(|r| r.error_for_status()).and_then(|r| r.json::<serde_json::Value>()) else { continue };
                let _ = store.fit_put_from_server(id, full["name"].as_str().unwrap_or(""), full["ship_type_id"].as_i64().unwrap_or(0), &full["doc"].to_string(), rev, full["updated_at"].as_i64().unwrap_or(0));
            }
            events.lock().unwrap().push(HostEvent::Status("\u{0}list".into()));
            ctx.request_repaint();
        });
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
