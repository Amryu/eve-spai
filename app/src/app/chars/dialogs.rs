//! The small dialogs of the Characters view: account, backups, restore and confirmations.

use super::*;

#[derive(Clone)]
enum Source {
    Backup(String),
    Sidecar(backup::Sidecar),
}

#[derive(Clone)]
pub(crate) struct Restore {
    source: Source,
    title: String,
    /// The character the files go onto.
    to: i64,
    has_char: bool,
    has_account: bool,
    char_file: bool,
    account_file: bool,
}

impl Restore {
    pub(crate) fn from_backup(meta: &backup::Meta, st: &CharsUi) -> Self {
        let selected = match st.selected {
            Pick::Char(id) => Some(id),
            _ => None,
        };
        let stored = meta.first(Kind::Char);
        let to = selected.or(meta.character_id).or(stored).filter(|id| st.row(*id).is_some()).or(st.rows.first().map(|r| r.id)).unwrap_or(0);
        let (has_char, has_account) = (stored.is_some(), meta.first(Kind::Account).is_some());
        // An automatic backup of several characters is restored one character at a time: the one
        // it is opened from.
        let own_char = selected.is_none_or(|id| meta.has(Kind::Char, id)) && has_char;
        Self {
            source: Source::Backup(meta.id.clone()),
            title: meta.title(),
            to,
            has_char,
            has_account,
            char_file: own_char,
            account_file: has_account && !own_char,
        }
    }

    pub(crate) fn from_sidecar(f: &backup::Sidecar, st: &CharsUi) -> Self {
        let to = match (f.kind, &st.selected) {
            (Kind::Char, _) => f.id,
            (Kind::Account, Pick::Char(id)) => *id,
            (Kind::Account, _) => st.mates(f.id).first().map_or(0, |r| r.id),
        };
        Self {
            source: Source::Sidecar(f.clone()),
            title: f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            to,
            has_char: f.kind == Kind::Char,
            has_account: f.kind == Kind::Account,
            char_file: f.kind == Kind::Char,
            account_file: f.kind == Kind::Account,
        }
    }

    /// The bytes to write and where, for the files ticked.
    fn targets(&self, st: &CharsUi) -> Result<Vec<copy::Target>, String> {
        let dir = st.profile_dir().ok_or_else(|| tr!("No EVE settings directory found.").to_owned())?;
        let read = |p: &std::path::Path| std::fs::read(p).map_err(|e| trf!("Reading the backup failed: {err}", err = e));
        let mut out = Vec::new();
        let source = |kind: Kind| -> Option<PathBuf> {
            match &self.source {
                Source::Sidecar(f) => (f.kind == kind).then(|| f.path.clone()),
                Source::Backup(id) => {
                    let meta = st.backups.iter().find(|b| b.id == *id)?;
                    // The selected character's own file when the backup has it, else the one it has.
                    let stored = Some(self.to).filter(|c| kind == Kind::Char && meta.has(kind, *c)).or_else(|| {
                        st.account_of(self.to).filter(|a| kind == Kind::Account && meta.has(kind, *a))
                    });
                    let stored = stored.or_else(|| meta.first(kind))?;
                    Some(backup::file_path(st.backup_root.as_ref()?, meta, kind, stored))
                }
            }
        };
        if self.char_file {
            if let Some(from) = source(Kind::Char) {
                out.push(copy::Target { kind: Kind::Char, id: self.to, path: dir.join(Kind::Char.file_name(self.to)), bytes: read(&from)? });
            }
        }
        if self.account_file {
            let account = match &self.source {
                Source::Sidecar(f) if f.kind == Kind::Account => Some(f.id),
                _ => st.account_of(self.to),
            };
            let account = account.ok_or_else(|| tr!("That character's account is not known, so there is nowhere to put the account settings.").to_owned())?;
            if let Some(from) = source(Kind::Account) {
                out.push(copy::Target { kind: Kind::Account, id: account, path: dir.join(Kind::Account.file_name(account)), bytes: read(&from)? });
            }
        }
        if out.is_empty() {
            return Err(tr!("Nothing was picked to restore.").to_owned());
        }
        Ok(out)
    }
}

