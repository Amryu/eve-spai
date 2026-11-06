//! The copy dialog: from one character onto others, everything or chosen parts.

use super::*;
use crate::evesettings::copy::{Part, Plan, What};

pub(crate) struct CopyDialog {
    pub(crate) source: i64,
    pub(crate) dst_profile: String,
    pub(crate) dests: BTreeSet<i64>,
    pub(crate) everything: bool,
    /// With `everything`: also replace the account files.
    pub(crate) with_account: bool,
    pub(crate) parts: BTreeSet<Part>,
}

impl CopyDialog {
    pub(crate) fn new(source: i64, profile: &str) -> Self {
        Self {
            source,
            dst_profile: profile.to_owned(),
            dests: BTreeSet::new(),
            everything: false,
            with_account: false,
            parts: BTreeSet::from([Part::Layout]),
        }
    }

    fn wants_account(&self) -> bool {
        if self.everything {
            self.with_account
        } else {
            self.parts.iter().any(|p| p.kind() == Kind::Account)
        }
    }

    fn wants_character(&self) -> bool {
        self.everything || self.parts.iter().any(|p| p.kind() == Kind::Char)
    }

    fn dest_accounts(&self, st: &CharsUi) -> Vec<i64> {
        if !self.wants_account() || st.account_of(self.source).is_none() {
            return Vec::new();
        }
        let same_profile = self.dst_profile == st.profile;
        let own = st.account_of(self.source);
        let mut out: Vec<i64> = self
            .dests
            .iter()
            .filter_map(|id| st.account_of(*id))
            .filter(|a| !(same_profile && Some(*a) == own))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    pub(crate) fn plan(&self, st: &CharsUi) -> Option<Plan> {
        let root = st.root.as_ref()?;
        Some(Plan {
            source_dir: charsettings::profile_dir(root, &st.profile),
            dest_dir: charsettings::profile_dir(root, &self.dst_profile),
            source_char: self.source,
            source_account: st.account_of(self.source),
            dest_chars: if self.wants_character() { self.dests.iter().copied().collect() } else { Vec::new() },
            dest_accounts: self.dest_accounts(st),
            what: if self.everything { What::Everything } else { What::Parts(self.parts.iter().copied().collect()) },
        })
    }

    /// What the copy will do and what it cannot, as lines for the summary; the bool marks a
    /// warning.
    pub(crate) fn summary(&self, st: &CharsUi) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        if self.dests.is_empty() {
            out.push((tr!("Pick at least one character to copy onto.").to_owned(), false));
            return out;
        }
        if !self.everything && self.parts.is_empty() {
            out.push((tr!("Pick at least one part to copy.").to_owned(), false));
            return out;
        }
        let accounts = self.dest_accounts(st);
        let chars = if self.wants_character() { self.dests.len() } else { 0 };
        out.push((
            trf!("Files to change: character {c}, account {a}.", c = chars, a = accounts.len()),
            false,
        ));
        if self.wants_account() {
            if st.account_of(self.source).is_none() {
                out.push((tr!("The source character's account is not known, so no account settings are copied.").to_owned(), true));
            } else {
                let unknown: Vec<String> = self.dests.iter().filter(|id| st.account_of(**id).is_none()).map(|id| st.name_of(*id)).collect();
                if !unknown.is_empty() {
                    out.push((trf!("No account known for {names}: they get character settings only.", names = unknown.join(", ")), true));
                }
                let also: Vec<String> = st
                    .rows
                    .iter()
                    .filter(|r| !self.dests.contains(&r.id) && r.id != self.source)
                    .filter(|r| r.account.is_some_and(|(a, _)| accounts.contains(&a)))
                    .map(|r| r.display())
                    .collect();
                if !also.is_empty() {
                    out.push((trf!("Account settings are shared: they also change for {names}.", names = also.join(", ")), true));
                }
            }
        }
        if !self.everything && self.wants_character() && self.dst_profile == st.profile {
            let fresh: Vec<String> = self.dests.iter().filter(|id| st.row(**id).is_some_and(|r| !r.has_file)).map(|id| st.name_of(*id)).collect();
            if !fresh.is_empty() {
                out.push((trf!("No settings file yet for {names}, and a part needs one to go into. Copy everything to create it.", names = fresh.join(", ")), true));
            }
        }
        out
    }

    fn ready(&self) -> bool {
        !self.dests.is_empty() && (self.everything || !self.parts.is_empty())
    }
}

