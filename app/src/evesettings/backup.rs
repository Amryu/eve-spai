//! Backups of EVE settings files, kept in this app's own data directory so they outlive a
//! reinstall of the game and never sit among the files the client reads.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Automatic backups kept; named ones are never dropped.
pub const KEEP_AUTOMATIC: usize = 40;
const META: &str = "backup.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Char,
    Account,
}

impl Kind {
    pub fn file_name(self, id: i64) -> String {
        match self {
            Kind::Char => format!("core_char_{id}.dat"),
            Kind::Account => format!("core_user_{id}.dat"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    pub kind: Kind,
    pub id: i64,
    pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// Directory name under the backup root.
    #[serde(skip)]
    pub id: String,
    /// The user's label; empty on an automatic backup.
    #[serde(default)]
    pub name: String,
    pub created: i64,
    #[serde(default)]
    pub automatic: bool,
    /// What was about to happen when an automatic backup was taken.
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub profile: String,
    /// The character a named backup was made of.
    #[serde(default)]
    pub character_id: Option<i64>,
    #[serde(default)]
    pub character_name: String,
    #[serde(default)]
    pub files: Vec<Stored>,
}

impl Meta {
    pub fn title(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if !self.reason.is_empty() {
            self.reason.clone()
        } else {
            tr!("Automatic backup").to_owned()
        }
    }

    pub fn has(&self, kind: Kind, id: i64) -> bool {
        self.files.iter().any(|f| f.kind == kind && f.id == id)
    }

    pub fn first(&self, kind: Kind) -> Option<i64> {
        self.files.iter().find(|f| f.kind == kind).map(|f| f.id)
    }

    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

pub fn root() -> Option<PathBuf> {
    crate::store::data_dir().ok().map(|d| d.join("settings-backups"))
}

pub fn file_path(root: &Path, meta: &Meta, kind: Kind, id: i64) -> PathBuf {
    root.join(&meta.id).join(kind.file_name(id))
}

pub fn list(root: &Path) -> Vec<Meta> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Meta> = entries
        .flatten()
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path().join(META)).ok()?;
            let mut meta: Meta = serde_json::from_str(&text).ok()?;
            meta.id = e.file_name().to_str()?.to_owned();
            Some(meta)
        })
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.id.cmp(&a.id)));
    out
}

fn save_meta(root: &Path, meta: &Meta) -> Result<(), String> {
    let text = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    std::fs::write(root.join(&meta.id).join(META), text).map_err(|e| e.to_string())
}

/// Copies `sources` into a new backup. Sources that do not exist are skipped; a backup with no
/// file at all is not created and `Ok(None)` comes back.
pub fn create(root: &Path, mut meta: Meta, sources: &[(Kind, i64, PathBuf)]) -> Result<Option<Meta>, String> {
    let present: Vec<&(Kind, i64, PathBuf)> = sources.iter().filter(|s| s.2.is_file()).collect();
    if present.is_empty() {
        return Ok(None);
    }
    meta.created = crate::clock::utc().timestamp();
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stem = crate::clock::utc().format("%Y%m%d-%H%M%S").to_string();
    let dir = (0..1000)
        .map(|n| if n == 0 { stem.clone() } else { format!("{stem}-{n}") })
        .find(|name| std::fs::create_dir(root.join(name)).is_ok())
        .ok_or_else(|| trf!("Could not create a backup folder in {dir}.", dir = root.display()))?;
    meta.id = dir;
    meta.files.clear();
    for (kind, id, path) in present {
        if meta.has(*kind, *id) {
            continue;
        }
        let to = file_path(root, &meta, *kind, *id);
        let copied = std::fs::copy(path, &to).map_err(|e| {
            let _ = std::fs::remove_dir_all(root.join(&meta.id));
            trf!("Backing up {file} failed: {err}", file = path.display(), err = e)
        })?;
        meta.files.push(Stored { kind: *kind, id: *id, size: copied });
    }
    save_meta(root, &meta).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(root.join(&meta.id));
    })?;
    if meta.automatic {
        prune(root, KEEP_AUTOMATIC);
    }
    Ok(Some(meta))
}

pub fn rename(root: &Path, id: &str, name: &str) -> Result<(), String> {
    let mut meta = list(root).into_iter().find(|m| m.id == id).ok_or_else(|| tr!("That backup is gone.").to_owned())?;
    meta.name = name.trim().to_owned();
    // Naming a backup is how it is kept: it stops counting as automatic.
    meta.automatic = meta.automatic && meta.name.is_empty();
    save_meta(root, &meta)
}

pub fn delete(root: &Path, id: &str) -> Result<(), String> {
    // The id comes from a directory listing, but it still must not walk out of the root.
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        return Err(tr!("That backup is gone.").to_owned());
    }
    std::fs::remove_dir_all(root.join(id)).map_err(|e| e.to_string())
}

fn prune(root: &Path, keep: usize) {
    for old in list(root).into_iter().filter(|m| m.automatic).skip(keep) {
        let _ = delete(root, &old.id);
    }
}

/// A backup another tool, or an older version of this one, left next to the settings files:
/// `core_char_<id>_<tool>_backup_<n>.dat`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sidecar {
    pub path: PathBuf,
    pub kind: Kind,
    pub id: i64,
    pub number: u32,
    pub modified: i64,
    pub size: u64,
}

