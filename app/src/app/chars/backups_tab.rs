//! The Backups tab: every backup that holds the selected character's settings.

use super::sidebar_tab::{row, scroll};
use super::*;

fn contents(meta: &backup::Meta, char_id: Option<i64>, account: Option<i64>) -> &'static str {
    let c = char_id.is_some_and(|id| meta.has(Kind::Char, id));
    let a = account.is_some_and(|id| meta.has(Kind::Account, id));
    match (c, a) {
        (true, true) => tr!("character and account"),
        (true, false) => tr!("character"),
        _ => tr!("account"),
    }
}

impl SpaiApp {
    pub(super) fn chars_backups_tab(&mut self, ui: &mut egui::Ui, st: &mut CharsUi, s: &Session) {
        let (char_id, account) = (s.char_id, s.account_id);
        let now = crate::clock::utc().timestamp();
        let mine: Vec<backup::Meta> = st
            .backups
            .iter()
            .filter(|m| char_id.is_some_and(|id| m.has(Kind::Char, id)) || account.is_some_and(|id| m.has(Kind::Account, id)))
            .cloned()
            .collect();
        let beside: Vec<backup::Sidecar> = st
            .sidecars
            .iter()
            .filter(|f| match f.kind {
                Kind::Char => Some(f.id) == char_id,
                Kind::Account => Some(f.id) == account,
            })
            .cloned()
            .collect();
        ui.add_space(4.0);
        let hint = trf!("A backup is taken before every change made here. The newest {n} automatic ones are kept; naming one keeps it for good.", n = backup::KEEP_AUTOMATIC);
        clipped(ui, egui::RichText::new(&hint).weak(), &hint);
        ui.add_space(4.0);
        scroll(ui, "backup_rows", |ui| {
            if mine.is_empty() {
                ui.label(egui::RichText::new(tr!("No backups of this character yet.")).weak());
            }
            for m in &mine {
                let glyph = if m.automatic { icon::CLOCK_COUNTER_CLOCKWISE } else { icon::ARCHIVE };
                let label = format!(
                    "{glyph}  {}   ·   {}   ·   {}   ·   {}",
                    m.title(),
                    ago(now, m.created),
                    contents(m, char_id, account),
                    size_text(m.size())
                );
                {
                    row(ui, "backup_rows", &m.id, 0.0, &label, m.automatic, |ui| {
                        if spai_ui::widgets::icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete this backup")).clicked() {
                            st.dialog = Dialog::DeleteBackup { id: m.id.clone() };
                        }
                        let keep = if m.automatic { tr!("Name it, which keeps it") } else { tr!("Rename") };
                        if spai_ui::widgets::icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text(keep).clicked() {
                            st.dialog = Dialog::RenameBackup { id: m.id.clone(), name: m.name.clone() };
                        }
                        if spai_ui::widgets::icon_button(ui, icon::ARROW_COUNTER_CLOCKWISE).on_hover_text(tr!("Restore…")).clicked() {
                            st.dialog = Dialog::Restore(dialogs::Restore::from_backup(m, st));
                        }
                        if spai_ui::widgets::icon_button(ui, icon::EYE).on_hover_text(tr!("Look at what it holds")).clicked() {
                            self.chars_select(st, Pick::Backup(m.id.clone()));
                        }
                    });
                }
            }
            if !beside.is_empty() {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(tr!("Left next to the game's own files")).strong())
                    .on_hover_text(tr!("Backup files written beside the settings by an earlier version or another tool"));
                for f in &beside {
                    let name = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let what = if f.kind == Kind::Char { tr!("character") } else { tr!("account") };
                    let label = format!("{name}   ·   {}   ·   {what}   ·   {}", ago(now, f.modified), size_text(f.size));
                    {
                        row(ui, "backup_files", &f.path, 0.0, &label, true, |ui| {
                            if spai_ui::widgets::icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete this file")).clicked() {
                                st.dialog = Dialog::DeleteSidecar(f.clone());
                            }
                            if spai_ui::widgets::icon_button(ui, icon::ARROW_COUNTER_CLOCKWISE).on_hover_text(tr!("Restore…")).clicked() {
                                st.dialog = Dialog::Restore(dialogs::Restore::from_sidecar(f, st));
                            }
                        });
                    }
                }
            }
        });
    }
}
