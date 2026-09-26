//! The wormhole sharing window: groups, members, invites and join requests. The work happens on
//! the share thread (`crate::share::engine`); this only shows its state and sends it commands.

use super::*;
use crate::share::engine::{self, Cmd};
use crate::share::ops::Role;
use crate::store::ShareGroup;

#[derive(Default)]
pub(crate) struct ShareUi {
    handle: Option<engine::Handle>,
    started: bool,
    pub(crate) open: bool,
    new_name: String,
    join_link: String,
    invite_for: String,
    char_id: Option<i64>,
    generation: u64,
    groups: Vec<ShareGroup>,
    refreshed: Option<std::time::Instant>,
    kicked: Option<std::time::Instant>,
    ctx: Option<egui::Context>,
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
            self.wh_share.groups = self.store.as_ref().map(|s| s.share_groups()).unwrap_or_default();
        }
        if self.wh_share.handle.is_none() && !self.wh_share.started && (!self.wh_share.groups.is_empty() || self.wh_share.open) {
            self.wh_share.started = true;
            self.wh_share.handle = Some(engine::spawn(ctx.clone(), self.settings.wh_share_target.clone()));
        }
        let Some(h) = &self.wh_share.handle else { return };
        let generation = h.status.lock().unwrap().generation;
        if generation != self.wh_share.generation {
            self.wh_share.generation = generation;
            self.wh_reloaded = None;
            self.wh_graph.forget_sigs();
        }
        // The thread may pick a first group as the target itself.
        let target = h.target.lock().unwrap().clone();
        if target != self.settings.wh_share_target {
            self.settings.wh_share_target = target;
            self.needs_save = true;
        }
        if self.wh_share.kicked.is_none_or(|t| now - t > std::time::Duration::from_secs(3)) {
            self.wh_share.kicked = Some(now);
            if self.store.as_ref().is_some_and(|s| s.share_outbox_len() > 0) && !h.status.lock().unwrap().busy {
                let _ = h.tx.send(Cmd::SyncNow);
            }
        }
    }

    fn share_send(&mut self, cmd: Cmd) {
        if self.wh_share.handle.is_none() {
            let Some(ctx) = self.wh_share.ctx.clone() else { return };
            self.wh_share.started = true;
            self.wh_share.handle = Some(engine::spawn(ctx, self.settings.wh_share_target.clone()));
        }
        if let Some(h) = &self.wh_share.handle {
            let _ = h.tx.send(cmd);
        }
        self.wh_share.refreshed = None;
    }

    fn set_share_target(&mut self, target: Option<String>) {
        self.settings.wh_share_target = target.clone();
        self.needs_save = true;
        if let Some(h) = &self.wh_share.handle {
            *h.target.lock().unwrap() = target;
        }
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
        let mut target: Option<Option<String>> = None;
        let mut copy: Option<String> = None;
        egui::Window::new(format!("{}  Wormhole sharing", icon::USERS_THREE))
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Groups are end-to-end encrypted: the server stores only what it cannot read, and only members hold the keys.",
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
                            ui.label("Syncing");
                        } else {
                            ui.label(egui::RichText::new("Up to date").weak());
                        }
                    },
                );
                if let Some(e) = &status.error {
                    ui.colored_label(crate::theme::standing::WARNING, e);
                }
                if let Some(fp) = &status.fingerprint {
                    ui.horizontal(|ui| {
                        ui.label("Your key fingerprint");
                        ui.label(egui::RichText::new(fp).monospace().strong());
                    })
                    .response
                    .on_hover_text("When you ask to join, read this to whoever approves you so they can check it is really you");
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Send my changes to");
                    let current = groups.iter().find(|g| Some(&g.id) == self.settings.wh_share_target.as_ref());
                    egui::ComboBox::from_id_salt("wh_share_target")
                        .selected_text(current.map_or("Nobody (local only)", |g| g.name.as_str()))
                        .show_ui(ui, |ui| {
                            if ui.menu_label(current.is_none(), "Nobody (local only)").clicked() {
                                target = Some(None);
                            }
                            for g in &groups {
                                if ui.menu_label(current.is_some_and(|c| c.id == g.id), g.name.as_str()).clicked() {
                                    target = Some(Some(g.id.clone()));
                                }
                            }
                        });
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    if groups.is_empty() {
                        ui.label(egui::RichText::new("Not in any group yet.").weak());
                    }
                    for g in &groups {
                        let has_key = self.store.as_ref().is_some_and(|s| s.share_key(&g.id, g.epoch).is_some());
                        let synced = status.synced_at.get(&g.id).map(|t| format!(", synced {} ago", human_ago(chrono::Utc::now().timestamp() - t)));
                        let title = if has_key {
                            format!("{}  ({}{})", g.name, g.role.label(), synced.unwrap_or_default())
                        } else {
                            format!("{}  (waiting for an admin to approve)", g.name)
                        };
                        egui::CollapsingHeader::new(title).id_salt(("wh_share_group", &g.id)).default_open(true).show(ui, |ui| {
                            if !has_key {
                                if let Some(fp) = &status.fingerprint {
                                    ui.label(format!("Read your fingerprint {fp} to the admin approving you."));
                                }
                            }
                            let members = self.store.as_ref().map(|s| s.share_members(&g.id)).unwrap_or_default();
                            egui::Grid::new(("wh_share_members", &g.id)).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                                for m in &members {
                                    ui.label(&m.name);
                                    ui.label(m.role.label());
                                    ui.label(egui::RichText::new(m.keys.fingerprint()).monospace().weak())
                                        .on_hover_text("Key fingerprint: compare it with the member over voice to be sure it is theirs");
                                    ui.horizontal(|ui| {
                                        let me = m.char_id == g.char_id;
                                        if g.role == Role::Owner && !me && m.role != Role::Owner {
                                            let (to, label) = if m.role == Role::Admin { (Role::Member, "Make member") } else { (Role::Admin, "Make admin") };
                                            if ui.small_button(label).clicked() {
                                                cmd = Some(Cmd::SetRole { group: g.id.clone(), char_id: m.char_id, role: to });
                                            }
                                        }
                                        let may_remove = !me
                                            && m.role != Role::Owner
                                            && (g.role == Role::Owner || (g.role == Role::Admin && m.role == Role::Member));
                                        if may_remove && ui.small_button("Remove").on_hover_text("Takes them out and moves everyone else to a new key").clicked() {
                                            cmd = Some(Cmd::Remove { group: g.id.clone(), char_id: m.char_id });
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                            if g.role.can_manage() && has_key {
                                let reqs = status.requests.get(&g.id).cloned().unwrap_or_default();
                                if !reqs.is_empty() {
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new("Asking to join").strong());
                                    ui.label(
                                        egui::RichText::new("To be certain, have them read their key fingerprint to you and compare it before approving.")
                                            .weak(),
                                    );
                                }
                                for r in reqs {
                                    ui.horizontal(|ui| {
                                        ui.label(&r.row.name);
                                        if r.verified {
                                            ui.label(egui::RichText::new(format!("{} came through our invite", icon::CHECK)).color(crate::theme::chip::CLEAR));
                                        } else if let Some(meant) = &r.meant_for {
                                            ui.colored_label(crate::theme::standing::WARNING, format!("{} the invite was for {meant}", icon::WARNING))
                                                .on_hover_text("Someone else used the link. Do not approve; make a new invite for the right character.");
                                        } else {
                                            ui.colored_label(crate::theme::standing::WARNING, format!("{} does not match an invite of ours", icon::WARNING))
                                                .on_hover_text("The keys it carries are not the ones our invite link vouches for. Do not trust it.");
                                        }
                                        if let Some(k) = r.keys {
                                            ui.label(egui::RichText::new(k.fingerprint()).monospace().weak());
                                        }
                                        if ui.add_enabled(r.verified, egui::Button::new("Approve")).clicked() {
                                            cmd = Some(Cmd::Approve { group: g.id.clone(), char_id: r.row.char_id });
                                        }
                                        if ui.button("Reject").clicked() {
                                            cmd = Some(Cmd::Reject { group: g.id.clone(), char_id: r.row.char_id });
                                        }
                                    });
                                }
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut self.wh_share.invite_for).hint_text("Character it is for").desired_width(180.0));
                                    let ok = !self.wh_share.invite_for.trim().is_empty();
                                    if ui
                                        .add_enabled(ok, egui::Button::new(format!("{}  New invite link", icon::LINK)))
                                        .on_hover_text("Only that character can use it, once, within two days; you still approve them")
                                        .clicked()
                                    {
                                        cmd = Some(Cmd::Invite { group: g.id.clone(), for_name: self.wh_share.invite_for.trim().to_owned() });
                                    }
                                });
                                if let Some((gid, link, for_name)) = &status.invite {
                                    if *gid == g.id {
                                        ui.horizontal(|ui| {
                                            ui.add(egui::TextEdit::singleline(&mut link.clone()).desired_width(360.0));
                                            if ui.button(icon::COPY).on_hover_text("Copy the link").clicked() {
                                                copy = Some(link.clone());
                                            }
                                        });
                                        ui.label(egui::RichText::new(format!("For {for_name} only. Send it to them privately.")).weak());
                                    }
                                }
                            }
                            if g.role != Role::Owner && ui.button(format!("{}  Leave", icon::SIGN_OUT)).clicked() {
                                cmd = Some(Cmd::Leave { group: g.id.clone() });
                            }
                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("As");
                    let name = chars.iter().find(|c| Some(c.0) == self.wh_share.char_id).map_or("no character", |c| c.1.as_str());
                    egui::ComboBox::from_id_salt("wh_share_char").selected_text(name).show_ui(ui, |ui| {
                        for (id, n) in &chars {
                            if ui.menu_label(Some(*id) == self.wh_share.char_id, n.as_str()).clicked() {
                                self.wh_share.char_id = Some(*id);
                            }
                        }
                    });
                });
                let char_id = self.wh_share.char_id;
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.wh_share.new_name).hint_text("Group name").desired_width(200.0));
                    let ok = char_id.is_some() && !self.wh_share.new_name.trim().is_empty();
                    if ui.add_enabled(ok, egui::Button::new(format!("{}  Create group", icon::PLUS))).clicked() {
                        let char_name = chars.iter().find(|c| Some(c.0) == char_id).map(|c| c.1.clone()).unwrap_or_default();
                        cmd = Some(Cmd::Create { name: self.wh_share.new_name.trim().to_owned(), char_id: char_id.unwrap_or_default(), char_name });
                        self.wh_share.new_name.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.wh_share.join_link).hint_text("eve-spai://join/…").desired_width(200.0));
                    let ok = char_id.is_some() && engine::parse_link(&self.wh_share.join_link).is_some();
                    if ui.add_enabled(ok, egui::Button::new(format!("{}  Join", icon::SIGN_IN))).clicked() {
                        cmd = Some(Cmd::Join { link: self.wh_share.join_link.trim().to_owned(), char_id: char_id.unwrap_or_default() });
                        self.wh_share.join_link.clear();
                    }
                });
            });
        if let Some(t) = target {
            self.set_share_target(t);
        }
        if let Some(c) = cmd {
            self.share_send(c);
        }
        if let Some(text) = copy {
            ctx.copy_text(text);
        }
        self.wh_share.open = open;
    }
}