pub(crate) fn body(ui: &mut egui::Ui, st: &CharsUi, d: &mut CopyDialog, blocked: Option<&str>) -> (bool, bool) {
    let (mut go, mut cancel) = (false, false);
    let who = st.name_of(d.source);
    ui.horizontal(|ui| {
        ui.add(egui::Image::new(eve_portrait_url(d.source, 32.0)).fit_to_exact_size(egui::Vec2::splat(28.0)).corner_radius(2.0));
        let title = trf!("From {name}", name = who);
        clipped(ui, egui::RichText::new(&title).strong().size(16.0), &title);
    });
    if st.profiles.len() > 1 {
        ui.horizontal(|ui| {
            ui.label(trf!("Profile {from} to", from = st.profile));
            egui::ComboBox::from_id_salt("copy_dst_profile").selected_text(d.dst_profile.clone()).show_ui(ui, |ui| {
                for p in &st.profiles {
                    ui.menu_value(&mut d.dst_profile, p.clone(), p.as_str());
                }
            });
        });
    }
    ui.add_space(6.0);

    egui::Panel::bottom("copy_foot").frame(egui::Frame::NONE).show_inside(ui, |ui| {
        ui.add_space(6.0);
        let out = egui::ScrollArea::vertical().id_salt("copy_summary").max_height(84.0).min_scrolled_height(84.0).auto_shrink([false, false]).show(ui, |ui| {
            for (line, warn) in d.summary(st) {
                let text = egui::RichText::new(line);
                ui.add(egui::Label::new(if warn { text.color(standing::WARNING) } else { text }).wrap());
            }
            if let Some(why) = blocked {
                ui.add(egui::Label::new(egui::RichText::new(why).color(standing::HOSTILE)).wrap());
            }
        });
        spai_fit_ui::rows::note_scrolled(ui.ctx(), &out);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let b = ui.add_enabled(d.ready() && blocked.is_none(), egui::Button::new(trf!("{icon}  Copy", icon = icon::COPY)));
            go = b.on_hover_text(tr!("What is there now is backed up first")).clicked();
            cancel = ui.button(tr!("Cancel")).clicked();
        });
    });

    // One scroll area for both columns: the two lists are short, and a dialog at its smallest
    // then has a single thing to scroll.
    sidebar_tab::scroll(ui, "copy_body", |ui| ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr!("Onto")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr!("None")).clicked() {
                    d.dests.clear();
                }
                if ui.button(tr!("All")).clicked() {
                    d.dests = st.rows.iter().map(|r| r.id).filter(|id| *id != d.source || d.dst_profile != st.profile).collect();
                }
            });
        });
        {
            for (account, ids) in st.grouped() {
                let head = account.map_or_else(|| tr!("Account not known").to_owned(), |a| st.account_label(a));
                ui.add(egui::Label::new(egui::RichText::new(head).weak()).truncate());
                for id in ids {
                    let is_source = id == d.source && d.dst_profile == st.profile;
                    let mut on = d.dests.contains(&id);
                    let name = st.name_of(id);
                    let label = if is_source { trf!("{name} (the source)", name = name) } else { name };
                    ui.add_enabled_ui(!is_source, |ui| {
                        if ui.add(egui::Checkbox::new(&mut on, egui::RichText::new(label))).changed() {
                            if on {
                                d.dests.insert(id);
                            } else {
                                d.dests.remove(&id);
                            }
                        }
                    });
                }
                ui.add_space(4.0);
            }
        }

        let ui = &mut cols[1];
        ui.label(egui::RichText::new(tr!("What")).strong());
        {
            ui.radio_value(&mut d.everything, false, tr!("Chosen parts"))
                .on_hover_text(tr!("Changes only these parts and leaves the rest of each character's settings as it is"));
            ui.add_enabled_ui(!d.everything, |ui| {
                ui.indent("copy_parts", |ui| {
                    let mut tick = |ui: &mut egui::Ui, part: Part| {
                        let mut on = d.parts.contains(&part);
                        if ui.checkbox(&mut on, spai_ui::i18n::t(part.label())).changed() {
                            if on {
                                d.parts.insert(part);
                            } else {
                                d.parts.remove(&part);
                            }
                        }
                    };
                    ui.label(egui::RichText::new(tr!("Per character")).weak());
                    for part in Part::CHARACTER {
                        tick(ui, part);
                    }
                    ui.label(egui::RichText::new(tr!("Per account")).weak());
                    for part in Part::ACCOUNT {
                        tick(ui, part);
                    }
                });
            });
            ui.add_space(4.0);
            ui.radio_value(&mut d.everything, true, tr!("Everything"))
                .on_hover_text(tr!("Replaces each character's settings file with a copy of the source's"));
            ui.add_enabled_ui(d.everything, |ui| {
                ui.indent("copy_all", |ui| {
                    ui.checkbox(&mut d.with_account, tr!("And the account's settings"))
                        .on_hover_text(tr!("Overview, shortcuts, probe formations and everything else the game stores per account"));
                });
            });
        }
    }));
    (go, cancel)
}

