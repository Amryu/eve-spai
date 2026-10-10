//! The wormhole sharing window: groups, members, invites and join requests. The work happens on
//! the share thread (`crate::share::engine`); this only shows its state and sends it commands.

use spai_ui::i18n::Tr;

use super::*;
use crate::share::engine::{self, Cmd};
use crate::share::ops::Role;
use crate::store::{ShareGroup, SharePrefs};

#[derive(Default)]
pub(crate) struct ShareUi {
    pub(crate) right_edge: Option<f32>,
    handle: Option<engine::Handle>,
    started: bool,
    pub(crate) open: bool,
    new_name: String,
    join_link: String,
    invite_for: String,
    /// The next invite lets its character in as a viewer rather than a member.
    invite_viewer: bool,
    char_id: Option<i64>,
    /// What a group created or joined from the window gets sent.
    new_prefs: Option<SharePrefs>,
    migrated: bool,
    generation: u64,
    groups: Vec<ShareGroup>,
    refreshed: Option<std::time::Instant>,
    kicked: Option<std::time::Instant>,
    ctx: Option<egui::Context>,
    /// The group whose members dialog is open.
    members_of: Option<String>,
    members_filter: String,
    /// A group about to be deleted: its id, its name, and what the owner has typed so far.
    delete: Option<(String, String, String)>,
}

