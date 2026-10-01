//! The Group tab: who is in the browser's groups, and for admins the invites, join requests,
//! roles and removals the desktop's sharing window offers. The work is the shared engine's.

use spai_share::engine::{Cmd, Status};
use spai_share::ops::{Member, Role};
use spai_share::store::{ShareGroup, ShareStore};

#[derive(Default)]
pub struct GroupTab {
    invite_for: String,
    /// The next invite lets its character in as a viewer rather than a member.
    invite_viewer: bool,
    /// A removal waiting for its second click.
    confirm_remove: Option<(String, i64)>,
    /// A group about to be deleted, and its name as typed so far.
    delete: Option<(String, String)>,
}

/// An invite link the engine made, as a link that opens this web app.
pub fn web_link(origin: &str, link: &str) -> String {
    link.split_once("join/").map_or_else(|| link.to_owned(), |(_, rest)| format!("{origin}/wh/join/{rest}"))
}

impl GroupTab {
    /// Draws the tab; returns what to ask the engine to do.
    pub fn show(&mut self, ui: &mut egui::Ui, store: &dyn ShareStore, status: &Status, me: i64, origin: &str) -> Vec<Cmd> {
        let mut out = Vec::new();
        let groups: Vec<ShareGroup> = store.share_groups().into_iter().filter(|g| g.char_id == me).collect();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(760.0);
            if let Some(e) = &status.error {
                ui.label(egui::RichText::new(e).color(ui.visuals().error_fg_color));
            }
            for name in &status.deleted {
                ui.label(egui::RichText::new(format!("{name} was deleted by its owner.")).color(spai_ui::theme::standing::WARNING));
            }
            if status.busy {
                ui.label(egui::RichText::new("Syncing\u{2026}").weak());
            }
            for g in &groups {
                ui.add_space(8.0);
                ui.heading(format!("{}  ({})", g.name, g.role.label()));
                if store.share_key(&g.id, g.epoch).is_none() {
                    ui.label("Waiting to be let in: the invite does it as soon as the EVE Spai that made it next syncs.");
                    continue;
                }
                let members = store.share_members(&g.id);
                // Members invite too, and let their own invitees in as viewers only.
                if g.role.can_write() {
                    self.invites(ui, g, status, origin, &mut out);
                    requests(ui, g, status, &members, &mut out);
                }
                self.members(ui, g, &members, me, &mut out);
                if g.role != Role::Owner && ui.button("Leave the group").on_hover_text("This browser leaves; your other devices stay").clicked() {
                    out.push(Cmd::Leave { group: g.id.clone() });
                }
                if g.role == Role::Owner {
                    self.delete_ui(ui, g, &mut out);
                }
            }
            if groups.is_empty() {
                ui.label(egui::RichText::new("Not in a group yet.").weak());
            }
        });
        out
    }

    fn delete_ui(&mut self, ui: &mut egui::Ui, g: &ShareGroup, out: &mut Vec<Cmd>) {
        ui.add_space(8.0);
        match &mut self.delete {
            Some((id, typed)) if *id == g.id => {
                let frame = egui::Frame::group(ui.style()).stroke(egui::Stroke::new(1.0, spai_ui::theme::standing::HOSTILE));
                let mut done = false;
                frame.show(ui, |ui| {
                    ui.label("This ends the group for every member: its members, keys and log go, and nothing more syncs. It cannot be undone.");
                    ui.label(format!("Type the group's name, {}, to delete it:", g.name));
                    ui.add(egui::TextEdit::singleline(typed).hint_text(g.name.as_str()).desired_width(260.0));
                    ui.horizontal(|ui| {
                        if ui.add_enabled(*typed == g.name, egui::Button::new("Delete the group")).clicked() {
                            out.push(Cmd::Delete { group: g.id.clone() });
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            done = true;
                        }
                    });
                });
                if done {
                    self.delete = None;
                }
            }
            _ => {
                if ui.button("Delete group\u{2026}").on_hover_text("End the group for every member").clicked() {
                    self.delete = Some((g.id.clone(), String::new()));
                }
            }
        }
    }

    fn invites(&mut self, ui: &mut egui::Ui, g: &ShareGroup, status: &Status, origin: &str, out: &mut Vec<Cmd>) {
        ui.add_space(4.0);
        ui.strong("Invite");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.invite_for).hint_text("Character it is for").desired_width(200.0));
            let ok = !self.invite_for.trim().is_empty();
            // Members invite viewers only.
            let admin = g.role.can_manage();
            if admin {
                let v = &mut self.invite_viewer;
                egui::ComboBox::from_id_salt(("web_invite_role", &g.id)).width(90.0).selected_text(if *v { "as Viewer" } else { "as Member" }).show_ui(ui, |ui| {
                    ui.selectable_value(v, false, "as Member").on_hover_text("Sees and shares");
                    ui.selectable_value(v, true, "as Viewer").on_hover_text("Sees the group's wormholes, shares nothing");
                });
            } else {
                ui.label(egui::RichText::new("as Viewer").weak());
            }
            let tip = "For that character only, valid two days. Using it lets them in, while this page is open.";
            if ui.add_enabled(ok, egui::Button::new("New invite link")).on_hover_text(tip).clicked() {
                let role = if admin && !self.invite_viewer { Role::Member } else { Role::Viewer };
                out.push(Cmd::Invite { group: g.id.clone(), for_name: self.invite_for.trim().to_owned(), role });
            }
        });
        if let Some((gid, link, for_name)) = &status.invite {
            if *gid == g.id {
                let web = web_link(origin, link);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut web.clone()).desired_width(560.0));
                    if ui.button(egui_phosphor::regular::COPY).on_hover_text("Copy the link").clicked() {
                        ui.ctx().copy_text(web.clone());
                    }
                });
                ui.label(egui::RichText::new(format!("For {for_name} only. Send it to them privately: it opens in EVE Spai or in a browser.")).weak());
            }
        }
    }

    fn members(&mut self, ui: &mut egui::Ui, g: &ShareGroup, members: &[Member], me: i64, out: &mut Vec<Cmd>) {
        ui.add_space(4.0);
        ui.strong(format!("Members ({})", members.len()));
        let mut sorted: Vec<&Member> = members.iter().collect();
        sorted.sort_by(|a, b| b.role.cmp(&a.role).then(a.name.cmp(&b.name)));
        egui::Grid::new(("web_members", &g.id)).num_columns(4).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
            for m in sorted {
                ui.label(&m.name);
                let mut role = m.role;
                if g.role == Role::Owner && m.role != Role::Owner {
                    egui::ComboBox::from_id_salt(("web_role", &g.id, m.char_id)).width(90.0).selected_text(role.label()).show_ui(ui, |ui| {
                        for r in [Role::Viewer, Role::Member, Role::Admin] {
                            ui.selectable_value(&mut role, r, r.label());
                        }
                    });
                    if role != m.role {
                        out.push(Cmd::SetRole { group: g.id.clone(), char_id: m.char_id, role });
                    }
                } else {
                    ui.label(m.role.label());
                }
                let n = m.devices.len();
                ui.label(egui::RichText::new(format!("{n} device{}", if n == 1 { "" } else { "s" })).weak())
                    .on_hover_text(m.devices.iter().map(|d| format!("{}  {}", d.keys.fingerprint(), d.label)).collect::<Vec<_>>().join("\n"));
                // Admins take out members and viewers; only the owner takes out an admin.
                let may = m.char_id != me && m.role != Role::Owner && (g.role == Role::Owner || (g.role == Role::Admin && m.role < Role::Admin));
                if may {
                    let armed = self.confirm_remove == Some((g.id.clone(), m.char_id));
                    let label = if armed { "Sure? Remove" } else { "Remove" };
                    if ui.button(label).on_hover_text("They keep what they read, and nothing new reaches them").clicked() {
                        if armed {
                            out.push(Cmd::Remove { group: g.id.clone(), char_id: m.char_id });
                            self.confirm_remove = None;
                        } else {
                            self.confirm_remove = Some((g.id.clone(), m.char_id));
                        }
                    }
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    }
}

fn requests(ui: &mut egui::Ui, g: &ShareGroup, status: &Status, members: &[Member], out: &mut Vec<Cmd>) {
    let reqs = status.requests.get(&g.id).cloned().unwrap_or_default();
    if reqs.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.strong("Asking to join");
    ui.label(egui::RichText::new("To be certain, have them read their fingerprint to you and compare it before approving.").weak());
    egui::Grid::new(("web_requests", &g.id)).num_columns(4).spacing([12.0, 4.0]).show(ui, |ui| {
        for r in reqs {
            let existing = members.iter().find(|m| m.char_id == r.row.char_id).map(|m| m.role);
            ui.label(match existing {
                Some(role) => format!("{} (another device, {})", r.row.name, role.label()),
                None => r.row.name.clone(),
            });
            if r.verified {
                ui.label(egui::RichText::new("came through our invite").color(spai_ui::theme::chip::CLEAR));
            } else if let Some(meant) = &r.meant_for {
                ui.label(egui::RichText::new(format!("the invite was for {meant}")).color(spai_ui::theme::standing::WARNING))
                    .on_hover_text("Someone else used the link. Do not approve; make a new invite for the right character.");
            } else {
                ui.label(egui::RichText::new("does not match an invite of ours").color(spai_ui::theme::standing::WARNING));
            }
            ui.label(egui::RichText::new(r.keys.map(|k| k.fingerprint()).unwrap_or_default()).monospace());
            ui.horizontal(|ui| {
                let approve = |role: Role| Cmd::Approve { group: g.id.clone(), char_id: r.row.char_id, device_id: r.row.device_id.clone(), role };
                if !g.role.can_manage() {
                    if existing.is_none() && ui.add_enabled(r.verified, egui::Button::new("Approve as viewer")).on_hover_text("Sees the group's wormholes, shares nothing").clicked() {
                        out.push(approve(Role::Viewer));
                    }
                } else if existing.is_some() {
                    if ui.add_enabled(r.verified, egui::Button::new("Approve device")).clicked() {
                        out.push(approve(Role::Member));
                    }
                } else {
                    if ui.add_enabled(r.verified, egui::Button::new("Approve")).clicked() {
                        out.push(approve(Role::Member));
                    }
                    if ui.add_enabled(r.verified, egui::Button::new("As viewer")).on_hover_text("Sees the group's wormholes, shares nothing").clicked() {
                        out.push(approve(Role::Viewer));
                    }
                }
                if ui.button("Reject").clicked() {
                    out.push(Cmd::Reject { group: g.id.clone(), char_id: r.row.char_id, device_id: r.row.device_id.clone() });
                }
            });
            ui.end_row();
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_invite_opens_in_the_web_app() {
        assert_eq!(super::web_link("https://eve-spai.com", "eve-spai://join/abc#s3cr3t"), "https://eve-spai.com/wh/join/abc#s3cr3t");
    }
}
