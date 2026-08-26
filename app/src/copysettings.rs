use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::charsettings::{self, CopyPlan, CopyReport};
use crate::eveproc::Clients;
use crate::store::{AssocSource, Store};
use crate::theme::standing;

#[derive(Clone, Debug)]
struct Row {
    id: i64,
    name: Option<String>,
    linked: bool,
    has_file: bool,
    account: Option<(i64, AssocSource)>,
}

impl Row {
    fn display(&self) -> String {
        self.name.clone().unwrap_or_else(|| format!("Character {}", self.id))
    }
}

#[derive(Default)]
pub struct CopyState {
    pub active: bool,
    root: Option<PathBuf>,
    profiles: Vec<String>,
    src_profile: String,
    dst_profile: String,
    hinted_profile: Option<String>,
    rows: Vec<Row>,
    /// Every account id with a settings file, which is the full set that has logged in here.
    accounts: Vec<i64>,
    source: Option<i64>,
    dests: BTreeSet<i64>,
    confirm: bool,
    result: Option<Result<CopyReport, String>>,
    assign_open: Option<i64>,
    assign_input: String,
    loaded: bool,
    names_requested: bool,
}

impl CopyState {
    pub fn clear_selection(&mut self) {
        self.source = None;
        self.dests.clear();
        self.confirm = false;
        self.assign_open = None;
        self.assign_input.clear();
    }

    pub fn invalidate(&mut self) {
        self.loaded = false;
    }

    fn reload(&mut self, store: &Store, configured: &str) {
        self.loaded = true;
        self.root = charsettings::settings_root(configured);
        let Some(root) = self.root.clone() else {
            self.rows.clear();
            self.accounts.clear();
            self.profiles.clear();
            return;
        };
        self.profiles = charsettings::profiles(&root);
        // A running client names its profile on the command line; that beats guessing the first
        // one alphabetically, since the profile the user plays is the one worth copying.
        self.hinted_profile = store
            .kv_get(crate::eveproc::LAST_PROFILE_KEY)
            .filter(|p| self.profiles.iter().any(|known| known == p));
        if !self.profiles.iter().any(|p| *p == self.src_profile) {
            self.src_profile = self
                .hinted_profile
                .clone()
                .or_else(|| self.profiles.first().cloned())
                .unwrap_or_else(|| "Default".to_owned());
        }
        if !self.profiles.iter().any(|p| *p == self.dst_profile) {
            self.dst_profile = self.src_profile.clone();
        }

        let linked: BTreeMap<i64, String> =
            store.list_characters().into_iter().map(|c| (c.id, c.name)).collect();
        let cached = store.char_names();
        let accounts = store.char_accounts();
        let on_disk = charsettings::scan(&root, &self.src_profile);
        // BTreeMap keys are already ascending, which is the order the account suggestions want.
        self.accounts = on_disk.accounts.keys().copied().collect();

        let mut ids: BTreeSet<i64> = on_disk.chars.keys().copied().collect();
        ids.extend(linked.keys().copied());

        self.rows = ids
            .into_iter()
            .map(|id| Row {
                id,
                name: linked.get(&id).or_else(|| cached.get(&id)).cloned(),
                linked: linked.contains_key(&id),
                has_file: on_disk.chars.contains_key(&id),
                account: accounts.get(&id).copied(),
            })
            .collect();
        self.rows.sort_by(|a, b| a.display().to_lowercase().cmp(&b.display().to_lowercase()));

        self.source = self.source.filter(|id| self.rows.iter().any(|r| r.id == *id && r.has_file));
        self.dests.retain(|id| self.rows.iter().any(|r| r.id == *id));
    }

    /// Names for characters that own a settings file but are not authenticated here. The logs name
    /// everyone who has played on this machine, offline and instantly, so ESI is only asked about
    /// whatever is left. Both results are cached, so this runs once per new character.
    fn resolve_names(&mut self, ctx: &egui::Context, logs_configured: &str) {
        if self.names_requested {
            return;
        }
        let missing: Vec<i64> = self.rows.iter().filter(|r| r.name.is_none()).map(|r| r.id).collect();
        if missing.is_empty() {
            return;
        }
        self.names_requested = true;
        let ctx = ctx.clone();
        let logs = logs_configured.to_owned();
        std::thread::spawn(move || {
            let store = Store::open().ok();
            let local = crate::charsettings::names_from_logs(&logs, &missing);
            if let Some(store) = &store {
                for (id, name) in &local {
                    store.set_char_name(*id, name);
                }
            }
            if !local.is_empty() {
                ctx.request_repaint();
            }
            let rest: Vec<i64> =
                missing.into_iter().filter(|id| !local.contains_key(id)).collect();
            if rest.is_empty() {
                return;
            }
            let remote = crate::lookup::resolve_type_names(&rest);
            if remote.is_empty() {
                return;
            }
            if let Some(store) = &store {
                for (id, name) in &remote {
                    store.set_char_name(*id, name);
                }
            }
            ctx.request_repaint();
        });
    }

