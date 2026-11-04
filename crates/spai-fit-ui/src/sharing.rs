//! Keeping the library on the server, which the pilot opts into, and sharing a saved fit with
//! other characters by a link that always opens the owner's current fit.

use spai_ui::{tr, trf};

use crate::{HostRequest, Session, SimUi};

/// A fit's sharing as the server keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SharingInfo {
    pub link: String,
    pub public: bool,
    pub grantees: Vec<(i64, String)>,
}

/// A fit someone else shares with this character.
#[derive(Clone, Debug, PartialEq)]
pub struct SharedFit {
    pub link: String,
    pub owner_id: i64,
    pub owner_name: String,
    pub id: String,
    pub name: String,
    pub ship: u32,
    pub updated_at: i64,
}

/// The fit behind a link, as its owner has it now.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkedFit {
    pub link: String,
    pub owner_name: String,
    pub mine: bool,
    pub id: String,
    pub rev: i64,
    pub doc: String,
}

/// What a host answers when the fit is not on the server (yet).
pub const MISSING: &str = "\u{0}missing";
/// Links open in a tab are asked for again this often, to follow their owner's saves.
pub const FOLLOW_MS: u64 = 60_000;
const RETRY_MS: u64 = 2_000;

/// A tab showing someone's shared fit: read as they keep it until edited here.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRef {
    pub link: String,
    pub owner: String,
    pub rev: i64,
}

#[derive(Default)]
pub struct ShareDialog {
    pub fit_id: String,
    pub fit_name: String,
    pub info: Option<Result<SharingInfo, String>>,
    pub name: String,
    pub busy: bool,
    pub asked_at: u64,
}

#[derive(Default)]
pub struct SharingState {
    /// Fits, fleets and doctrines go to the server too; off keeps them local.
    pub sync_on: bool,
    /// The sync button is drawn in the simulator's top bar; a host with its own account bar
    /// draws it there instead.
    pub button_in_bar: bool,
    /// Wording for where the local copy lives: the browser's storage or this computer.
    pub in_browser: bool,
    pub dialog: Option<ShareDialog>,
    pub shared: Vec<SharedFit>,
    pub shared_open: bool,
    /// A link the pilot opened that this character may not open.
    pub refused: bool,
    /// A link the pilot opened, so its answer opens a tab; a follow-up answer only refreshes.
    opening: Option<String>,
    followed_at: u64,
    forget_armed: bool,
}

/// The server's answer about a fit's sharing.
pub fn parse_sharing(v: &serde_json::Value) -> SharingInfo {
    SharingInfo {
        link: v["link"].as_str().unwrap_or_default().to_owned(),
        public: v["public"].as_bool().unwrap_or(false),
        grantees: v["grantees"].as_array().map(|a| a.iter().filter_map(|g| Some((g["char_id"].as_i64()?, g["name"].as_str()?.to_owned()))).collect()).unwrap_or_default(),
    }
}

