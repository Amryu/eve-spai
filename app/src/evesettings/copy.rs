//! Writing settings files: copies between characters, whole or by part, and every other save.
//! Each write is checked first, backed up, then replaced atomically.

use std::path::{Path, PathBuf};

use super::backup::{self, Kind, Meta};
use super::{chat, SettingsFile};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    Layout,
    Neocom,
    Watchlist,
    Chat,
    Overview,
    Probes,
    Shortcuts,
}

impl Part {
    pub const CHARACTER: [Part; 4] = [Part::Layout, Part::Neocom, Part::Chat, Part::Watchlist];
    pub const ACCOUNT: [Part; 3] = [Part::Overview, Part::Probes, Part::Shortcuts];

    pub fn label(self) -> &'static str {
        match self {
            Part::Layout => "Window layout",
            Part::Neocom => "Neocom buttons",
            Part::Watchlist => "Fleet watch lists",
            Part::Chat => "Chat channels",
            Part::Overview => "Overview",
            Part::Probes => "Probe formations",
            Part::Shortcuts => "Keyboard shortcuts",
        }
    }

    pub fn kind(self) -> Kind {
        if Part::ACCOUNT.contains(&self) {
            Kind::Account
        } else {
            Kind::Char
        }
    }

    /// Makes this part of `to` match `from`.
    pub fn apply(self, from: &SettingsFile, to: &mut SettingsFile) {
        let entries = |to: &mut SettingsFile, keys: &[&str]| {
            for key in keys {
                to.take_entry(from, "ui", key);
            }
        };
        match self {
            Part::Layout => to.take_section(from, "windows"),
            Part::Neocom => entries(
                to,
                &["neocomButtonRawData", "neocomWidth", "neocomSizeLocked", "neocom_light_background_enabled"],
            ),
            Part::Watchlist => {
                entries(to, &["fleetWathlistMemberInfo", "fleet_watchlistcolors", "fleet_watchlist_twoColumns"]);
            }
            Part::Chat => {
                let Some(theirs) = chat::read(from) else {
                    return;
                };
                let mine = chat::read(to).unwrap_or_default();
                // Corporation, alliance and local chat carry the owner's id, so the destination
                // keeps its own and only takes their place in the order.
                let merged: Vec<chat::Channel> = theirs
                    .into_iter()
                    .filter_map(|c| {
                        if c.built_in() {
                            mine.iter().find(|m| m.key == c.key).cloned()
                        } else {
                            Some(c)
                        }
                    })
                    .collect();
                chat::write(to, &merged);
            }
            Part::Overview => {
                to.take_section(from, "overview");
                to.take_section(from, "defaultoverview");
            }
            Part::Probes => {
                entries(to, &["probescanning.customFormations", "probescanning.selectedFormationID"]);
            }
            Part::Shortcuts => to.take_entry(from, "cmd", "customCmds"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    /// Replace the destination files with the source files.
    Everything,
    /// Change only these parts of the destination files.
    Parts(Vec<Part>),
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub source_dir: PathBuf,
    pub dest_dir: PathBuf,
    pub source_char: i64,
    pub source_account: Option<i64>,
    pub dest_chars: Vec<i64>,
    /// Destination accounts that take the source account's settings.
    pub dest_accounts: Vec<i64>,
    pub what: What,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub char_files: usize,
    pub account_files: usize,
    /// Destinations left alone because they have no settings file to change a part of.
    pub skipped: Vec<(Kind, i64)>,
    pub backup: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub kind: Kind,
    pub id: i64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("settings");
    let tmp = path.with_file_name(format!(".{name}.spai-tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Backs up whatever `targets` are about to replace, then writes them. Nothing is written when
/// the backup fails.
pub fn commit(backups: &Path, mut info: Meta, targets: &[Target]) -> Result<Option<Meta>, String> {
    if targets.is_empty() {
        return Ok(None);
    }
    // A full disk would leave a half-written settings file where a working one was.
    if crate::disk::level() != crate::disk::Level::Normal {
        return Err(tr!("Not enough free disk space to change settings safely. Free some space and try again.").to_owned());
    }
    info.automatic = true;
    let sources: Vec<(Kind, i64, PathBuf)> = targets.iter().map(|t| (t.kind, t.id, t.path.clone())).collect();
    let made = backup::create(backups, info, &sources)?;
    for t in targets {
        if let Some(dir) = t.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| trf!("Could not create {dir}: {err}", dir = dir.display(), err = e))?;
        }
        write_atomic(&t.path, &t.bytes)
            .map_err(|e| trf!("Writing {file} failed: {err}", file = t.path.display(), err = e))?;
    }
    Ok(made)
}

fn sorted(ids: &[i64]) -> Vec<i64> {
    let mut v = ids.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

/// Works out every file the plan writes, without touching the disk beyond reading.
pub fn prepare(plan: &Plan) -> Result<(Vec<Target>, Vec<(Kind, i64)>), String> {
    let src_char = plan.source_dir.join(Kind::Char.file_name(plan.source_char));
    let char_bytes = std::fs::read(&src_char)
        .map_err(|_| tr!("The source character has no settings file in this profile.").to_owned())?;
    let account_bytes = match plan.source_account.filter(|_| !plan.dest_accounts.is_empty()) {
        Some(id) => Some(
            std::fs::read(plan.source_dir.join(Kind::Account.file_name(id)))
                .map_err(|_| trf!("Account {id} has no settings file in this profile.", id = id))?,
        ),
        None => None,
    };
    let parts = match &plan.what {
        What::Everything => None,
        What::Parts(parts) => Some(parts.clone()),
    };
    let parse = |bytes: &[u8]| {
        SettingsFile::parse(bytes).map_err(|e| trf!("A settings file could not be read: {err}", err = e))
    };
    let mut targets = Vec::new();
    let mut skipped = Vec::new();
    let mut each = |kind: Kind, ids: &[i64], bytes: &[u8], same: Option<i64>| -> Result<(), String> {
        let source = match &parts {
            Some(_) => Some(parse(bytes)?),
            None => None,
        };
        let wanted: Vec<Part> = parts.iter().flatten().copied().filter(|p| p.kind() == kind).collect();
        if parts.is_some() && wanted.is_empty() {
            return Ok(());
        }
        for id in sorted(ids) {
            let path = plan.dest_dir.join(kind.file_name(id));
            if same == Some(id) && plan.dest_dir == plan.source_dir {
                continue;
            }
            let out = match &source {
                None => bytes.to_vec(),
                Some(source) => {
                    let Ok(existing) = std::fs::read(&path) else {
                        skipped.push((kind, id));
                        continue;
                    };
                    let mut dest = parse(&existing)?;
                    for part in &wanted {
                        part.apply(source, &mut dest);
                    }
                    dest.bytes()?
                }
            };
            targets.push(Target { kind, id, path, bytes: out });
        }
        Ok(())
    };
    each(Kind::Char, &plan.dest_chars, &char_bytes, Some(plan.source_char))?;
    if let Some(bytes) = &account_bytes {
        each(Kind::Account, &plan.dest_accounts, bytes, plan.source_account)?;
    }
    Ok((targets, skipped))
}

pub fn run(plan: &Plan, backups: &Path, info: Meta) -> Result<Report, String> {
    let (targets, skipped) = prepare(plan)?;
    let made = commit(backups, info, &targets)?;
    Ok(Report {
        char_files: targets.iter().filter(|t| t.kind == Kind::Char).count(),
        account_files: targets.iter().filter(|t| t.kind == Kind::Account).count(),
        skipped,
        backup: made.map(|m| m.id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evesettings::marshal::Node;

    struct Tmp(PathBuf);

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn file(marker: i64, channels: &[(&str, &str, &str)]) -> SettingsFile {
        let mut f = SettingsFile::empty();
        f.set("ui", "neocomWidth", Node::int(marker));
        f.set("ui", "private", Node::int(marker));
        f.set("windows", "shipuialignleftoffset", Node::int(marker));
        f.set("overview", "useSmallText", Node::bool(marker == 1));
        f.set("cmd", "customCmds", Node::dict(vec![(Node::text("CmdX"), Node::int(marker))]));
        chat::write(
            &mut f,
            &channels
                .iter()
                .map(|c| chat::Channel { key: c.0.into(), id: c.1.into(), name: c.2.into() })
                .collect::<Vec<_>>(),
        );
        f
    }

    fn tree(tag: &str) -> (Tmp, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("evespai-copy-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let live = dir.join("settings_Default");
        std::fs::create_dir_all(&live).unwrap();
        let put = |name: &str, f: &SettingsFile| std::fs::write(live.join(name), f.bytes().unwrap()).unwrap();
        put("core_char_100.dat", &file(1, &[("corp", "corp_1", "Corp"), ("player_a", "player_a", "intel"), ("local", "local_5", "Local")]));
        put("core_char_200.dat", &file(2, &[("local", "local_9", "Local"), ("corp", "corp_2", "Corp"), ("player_z", "player_z", "old")]));
        put("core_user_10.dat", &file(1, &[]));
        put("core_user_20.dat", &file(2, &[]));
        let backups = dir.join("backups");
        (Tmp(dir), live, backups)
    }

    fn plan(live: &Path, what: What) -> Plan {
        Plan {
            source_dir: live.to_path_buf(),
            dest_dir: live.to_path_buf(),
            source_char: 100,
            source_account: Some(10),
            dest_chars: vec![200, 300, 100],
            dest_accounts: vec![20, 10],
            what,
        }
    }

    fn load(live: &Path, name: &str) -> SettingsFile {
        SettingsFile::read(&live.join(name)).unwrap()
    }

    fn int(f: &SettingsFile, section: &str, key: &str) -> Option<i64> {
        f.get(section, key).and_then(|v| v.int())
    }

    #[test]
    fn everything_replaces_files_and_backs_up_what_was_there() {
        let (_t, live, backups) = tree("all");
        let report = run(&plan(&live, What::Everything), &backups, Meta::default()).unwrap();
        // 200 and the new 300 are written; the source itself is skipped.
        assert_eq!((report.char_files, report.account_files), (2, 1));
        assert_eq!(std::fs::read(live.join("core_char_200.dat")).unwrap(), std::fs::read(live.join("core_char_100.dat")).unwrap());
        assert!(live.join("core_char_300.dat").exists());
        assert_eq!(int(&load(&live, "core_user_20.dat"), "ui", "private"), Some(1));

        let saved = backup::list(&backups);
        assert_eq!(saved.len(), 1);
        assert!(saved[0].automatic);
        assert!(saved[0].has(Kind::Char, 200) && saved[0].has(Kind::Account, 20) && !saved[0].has(Kind::Char, 300));
        let old = SettingsFile::read(&backup::file_path(&backups, &saved[0], Kind::Char, 200)).unwrap();
        assert_eq!(int(&old, "ui", "private"), Some(2));
        assert!(!live.read_dir().unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains("spai-tmp")));
    }

    #[test]
    fn parts_change_only_what_was_picked() {
        let (_t, live, backups) = tree("parts");
        let what = What::Parts(vec![Part::Neocom, Part::Chat, Part::Shortcuts]);
        let report = run(&plan(&live, what), &backups, Meta::default()).unwrap();
        assert_eq!((report.char_files, report.account_files), (1, 1));
        assert_eq!(report.skipped, vec![(Kind::Char, 300)]);

        let dest = load(&live, "core_char_200.dat");
        assert_eq!(int(&dest, "ui", "neocomWidth"), Some(1));
        assert_eq!(int(&dest, "ui", "private"), Some(2));
        assert_eq!(int(&dest, "windows", "shipuialignleftoffset"), Some(2));
        // The source's order, the destination's own corp and local ids, and no channel of its own
        // that the source did not have.
        let ids: Vec<String> = chat::read(&dest).unwrap().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["corp_2", "player_a", "local_9"]);

        let account = load(&live, "core_user_20.dat");
        assert_eq!(account.get("cmd", "customCmds").unwrap().get("CmdX").unwrap().int(), Some(1));
        assert_eq!(account.get("overview", "useSmallText").unwrap().truthy(), Some(false));
    }

    #[test]
    fn account_parts_alone_leave_character_files_untouched() {
        let (_t, live, backups) = tree("acct");
        let before = std::fs::read(live.join("core_char_200.dat")).unwrap();
        let report = run(&plan(&live, What::Parts(vec![Part::Overview])), &backups, Meta::default()).unwrap();
        assert_eq!((report.char_files, report.account_files), (0, 1));
        assert_eq!(std::fs::read(live.join("core_char_200.dat")).unwrap(), before);
        assert_eq!(load(&live, "core_user_20.dat").get("overview", "useSmallText").unwrap().truthy(), Some(true));
    }

    #[test]
    fn a_missing_source_writes_nothing() {
        let (_t, live, backups) = tree("nosrc");
        let mut p = plan(&live, What::Everything);
        p.source_char = 999;
        assert!(run(&p, &backups, Meta::default()).is_err());
        assert_eq!(int(&load(&live, "core_char_200.dat"), "ui", "private"), Some(2));
        assert!(backup::list(&backups).is_empty());
    }

    #[test]
    fn another_profile_takes_the_source_id_too() {
        let (_t, live, backups) = tree("profile");
        let mut p = plan(&live, What::Everything);
        p.dest_dir = live.with_file_name("settings_Fresh");
        let report = run(&p, &backups, Meta::default()).unwrap();
        assert_eq!((report.char_files, report.account_files), (3, 2));
        assert!(p.dest_dir.join("core_char_100.dat").exists());
        assert_eq!(report.backup, None);
    }
}