    fn account_of(&self, id: i64) -> Option<i64> {
        self.rows.iter().find(|r| r.id == id).and_then(|r| r.account).map(|(a, _)| a)
    }

    fn dest_accounts(&self) -> Vec<i64> {
        let mut out: Vec<i64> = self.dests.iter().filter_map(|id| self.account_of(*id)).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Characters that share an account with a destination but were not selected. Their overview
    /// and shortcuts get overwritten too, because the account file is shared.
    fn collateral(&self) -> Vec<String> {
        let targets = self.dest_accounts();
        if targets.is_empty() {
            return Vec::new();
        }
        self.rows
            .iter()
            .filter(|r| !self.dests.contains(&r.id) && Some(r.id) != self.source)
            .filter(|r| r.account.is_some_and(|(a, _)| targets.contains(&a)))
            .map(|r| r.display())
            .collect()
    }

    fn plan(&self) -> CopyPlan {
        CopyPlan {
            source_char: self.source.unwrap_or(0),
            source_profile: self.src_profile.clone(),
            dest_chars: self.dests.iter().copied().collect(),
            dest_profile: self.dst_profile.clone(),
            source_account: self.source.and_then(|id| self.account_of(id)),
            dest_accounts: self.dest_accounts(),
        }
    }
}

fn running_names(clients: &Clients) -> Option<Vec<String>> {
    let list = clients.as_ref()?;
    Some(
        list.iter()
            .map(|c| match c.character_id {
                Some(id) => format!("pid {} (character {id})", c.pid),
                None => format!("pid {}", c.pid),
            })
            .collect(),
    )
}

pub fn ui(
    state: &mut CopyState,
    ui: &mut egui::Ui,
    store: &Store,
    configured: &str,
    logs_configured: &str,
    clients: &Arc<Mutex<Clients>>,
) {
    if !state.loaded {
        state.reload(store, configured);
    }
    state.resolve_names(ui.ctx(), logs_configured);

    let clients = clients.lock().map(|c| c.clone()).unwrap_or(None);
    let running = running_names(&clients);
    let blocked = running.as_ref().is_some_and(|r| !r.is_empty());
    let unverified = running.is_none();

    if blocked {
        let names = running.clone().unwrap_or_default();
        ui.colored_label(
            standing::HOSTILE,
            format!(
                "{} EVE is running ({}). Close every client before copying, or the game will \
                 overwrite the files again on exit.",
                egui_phosphor::regular::WARNING,
                names.join(", ")
            ),
        );
        ui.add_space(4.0);
    } else if unverified {
        ui.colored_label(
            standing::WARNING,
            format!(
                "{} Could not check whether EVE is running. Make sure every client is closed.",
                egui_phosphor::regular::WARNING
            ),
        );
        ui.add_space(4.0);
    }

    let Some(root) = state.root.clone() else {
        ui.colored_label(
            standing::WARNING,
            "No EVE settings directory found. Set it in Settings > EVE settings directory.",
        );
        return;
    };
    ui.label(egui::RichText::new(root.display().to_string()).weak());
    ui.add_space(6.0);

    if state.profiles.len() > 1 {
        ui.horizontal(|ui| {
            ui.label("Copy from profile");
            let hint = state.hinted_profile.clone();
            let mut changed = false;
            egui::ComboBox::from_id_salt("copy_src_profile")
                .selected_text(state.src_profile.clone())
                .show_ui(ui, |ui| {
                    for p in state.profiles.clone() {
                        changed |= ui
                            .selectable_value(&mut state.src_profile, p.clone(), p)
                            .changed();
                    }
                });
            ui.label("to");
            egui::ComboBox::from_id_salt("copy_dst_profile")
                .selected_text(state.dst_profile.clone())
                .show_ui(ui, |ui| {
                    for p in state.profiles.clone() {
                        ui.selectable_value(&mut state.dst_profile, p.clone(), p);
                    }
                });
            if let Some(hint) = hint {
                ui.label(egui::RichText::new(format!("· {hint} in use")).weak()).on_hover_text(
                    "The profile your EVE client was last launched with, read from its command \
                     line",
                );
            }
            if changed {
                state.invalidate();
            }
        });
        ui.add_space(6.0);
    }

    // Every account discovered on disk, plus any assigned by hand that has no file here, ascending.
    let known_accounts: Vec<i64> = {
        let mut a: Vec<i64> = state.accounts.clone();
        a.extend(state.rows.iter().filter_map(|r| r.account.map(|(acct, _)| acct)));
        a.sort_unstable();
        a.dedup();
        a
    };
    // EVE allows three characters per account, so a count of three means the account is full.
    let assigned_count = |acct: i64| {
        state.rows.iter().filter(|r| r.account.is_some_and(|(a, _)| a == acct)).count()
    };

    let mut assign: Option<(i64, Option<i64>)> = None;
    let mut toggle_assign: Option<i64> = None;
    let mut set_source: Option<i64> = None;
    let mut flip_dest: Option<(i64, bool)> = None;

    let source = state.source;
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        for row in &state.rows {
            let is_source = source == Some(row.id);
            ui.horizontal(|ui| {
                let can_source = row.has_file;
                ui.add_enabled_ui(can_source, |ui| {
                    if ui
                        .radio(is_source, "")
                        .on_hover_text("Copy settings from this character")
                        .clicked()
                    {
                        set_source = Some(row.id);
                    }
                });
                let mut checked = state.dests.contains(&row.id);
                ui.add_enabled_ui(!is_source, |ui| {
                    if ui
                        .checkbox(&mut checked, "")
                        .on_hover_text("Overwrite this character's settings")
                        .changed()
                    {
                        flip_dest = Some((row.id, checked));
                    }
                });

                ui.label(egui::RichText::new(row.display()).strong());
                if !row.linked {
                    ui.label(egui::RichText::new("not linked").weak()).on_hover_text(
                        "Found by its settings file. Not authenticated in EVE Spai, which is fine \
                         for copying.",
                    );
                }
                if !row.has_file {
                    ui.label(
                        egui::RichText::new("no settings file").color(standing::WARNING),
                    )
                    .on_hover_text(
                        "This character has never been logged in on this machine, so it cannot be \
                         a source. As a destination its file is created.",
                    );
                }
                match row.account {
                    Some((acct, src)) => {
                        ui.label(egui::RichText::new(format!("account {acct}")).weak())
                            .on_hover_text(src.label());
                    }
                    None => {
                        ui.label(egui::RichText::new("account unknown").color(standing::WARNING))
                            .on_hover_text(
                                "Account settings (overview, shortcuts) cannot be copied for this \
                                 character until its account is known. It is detected \
                                 automatically while the character is logged in, or set it here.",
                            );
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Set account").clicked() {
                        toggle_assign = Some(row.id);
                    }
                });
            });

            if state.assign_open == Some(row.id) {
                ui.indent(row.id, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} accounts found on this machine, oldest id first:",
                            known_accounts.len()
                        ))
                        .weak(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for acct in &known_accounts {
                            let taken = assigned_count(*acct);
                            let mine = row.account.is_some_and(|(a, _)| a == *acct);
                            let label = if taken > 0 {
                                format!("{acct}  ({taken}/3)")
                            } else {
                                acct.to_string()
                            };
                            let full = taken >= 3 && !mine;
                            let btn = ui.add(egui::Button::new(label).selected(mine));
                            let btn = if full {
                                btn.on_hover_text(
                                    "Already has three characters, which is an EVE account's \
                                     limit. Pick it only if one of those is wrong.",
                                )
                            } else {
                                btn
                            };
                            if btn.clicked() {
                                assign = Some((row.id, Some(*acct)));
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut state.assign_input)
                                .hint_text("other account id")
                                .desired_width(140.0),
                        );
                        if ui.button("Set").clicked() {
                            if let Ok(id) = state.assign_input.trim().parse::<i64>() {
                                assign = Some((row.id, Some(id)));
                            }
                        }
                        if row.account.is_some() && ui.button("Clear").clicked() {
                            assign = Some((row.id, None));
                        }
                    });
                });
            }
        }
    });

    if let Some(id) = toggle_assign {
        state.assign_open = if state.assign_open == Some(id) { None } else { Some(id) };
        state.assign_input.clear();
    }
    if let Some((id, acct)) = assign {
        match acct {
            Some(acct) => store.set_char_account(id, acct, AssocSource::Manual),
            None => store.clear_char_account(id),
        }
        state.assign_open = None;
        state.assign_input.clear();
        state.invalidate();
    }
    if let Some(id) = set_source {
        state.source = Some(id);
        state.dests.remove(&id);
    }
    if let Some((id, on)) = flip_dest {
        if on {
            state.dests.insert(id);
        } else {
            state.dests.remove(&id);
        }
    }

    ui.add_space(8.0);

    let plan = state.plan();
    let source_name =
        state.source.and_then(|id| state.rows.iter().find(|r| r.id == id)).map(|r| r.display());

    if let Some(name) = &source_name {
        if !state.dests.is_empty() {
            let dest_names: Vec<String> = state
                .rows
                .iter()
                .filter(|r| state.dests.contains(&r.id))
                .map(|r| r.display())
                .collect();
            ui.label(format!("Copying {} to {}.", name, dest_names.join(", ")));

            if plan.source_account.is_none() {
                ui.colored_label(
                    standing::WARNING,
                    "The source character's account is unknown, so only per-character settings \
                     (windows, chat tabs) are copied. Overview and shortcuts are not.",
                );
            } else {
                let no_account: Vec<String> = state
                    .rows
                    .iter()
                    .filter(|r| state.dests.contains(&r.id) && r.account.is_none())
                    .map(|r| r.display())
                    .collect();
                if !no_account.is_empty() {
                    ui.colored_label(
                        standing::WARNING,
                        format!(
                            "No account known for {}, so they get per-character settings only.",
                            no_account.join(", ")
                        ),
                    );
                }
                let collateral = state.collateral();
                if !collateral.is_empty() {
                    ui.colored_label(
                        standing::WARNING,
                        format!(
                            "Account settings are shared. Overview and shortcuts will also change \
                             for {}.",
                            collateral.join(", ")
                        ),
                    );
                }
            }
        }
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let can_copy = !blocked && state.source.is_some() && !state.dests.is_empty();
        let btn = ui.add_enabled(can_copy, egui::Button::new("Copy settings"));
        let btn = if blocked {
            btn.on_disabled_hover_text("Close every EVE client first.")
        } else if state.source.is_none() {
            btn.on_disabled_hover_text("Pick a source character.")
        } else if state.dests.is_empty() {
            btn.on_disabled_hover_text("Pick at least one destination character.")
        } else {
            btn.on_hover_text("Overwrite the destination settings, keeping a backup of each")
        };
        if btn.clicked() {
            state.confirm = true;
        }
        if ui.button("Cancel").clicked() {
            state.clear_selection();
        }
    });

    if state.confirm {
        let dest_names: Vec<String> = state
            .rows
            .iter()
            .filter(|r| state.dests.contains(&r.id))
            .map(|r| r.display())
            .collect();
        let mut go = false;
        let resp = egui::Modal::new(egui::Id::new("copy_settings_confirm")).show(ui.ctx(), |ui| {
            ui.set_min_width(360.0);
            ui.heading("Copy character settings");
            ui.add_space(6.0);
            ui.label(format!(
                "From {} to {}.",
                source_name.clone().unwrap_or_default(),
                dest_names.join(", ")
            ));
            ui.label(format!(
                "{} character file(s){}.",
                dest_names.len(),
                if plan.dest_accounts.is_empty() {
                    String::new()
                } else {
                    format!(" and {} account file(s)", plan.dest_accounts.len())
                }
            ));
            ui.label(
                egui::RichText::new("Every file that gets overwritten is backed up next to it.")
                    .weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Copy").clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    state.confirm = false;
                }
            });
        });
        if resp.should_close() {
            state.confirm = false;
        }
        if go {
            state.confirm = false;
            // A client can start while the dialog is open, and copying then would be undone on
            // its exit, so the gate is checked again here and not only when the button was drawn.
            let now = crate::eveproc::running_clients();
            if now.as_ref().is_some_and(|c| !c.is_empty()) {
                state.result = Some(Err("EVE started while the dialog was open. Nothing was copied.".to_owned()));
            } else {
                state.result = Some(charsettings::copy(&root, &plan));
                state.invalidate();
            }
        }
    }

    if let Some(result) = state.result.clone() {
        let mut open = true;
        egui::Window::new(match &result {
            Ok(_) => format!("{}  Settings copied", egui_phosphor::regular::CHECK),
            Err(_) => format!("{}  Copy failed", egui_phosphor::regular::WARNING),
        })
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            ui.set_min_width(320.0);
            match &result {
                Ok(report) => {
                    ui.label(format!(
                        "{} character file(s) and {} account file(s) written.",
                        report.char_files, report.account_files
                    ));
                    if !report.backups.is_empty() {
                        ui.label(format!("{} backup(s) kept.", report.backups.len()));
                    }
                    ui.label(
                        egui::RichText::new("Start EVE to see the copied settings.").weak(),
                    );
                }
                Err(e) => {
                    ui.colored_label(standing::HOSTILE, e);
                }
            }
            ui.add_space(6.0);
            if ui.button("Close").clicked() {
                state.result = None;
            }
        });
        if !open {
            state.result = None;
        }
    }
}