pub fn parse_shared(v: &serde_json::Value) -> Vec<SharedFit> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| {
                    Some(SharedFit {
                        link: f["link"].as_str().filter(|l| !l.is_empty())?.to_owned(),
                        owner_id: f["owner_id"].as_i64()?,
                        owner_name: f["owner_name"].as_str().unwrap_or_default().to_owned(),
                        id: f["id"].as_str()?.to_owned(),
                        name: f["name"].as_str().unwrap_or_default().to_owned(),
                        ship: u32::try_from(f["ship_type_id"].as_i64().unwrap_or(0)).unwrap_or(0),
                        updated_at: f["updated_at"].as_i64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_linked(v: &serde_json::Value) -> Result<LinkedFit, String> {
    let text = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
    if !v["doc"].is_object() {
        return Err(tr!("The server sent no fit").to_owned());
    }
    Ok(LinkedFit { link: text("link"), owner_name: text("owner_name"), mine: v["mine"].as_bool().unwrap_or(false), id: text("id"), rev: v["rev"].as_i64().unwrap_or(0), doc: v["doc"].to_string() })
}

/// The link id out of a sharing link, or a bare `s=` parameter.
pub fn link_id(text: &str) -> Option<&str> {
    let t = text.trim();
    let params = if t.starts_with("s=") { t } else { t.split_once(['#', '?'])?.1 };
    let tail = params.split(['&', '#', '?']).find_map(|kv| kv.strip_prefix("s="))?;
    (!tail.is_empty() && tail.len() <= 32 && tail.chars().all(|c| c.is_ascii_alphanumeric())).then_some(tail)
}

impl SimUi {
    pub fn share_link(&self, link: &str) -> String {
        let base = self.link_base.clone().unwrap_or_else(|| "https://eve-spai.com/sim/".to_owned());
        format!("{base}#s={link}")
    }

    /// Opens the fit behind a sharing link in a tab, once the host has it.
    pub fn open_link(&mut self, link: &str) {
        if let Some(i) = self.sessions.iter().position(|s| s.linked.as_ref().is_some_and(|l| l.link == link)) {
            self.activate(i);
        }
        self.sharing.opening = Some(link.to_owned());
        self.requests.push(HostRequest::OpenLink(link.to_owned()));
    }

    pub(crate) fn linked(&mut self, result: Result<LinkedFit, String>, now_ms: u64) {
        let asked = match &result {
            Ok(l) => self.sharing.opening.as_deref() == Some(l.link.as_str()),
            Err(_) => self.sharing.opening.is_some(),
        };
        if asked {
            self.sharing.opening = None;
        }
        let l = match result {
            Ok(l) => l,
            Err(e) => {
                if asked && e == MISSING {
                    self.sharing.refused = true;
                } else if asked {
                    self.status(trf!("Could not open the link: {e}", e = e));
                }
                return;
            }
        };
        let Ok(fit) = spai_fit::doc::from_json(&l.doc) else {
            if asked {
                self.status(tr!("The shared fit could not be read").to_owned());
            }
            return;
        };
        if l.mine {
            if asked {
                if self.library.iter().any(|e| e.id == l.id) {
                    self.requests.push(HostRequest::LoadFit(l.id));
                } else {
                    let mut s = Session::new(l.id, fit);
                    s.dirty = true;
                    self.open_session(s);
                }
            }
            return;
        }
        let at = self.sessions.iter().position(|s| s.linked.as_ref().is_some_and(|x| x.link == l.link));
        match at {
            Some(i) => {
                let s = &mut self.sessions[i];
                let stale = s.linked.as_ref().is_some_and(|x| x.rev != l.rev);
                if !s.dirty && stale {
                    s.fit = fit;
                    s.rev += 1;
                    s.computed_for = None;
                    s.history = Default::default();
                    s.linked = Some(LinkRef { link: l.link, owner: l.owner_name, rev: l.rev });
                }
                if asked {
                    self.activate(i);
                }
            }
            None if asked => {
                let id = self.new_id(now_ms);
                let mut s = Session::new(id, fit);
                s.linked = Some(LinkRef { link: l.link, owner: l.owner_name, rev: l.rev });
                self.open_session(s);
            }
            None => {}
        }
    }

    /// Linked tabs follow their owner while untouched; the first edit makes the tab the pilot's
    /// own copy, saved under its own id.
    pub(crate) fn follow_links(&mut self, now_ms: u64) {
        let mut copied = false;
        for s in &mut self.sessions {
            if s.dirty && s.linked.take().is_some() {
                copied = true;
            }
        }
        if copied {
            self.status(tr!("Your own copy now: the shared fit stays as its owner keeps it").to_owned());
        }
        if now_ms < self.sharing.followed_at + FOLLOW_MS {
            return;
        }
        self.sharing.followed_at = now_ms;
        let links: Vec<String> = self.sessions.iter().filter_map(|s| s.linked.as_ref().map(|l| l.link.clone())).collect();
        for l in links {
            self.requests.push(HostRequest::OpenLink(l));
        }
        if self.access.signed_in {
            self.requests.push(HostRequest::ListShared);
        }
    }

    /// Shares a library fit, open or not.
    pub fn share_saved(&mut self, id: &str) {
        if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
            self.active = i;
            self.start_share();
            return;
        }
        let name = self.library.iter().find(|e| e.id == id).map(|e| e.name.clone()).unwrap_or_default();
        self.sharing.dialog = Some(ShareDialog { fit_id: id.to_owned(), fit_name: name, ..Default::default() });
        if self.access.signed_in && self.sharing.sync_on {
            self.requests.push(HostRequest::Sharing(id.to_owned()));
        }
    }

    pub fn start_share(&mut self) {
        let Some(s) = self.session() else { return };
        if s.linked.is_some() {
            self.status(tr!("Someone else's fit: copy it to share your own").to_owned());
            return;
        }
        let (fit_id, fit_name) = (s.id.clone(), s.fit.name.clone());
        if s.dirty {
            self.save();
        }
        self.sharing.dialog = Some(ShareDialog { fit_id: fit_id.clone(), fit_name, ..Default::default() });
        if self.access.signed_in && self.sharing.sync_on {
            self.requests.push(HostRequest::Sharing(fit_id));
        }
    }

    pub(crate) fn sharing_answer(&mut self, fit: String, result: Result<SharingInfo, String>, now_ms: u64) {
        if let Some(d) = self.sharing.dialog.as_mut().filter(|d| d.fit_id == fit) {
            d.busy = false;
            d.asked_at = now_ms;
            match (&result, &d.info) {
                // A refused add keeps the list and says why.
                (Err(e), Some(Ok(_))) if e != MISSING => {
                    let e = e.clone();
                    self.status(e);
                }
                _ => d.info = Some(result),
            }
        }
    }

    pub(crate) fn sharing_window(&mut self, ctx: &egui::Context, now_ms: u64) {
        use egui_phosphor::regular as icon;
        let Some(d) = &mut self.sharing.dialog else { return };
        // Not up yet: asked again until the save gets there.
        if matches!(&d.info, Some(Err(e)) if e == MISSING) && !d.busy && now_ms >= d.asked_at + RETRY_MS && self.sharing.sync_on {
            d.busy = true;
            self.requests.push(HostRequest::Sharing(d.fit_id.clone()));
        }
        if matches!(&d.info, Some(Err(e)) if e == MISSING) {
            ctx.request_repaint_after(std::time::Duration::from_millis(RETRY_MS));
        }
        let mut open = true;
        let mut out: Vec<HostRequest> = Vec::new();
        let mut copy: Option<String> = None;
        let mut turn_on = false;
        let link = d.info.as_ref().and_then(|r| r.as_ref().ok()).map(|i| i.link.clone());
        let link_url = link.as_ref().map(|l| {
            let base = self.link_base.clone().unwrap_or_else(|| "https://eve-spai.com/sim/".to_owned());
            format!("{base}#s={l}")
        });
        let size = egui::vec2(440.0, 0.0);
        egui::Window::new(trf!("Share {fit}", fit = crate::text::clip(&d.fit_name, 40)))
            .id(egui::Id::new("sim_share_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(size.x)
            .default_pos(ctx.content_rect().center() - egui::vec2(size.x / 2.0, 160.0))
            .show(ctx, |ui| {
                ui.set_width(size.x);
                if !self.access.signed_in {
                    ui.label(tr!("Sign in with EVE to share fits."));
                    return;
                }
                if !self.sharing.sync_on {
                    ui.label(tr!("A shared fit is kept on eve-spai.com, so sharing needs server sync on."));
                    ui.add_space(4.0);
                    if ui.button(trf!("{icon} Turn on server sync", icon = icon::CLOUD_CHECK)).clicked() {
                        turn_on = true;
                    }
                    return;
                }
                ui.label(egui::RichText::new(tr!("Those you share it with see your fit as you change it; they copy it to make changes of their own.")).weak());
                ui.add_space(6.0);
                let info = match &d.info {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr!("Asking the server…"));
                        });
                        return;
                    }
                    Some(Err(e)) if e == MISSING => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr!("Waiting for the fit to reach the server…"));
                        });
                        return;
                    }
                    Some(Err(e)) => {
                        ui.label(egui::RichText::new(e.clone()).color(ui.visuals().error_fg_color));
                        if ui.button(tr!("Try again")).clicked() {
                            out.push(HostRequest::Sharing(d.fit_id.clone()));
                        }
                        return;
                    }
                    Some(Ok(info)) => info.clone(),
                };
                ui.label(egui::RichText::new(tr!("Link")).strong());
                ui.horizontal(|ui| {
                    let mut url = link_url.clone().unwrap_or_default();
                    let w = ui.available_width() - 80.0;
                    ui.add_sized([w, ui.spacing().interact_size.y], egui::TextEdit::singleline(&mut url).interactive(false));
                    if ui.add_sized([72.0, ui.spacing().interact_size.y], egui::Button::new(trf!("{icon} Copy", icon = icon::COPY))).clicked() {
                        copy = link_url.clone();
                    }
                });
                let mut public = info.public;
                if ui.checkbox(&mut public, tr!("Anyone with the link may open it")).on_hover_text(tr!("Off: only you and the characters below, signed in")).changed() {
                    d.busy = true;
                    out.push(HostRequest::SetLinkPublic { fit: d.fit_id.clone(), public });
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr!("Shared with")).strong());
                if info.grantees.is_empty() {
                    ui.label(egui::RichText::new(tr!("Nobody yet.")).weak());
                }
                for (id, name) in &info.grantees {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(crate::text::clip(name, 40)).truncate());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(!d.busy, egui::Button::new(tr!("Remove"))).clicked() {
                                d.busy = true;
                                out.push(HostRequest::Unshare { fit: d.fit_id.clone(), char_id: *id });
                            }
                        });
                    });
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 80.0;
                    let edit = ui.add_sized([w, ui.spacing().interact_size.y], egui::TextEdit::singleline(&mut d.name).hint_text(tr!("Character name")));
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let can = !d.busy && d.name.trim().len() >= 3;
                    if ui.add_enabled(can, egui::Button::new(trf!("{icon} Add", icon = icon::PLUS)).min_size(egui::vec2(72.0, 0.0))).clicked() || enter && can {
                        d.busy = true;
                        out.push(HostRequest::ShareWith { fit: d.fit_id.clone(), name: d.name.trim().to_owned() });
                        d.name.clear();
                    }
                });
            });
        self.requests.extend(out);
        if let Some(text) = copy {
            self.requests.push(HostRequest::CopyText(text));
            self.status(tr!("Link copied").to_owned());
        }
        if turn_on {
            self.set_sync(true);
            if let Some(d) = &self.sharing.dialog {
                self.requests.push(HostRequest::Sharing(d.fit_id.clone()));
            }
        }
        if !open {
            self.sharing.dialog = None;
        }
    }

    pub fn set_sync(&mut self, on: bool) {
        self.sharing.sync_on = on;
        self.requests.push(HostRequest::SetSync(on));
    }

    /// The button that says whether the library goes to the server, and switches it.
    pub fn sync_button(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        use spai_ui::widgets::SteadySelect as _;
        let on = self.sharing.sync_on && self.access.signed_in;
        let label = if on { trf!("{icon} Synced", icon = icon::CLOUD_CHECK) } else { trf!("{icon} Local", icon = icon::CLOUD_SLASH) };
        let button = ui.button(label).on_hover_text(if on { tr!("Fits are saved locally and synced to eve-spai.com") } else { tr!("Fits are saved locally only") });
        let mut set: Option<bool> = None;
        let mut forget = false;
        egui::Popup::from_toggle_button_response(&button).id(egui::Id::new("sim_sync_popup")).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(320.0);
            ui.label(egui::RichText::new(tr!("Server sync")).strong());
            ui.label(if self.sharing.in_browser { tr!("Fits, fleets and doctrines are saved locally in this browser.") } else { tr!("Fits, fleets and doctrines are saved locally on this computer.") });
            ui.label(egui::RichText::new(tr!("With sync on they are also kept on eve-spai.com under your character, so your other devices see them. Only you can read them, and those you share a fit with.")).weak());
            ui.add_space(4.0);
            ui.add_enabled_ui(self.access.signed_in, |ui| {
                ui.horizontal(|ui| {
                    let w = egui::vec2(72.0, 24.0);
                    if ui.menu_label_sized(w, !self.sharing.sync_on, tr!("Off")).clicked() && self.sharing.sync_on {
                        set = Some(false);
                    }
                    if ui.menu_label_sized(w, self.sharing.sync_on, tr!("On")).clicked() && !self.sharing.sync_on {
                        set = Some(true);
                    }
                });
            });
            if !self.access.signed_in {
                ui.label(egui::RichText::new(tr!("Sign in with EVE to sync.")).weak());
                return;
            }
            ui.add_space(6.0);
            let text = if self.sharing.forget_armed { tr!("Click again to delete them from the server") } else { tr!("Delete my fits from the server") };
            if ui.button(trf!("{icon} {text}", icon = icon::TRASH, text = text)).on_hover_text(tr!("Fits, fleets and doctrines on eve-spai.com, and their shares; the local copies stay")).clicked() {
                if self.sharing.forget_armed {
                    forget = true;
                } else {
                    self.sharing.forget_armed = true;
                }
            }
        });
        if !egui::Popup::is_id_open(ui.ctx(), egui::Id::new("sim_sync_popup")) {
            self.sharing.forget_armed = false;
        }
        if let Some(on) = set {
            self.set_sync(on);
        }
        if forget {
            self.sharing.forget_armed = false;
            self.set_sync(false);
            self.requests.push(HostRequest::ForgetServerLibrary);
            self.status(tr!("Deleting your fits from the server; sync is off").to_owned());
        }
    }

    /// Why a link opened nothing, said where it cannot be missed.
    pub(crate) fn refused_window(&mut self, ctx: &egui::Context) {
        if !self.sharing.refused {
            return;
        }
        let mut open = true;
        let mut ok = false;
        let w = 380.0;
        egui::Window::new(tr!("No access to this fit")).id(egui::Id::new("sim_link_refused")).open(&mut open).collapsible(false).resizable(false).default_width(w).default_pos(ctx.content_rect().center() - egui::vec2(w / 2.0, 80.0)).show(ctx, |ui| {
            ui.set_width(w);
            ui.label(if self.access.signed_in {
                tr!("This fit is not shared with your character, or its owner deleted it. Ask the owner to share it with you.")
            } else {
                tr!("Only the characters this fit is shared with can open it. Sign in with EVE to open it with yours.")
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ok = ui.button(tr!("OK")).clicked();
                });
            });
        });
        if !open || ok {
            self.sharing.refused = false;
        }
    }

    /// The fits others share with this character, each opened from its link.
    pub(crate) fn shared_window(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        if !self.sharing.shared_open {
            return;
        }
        let mut open = true;
        let mut opening: Option<String> = None;
        let mut leave: Option<(i64, String)> = None;
        let size = egui::vec2(420.0, 320.0);
        egui::Window::new(tr!("Shared with you")).id(egui::Id::new("sim_shared_window")).open(&mut open).collapsible(false).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
            if !self.access.signed_in {
                ui.label(tr!("Sign in with EVE to see the fits others share with you."));
                return;
            }
            if self.sharing.shared.is_empty() {
                ui.label(egui::RichText::new(tr!("Nobody shares a fit with this character yet.")).weak());
                return;
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for f in &self.sharing.shared {
                    ui.horizontal(|ui| {
                        crate::icons::type_icon(ui, f.ship, 18.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Take it off this list")).clicked() {
                                leave = Some((f.owner_id, f.id.clone()));
                            }
                            if ui.button(tr!("Open")).clicked() {
                                opening = Some(f.link.clone());
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let r = ui.add(egui::Label::new(trf!("{fit}  ({owner})", fit = f.name, owner = f.owner_name)).truncate().sense(egui::Sense::click()));
                                if r.on_hover_text(tr!("Open; it follows its owner's changes")).clicked() {
                                    opening = Some(f.link.clone());
                                }
                            });
                        });
                    });
                }
            });
        });
        if let Some(l) = opening {
            self.open_link(&l);
        }
        if let Some((owner_id, id)) = leave {
            self.sharing.shared.retain(|f| !(f.owner_id == owner_id && f.id == id));
            self.requests.push(HostRequest::LeaveShared { owner_id, id });
        }
        if !open {
            self.sharing.shared_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::link_id;

    #[test]
    fn a_sharing_link_gives_its_id_and_nothing_else_does() {
        assert_eq!(link_id("https://eve-spai.com/sim/#s=Ab3dEf7hJk"), Some("Ab3dEf7hJk"));
        assert_eq!(link_id("#s=Ab3dEf7hJk"), Some("Ab3dEf7hJk"));
        assert_eq!(link_id("s=Ab3"), Some("Ab3"));
        assert_eq!(link_id("https://eve-spai.com/sim/?x=1&s=Q9"), Some("Q9"));
        assert_eq!(link_id("https://eve-spai.com/sim/#fit=z:abc"), None);
        assert_eq!(link_id("[Rifter, s=x]"), None);
        assert_eq!(link_id("https://eve-spai.com/sim/#s=../etc"), None);
    }
}