impl SpaiApp {
    /// Returns true when the dialog should close.
    pub(super) fn chars_copy_dialog(&mut self, ctx: &egui::Context, st: &mut CharsUi, d: &mut CopyDialog) -> bool {
        let blocked = self.eve_running().map(|_| tr!("EVE is running. Close every client first: the game writes its files again on exit.").to_owned());
        let (mut go, mut cancel) = (false, false);
        let keep = Self::dialog_viewport(ctx, "chars_copy", tr!("EVE Spai - Copy settings"), [620.0, 540.0], |ui| {
            (go, cancel) = body(ui, st, d, blocked.as_deref());
        });
        if !go {
            return cancel || !keep;
        }
        let who = st.name_of(d.source);
        let info = backup::Meta { reason: trf!("Before a copy from {name}", name = who), profile: d.dst_profile.clone(), ..Default::default() };
        // A client can start while the dialog is open.
        let running = !self.headless && crate::eveproc::running_clients().is_some_and(|c| !c.is_empty());
        let done = match (running, d.plan(st), st.backup_root.clone()) {
            (true, _, _) => Err(tr!("EVE started while the dialog was open. Nothing was copied.").to_owned()),
            (false, Some(plan), Some(root)) => copy::run(&plan, &root, info),
            _ => Err(tr!("No EVE settings directory found.").to_owned()),
        };
        match done {
            Ok(r) if r.skipped.is_empty() => {
                st.ok(trf!("Copied. Files written: character {c}, account {a}. Start EVE to see it.", c = r.char_files, a = r.account_files));
            }
            Ok(r) => {
                let names: Vec<String> = r.skipped.iter().filter(|(k, _)| *k == Kind::Char).map(|(_, id)| st.name_of(*id)).collect();
                st.ok(trf!("Copied. Files written: character {c}, account {a}. Skipped, no settings file yet: {names}.", c = r.char_files, a = r.account_files, names = names.join(", ")));
            }
            Err(e) => st.fail(e),
        }
        st.session = None;
        st.invalidate();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, account: Option<i64>, has_file: bool) -> Row {
        Row { id, name: Some(format!("Pilot {id}")), has_file, account: account.map(|a| (a, AssocSource::Manual)) }
    }

    /// 1 and 2 share account 10, 3 is on account 20 with 4, 5 has no known account and 6 no file.
    fn state() -> CharsUi {
        CharsUi {
            root: Some(PathBuf::from("/eve")),
            profile: "Default".into(),
            profiles: vec!["Default".into(), "Second".into()],
            rows: vec![
                row(1, Some(10), true),
                row(2, Some(10), true),
                row(3, Some(20), true),
                row(4, Some(20), true),
                row(5, None, true),
                row(6, Some(30), false),
            ],
            ..Default::default()
        }
    }

    fn dialog(dests: &[i64]) -> CopyDialog {
        let mut d = CopyDialog::new(1, "Default");
        d.dests = dests.iter().copied().collect();
        d
    }

    #[test]
    fn character_parts_leave_the_accounts_alone() {
        let st = state();
        let d = dialog(&[2, 3]);
        let plan = d.plan(&st).unwrap();
        assert_eq!(plan.dest_chars, vec![2, 3]);
        assert!(plan.dest_accounts.is_empty());
        assert_eq!(plan.what, What::Parts(vec![Part::Layout]));
        assert_eq!(plan.source_dir, PathBuf::from("/eve/settings_Default"));
    }

    #[test]
    fn account_parts_reach_each_other_account_once() {
        let st = state();
        let mut d = dialog(&[2, 3, 4, 5]);
        d.parts = BTreeSet::from([Part::Overview]);
        let plan = d.plan(&st).unwrap();
        // 2 shares the source's account, 3 and 4 share one, 5 has none.
        assert_eq!(plan.dest_accounts, vec![20]);
        assert!(plan.dest_chars.is_empty(), "no character part was picked");
        let lines = d.summary(&st);
        assert!(lines.iter().any(|(l, warn)| *warn && l.contains("Pilot 5")), "{lines:?}");
    }

    #[test]
    fn shared_accounts_name_who_else_changes() {
        let st = state();
        let mut d = dialog(&[3]);
        d.everything = true;
        d.with_account = true;
        let plan = d.plan(&st).unwrap();
        assert_eq!((plan.dest_chars.clone(), plan.dest_accounts.clone()), (vec![3], vec![20]));
        assert_eq!(plan.what, What::Everything);
        let lines = d.summary(&st);
        assert!(lines.iter().any(|(l, warn)| *warn && l.contains("Pilot 4")), "{lines:?}");
        assert!(!lines.iter().any(|(l, _)| l.contains("Pilot 2")), "the source's own account is not a destination");
    }

    #[test]
    fn another_profile_takes_the_source_account_too() {
        let st = state();
        let mut d = dialog(&[1, 2]);
        d.dst_profile = "Second".into();
        d.everything = true;
        d.with_account = true;
        let plan = d.plan(&st).unwrap();
        assert_eq!(plan.dest_accounts, vec![10]);
        assert_eq!(plan.dest_dir, PathBuf::from("/eve/settings_Second"));
    }

    #[test]
    fn a_part_cannot_go_into_a_missing_file_and_says_so() {
        let st = state();
        let d = dialog(&[6]);
        assert!(d.summary(&st).iter().any(|(l, warn)| *warn && l.contains("Pilot 6")));
        let mut whole = dialog(&[6]);
        whole.everything = true;
        assert!(!whole.summary(&st).iter().any(|(_, warn)| *warn));
    }

    #[test]
    fn nothing_picked_is_not_ready() {
        let st = state();
        assert!(!dialog(&[]).ready());
        let mut d = dialog(&[2]);
        d.parts.clear();
        assert!(!d.ready());
        assert_eq!(d.summary(&st).len(), 1);
        d.everything = true;
        assert!(d.ready());
    }
}