/// A modal with a heading, a body and a row of buttons. Returns true when it should close without
/// an action (clicked outside, Escape).
fn modal(ctx: &egui::Context, id: &str, heading: &str, body: impl FnOnce(&mut egui::Ui)) -> bool {
    egui::Modal::new(egui::Id::new(id))
        .show(ctx, |ui| {
            ui.set_width(400.0f32.min(ctx.content_rect().width() - 60.0));
            ui.heading(heading);
            ui.add_space(6.0);
            body(ui);
        })
        .should_close()
}

impl SpaiApp {
    fn chars_blocked(&self) -> Option<String> {
        if !self.headless && crate::eveproc::running_clients().is_some_and(|c| !c.is_empty()) {
            return Some(tr!("EVE is running. Close every client first: the game writes its files again on exit.").to_owned());
        }
        None
    }

    pub(super) fn chars_dialogs(&mut self, ctx: &egui::Context, st: &mut CharsUi) {
        let mut dialog = std::mem::take(&mut st.dialog);
        // `close` is set by a button inside a dialog, `outside` by a click beside it or Escape.
        let mut close = false;
        let mut outside = false;
        match &mut dialog {
            Dialog::None => return,
            Dialog::Copy(d) => close = self.chars_copy_dialog(ctx, st, d),
            Dialog::Account { id, input } => {
                let id = *id;
                let Some(row) = st.row(id).cloned() else {
                    return;
                };
                let mut known: Vec<i64> = st.accounts.clone();
                known.extend(st.rows.iter().filter_map(|r| r.account.map(|(a, _)| a)));
                known.sort_unstable();
                known.dedup();
                let mut assign: Option<Option<i64>> = None;
                outside = modal(ctx, "chars_account", &trf!("Account of {name}", name = row.display()), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(tr!("Overview, shortcuts and probe formations are stored per account. The account is found by itself while the character is logged in to the game; pick it here when that has not happened yet or went wrong.")).weak()).wrap());
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for a in &known {
                            let mates: Vec<String> = st.mates(*a).iter().filter(|r| r.id != id).map(|r| r.display()).collect();
                            let mine = row.account.is_some_and(|(cur, _)| cur == *a);
                            let who = if mates.is_empty() { tr!("no characters known").to_owned() } else { mates.join(", ") };
                            let label = format!("{}   ·   {who}", st.account_label(*a));
                            let r = ui.add_sized([ui.available_width(), 24.0], egui::Button::new(egui::RichText::new(label)).selected(mine).truncate());
                            // Three characters is an account's limit, so a full one is only right
                            // when one of its three is wrong.
                            let r = if mates.len() >= 3 && !mine { r.on_hover_text(tr!("Already has three characters, which is an account's limit.")) } else { r.on_hover_text(trf!("EVE account id {id}", id = a)) };
                            if r.clicked() {
                                assign = Some(Some(*a));
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_sized([150.0, 22.0], egui::TextEdit::singleline(input).hint_text(tr!("other account id")));
                        let typed = input.trim().parse::<i64>().ok().filter(|a| *a > 0);
                        if ui.add_enabled(typed.is_some(), egui::Button::new(tr!("Set"))).clicked() {
                            assign = Some(typed);
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(row.account.is_some(), egui::Button::new(tr!("Forget the account"))).clicked() {
                            assign = Some(None);
                        }
                        if ui.button(tr!("Cancel")).clicked() {
                            close = true;
                        }
                    });
                });
                if let (Some(a), Some(store)) = (assign, self.store.as_ref()) {
                    match a {
                        Some(a) => store.set_char_account(id, a, AssocSource::Manual),
                        None => store.clear_char_account(id),
                    }
                    st.session = None;
                    st.invalidate();
                    close = true;
                }
            }
            Dialog::NewBackup { name, with_account } => {
                let Pick::Char(id) = st.selected else {
                    return;
                };
                let who = st.name_of(id);
                let account = st.account_of(id);
                let mut go = false;
                outside = modal(ctx, "chars_backup_new", &trf!("Back up {name}", name = who), |ui| {
                    ui.add_sized([ui.available_width(), 22.0], egui::TextEdit::singleline(name).hint_text(tr!("Name, for example PvP layout")));
                    ui.add_space(4.0);
                    ui.add_enabled_ui(account.is_some(), |ui| {
                        ui.checkbox(with_account, tr!("Include the account's settings (overview, shortcuts, probe formations)"))
                            .on_disabled_hover_text(tr!("This character's account is not known."));
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        go = ui.button(trf!("{icon}  Back up", icon = icon::ARCHIVE)).clicked();
                        if ui.button(tr!("Cancel")).clicked() {
                            close = true;
                        }
                    });
                });
                if go {
                    close = true;
                    let label = if name.trim().is_empty() { trf!("{name}, {day}", name = who, day = day_text(crate::clock::utc().timestamp())) } else { name.trim().to_owned() };
                    let mut sources = Vec::new();
                    if let Some(dir) = st.profile_dir() {
                        sources.push((Kind::Char, id, dir.join(Kind::Char.file_name(id))));
                        if let Some(a) = account.filter(|_| *with_account) {
                            sources.push((Kind::Account, a, dir.join(Kind::Account.file_name(a))));
                        }
                    }
                    let meta = backup::Meta { name: label.clone(), profile: st.profile.clone(), character_id: Some(id), character_name: who, ..Default::default() };
                    match st.backup_root.clone().map(|root| backup::create(&root, meta, &sources)) {
                        Some(Ok(Some(_))) => st.ok(trf!("Backed up as {name}.", name = label)),
                        Some(Ok(None)) => st.fail(tr!("There is no settings file to back up.").to_owned()),
                        Some(Err(e)) => st.fail(e),
                        None => st.fail(tr!("No data directory, cannot keep a backup.").to_owned()),
                    }
                    st.invalidate();
                }
            }
            Dialog::RenameBackup { id, name } => {
                let mut go = false;
                outside = modal(ctx, "chars_backup_rename", tr!("Name this backup"), |ui| {
                    let edit = ui.add_sized([ui.available_width(), 22.0], egui::TextEdit::singleline(name));
                    go = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        go |= ui.add_enabled(!name.trim().is_empty(), egui::Button::new(tr!("Save"))).clicked();
                        if ui.button(tr!("Cancel")).clicked() {
                            close = true;
                        }
                    });
                });
                if go && !name.trim().is_empty() {
                    close = true;
                    if let Some(Err(e)) = st.backup_root.clone().map(|root| backup::rename(&root, id, name)) {
                        st.fail(e);
                    }
                    st.invalidate();
                }
            }
            Dialog::DeleteBackup { id } => {
                let title = st.backups.iter().find(|b| b.id == *id).map(|b| b.title()).unwrap_or_default();
                let mut go = false;
                outside = modal(ctx, "chars_backup_delete", tr!("Delete this backup?"), |ui| {
                    ui.add(egui::Label::new(title.as_str()).wrap());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        go = ui.button(trf!("{icon}  Delete", icon = icon::TRASH)).clicked();
                        if ui.button(tr!("Cancel")).clicked() {
                            close = true;
                        }
                    });
                });
                if go {
                    close = true;
                    if let Some(Err(e)) = st.backup_root.clone().map(|root| backup::delete(&root, id)) {
                        st.fail(e);
                    }
                    if st.selected == Pick::Backup(id.clone()) {
                        st.selected = Pick::None;
                        st.session = None;
                    }
                    st.invalidate();
                }
            }
            Dialog::DeleteSidecar(f) => {
                let name = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let mut go = false;
                outside = modal(ctx, "chars_sidecar_delete", tr!("Delete this backup file?"), |ui| {
                    ui.add(egui::Label::new(name.as_str()).wrap());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        go = ui.button(trf!("{icon}  Delete", icon = icon::TRASH)).clicked();
                        if ui.button(tr!("Cancel")).clicked() {
                            close = true;
                        }
                    });
                });
                if go {
                    close = true;
                    // Only ever a file the scan recognised as a backup, never a live settings file.
                    if backup::parse_sidecar(&name).is_some() {
                        if let Err(e) = std::fs::remove_file(&f.path) {
                            st.fail(e.to_string());
                        }
                    }
                    st.invalidate();
                }
            }
            Dialog::Restore(r) => {
                let blocked = self.chars_blocked();
                let mut go = false;
                outside = modal(ctx, "chars_restore", tr!("Restore settings"), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(r.title.as_str()).strong()).wrap());
                    ui.add_space(6.0);
                    let fixed = matches!(&r.source, Source::Sidecar(f) if f.kind == Kind::Char);
                    ui.horizontal(|ui| {
                        ui.label(tr!("Onto"));
                        ui.add_enabled_ui(!fixed, |ui| {
                            egui::ComboBox::from_id_salt("restore_to").width(220.0).truncate().selected_text(st.name_of(r.to)).show_ui(ui, |ui| {
                                for row in &st.rows {
                                    ui.menu_value(&mut r.to, row.id, row.display());
                                }
                            });
                        });
                    });
                    ui.add_enabled_ui(r.has_char, |ui| ui.checkbox(&mut r.char_file, tr!("Character settings (windows, sidebar, channels, watch lists)")));
                    let account = match &r.source {
                        Source::Sidecar(f) if f.kind == Kind::Account => Some(f.id),
                        _ => st.account_of(r.to),
                    };
                    ui.add_enabled_ui(r.has_account, |ui| ui.checkbox(&mut r.account_file, tr!("Account settings (overview, shortcuts, probe formations)")));
                    if r.account_file {
                        let others: Vec<String> = account.map(|a| st.mates(a).iter().filter(|m| m.id != r.to).map(|m| m.display()).collect()).unwrap_or_default();
                        if !others.is_empty() {
                            ui.add(egui::Label::new(egui::RichText::new(trf!("Account settings are shared: they also change for {names}.", names = others.join(", "))).color(standing::WARNING)).wrap());
                        }
                    }
                    ui.add(egui::Label::new(egui::RichText::new(tr!("What is there now is backed up first.")).weak()).wrap());
                    if let Some(why) = &blocked {
                        ui.add(egui::Label::new(egui::RichText::new(why).color(standing::WARNING)).wrap());
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let can = blocked.is_none() && (r.char_file || r.account_file);
                        go = ui.add_enabled(can, egui::Button::new(trf!("{icon}  Restore", icon = icon::ARROW_COUNTER_CLOCKWISE))).clicked();
                        if ui.button(tr!("Cancel")).clicked() {
                            close = true;
                        }
                    });
                });
                if go {
                    close = true;
                    let who = st.name_of(r.to);
                    let info = backup::Meta {
                        reason: trf!("Before restoring onto {name}", name = who),
                        profile: st.profile.clone(),
                        character_id: Some(r.to),
                        character_name: who.clone(),
                        ..Default::default()
                    };
                    let done = match (self.chars_blocked(), st.backup_root.clone()) {
                        (Some(why), _) => Err(why),
                        (None, None) => Err(tr!("No data directory, cannot keep a backup, so nothing was restored.").to_owned()),
                        (None, Some(root)) => r.targets(st).and_then(|t| copy::commit(&root, info, &t)),
                    };
                    match done {
                        Ok(_) => st.ok(trf!("Restored onto {name}. Start EVE to see it.", name = who)),
                        Err(e) => st.fail(e),
                    }
                    st.session = None;
                    st.invalidate();
                }
            }
            Dialog::Remove { id } => {
                let id = *id;
                let who = st.name_of(id);
                let mut go = false;
                outside = modal(ctx, "chars_remove", &trf!("Remove the login of {name}?", name = who), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(tr!("Location, fleet and skill features stop for this character. Its EVE settings files and backups are not touched.")).weak()).wrap());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        go = ui.button(trf!("{icon}  Remove login", icon = icon::SIGN_OUT)).clicked();
                        if ui.button(tr!("Cancel")).clicked() {
                            close = true;
                        }
                    });
                });
                if go {
                    close = true;
                    if let Some(store) = &self.store {
                        let _ = store.remove_character(id);
                    }
                    self.refresh_characters();
                    st.invalidate();
                }
            }
            Dialog::Discard(pending) => {
                let mut go = false;
                outside = modal(ctx, "chars_discard", tr!("Drop the unsaved changes?"), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(tr!("The settings shown have edits that were not saved.")).weak()).wrap());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        go = ui.button(tr!("Drop them")).clicked();
                        if ui.button(tr!("Keep editing")).clicked() {
                            close = true;
                        }
                    });
                });
                if go {
                    close = true;
                    st.session = None;
                    st.notice = None;
                    match pending.as_ref() {
                        Pending::Select(pick) => st.selected = pick.clone(),
                        Pending::Profile(p) => {
                            st.profile = p.clone();
                            st.invalidate();
                        }
                    }
                }
            }
        }
        if !(close || outside) && matches!(st.dialog, Dialog::None) {
            st.dialog = dialog;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Characters 1 and 2 on account 10, with a backup holding both character files and the
    /// account file, each file's bytes naming what it is.
    fn state(tag: &str) -> (Tmp, CharsUi) {
        let dir = std::env::temp_dir().join(format!("evespai-restore-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let live = dir.join("eve").join("settings_Default");
        std::fs::create_dir_all(&live).unwrap();
        let mut sources = Vec::new();
        for (kind, id) in [(Kind::Char, 1), (Kind::Char, 2), (Kind::Account, 10)] {
            let path = live.join(kind.file_name(id));
            std::fs::write(&path, format!("{kind:?} {id}")).unwrap();
            sources.push((kind, id, path));
        }
        let backups = dir.join("backups");
        backup::create(&backups, backup::Meta { automatic: true, ..Default::default() }, &sources).unwrap();
        let row = |id: i64, account: Option<i64>| Row { id, name: None, has_file: true, account: account.map(|a| (a, AssocSource::Manual)) };
        let st = CharsUi {
            root: Some(dir.join("eve")),
            profile: "Default".into(),
            rows: vec![row(1, Some(10)), row(2, Some(10)), row(3, None)],
            backups: backup::list(&backups),
            backup_root: Some(backups),
            selected: Pick::Char(2),
            ..Default::default()
        };
        (Tmp(dir), st)
    }

    fn text(t: &copy::Target) -> String {
        String::from_utf8(t.bytes.clone()).unwrap()
    }

    #[test]
    fn a_backup_of_several_characters_restores_the_one_it_is_opened_from() {
        let (_t, st) = state("own");
        let r = Restore::from_backup(&st.backups[0], &st);
        assert_eq!(r.to, 2);
        assert!(r.char_file && !r.account_file);
        let targets = r.targets(&st).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(text(&targets[0]), "Char 2");
        assert!(targets[0].path.ends_with("settings_Default/core_char_2.dat"));
    }

    #[test]
    fn the_account_file_goes_to_the_target_characters_account() {
        let (_t, st) = state("account");
        let mut r = Restore::from_backup(&st.backups[0], &st);
        r.account_file = true;
        let targets = r.targets(&st).unwrap();
        assert_eq!(targets.iter().map(text).collect::<Vec<_>>(), vec!["Char 2", "Account 10"]);
        assert!(targets[1].path.ends_with("core_user_10.dat"));

        // Onto a character whose account is unknown there is nowhere for it to go.
        r.to = 3;
        assert!(r.targets(&st).is_err());
        r.account_file = false;
        let onto_other = r.targets(&st).unwrap();
        // 3 has no file of its own in the backup, so it takes the first one there.
        assert_eq!(text(&onto_other[0]), "Char 1");
        assert!(onto_other[0].path.ends_with("core_char_3.dat"));
    }

    #[test]
    fn nothing_ticked_is_refused() {
        let (_t, st) = state("none");
        let mut r = Restore::from_backup(&st.backups[0], &st);
        r.char_file = false;
        assert!(r.targets(&st).is_err());
    }

    #[test]
    fn a_file_left_beside_the_settings_restores_onto_its_own_id() {
        let (t, st) = state("sidecar");
        let path = t.0.join("eve/settings_Default/core_char_1_spai_backup_4.dat");
        std::fs::write(&path, "old char 1").unwrap();
        let side = backup::sidecars(path.parent().unwrap()).remove(0);
        let r = Restore::from_sidecar(&side, &st);
        assert_eq!(r.to, 1);
        let targets = r.targets(&st).unwrap();
        assert_eq!(text(&targets[0]), "old char 1");
        assert!(targets[0].path.ends_with("core_char_1.dat"));
    }
}