pub fn parse_sidecar(name: &str) -> Option<(Kind, i64, u32)> {
    let (kind, rest) = match name.strip_prefix("core_char_") {
        Some(rest) => (Kind::Char, rest),
        None => (Kind::Account, name.strip_prefix("core_user_")?),
    };
    let rest = rest.strip_suffix(".dat")?;
    let (id, tail) = rest.split_once('_')?;
    let (tool, number) = tail.rsplit_once("_backup_")?;
    if tool.is_empty() || !tool.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    Some((kind, id.parse().ok()?, number.parse().ok()?))
}

pub fn sidecars(profile_dir: &Path) -> Vec<Sidecar> {
    let Ok(entries) = std::fs::read_dir(profile_dir) else {
        return Vec::new();
    };
    let mut out: Vec<Sidecar> = entries
        .flatten()
        .filter_map(|e| {
            let (kind, id, number) = parse_sidecar(e.file_name().to_str()?)?;
            let meta = e.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            Some(Sidecar { path: e.path(), kind, id, number, modified, size: meta.len() })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.path.cmp(&b.path)));
    out
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

    fn tmp(tag: &str) -> Tmp {
        let dir = std::env::temp_dir().join(format!("evespai-backup-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("live")).unwrap();
        Tmp(dir)
    }

    #[test]
    fn create_list_rename_delete() {
        let t = tmp("crud");
        let live = t.0.join("live");
        std::fs::write(live.join("core_char_100.dat"), b"char").unwrap();
        std::fs::write(live.join("core_user_10.dat"), b"account").unwrap();
        let root = t.0.join("backups");
        let sources = vec![
            (Kind::Char, 100, live.join("core_char_100.dat")),
            (Kind::Account, 10, live.join("core_user_10.dat")),
            (Kind::Char, 999, live.join("core_char_999.dat")),
        ];
        let meta = Meta { name: "PvP".into(), character_id: Some(100), character_name: "Pilot".into(), ..Meta::default() };
        let made = create(&root, meta, &sources).unwrap().unwrap();
        assert_eq!(made.files.len(), 2);
        assert!(made.has(Kind::Char, 100) && made.has(Kind::Account, 10) && !made.has(Kind::Char, 999));
        assert_eq!(std::fs::read(file_path(&root, &made, Kind::Char, 100)).unwrap(), b"char");

        let listed = list(&root);
        assert_eq!(listed, vec![made.clone()]);
        assert_eq!(listed[0].title(), "PvP");

        rename(&root, &made.id, " Fleet setup ").unwrap();
        assert_eq!(list(&root)[0].name, "Fleet setup");
        delete(&root, &made.id).unwrap();
        assert!(list(&root).is_empty());
        assert!(delete(&root, "../live").is_err());
        assert!(live.join("core_char_100.dat").exists());
    }

    #[test]
    fn nothing_to_back_up_makes_no_backup() {
        let t = tmp("empty");
        let root = t.0.join("backups");
        let made = create(&root, Meta::default(), &[(Kind::Char, 1, t.0.join("live/nope.dat"))]).unwrap();
        assert_eq!(made, None);
        assert!(list(&root).is_empty());
    }

    #[test]
    fn automatic_backups_are_pruned_and_named_ones_kept() {
        let t = tmp("prune");
        let live = t.0.join("live");
        std::fs::write(live.join("core_char_100.dat"), b"char").unwrap();
        let root = t.0.join("backups");
        let src = vec![(Kind::Char, 100, live.join("core_char_100.dat"))];
        let named = create(&root, Meta { name: "Keep".into(), ..Meta::default() }, &src).unwrap().unwrap();
        for _ in 0..KEEP_AUTOMATIC + 3 {
            create(&root, Meta { automatic: true, reason: "Before a copy".into(), ..Meta::default() }, &src).unwrap();
        }
        let all = list(&root);
        assert_eq!(all.iter().filter(|m| m.automatic).count(), KEEP_AUTOMATIC);
        assert!(all.iter().any(|m| m.id == named.id));
    }

    #[test]
    fn naming_an_automatic_backup_keeps_it() {
        let t = tmp("promote");
        let live = t.0.join("live");
        std::fs::write(live.join("core_char_100.dat"), b"char").unwrap();
        let root = t.0.join("backups");
        let src = vec![(Kind::Char, 100, live.join("core_char_100.dat"))];
        let auto = create(&root, Meta { automatic: true, ..Meta::default() }, &src).unwrap().unwrap();
        rename(&root, &auto.id, "Before the patch").unwrap();
        assert!(!list(&root)[0].automatic);
    }

    #[test]
    fn sidecar_names() {
        assert_eq!(parse_sidecar("core_char_123_spai_backup_2.dat"), Some((Kind::Char, 123, 2)));
        assert_eq!(parse_sidecar("core_user_45_other_backup_10.dat"), Some((Kind::Account, 45, 10)));
        assert_eq!(parse_sidecar("core_char_123.dat"), None);
        assert_eq!(parse_sidecar("core_char_123__backup_1.dat"), None);
        assert_eq!(parse_sidecar("core_char_abc_x_backup_1.dat"), None);
        assert_eq!(parse_sidecar("core_public__.yaml"), None);
    }

    #[test]
    fn sidecars_are_found_newest_first() {
        let t = tmp("sidecars");
        let live = t.0.join("live");
        for name in ["core_char_100.dat", "core_char_100_spai_backup_1.dat", "core_user_10_other_backup_3.dat"] {
            std::fs::write(live.join(name), b"x").unwrap();
        }
        let found = sidecars(&live);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|s| s.kind == Kind::Account && s.id == 10 && s.number == 3));
    }
}