impl SpaiApp {
    /// Starts the share thread once there is a group to keep in step, reloads what it changed, and
    /// nudges it when local changes are waiting.
    pub(crate) fn wh_share_tick(&mut self, ctx: &egui::Context) {
        if self.wh_share.ctx.is_none() {
            self.wh_share.ctx = Some(ctx.clone());
        }
        let now = std::time::Instant::now();
        if self.wh_share.refreshed.is_none_or(|t| now - t > std::time::Duration::from_secs(2)) {
            self.wh_share.refreshed = Some(now);
            if !std::mem::replace(&mut self.wh_share.migrated, true) {
                if let Some(s) = &self.store {
                    s.share_prefs_migrate(self.settings.wh_share_target.as_deref());
                }
            }
            self.wh_share.groups = self.store.as_ref().map(|s| s.share_groups()).unwrap_or_default();
        }
        if self.wh_share.handle.is_none() && !self.wh_share.started && (!self.wh_share.groups.is_empty() || self.wh_share.open) {
            self.wh_share.started = true;
            self.wh_share.handle = Some(engine::spawn(ctx.clone()));
        }
        let Some(h) = &self.wh_share.handle else { return };
        let generation = h.status.lock().unwrap().generation;
        if generation != self.wh_share.generation {
            self.wh_share.generation = generation;
            self.wh_reloaded = None;
            self.wh_graph.forget_sigs();
        }
        if self.wh_share.kicked.is_none_or(|t| now - t > std::time::Duration::from_secs(3)) {
            self.wh_share.kicked = Some(now);
            if self.store.as_ref().is_some_and(|s| s.share_outbox_len() > 0) && !h.status.lock().unwrap().busy {
                let _ = h.tx.send(Cmd::SyncNow);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn wh_share_seed(&mut self, groups: Vec<ShareGroup>) {
        // The scratch profile outlives a run: what another scene stored for these groups (keys,
        // members) must not show here.
        if let Some(store) = &self.store {
            for g in &groups {
                store.share_group_forget(&g.id);
            }
        }
        self.wh_share.groups = groups;
    }

    /// The delete dialog open for `group`, with `typed` so far.
    #[cfg(test)]
    pub(crate) fn wh_share_seed_delete(&mut self, group: &str, name: &str, typed: &str) {
        self.wh_share.delete = Some((group.to_owned(), name.to_owned(), typed.to_owned()));
    }

    /// An invite link just made for `group`, as the share thread reports it, in a group this
    /// install holds the key of.
    #[cfg(test)]
    pub(crate) fn wh_share_seed_invite(&mut self, group: &str, link: &str, for_name: &str) {
        if let Some(store) = &self.store {
            store.share_key_save(group, 0, &[1; 32]);
        }
        let (tx, _) = std::sync::mpsc::channel();
        let status = engine::Status { invite: Some((group.to_owned(), link.to_owned(), for_name.to_owned())), ..Default::default() };
        self.wh_share.handle = Some(engine::Handle { tx, status: std::sync::Arc::new(std::sync::Mutex::new(status)) });
    }

    /// Members stored for `group`, and optionally its members dialog open.
    #[cfg(test)]
    pub(crate) fn wh_share_seed_members(&mut self, group: &str, members: &[crate::share::ops::Member], open: bool) {
        if let Some(s) = &self.store {
            s.share_members_save(group, members);
        }
        if open {
            self.wh_share.members_of = Some(group.to_owned());
        }
    }

    fn share_send(&mut self, cmd: Cmd) {
        if self.wh_share.handle.is_none() {
            let Some(ctx) = self.wh_share.ctx.clone() else { return };
            self.wh_share.started = true;
            self.wh_share.handle = Some(engine::spawn(ctx));
        }
        if let Some(h) = &self.wh_share.handle {
            let _ = h.tx.send(cmd);
        }
        self.wh_share.refreshed = None;
    }

    /// Opens the sharing window with an invite link ready to join.
    pub(crate) fn wh_share_open_join(&mut self, link: &str) {
        self.wh_share.join_link = link.to_owned();
        self.wh_share.open = true;
    }

    /// Saves a group's choices and acts on what changed: what is now sent goes out, what is now
    /// taken is read again from the group's log, and hiding shows or drops its data here.
    fn set_share_prefs(&mut self, group: &str, was: SharePrefs, now: SharePrefs) {
        let Some(store) = self.store.as_ref() else { return };
        store.share_prefs_save(group, now);
        let (holes_on, sigs_on) = (now.send_holes && !was.send_holes, now.send_sigs && !was.send_sigs);
        if holes_on || sigs_on {
            store.share_queue_group(group, holes_on, sigs_on);
        }
        if (now.recv_holes && !was.recv_holes) || (now.recv_sigs && !was.recv_sigs) {
            self.share_send(Cmd::Rescan { group: group.to_owned() });
        }
        if now.hidden != was.hidden {
            self.wh_reloaded = None;
            self.wh_graph.forget_sigs();
            self.sig_browser_refresh();
        }
        if let Some(g) = self.wh_share.groups.iter_mut().find(|g| g.id == group) {
            g.prefs = now;
        }
    }

    /// The sharing state as one line of text for the Wormholes header: nothing when not in a
    /// group. Text only, so the row never changes height as syncs come and go.
    pub(crate) fn share_status_line(&self) -> Option<(String, egui::Color32, String)> {
        use egui_phosphor::regular as icon;
        if self.wh_share.groups.is_empty() {
            return None;
        }
        let status = self.wh_share.handle.as_ref()?.status.lock().unwrap().clone();
        let weak = egui::Color32::from_gray(150);
        if let Some(e) = status.error {
            return Some((trf!("{icon}  Sync failed", icon = icon::CLOUD_WARNING), crate::theme::standing::WARNING, e));
        }
        if status.busy {
            return Some((trf!("{icon}  Syncing", icon = icon::CLOUD_ARROW_UP), weak, tr!("Sending and fetching group changes").into()));
        }
        let last = status.synced_at.values().copied().min();
        let names: Vec<&str> = self.wh_share.groups.iter().map(|g| g.name.as_str()).collect();
        let text = match last {
            Some(t) => trf!("{icon}  Synced {ago} ago", icon = icon::CLOUD_CHECK, ago = human_ago(crate::clock::utc().timestamp() - t)),
            None => trf!("{icon}  Not synced yet", icon = icon::CLOUD_CHECK),
        };
        Some((text, weak, format!("Sharing with {}", names.join(", "))))
    }

    /// The name of the sharing group a hole came from, for badges.
    pub(crate) fn share_group_name(&self, id: &str) -> Option<&str> {
        self.wh_share.groups.iter().find(|g| g.id == id).map(|g| g.name.as_str())
    }

    pub(crate) fn wh_share_window(&mut self, ctx: &egui::Context) {
        if !self.wh_share.open {
            return;
        }
        use egui_phosphor::regular as icon;
        let mut open = true;
        let status = self.wh_share.handle.as_ref().map(|h| h.status.lock().unwrap().clone()).unwrap_or_default();
        let groups = self.wh_share.groups.clone();
        let chars: Vec<(i64, String)> = self.characters.iter().map(|c| (c.id, c.name.clone())).collect();
        if self.wh_share.char_id.is_none() {
            self.wh_share.char_id = chars.first().map(|c| c.0);
        }
        let mut cmd: Option<Cmd> = None;
        let mut prefs: Option<(String, SharePrefs, SharePrefs)> = None;
        let counts: Vec<(i64, i64)> = groups.iter().map(|g| self.store.as_ref().map_or((0, 0), |s| s.share_group_counts(&g.id))).collect();
        let mut copy: Option<String> = None;
        // Wide enough for the groups table in the chosen language, whose column titles can run
        // longer than English.
        let table_w = {
            let font = egui::TextStyle::Body.resolve(&ctx.global_style());
            let w = |t: &str| ctx.fonts_mut(|f| f.layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::WHITE).size().x);
            let names = groups.iter().map(|g| w(&g.name)).fold(w(tr!("Group")), f32::max);
            names + 2.0 * (w(tr!("Wormholes")) + w(tr!("Probe scans"))) + w(tr!("Hide")) + 20.0 + 5.0 * 14.0 + 16.0
        };
        let shown = egui::Window::new(trf!("{icon}  Wormhole sharing", icon = icon::USERS_THREE))
            .open(&mut open)
            .resizable(true)
            .default_width(table_w.max(520.0))
            // Left of centre and below the toolbars, leaving room beside it for the members dialog.
            .pivot(egui::Align2::RIGHT_TOP)
            .default_pos(egui::pos2(ctx.content_rect().center().x - 4.0, ctx.content_rect().top() + 150.0))
            // A longer translation widens it to the left; the nav rail stays clear.
            .constrain_to(ctx.content_rect().with_min_x(ctx.content_rect().left() + self.nav_width() + 8.0))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        tr!("Groups are end-to-end encrypted: the server stores only what it cannot read, and only members hold the keys."),
                    )
                    .weak(),
                );
                // Always there at one height, so the window does not jump as syncs start and end.
                let row = ui.spacing().interact_size.y;
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), row),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_height(row);
                        if status.busy {
                            ui.add(egui::Spinner::new().size(row * 0.7));
                            ui.label(tr!("Syncing"));
                        } else {
                            ui.label(egui::RichText::new(tr!("Up to date")).weak());
                        }
                    },
                );
                if let Some(e) = &status.error {
                    ui.colored_label(crate::theme::standing::WARNING, e);
                }
                for name in &status.deleted {
                    ui.colored_label(crate::theme::standing::WARNING, trf!("{name} was deleted by its owner.", name = name));
                }
                if let Some(fp) = &status.fingerprint {
                    ui.horizontal(|ui| {
                        ui.label(tr!("Your key fingerprint"));
                        ui.label(egui::RichText::new(fp).monospace().strong());
                    })
                    .response
                    .on_hover_text(tr!("When you ask to join, read this to whoever approves you so they can check it is really you"));
                }
                ui.add_space(6.0);
                if !groups.is_empty() {
                    ui.label(egui::RichText::new(tr!("What each group gets and gives")).strong());
                    // Both ways: a language with longer column titles scrolls the table rather than widening
                    // the window over the dialogs that open beside it.
                    egui::ScrollArea::both().id_salt("wh_share_prefs_scroll").max_height(160.0).max_width(ui.available_width()).show(ui, |ui| {
                    egui::Grid::new("wh_share_prefs").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                        ui.label("");
                        ui.label(trf!("{icon}  Send", icon = icon::UPLOAD_SIMPLE)).on_hover_text(tr!("What of yours goes to the group. What came from another group never does."));
                        ui.label("");
                        ui.label(trf!("{icon}  Receive", icon = icon::DOWNLOAD_SIMPLE)).on_hover_text(tr!("What the group's members share that is taken in here"));
                        ui.label("");
                        ui.label("");
                        ui.end_row();
                        ui.label(egui::RichText::new(tr!("Group")).weak());
                        for _ in 0..2 {
                            ui.label(egui::RichText::new(tr!("Wormholes")).weak());
                            ui.label(egui::RichText::new(tr!("Probe scans")).weak());
                        }
                        ui.label(egui::RichText::new(tr!("Hide")).weak());
                        ui.end_row();
                        for (g, (holes, sigs)) in groups.iter().zip(&counts) {
                            let was = g.prefs;
                            let mut p = was;
                            ui.label(&g.name);
                            let viewer = !g.role.can_write();
                            let why = "A viewer sees the group's wormholes and shares nothing";
                            ui.add_enabled(!viewer, egui::Checkbox::without_text(&mut p.send_holes))
                                .on_hover_text(trf!("Send your wormholes to {v}", v = g.name))
                                .on_disabled_hover_text(why);
                            ui.add_enabled(!viewer, egui::Checkbox::without_text(&mut p.send_sigs))
                                .on_hover_text(trf!("Send your probe scans to {v}", v = g.name))
                                .on_disabled_hover_text(why);
                            ui.checkbox(&mut p.recv_holes, "").on_hover_text(trf!("Take in the wormholes {v} shares", v = g.name));
                            ui.checkbox(&mut p.recv_sigs, "").on_hover_text(trf!("Take in the probe scans {v} shares", v = g.name));
                            let eye = if p.hidden { icon::EYE_SLASH } else { icon::EYE };
                            ui.checkbox(&mut p.hidden, eye).on_hover_text(trf!("Hide the {holes} wormholes and {sigs} signatures that came from {v} from the map, table and lists, until you switch it off. They stay stored and keep syncing.", holes = holes, sigs = sigs, v = g.name));
                            ui.end_row();
                            if p != was {
                                prefs = Some((g.id.clone(), was, p));
                            }
                        }
                    });
                    });
                }
                ui.separator();
                // Half the window at most, whatever the number of groups: the members of each are in
                // their own dialog, so this lists only what needs doing.
                let groups_h = (ctx.content_rect().height() * 0.5).max(200.0);
                egui::ScrollArea::vertical().id_salt("wh_share_groups").max_height(groups_h).show(ui, |ui| {
                    if groups.is_empty() {
                        ui.label(egui::RichText::new(tr!("Not in any group yet.")).weak());
                    }
                    for g in &groups {
                        let has_key = self.store.as_ref().is_some_and(|s| s.share_key(&g.id, g.epoch).is_some());
                        let synced = status.synced_at.get(&g.id).map(|t| trf!(", synced {ago} ago", ago = human_ago(crate::clock::utc().timestamp() - t)));
                        let title = if has_key {
                            format!("{}  ({}{})", g.name, g.role.label().tr(), synced.unwrap_or_default())
                        } else {
                            trf!("{name}  (waiting to be let in)", name = g.name)
                        };
                        egui::CollapsingHeader::new(title).id_salt(("wh_share_group", &g.id)).default_open(true).show(ui, |ui| {
                            if !has_key {
                                if let Some(fp) = &status.fingerprint {
                                    ui.label(tr!("The invite lets you in as soon as the EVE Spai that made it next syncs."));
                                    ui.label(trf!("Fingerprint: {fp}", fp = fp));
                                }
                            }
                            let members = self.store.as_ref().map(|s| s.share_members(&g.id)).unwrap_or_default();
                            ui.horizontal(|ui| {
                                let owner = members.iter().find(|m| m.role == Role::Owner).map_or("", |m| m.name.as_str());
                                let n = members.len();
                                ui.label(if n == 1 { trf!("1 member \u{b7} owner {owner}", owner = owner) } else { trf!("{n} members \u{b7} owner {owner}", n = n, owner = owner) });
                                if ui.button(trf!("{icon}  Members\u{2026}", icon = icon::USERS)).on_hover_text(tr!("Everyone in the group, their roles and keys")).clicked() {
                                    self.wh_share.members_of = Some(g.id.clone());
                                    self.wh_share.members_filter.clear();
                                }
                                if g.role == Role::Owner && has_key && ui.button(icon::TRASH).on_hover_text(tr!("Delete the group for every member")).clicked() {
                                    self.wh_share.delete = Some((g.id.clone(), g.name.clone(), String::new()));
                                }
                            });
                            // Members invite too, and let their own invitees in as viewers only; anyone
                            // invites their own other devices.
                            if has_key {
                                let admin = g.role.can_manage();
                                let reqs = status.requests.get(&g.id).cloned().unwrap_or_default();
                                if !reqs.is_empty() {
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new(tr!("Asking to join")).strong());
                                    ui.label(
                                        egui::RichText::new(tr!("To be certain, have them read their key fingerprint to you and compare it before approving."))
                                            .weak(),
                                    );
                                }
                                for r in reqs {
                                    // Another device of someone already in: it takes their role.
                                    let existing = members.iter().find(|m| m.char_id == r.row.char_id).map(|m| m.role);
                                    ui.horizontal(|ui| {
                                        ui.label(&r.row.name);
                                        if let Some(role) = existing {
                                            ui.label(egui::RichText::new(trf!("another device ({role})", role = role.label().tr())).weak())
                                                .on_hover_text(tr!("Already a member: the new device gets their role"));
                                        }
                                        if r.verified {
                                            ui.label(egui::RichText::new(trf!("{icon} came through our invite", icon = icon::CHECK)).color(crate::theme::chip::CLEAR));
                                        } else if let Some(meant) = &r.meant_for {
                                            ui.colored_label(crate::theme::standing::WARNING, trf!("{icon} the invite was for {meant}", icon = icon::WARNING, meant = meant))
                                                .on_hover_text(tr!("Someone else used the link. Do not approve; make a new invite for the right character."));
                                        } else {
                                            ui.colored_label(crate::theme::standing::WARNING, trf!("{icon} does not match an invite of ours", icon = icon::WARNING))
                                                .on_hover_text(tr!("The keys it carries are not the ones our invite link vouches for. Do not trust it."));
                                        }
                                        if let Some(k) = r.keys {
                                            ui.label(egui::RichText::new(k.fingerprint()).monospace().weak());
                                        }
                                        let approve = |role: Role| Cmd::Approve {
                                            group: g.id.clone(),
                                            char_id: r.row.char_id,
                                            device_id: r.row.device_id.clone(),
                                            role,
                                        };
                                        if !admin {
                                            if existing.is_none()
                                                && ui.add_enabled(r.verified, egui::Button::new(tr!("Approve as viewer"))).on_hover_text(tr!("Sees the group's wormholes, shares nothing")).clicked()
                                            {
                                                cmd = Some(approve(Role::Viewer));
                                            }
                                        } else if existing.is_some() {
                                            if ui.add_enabled(r.verified, egui::Button::new(tr!("Approve device"))).clicked() {
                                                cmd = Some(approve(Role::Member));
                                            }
                                        } else {
                                            if ui.add_enabled(r.verified, egui::Button::new(tr!("Approve"))).on_hover_text(tr!("As a member: sees and shares")).clicked() {
                                                cmd = Some(approve(Role::Member));
                                            }
                                            if ui.add_enabled(r.verified, egui::Button::new(tr!("As viewer"))).on_hover_text(tr!("Sees the group's wormholes, shares nothing")).clicked() {
                                                cmd = Some(approve(Role::Viewer));
                                            }
                                        }
                                        if ui.button(tr!("Reject")).clicked() {
                                            cmd = Some(Cmd::Reject { group: g.id.clone(), char_id: r.row.char_id, device_id: r.row.device_id.clone() });
                                        }
                                    });
                                }
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut self.wh_share.invite_for).hint_text(tr!("Character it is for")).desired_width(180.0));
                                    let typed = self.wh_share.invite_for.trim();
                                    // Yourself is another device: at your own rank, which no one picks.
                                    let me = members.iter().find(|m| m.char_id == g.char_id).is_some_and(|m| m.name.eq_ignore_ascii_case(typed));
                                    let ok = !typed.is_empty() && (me || g.role.can_write());
                                    if me {
                                        ui.label(egui::RichText::new(trf!("as {v}", v = g.role.label().tr())).weak())
                                            .on_hover_text(tr!("Your own other device: it keeps your rank"));
                                    } else if !g.role.can_write() {
                                        ui.label(egui::RichText::new(tr!("your own devices only")).weak())
                                            .on_hover_text(tr!("A viewer invites only their own character, for another device"));
                                    } else if admin {
                                        let v = &mut self.wh_share.invite_viewer;
                                        egui::ComboBox::from_id_salt(("invite_role", &g.id))
                                            .width(80.0)
                                            .selected_text(if *v { "as Viewer" } else { "as Member" })
                                            .show_ui(ui, |ui| {
                                                ui.menu_value(v, false, tr!("as Member")).on_hover_text(tr!("Sees and shares"));
                                                ui.menu_value(v, true, tr!("as Viewer")).on_hover_text(tr!("Sees the group's wormholes, shares nothing"));
                                            });
                                    } else {
                                        ui.label(egui::RichText::new(tr!("as Viewer")).weak());
                                    }
                                    if ui
                                        .add_enabled(ok, egui::Button::new(trf!("{icon}  New invite link", icon = icon::LINK)))
                                        .on_hover_text(
                                            tr!("Only that character can use it, once, within two days. Using it lets them in, while this app is running."),
                                        )
                                        .clicked()
                                    {
                                        let role = if admin && !self.wh_share.invite_viewer { Role::Member } else { Role::Viewer };
                                        cmd = Some(Cmd::Invite { group: g.id.clone(), for_name: self.wh_share.invite_for.trim().to_owned(), role });
                                    }
                                });
                                if let Some((gid, link, for_name)) = &status.invite {
                                    if *gid == g.id {
                                        // One link for everyone: it opens a page that offers EVE Spai or the browser.
                                        let web = link.split_once("join/").map_or_else(|| link.clone(), |(_, rest)| format!("{}/wh/join/{rest}", crate::brshare::api_base()));
                                        ui.horizontal(|ui| {
                                            let copy_w = ui.spacing().interact_size.y + ui.spacing().item_spacing.x * 2.0 + 8.0;
                                            ui.add(egui::TextEdit::singleline(&mut web.clone()).desired_width((ui.available_width() - copy_w).max(80.0)));
                                            if ui.button(icon::COPY).on_hover_text(tr!("Copy the link")).clicked() {
                                                copy = Some(web.clone());
                                            }
                                        });
                                        ui.label(egui::RichText::new(trf!("For {for_name} only. Send it to them privately: it opens in EVE Spai or in a browser.", for_name = for_name)).weak());
                                    }
                                }
                            }
                            if g.role != Role::Owner && ui.button(trf!("{icon}  Leave", icon = icon::SIGN_OUT)).clicked() {
                                cmd = Some(Cmd::Leave { group: g.id.clone() });
                            }

                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr!("As"));
                    let name = chars.iter().find(|c| Some(c.0) == self.wh_share.char_id).map_or(tr!("no character"), |c| c.1.as_str());
                    egui::ComboBox::from_id_salt("wh_share_char").selected_text(name).show_ui(ui, |ui| {
                        for (id, n) in &chars {
                            if ui.menu_label(Some(*id) == self.wh_share.char_id, n.as_str()).clicked() {
                                self.wh_share.char_id = Some(*id);
                            }
                        }
                    });
                });
                let char_id = self.wh_share.char_id;
                let new_prefs = self.wh_share.new_prefs.get_or_insert_with(SharePrefs::default);
                ui.horizontal(|ui| {
                    ui.label(tr!("A new group gets"));
                    ui.checkbox(&mut new_prefs.send_holes, tr!("my wormholes"));
                    ui.checkbox(&mut new_prefs.send_sigs, tr!("my probe scans"));
                })
                .response
                .on_hover_text(tr!("What of yours goes to a group you create or join here. Change it for each group above later."));
                let new_prefs = *new_prefs;
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.wh_share.new_name).hint_text(tr!("Group name")).desired_width(200.0));
                    let ok = char_id.is_some() && !self.wh_share.new_name.trim().is_empty();
                    if ui.add_enabled(ok, egui::Button::new(trf!("{icon}  Create group", icon = icon::PLUS))).clicked() {
                        let char_name = chars.iter().find(|c| Some(c.0) == char_id).map(|c| c.1.clone()).unwrap_or_default();
                        cmd = Some(Cmd::Create {
                            name: self.wh_share.new_name.trim().to_owned(),
                            char_id: char_id.unwrap_or_default(),
                            char_name,
                            prefs: new_prefs,
                        });
                        self.wh_share.new_name.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.wh_share.join_link).hint_text("https://eve-spai.com/wh/join/…").desired_width(200.0));
                    let ok = char_id.is_some() && engine::parse_link(&self.wh_share.join_link).is_some();
                    if ui.add_enabled(ok, egui::Button::new(trf!("{icon}  Join", icon = icon::SIGN_IN))).clicked() {
                        cmd = Some(Cmd::Join { link: self.wh_share.join_link.trim().to_owned(), char_id: char_id.unwrap_or_default(), prefs: new_prefs });
                        self.wh_share.join_link.clear();
                    }
                });
            });
        // The members and delete dialogs open beside it, wherever its width put its edge.
        self.wh_share.right_edge = shown.map(|r| r.response.rect.right());
        if let Some((group, was, now)) = prefs {
            self.set_share_prefs(&group, was, now);
        }
        if let Some(c) = cmd {
            self.share_send(c);
        }
        if let Some(text) = copy {
            ctx.copy_text(text);
        }
        self.wh_share.open = open;
        self.wh_share_members_window(ctx);
        self.wh_share_delete_window(ctx);
    }

    /// Asks the owner to type the group's name before it goes, for every member at once.
    fn wh_share_delete_window(&mut self, ctx: &egui::Context) {
        let Some((id, name, mut typed)) = self.wh_share.delete.take() else { return };
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        egui::Window::new(trf!("Delete {name}?", name = name))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            // Beside the sharing window, where the members dialog goes, not over its table.
            .pivot(egui::Align2::LEFT_TOP)
            .default_pos(egui::pos2(self.wh_share.right_edge.map_or(ctx.content_rect().center().x, |r| r + 8.0), ctx.content_rect().top() + 150.0))
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(tr!("This ends the group for every member: its members, keys and log go, and nothing more syncs. It cannot be undone."));
                ui.label(tr!("Holes already on members' maps stay until they expire."));
                ui.add_space(6.0);
                ui.label(trf!("Type the group's name, {name}, to delete it:", name = name));
                let edit = ui.add(egui::TextEdit::singleline(&mut typed).hint_text(name.as_str()).desired_width(260.0));
                let matches = typed == name;
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let delete = egui::Button::new(egui::RichText::new(tr!("Delete the group")).color(if matches { crate::theme::standing::HOSTILE } else { ui.visuals().weak_text_color() }));
                    go = ui.add_enabled(matches, delete).clicked() || (matches && edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    cancel = ui.button(tr!("Cancel")).clicked();
                });
            });
        if go {
            self.share_send(Cmd::Delete { group: id });
        } else if open && !cancel {
            self.wh_share.delete = Some((id, name, typed));
        }
    }

    /// One group's members in a window of their own, filtered and scrolling, so a big group does
    /// not stretch the sharing window.
    fn wh_share_members_window(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        let Some(gid) = self.wh_share.members_of.clone() else { return };
        let Some(g) = self.wh_share.groups.iter().find(|g| g.id == gid).cloned() else {
            self.wh_share.members_of = None;
            return;
        };
        let mut members = self.store.as_ref().map(|s| s.share_members(&g.id)).unwrap_or_default();
        members.sort_by(|a, b| b.role.cmp(&a.role).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        let mut open = true;
        let mut cmd: Option<Cmd> = None;
        egui::Window::new(trf!("{icon}  Members of {v}", icon = icon::USERS, v = g.name))
            .id(egui::Id::new("wh_share_members"))
            .open(&mut open)
            .resizable(true)
            .default_size([480.0, 420.0])
            .pivot(egui::Align2::LEFT_TOP)
            .default_pos(egui::pos2(self.wh_share.right_edge.map_or(ctx.content_rect().center().x, |r| r + 8.0), ctx.content_rect().top() + 150.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.wh_share.members_filter).hint_text(tr!("Filter by name")).desired_width(200.0));
                    ui.label(egui::RichText::new(trf!("{members} members", members = members.len())).weak());
                });
                let q = self.wh_share.members_filter.trim().to_lowercase();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new(("wh_share_members", &g.id)).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                        for m in members.iter().filter(|m| q.is_empty() || m.name.to_lowercase().contains(&q)) {
                            let me = m.char_id == g.char_id;
                            ui.label(&m.name);
                            if g.role == Role::Owner && !me && m.role != Role::Owner {
                                let mut role = m.role;
                                egui::ComboBox::from_id_salt(("wh_share_role", m.char_id)).selected_text(role.label().tr()).show_ui(ui, |ui| {
                                    for r in [Role::Viewer, Role::Member, Role::Admin] {
                                        ui.menu_value(&mut role, r, r.label().tr());
                                    }
                                });
                                if role != m.role {
                                    cmd = Some(Cmd::SetRole { group: g.id.clone(), char_id: m.char_id, role });
                                }
                            } else {
                                ui.label(m.role.label().tr());
                            }
                            // Each device with its key, compared over voice to be sure it is theirs.
                            let manage = g.role == Role::Owner || (g.role == Role::Admin && m.role < Role::Admin);
                            ui.vertical(|ui| {
                                for d in &m.devices {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(d.keys.fingerprint()).monospace().weak())
                                            .on_hover_text(tr!("Key fingerprint: compare it with the member over voice to be sure it is theirs"));
                                        if !d.label.is_empty() {
                                            ui.label(egui::RichText::new(&d.label).weak());
                                        }
                                        let last_of_owner = m.role == Role::Owner && m.devices.len() == 1;
                                        if (manage || me) && m.devices.len() > 1 && !last_of_owner
                                            && spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Remove this device; the others keep their access")).clicked()
                                        {
                                            cmd = Some(Cmd::RemoveDevice { group: g.id.clone(), char_id: m.char_id, device_id: d.id.clone() });
                                        }
                                    });
                                }
                            });
                            let may_remove = !me && m.role != Role::Owner && manage;
                            if may_remove && ui.small_button(tr!("Remove")).on_hover_text(tr!("Takes them out, every device, and moves everyone else to a new key")).clicked() {
                                cmd = Some(Cmd::Remove { group: g.id.clone(), char_id: m.char_id });
                            }
                            ui.end_row();
                        }
                    });
                });
            });
        if !open {
            self.wh_share.members_of = None;
        }
        if let Some(c) = cmd {
            self.share_send(c);
        }
    }
}
