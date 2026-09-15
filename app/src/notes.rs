//! The user's notes and tags on systems and hostile pilots, kept in a tree of folders.
//!
//! Every tag and note lives in a folder; there is no root to save into, so a write without one lands
//! in a "Default" folder made on demand. Only top-level folders switch online and offline, and an
//! offline folder is ignored everywhere, its tags included. Folders carry a uuid so an exported
//! folder can be imported again, over the original or into it.
//!
//! [`NoteBook`] is the tree, edited only through [`NoteBook::apply`]. [`NotesView`] is what the tree
//! means right now: the active tags and, per system and pilot, the merged tags and notes of every
//! online folder. Cards, the map, the alert engine, the overlay and the web page all read the view.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub const NOTE_MAX: usize = 2000;
pub const NAME_MAX: usize = 64;
pub const TAGS_PER_FOLDER: usize = 64;
pub const ENTRIES_PER_FOLDER: usize = 10_000;
pub const FOLDERS_MAX: usize = 256;
pub const DEPTH_MAX: usize = 8;
pub const PILOT_NAME_MAX: usize = 37;
/// In an alert rule's tag list: any tag at all.
pub const ANY_TAG: &str = "*";
pub const DEFAULT_FOLDER: &str = "Default";
/// The built-in docking tags, which the jump planner reads as where capitals can sit.
pub const SUPER_DOCKING: &str = "d:sys:super-docking";
pub const CAPITAL_DOCKING: &str = "d:sys:capital-docking";
const EXPORT_FORMAT: &str = "eve-spai-notes";
const COMPRESSED_PREFIX: &str = "SPAINOTES1:";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoteKind {
    System,
    Pilot,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Subject {
    System(i64),
    /// The name rides along because the alert engine only ever sees names: it fires before ESI has
    /// resolved who a pilot is.
    Pilot { id: i64, name: String },
}

impl Subject {
    pub fn kind(&self) -> NoteKind {
        match self {
            Self::System(_) => NoteKind::System,
            Self::Pilot { .. } => NoteKind::Pilot,
        }
    }

    pub fn key(&self) -> i64 {
        match self {
            Self::System(id) | Self::Pilot { id, .. } => *id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub kind: NoteKind,
    pub name: String,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entry {
    /// Display name. For a pilot it is refreshed on every edit, so a renamed character keeps its old
    /// name here until then.
    pub name: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub updated: i64,
}

impl Entry {
    fn is_empty(&self) -> bool {
        self.note.is_empty() && self.tags.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// Read on top-level folders only; a subfolder follows its top-level folder.
    #[serde(default = "yes")]
    pub online: bool,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub systems: BTreeMap<i64, Entry>,
    #[serde(default)]
    pub pilots: BTreeMap<i64, Entry>,
    #[serde(default)]
    pub children: Vec<Folder>,
}

fn yes() -> bool {
    true
}

impl Folder {
    pub fn new(name: &str) -> Self {
        Folder {
            id: new_id(),
            name: name.to_owned(),
            online: true,
            tags: Vec::new(),
            systems: BTreeMap::new(),
            pilots: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    pub fn entries(&self, kind: NoteKind) -> &BTreeMap<i64, Entry> {
        match kind {
            NoteKind::System => &self.systems,
            NoteKind::Pilot => &self.pilots,
        }
    }

    fn entries_mut(&mut self, kind: NoteKind) -> &mut BTreeMap<i64, Entry> {
        match kind {
            NoteKind::System => &mut self.systems,
            NoteKind::Pilot => &mut self.pilots,
        }
    }

    fn walk<'a>(&'a self, out: &mut Vec<&'a Folder>) {
        out.push(self);
        for c in &self.children {
            c.walk(out);
        }
    }

    fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Folder)) {
        f(self);
        for c in &mut self.children {
            c.walk_mut(f);
        }
    }

    fn depth(&self) -> usize {
        1 + self.children.iter().map(Folder::depth).max().unwrap_or(0)
    }

    fn count(&self) -> usize {
        1 + self.children.iter().map(Folder::count).sum::<usize>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportMode {
    /// Refused when the folder's uuid is already present, so the page can ask which of the other two.
    Add,
    /// Drop the existing folder and keep only what was imported.
    Overwrite,
    /// Keep both; where an item exists on both sides the import wins.
    Merge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderExport {
    pub format: String,
    pub version: u32,
    pub folder: Folder,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotesOp {
    SetEntry { folder: String, subject: Subject, note: String, tags: Vec<String> },
    /// One tag on or off in one folder, leaving the note alone, so a quick toggle cannot overwrite a
    /// note being edited somewhere else.
    SetTag { folder: String, subject: Subject, tag: String, on: bool },
    RemoveEntry { folder: String, subject: Subject },
    MoveEntry { from: String, to: String, subject: Subject },
    /// `id: None` creates a tag in `folder`; with an id it renames or recolours that tag wherever it is.
    PutTag { folder: String, id: Option<String>, kind: NoteKind, name: String, color: [u8; 3] },
    DeleteTag { id: String },
    MoveTag { id: String, to: String },
    CreateFolder { parent: Option<String>, name: String },
    RenameFolder { id: String, name: String },
    MoveFolder { id: String, parent: Option<String> },
    SetOnline { id: String, on: bool },
    DeleteFolder { id: String },
    Import { export: FolderExport, mode: ImportMode, parent: Option<String> },
}

/// What an applied op touched, so the caller can remember the folder as the next quick-edit target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub folder: Option<String>,
    pub tag: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteBook {
    pub rev: u64,
    pub folders: Vec<Folder>,
}

impl NoteBook {
    pub fn all(&self) -> Vec<&Folder> {
        let mut out = Vec::new();
        for f in &self.folders {
            f.walk(&mut out);
        }
        out
    }

    pub fn find(&self, id: &str) -> Option<&Folder> {
        fn go<'a>(fs: &'a [Folder], id: &str) -> Option<&'a Folder> {
            fs.iter().find_map(|f| if f.id == id { Some(f) } else { go(&f.children, id) })
        }
        go(&self.folders, id)
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut Folder> {
        fn go<'a>(fs: &'a mut [Folder], id: &str) -> Option<&'a mut Folder> {
            for f in fs {
                if f.id == id {
                    return Some(f);
                }
                if let Some(hit) = go(&mut f.children, id) {
                    return Some(hit);
                }
            }
            None
        }
        go(&mut self.folders, id)
    }

    /// Names from the top-level folder down, for a breadcrumb.
    pub fn path(&self, id: &str) -> Vec<&str> {
        fn go<'a>(fs: &'a [Folder], id: &str, trail: &mut Vec<&'a str>) -> bool {
            for f in fs {
                trail.push(&f.name);
                if f.id == id || go(&f.children, id, trail) {
                    return true;
                }
                trail.pop();
            }
            false
        }
        let mut trail = Vec::new();
        go(&self.folders, id, &mut trail);
        trail
    }

    pub fn is_top(&self, id: &str) -> bool {
        self.folders.iter().any(|f| f.id == id)
    }

    fn detach(&mut self, id: &str) -> Option<Folder> {
        fn go(fs: &mut Vec<Folder>, id: &str) -> Option<Folder> {
            if let Some(i) = fs.iter().position(|f| f.id == id) {
                return Some(fs.remove(i));
            }
            fs.iter_mut().find_map(|f| go(&mut f.children, id))
        }
        go(&mut self.folders, id)
    }

    fn all_tags(&self) -> impl Iterator<Item = &Tag> {
        default_tags().iter().chain(self.all().into_iter().flat_map(|f| f.tags.iter()))
    }

    pub fn tag(&self, id: &str) -> Option<&Tag> {
        self.all_tags().find(|t| t.id == id)
    }

    /// The folder a write without a usable folder falls back to, made if it does not exist.
    fn resolve_folder(&mut self, id: &str) -> String {
        if !id.is_empty() && self.find(id).is_some() {
            return id.to_owned();
        }
        if let Some(f) = self.folders.iter().find(|f| f.name == DEFAULT_FOLDER) {
            return f.id.clone();
        }
        let f = Folder::new(DEFAULT_FOLDER);
        let id = f.id.clone();
        self.folders.push(f);
        id
    }

    /// The folder a quick edit should go to, without creating anything: the view shows the target's
    /// state before the first write has made it.
    pub fn target(&self, preferred: &str) -> String {
        if !preferred.is_empty() && self.find(preferred).is_some() {
            return preferred.to_owned();
        }
        self.folders
            .iter()
            .find(|f| f.name == DEFAULT_FOLDER)
            .map(|f| f.id.clone())
            .unwrap_or_default()
    }

    /// Validates and applies one edit. Input is bounded here rather than in the UI because the web
    /// page can post anything.
    pub fn apply(
        &mut self,
        op: NotesOp,
        now: i64,
        system_name: &dyn Fn(i64) -> Option<String>,
    ) -> Result<Applied, &'static str> {
        let mut scratch = self.clone();
        let applied = scratch.apply_inner(op, now, system_name)?;
        scratch.rev = self.rev + 1;
        *self = scratch;
        Ok(applied)
    }

    fn apply_inner(
        &mut self,
        op: NotesOp,
        now: i64,
        system_name: &dyn Fn(i64) -> Option<String>,
    ) -> Result<Applied, &'static str> {
        let mut applied = Applied::default();
        match op {
            NotesOp::SetEntry { folder, subject, note, tags } => {
                let name = subject_name(&subject, system_name)?;
                let note = clean_note(&note)?;
                let kind = subject.kind();
                let tags = self.checked_tags(kind, tags)?;
                let folder = self.resolve_folder(&folder);
                let f = self.find_mut(&folder).expect("resolved");
                let entry = Entry { name, note, tags, updated: now };
                if entry.is_empty() {
                    f.entries_mut(kind).remove(&subject.key());
                } else {
                    put_entry(f, kind, subject.key(), entry)?;
                }
                applied.folder = Some(folder);
            }
            NotesOp::SetTag { folder, subject, tag, on } => {
                let name = subject_name(&subject, system_name)?;
                let kind = subject.kind();
                self.checked_tags(kind, vec![tag.clone()])?;
                let folder = self.resolve_folder(&folder);
                let f = self.find_mut(&folder).expect("resolved");
                let mut e = f.entries(kind).get(&subject.key()).cloned().unwrap_or_default();
                e.name = name;
                e.updated = now;
                e.tags.retain(|t| *t != tag);
                if on {
                    e.tags.push(tag);
                }
                if e.is_empty() {
                    f.entries_mut(kind).remove(&subject.key());
                } else {
                    put_entry(f, kind, subject.key(), e)?;
                }
                applied.folder = Some(folder);
            }
            NotesOp::RemoveEntry { folder, subject } => {
                let f = self.find_mut(&folder).ok_or("unknown folder")?;
                f.entries_mut(subject.kind()).remove(&subject.key()).ok_or("no such entry")?;
                applied.folder = Some(folder);
            }
            NotesOp::MoveEntry { from, to, subject } => {
                if from == to {
                    return Ok(applied);
                }
                let kind = subject.kind();
                self.find(&to).ok_or("unknown folder")?;
                let e = self
                    .find_mut(&from)
                    .ok_or("unknown folder")?
                    .entries_mut(kind)
                    .remove(&subject.key())
                    .ok_or("no such entry")?;
                put_entry(self.find_mut(&to).expect("checked"), kind, subject.key(), e)?;
                applied.folder = Some(to);
            }
            NotesOp::PutTag { folder, id, kind, name, color } => {
                let name = clean_name(&name)?;
                match id {
                    Some(id) => {
                        if id.starts_with("d:") {
                            return Err("default tags are fixed");
                        }
                        let home = self
                            .all()
                            .into_iter()
                            .find(|f| f.tags.iter().any(|t| t.id == id))
                            .map(|f| f.id.clone())
                            .ok_or("unknown tag")?;
                        let f = self.find_mut(&home).expect("found");
                        if f.tags.iter().any(|t| t.id != id && t.kind == kind && t.name.eq_ignore_ascii_case(&name)) {
                            return Err("tag name taken");
                        }
                        let t = f.tags.iter_mut().find(|t| t.id == id).expect("found");
                        if t.kind != kind {
                            return Err("tag kind is fixed");
                        }
                        t.name = name;
                        t.color = color;
                        applied.tag = Some(id);
                    }
                    None => {
                        if default_tags().iter().any(|t| t.kind == kind && t.name.eq_ignore_ascii_case(&name)) {
                            return Err("tag name taken");
                        }
                        let folder = self.resolve_folder(&folder);
                        let f = self.find_mut(&folder).expect("resolved");
                        if f.tags.iter().any(|t| t.kind == kind && t.name.eq_ignore_ascii_case(&name)) {
                            return Err("tag name taken");
                        }
                        if f.tags.len() >= TAGS_PER_FOLDER {
                            return Err("too many tags");
                        }
                        let id = new_id();
                        f.tags.push(Tag { id: id.clone(), kind, name, color });
                        applied.tag = Some(id);
                        applied.folder = Some(folder);
                    }
                }
            }
            NotesOp::DeleteTag { id } => {
                let mut found = false;
                for f in &mut self.folders {
                    f.walk_mut(&mut |f| {
                        let before = f.tags.len();
                        f.tags.retain(|t| t.id != id);
                        found |= f.tags.len() != before;
                    });
                }
                if !found {
                    return Err("unknown tag");
                }
                self.prune_dangling();
            }
            NotesOp::MoveTag { id, to } => {
                self.find(&to).ok_or("unknown folder")?;
                let mut tag = None;
                for f in &mut self.folders {
                    f.walk_mut(&mut |f| {
                        if let Some(i) = f.tags.iter().position(|t| t.id == id) {
                            tag = Some(f.tags.remove(i));
                        }
                    });
                }
                let tag = tag.ok_or("unknown tag")?;
                let f = self.find_mut(&to).expect("checked");
                if f.tags.len() >= TAGS_PER_FOLDER {
                    return Err("too many tags");
                }
                f.tags.push(tag);
                applied.folder = Some(to);
            }
            NotesOp::CreateFolder { parent, name } => {
                let name = clean_name(&name)?;
                if self.all().len() >= FOLDERS_MAX {
                    return Err("too many folders");
                }
                let f = Folder::new(&name);
                applied.folder = Some(f.id.clone());
                match parent {
                    None => self.folders.push(f),
                    Some(p) => {
                        if self.path(&p).len() >= DEPTH_MAX {
                            return Err("folders nested too deep");
                        }
                        self.find_mut(&p).ok_or("unknown folder")?.children.push(f);
                    }
                }
            }
            NotesOp::RenameFolder { id, name } => {
                let name = clean_name(&name)?;
                self.find_mut(&id).ok_or("unknown folder")?.name = name;
            }
            NotesOp::MoveFolder { id, parent } => {
                let moving = self.find(&id).ok_or("unknown folder")?;
                if let Some(p) = &parent {
                    let mut inside = Vec::new();
                    moving.walk(&mut inside);
                    if inside.iter().any(|f| f.id == *p) {
                        return Err("a folder cannot move into itself");
                    }
                    if self.find(p).is_none() {
                        return Err("unknown folder");
                    }
                    if self.path(p).len() + moving.depth() > DEPTH_MAX {
                        return Err("folders nested too deep");
                    }
                }
                let f = self.detach(&id).expect("found");
                match parent {
                    None => self.folders.push(f),
                    Some(p) => self.find_mut(&p).expect("checked").children.push(f),
                }
            }
            NotesOp::SetOnline { id, on } => {
                let f = self.folders.iter_mut().find(|f| f.id == id).ok_or("only top-level folders go offline")?;
                f.online = on;
            }
            NotesOp::DeleteFolder { id } => {
                self.detach(&id).ok_or("unknown folder")?;
                self.prune_dangling();
            }
            NotesOp::Import { export, mode, parent } => {
                applied.folder = Some(self.import(export, mode, parent, system_name)?);
            }
        }
        Ok(applied)
    }

    fn checked_tags(&self, kind: NoteKind, tags: Vec<String>) -> Result<Vec<String>, &'static str> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for t in tags {
            match self.tag(&t) {
                Some(tag) if tag.kind == kind => {
                    if seen.insert(t.clone()) {
                        out.push(t);
                    }
                }
                _ => return Err("unknown tag"),
            }
        }
        Ok(out)
    }

    /// Strips references to tags that no longer exist and drops entries left with nothing in them.
    fn prune_dangling(&mut self) {
        let known: HashSet<String> = self.all_tags().map(|t| t.id.clone()).collect();
        for f in &mut self.folders {
            f.walk_mut(&mut |f| {
                for map in [&mut f.systems, &mut f.pilots] {
                    for e in map.values_mut() {
                        e.tags.retain(|t| known.contains(t));
                    }
                    map.retain(|_, e| !e.is_empty());
                }
            });
        }
    }

    fn import(
        &mut self,
        export: FolderExport,
        mode: ImportMode,
        parent: Option<String>,
        system_name: &dyn Fn(i64) -> Option<String>,
    ) -> Result<String, &'static str> {
        if export.format != EXPORT_FORMAT {
            return Err("not an EVE Spai notes export");
        }
        if export.version > 1 {
            return Err("made by a newer EVE Spai");
        }
        let mut incoming = sanitize(export.folder, system_name)?;
        let id = incoming.id.clone();
        let existing = self.find(&id).is_some();

        // Uuids from the import that already exist outside the folder being replaced or merged would
        // make two folders or two tags with one identity. The existing ones keep theirs.
        let (keep_folders, keep_tags): (HashSet<String>, HashSet<String>) = {
            let mut inside = Vec::new();
            if let Some(f) = self.find(&id) {
                f.walk(&mut inside);
            }
            let inside_ids: HashSet<&str> = inside.iter().map(|f| f.id.as_str()).collect();
            let outside: Vec<&Folder> =
                self.all().into_iter().filter(|f| !inside_ids.contains(f.id.as_str())).collect();
            (
                outside.iter().map(|f| f.id.clone()).collect(),
                outside.iter().flat_map(|f| f.tags.iter().map(|t| t.id.clone())).collect(),
            )
        };
        incoming.walk_mut(&mut |f| {
            if keep_folders.contains(&f.id) {
                f.id = new_id();
            }
            f.tags.retain(|t| !keep_tags.contains(&t.id));
        });

        match (existing, mode) {
            (true, ImportMode::Add) => return Err("folder exists"),
            (true, ImportMode::Overwrite) => {
                let slot = self.find_mut(&id).expect("exists");
                incoming.online = slot.online;
                *slot = incoming;
            }
            (true, ImportMode::Merge) => merge(self.find_mut(&id).expect("exists"), incoming),
            (false, _) => {
                if self.all().len() + incoming.count() > FOLDERS_MAX {
                    return Err("too many folders");
                }
                match parent {
                    Some(p) => {
                        if self.path(&p).len() + incoming.depth() > DEPTH_MAX {
                            return Err("folders nested too deep");
                        }
                        self.find_mut(&p).ok_or("unknown folder")?.children.push(incoming);
                    }
                    None => self.folders.push(incoming),
                }
            }
        }
        if self.all().len() > FOLDERS_MAX {
            return Err("too many folders");
        }
        self.prune_dangling();
        Ok(id)
    }

    /// The folder and its subfolders, plus copies of any tags its entries use that are defined in
    /// other folders, so the file stands on its own.
    pub fn export(&self, id: &str) -> Option<FolderExport> {
        let mut folder = self.find(id)?.clone();
        let mut inside = Vec::new();
        folder.walk(&mut inside);
        let defined: HashSet<&str> = inside.iter().flat_map(|f| f.tags.iter().map(|t| t.id.as_str())).collect();
        let used: HashSet<&str> = inside
            .iter()
            .flat_map(|f| f.systems.values().chain(f.pilots.values()))
            .flat_map(|e| e.tags.iter().map(String::as_str))
            .collect();
        let borrowed: Vec<Tag> = used
            .into_iter()
            .filter(|t| !defined.contains(t) && !t.starts_with("d:"))
            .filter_map(|t| self.tag(t).cloned())
            .collect();
        folder.tags.extend(borrowed);
        folder.online = true;
        Some(FolderExport { format: EXPORT_FORMAT.to_owned(), version: 1, folder })
    }

    #[cfg(test)]
    pub fn view(&self, preferred_target: &str) -> NotesView {
        self.view_with(preferred_target, &BTreeMap::new())
    }

    /// `colors` recolours built-in tags, which are fixed in code and so cannot carry the user's
    /// choice themselves.
    pub fn view_with(&self, preferred_target: &str, colors: &BTreeMap<String, [u8; 3]>) -> NotesView {
        let target = self.target(preferred_target);
        let target_path = self.path(&target).join(" / ");
        let mut v = NotesView { rev: self.rev, target, target_path, ..Default::default() };
        let mut active: Vec<(&Folder, String)> = Vec::new();
        for top in self.folders.iter().filter(|f| f.online) {
            let mut inside = Vec::new();
            top.walk(&mut inside);
            for f in inside {
                active.push((f, self.path(&f.id).join(" / ")));
            }
        }
        for t in default_tags().iter().chain(active.iter().flat_map(|(f, _)| f.tags.iter())) {
            let mut t = t.clone();
            if let Some(c) = colors.get(&t.id).filter(|_| t.id.starts_with("d:")) {
                t.color = *c;
            }
            match t.kind {
                NoteKind::System => v.system_tags.push(t),
                NoteKind::Pilot => v.pilot_tags.push(t),
            }
        }
        for (f, path) in &active {
            for kind in [NoteKind::System, NoteKind::Pilot] {
                let catalogue = v.tags(kind).iter().map(|t| t.id.clone()).collect::<HashSet<_>>();
                for (id, e) in f.entries(kind) {
                    let tags: Vec<String> = e.tags.iter().filter(|t| catalogue.contains(*t)).cloned().collect();
                    if e.note.is_empty() && tags.is_empty() {
                        continue;
                    }
                    let m = match kind {
                        NoteKind::System => v.systems.entry(*id).or_default(),
                        NoteKind::Pilot => v.pilots.entry(*id).or_default(),
                    };
                    if m.updated <= e.updated {
                        m.name = e.name.clone();
                        m.updated = e.updated;
                    }
                    m.parts.push(Part { folder: f.id.clone(), path: path.clone(), note: e.note.clone(), tags });
                }
            }
        }
        for kind in [NoteKind::System, NoteKind::Pilot] {
            let order: Vec<String> = v.tags(kind).iter().map(|t| t.id.clone()).collect();
            let map = match kind {
                NoteKind::System => &mut v.systems,
                NoteKind::Pilot => &mut v.pilots,
            };
            for m in map.values_mut() {
                let on: HashSet<&String> = m.parts.iter().flat_map(|p| p.tags.iter()).collect();
                m.tags = order.iter().filter(|t| on.contains(t)).cloned().collect();
            }
        }
        v.reindex();
        v
    }
}

fn put_entry(f: &mut Folder, kind: NoteKind, key: i64, e: Entry) -> Result<(), &'static str> {
    let map = f.entries_mut(kind);
    if !map.contains_key(&key) && map.len() >= ENTRIES_PER_FOLDER {
        return Err("folder is full");
    }
    map.insert(key, e);
    Ok(())
}

fn merge(dst: &mut Folder, src: Folder) {
    dst.name = src.name;
    for t in src.tags {
        match dst.tags.iter_mut().find(|d| d.id == t.id) {
            Some(d) => *d = t,
            None => dst.tags.push(t),
        }
    }
    dst.systems.extend(src.systems);
    dst.pilots.extend(src.pilots);
    for c in src.children {
        match dst.children.iter_mut().find(|d| d.id == c.id) {
            Some(d) => merge(d, c),
            None => dst.children.push(c),
        }
    }
}

/// Bounds an imported folder the same way [`NoteBook::apply`] bounds a single edit.
fn sanitize(mut f: Folder, system_name: &dyn Fn(i64) -> Option<String>) -> Result<Folder, &'static str> {
    if f.depth() > DEPTH_MAX {
        return Err("folders nested too deep");
    }
    if f.count() > FOLDERS_MAX {
        return Err("too many folders");
    }
    let mut ids = HashSet::new();
    let mut tag_ids = HashSet::new();
    let mut bad = None;
    f.walk_mut(&mut |f| {
        if f.id.is_empty() || !ids.insert(f.id.clone()) {
            f.id = new_id();
            ids.insert(f.id.clone());
        }
        match clean_name(&f.name) {
            Ok(n) => f.name = n,
            Err(_) => f.name = "Imported".to_owned(),
        }
        f.tags.retain(|t| !t.id.starts_with("d:") && !t.id.is_empty() && tag_ids.insert(t.id.clone()));
        f.tags.truncate(TAGS_PER_FOLDER);
        for t in &mut f.tags {
            t.name = clean_name(&t.name).unwrap_or_else(|_| "tag".to_owned());
        }
        f.systems.retain(|id, e| match system_name(*id) {
            Some(n) => {
                e.name = n;
                true
            }
            None => false,
        });
        f.pilots.retain(|id, e| *id > 0 && !e.name.trim().is_empty() && e.name.chars().count() <= PILOT_NAME_MAX);
        for e in f.systems.values_mut().chain(f.pilots.values_mut()) {
            if e.note.chars().count() > NOTE_MAX {
                bad = Some("note too long");
            }
        }
        if f.systems.len() > ENTRIES_PER_FOLDER || f.pilots.len() > ENTRIES_PER_FOLDER {
            bad = Some("folder is full");
        }
    });
    match bad {
        Some(e) => Err(e),
        None => Ok(f),
    }
}

fn subject_name(subject: &Subject, system_name: &dyn Fn(i64) -> Option<String>) -> Result<String, &'static str> {
    match subject {
        Subject::System(id) => system_name(*id).ok_or("unknown system"),
        Subject::Pilot { id, name } => {
            let name = name.trim();
            if *id <= 0 || name.is_empty() || name.chars().count() > PILOT_NAME_MAX {
                return Err("bad pilot");
            }
            Ok(name.to_owned())
        }
    }
}

fn clean_note(note: &str) -> Result<String, &'static str> {
    let note = note.trim_end();
    if note.chars().count() > NOTE_MAX {
        return Err("note too long");
    }
    Ok(note.to_owned())
}

fn clean_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("empty name");
    }
    if name.chars().count() > NAME_MAX {
        return Err("name too long");
    }
    Ok(name.to_owned())
}

/// A folder's notes from one source, as merged into a [`Merged`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Part {
    pub folder: String,
    pub path: String,
    pub note: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Merged {
    pub name: String,
    /// Active tags from every online folder, deduplicated, in catalogue order.
    pub tags: Vec<String>,
    pub parts: Vec<Part>,
    #[serde(skip)]
    updated: i64,
}

impl Merged {
    pub fn has_note(&self) -> bool {
        self.parts.iter().any(|p| !p.note.is_empty())
    }

    pub fn part_in(&self, folder: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.folder == folder)
    }
}

/// BTreeMap and Vec only: the web publisher and the overlay hash the serialized view to decide
/// whether to push it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotesView {
    pub rev: u64,
    /// Where quick edits go. Empty until a Default folder exists.
    pub target: String,
    /// The target's breadcrumb, for menus.
    #[serde(default)]
    pub target_path: String,
    pub system_tags: Vec<Tag>,
    pub pilot_tags: Vec<Tag>,
    pub systems: BTreeMap<i64, Merged>,
    pub pilots: BTreeMap<i64, Merged>,
    #[serde(skip)]
    by_name: HashMap<String, i64>,
}

impl NotesView {
    pub fn tags(&self, kind: NoteKind) -> &[Tag] {
        match kind {
            NoteKind::System => &self.system_tags,
            NoteKind::Pilot => &self.pilot_tags,
        }
    }

    pub fn tag(&self, id: &str) -> Option<&Tag> {
        self.system_tags.iter().chain(&self.pilot_tags).find(|t| t.id == id)
    }

    pub fn tags_of<'a>(&'a self, m: &'a Merged) -> impl Iterator<Item = &'a Tag> + 'a {
        m.tags.iter().filter_map(|t| self.tag(t))
    }

    pub fn system(&self, id: i64) -> Option<&Merged> {
        self.systems.get(&id)
    }

    pub fn pilot(&self, id: i64) -> Option<&Merged> {
        self.pilots.get(&id)
    }

    pub fn pilot_by_name(&self, name: &str) -> Option<&Merged> {
        self.by_name.get(&name.to_lowercase()).and_then(|id| self.pilots.get(id))
    }

    pub fn entry(&self, subject: &Subject) -> Option<&Merged> {
        match subject {
            Subject::System(id) => self.system(*id),
            Subject::Pilot { id, .. } => self.pilot(*id),
        }
    }

    /// Tag names and note text against a lowercased needle.
    pub fn matches(&self, m: &Merged, needle_lc: &str) -> bool {
        m.parts.iter().any(|p| p.note.to_lowercase().contains(needle_lc))
            || self.tags_of(m).any(|t| t.name.to_lowercase().contains(needle_lc))
    }

    /// The tags and only the listed entries, for the alert overlay, which shows a handful of cards.
    pub fn subset(&self, systems: impl IntoIterator<Item = i64>, pilots: impl IntoIterator<Item = i64>) -> NotesView {
        let mut out = NotesView {
            rev: self.rev,
            target: self.target.clone(),
            target_path: self.target_path.clone(),
            system_tags: self.system_tags.clone(),
            pilot_tags: self.pilot_tags.clone(),
            systems: systems.into_iter().filter_map(|id| self.systems.get(&id).map(|m| (id, m.clone()))).collect(),
            pilots: pilots.into_iter().filter_map(|id| self.pilots.get(&id).map(|m| (id, m.clone()))).collect(),
            by_name: HashMap::new(),
        };
        out.reindex();
        out
    }

    /// Needed after deserializing, since the name index is not sent.
    pub fn reindex(&mut self) {
        self.by_name = self.pilots.iter().map(|(id, m)| (m.name.to_lowercase(), *id)).collect();
    }
}

/// Whether merged tags include any of `wanted`, where [`ANY_TAG`] matches any tag at all.
pub fn has_any(m: &Merged, wanted: &[String]) -> bool {
    wanted.iter().any(|w| if w == ANY_TAG { !m.tags.is_empty() } else { m.tags.contains(w) })
}

pub fn color32(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// Light tones only: tag markers sit on the map's near-black background, where mid and dark shades
/// all but vanish.
const PALETTE: [[u8; 3]; 10] = [
    [0xFF, 0x6B, 0x6B],
    [0xFF, 0xA9, 0x4D],
    [0xFF, 0xD4, 0x3B],
    [0x69, 0xDB, 0x7C],
    [0x3B, 0xC9, 0xDB],
    [0x4D, 0xAB, 0xF7],
    [0xB1, 0x97, 0xFC],
    [0xF7, 0x83, 0xAC],
    [0xA9, 0xE3, 0x4B],
    [0xDE, 0xE2, 0xE6],
];

/// A new tag's starting colour, cycling so consecutive tags are told apart without the picker.
pub fn default_color(n: usize) -> [u8; 3] {
    PALETTE[n % PALETTE.len()]
}

const DEFAULT_SYSTEM_TAGS: &[(&str, [u8; 3])] = &[
    ("Staging", [0x4D, 0xAB, 0xF7]),
    ("Super Docking", [0xB1, 0x97, 0xFC]),
    ("Capital Docking", [0xE5, 0x99, 0xF7]),
    ("Industry", [0xFF, 0xA9, 0x4D]),
    ("Research", [0x74, 0xC0, 0xFC]),
    ("Reactions", [0xFF, 0x87, 0x87]),
    ("Invention", [0x63, 0xE6, 0xBE]),
    ("Mining", [0xFF, 0xD4, 0x3B]),
    ("Ratting", [0xA9, 0xE3, 0x4B]),
    ("Exploration", [0x3B, 0xC9, 0xDB]),
    ("Market Hub", [0xFF, 0xC0, 0x78]),
];

const DEFAULT_PILOT_TAGS: &[(&str, [u8; 3])] = &[
    ("Cyno", [0xFF, 0x6B, 0x6B]),
    ("FC", [0xFF, 0xD4, 0x3B]),
    ("Wormhole", [0x3B, 0xC9, 0xDB]),
    ("Lowsec", [0xFF, 0xA9, 0x4D]),
    ("Highsec", [0x69, 0xDB, 0x7C]),
    ("Nullsec", [0xF7, 0x83, 0xAC]),
    ("Pochven", [0xB1, 0x97, 0xFC]),
    ("Ganker", [0xFF, 0xC0, 0x78]),
    ("Crabber", [0xA9, 0xE3, 0x4B]),
    ("Miner", [0xC5, 0xF6, 0xFA]),
    ("Ratter", [0x63, 0xE6, 0xBE]),
    ("Capital", [0x74, 0xC0, 0xFC]),
    ("Super", [0xE5, 0x99, 0xF7]),
    ("Titan", [0xDE, 0xE2, 0xE6]),
];

/// Built in, shared by every folder, and fixed. Their ids are derived from the name so a rule or an
/// export naming one means the same tag on every machine.
pub fn default_tags() -> &'static [Tag] {
    static TAGS: std::sync::LazyLock<Vec<Tag>> = std::sync::LazyLock::new(|| {
        let mk = |kind: NoteKind, prefix: &str, names: &[(&str, [u8; 3])]| {
            names
                .iter()
                .map(|(n, color)| Tag {
                    id: format!("d:{prefix}:{}", n.to_lowercase().replace(' ', "-")),
                    kind,
                    name: (*n).to_owned(),
                    color: *color,
                })
                .collect::<Vec<_>>()
        };
        let mut v = mk(NoteKind::System, "sys", DEFAULT_SYSTEM_TAGS);
        v.extend(mk(NoteKind::Pilot, "pilot", DEFAULT_PILOT_TAGS));
        v
    });
    &TAGS
}

/// A random version 4 uuid.
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        // No OS randomness is not worth failing an edit over; the clock is unique enough per machine.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        b.copy_from_slice(&t.as_nanos().to_le_bytes());
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

pub fn to_json(e: &FolderExport) -> String {
    serde_json::to_string(e).unwrap_or_default()
}

pub fn to_compressed(e: &FolderExport) -> String {
    use base64::Engine;
    use std::io::Write;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let _ = z.write_all(to_json(e).as_bytes());
    let bytes = z.finish().unwrap_or_default();
    format!("{COMPRESSED_PREFIX}{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Either form [`to_json`] or [`to_compressed`] produces, pasted or read from a file.
pub fn parse_export(text: &str) -> Result<FolderExport, String> {
    use base64::Engine;
    use std::io::Read;
    const LIMIT: u64 = 16 * 1024 * 1024;
    let text = text.trim();
    let json = if text.starts_with('{') {
        text.to_owned()
    } else {
        let b64: String = text
            .strip_prefix(COMPRESSED_PREFIX)
            .unwrap_or(text)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let raw = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(&b64))
            .map_err(|_| "not valid export text".to_owned())?;
        let mut out = String::new();
        flate2::read::ZlibDecoder::new(&raw[..])
            .take(LIMIT)
            .read_to_string(&mut out)
            .map_err(|_| "not valid export text".to_owned())?;
        out
    };
    let e: FolderExport = serde_json::from_str(&json).map_err(|e| format!("not a notes export: {e}"))?;
    if e.format != EXPORT_FORMAT {
        return Err("not an EVE Spai notes export".to_owned());
    }
    Ok(e)
}

#[cfg(test)]
pub(crate) fn test_system_name(id: i64) -> Option<String> {
    match id {
        30_004_759 => Some("1DQ1-A".to_owned()),
        30_004_608 => Some("319-3D".to_owned()),
        30_003_704 => Some("7-K5EL".to_owned()),
        30_000_142 => Some("Jita".to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DQ: i64 = 30_004_759;

    fn pilot(id: i64, name: &str) -> Subject {
        Subject::Pilot { id, name: name.to_owned() }
    }

    fn run(b: &mut NoteBook, op: NotesOp) -> Applied {
        b.apply(op, 100, &test_system_name).unwrap()
    }

    fn folder(b: &mut NoteBook, parent: Option<&str>, name: &str) -> String {
        run(b, NotesOp::CreateFolder { parent: parent.map(str::to_owned), name: name.into() }).folder.unwrap()
    }

    fn tag(b: &mut NoteBook, folder: &str, kind: NoteKind, name: &str) -> String {
        run(b, NotesOp::PutTag { folder: folder.into(), id: None, kind, name: name.into(), color: [1, 2, 3] })
            .tag
            .unwrap()
    }

    fn set_tag(b: &mut NoteBook, folder: &str, s: Subject, t: &str, on: bool) {
        run(b, NotesOp::SetTag { folder: folder.into(), subject: s, tag: t.into(), on });
    }

    #[test]
    fn a_write_without_a_folder_lands_in_default() {
        let mut b = NoteBook::default();
        let a = run(&mut b, NotesOp::SetEntry { folder: String::new(), subject: Subject::System(DQ), note: "home".into(), tags: vec![] });
        assert_eq!(b.folders.len(), 1);
        assert_eq!(b.folders[0].name, DEFAULT_FOLDER);
        assert_eq!(a.folder.as_deref(), Some(b.folders[0].id.as_str()));
        run(&mut b, NotesOp::SetEntry { folder: "gone".into(), subject: Subject::System(DQ), note: "again".into(), tags: vec![] });
        assert_eq!(b.folders.len(), 1, "a deleted target falls back to the same Default");
    }

    #[test]
    fn built_in_colours_can_be_overridden_per_user() {
        let b = NoteBook::default();
        let mut colors = BTreeMap::new();
        colors.insert("d:pilot:cyno".to_owned(), [1, 2, 3]);
        colors.insert("user-tag".to_owned(), [9, 9, 9]);
        let v = b.view_with("", &colors);
        assert_eq!(v.tag("d:pilot:cyno").unwrap().color, [1, 2, 3]);
        assert_eq!(b.view("").tag("d:pilot:cyno").unwrap().color, [0xFF, 0x6B, 0x6B]);
    }

    #[test]
    fn built_in_colours_are_light_enough_for_the_dark_map() {
        for t in default_tags().iter().map(|t| t.color).chain(PALETTE) {
            let luma = 0.2126 * t[0] as f32 + 0.7152 * t[1] as f32 + 0.0722 * t[2] as f32;
            assert!(luma >= 110.0, "{t:?} is too dark ({luma})");
        }
    }

    #[test]
    fn default_tags_are_fixed_and_usable() {
        let mut b = NoteBook::default();
        let cyno = "d:pilot:cyno";
        assert_eq!(b.tag(cyno).unwrap().name, "Cyno");
        set_tag(&mut b, "", pilot(5, "Five"), cyno, true);
        assert!(b.apply(NotesOp::PutTag { folder: String::new(), id: Some(cyno.into()), kind: NoteKind::Pilot, name: "x".into(), color: [0; 3] }, 0, &test_system_name).is_err());
        assert!(b.apply(NotesOp::PutTag { folder: String::new(), id: None, kind: NoteKind::Pilot, name: "cyno".into(), color: [0; 3] }, 0, &test_system_name).is_err());
        assert!(b.apply(NotesOp::SetTag { folder: String::new(), subject: Subject::System(DQ), tag: cyno.into(), on: true }, 0, &test_system_name).is_err(), "a pilot tag on a system");
    }

    #[test]
    fn view_merges_folders_and_untag_only_touches_the_target() {
        let mut b = NoteBook::default();
        let mine = folder(&mut b, None, "Mine");
        let intel = folder(&mut b, None, "Alliance intel");
        let hunter = tag(&mut b, &intel, NoteKind::Pilot, "Hunter");
        set_tag(&mut b, &mine, pilot(5, "Bob"), &hunter, true);
        set_tag(&mut b, &intel, pilot(5, "Bob"), &hunter, true);
        run(&mut b, NotesOp::SetEntry { folder: intel.clone(), subject: pilot(5, "Bob"), note: "seen in Delve".into(), tags: vec![hunter.clone()] });

        let v = b.view(&mine);
        let m = v.pilot(5).unwrap();
        assert_eq!(m.tags, vec![hunter.clone()], "shown once");
        assert_eq!(m.parts.len(), 2);
        assert_eq!(v.target, mine);

        set_tag(&mut b, &mine, pilot(5, "Bob"), &hunter, false);
        let v = b.view(&mine);
        let m = v.pilot(5).unwrap();
        assert_eq!(m.tags, vec![hunter.clone()], "the other folder still sets it");
        assert!(m.part_in(&mine).is_none());
        assert_eq!(m.part_in(&intel).unwrap().note, "seen in Delve");
    }

    #[test]
    fn offline_top_level_folder_hides_its_subtree_and_its_tags() {
        let mut b = NoteBook::default();
        let top = folder(&mut b, None, "Top");
        let sub = folder(&mut b, Some(&top), "Sub");
        let other = folder(&mut b, None, "Other");
        let t = tag(&mut b, &sub, NoteKind::System, "Hot");
        set_tag(&mut b, &sub, Subject::System(DQ), &t, true);
        set_tag(&mut b, &other, Subject::System(DQ), &t, true);
        assert!(b.apply(NotesOp::SetOnline { id: sub.clone(), on: false }, 0, &test_system_name).is_err(), "subfolders have no switch");
        run(&mut b, NotesOp::SetOnline { id: top, on: false });
        let v = b.view("");
        assert!(v.tag(&t).is_none(), "a tag defined offline is inactive everywhere");
        assert!(v.system(DQ).is_none());
        assert_eq!(v.pilot_tags.len(), DEFAULT_PILOT_TAGS.len());
    }

    #[test]
    fn deleting_a_tag_or_folder_prunes_references() {
        let mut b = NoteBook::default();
        let a = folder(&mut b, None, "A");
        let c = folder(&mut b, None, "C");
        let t = tag(&mut b, &a, NoteKind::Pilot, "Hunter");
        set_tag(&mut b, &c, pilot(1, "One"), &t, true);
        run(&mut b, NotesOp::SetEntry { folder: c.clone(), subject: pilot(2, "Two"), note: "keep".into(), tags: vec![t.clone()] });
        run(&mut b, NotesOp::DeleteFolder { id: a });
        let f = b.find(&c).unwrap();
        assert!(!f.pilots.contains_key(&1), "emptied entry dropped");
        assert!(f.pilots[&2].tags.is_empty());
        assert_eq!(f.pilots[&2].note, "keep");
    }

    #[test]
    fn folders_move_but_not_into_themselves() {
        let mut b = NoteBook::default();
        let top = folder(&mut b, None, "Top");
        let sub = folder(&mut b, Some(&top), "Sub");
        assert!(b.apply(NotesOp::MoveFolder { id: top.clone(), parent: Some(sub.clone()) }, 0, &test_system_name).is_err());
        run(&mut b, NotesOp::MoveFolder { id: sub.clone(), parent: None });
        assert!(b.is_top(&sub));
        assert_eq!(b.path(&sub), ["Sub"]);
        run(&mut b, NotesOp::RenameFolder { id: sub.clone(), name: "Renamed".into() });
        assert_eq!(b.find(&sub).unwrap().name, "Renamed");
    }

    #[test]
    fn refused_edits_change_nothing() {
        let mut b = NoteBook::default();
        let f = folder(&mut b, None, "F");
        let before = b.clone();
        let refuse = |b: &mut NoteBook, op| assert!(b.apply(op, 0, &test_system_name).is_err());
        refuse(&mut b, NotesOp::SetEntry { folder: f.clone(), subject: Subject::System(1), note: "x".into(), tags: vec![] });
        refuse(&mut b, NotesOp::SetEntry { folder: f.clone(), subject: pilot(1, "A"), note: "x".repeat(NOTE_MAX + 1), tags: vec![] });
        refuse(&mut b, NotesOp::SetEntry { folder: f.clone(), subject: pilot(0, "A"), note: "x".into(), tags: vec![] });
        refuse(&mut b, NotesOp::SetEntry { folder: f.clone(), subject: pilot(1, "A"), note: "x".into(), tags: vec!["nope".into()] });
        refuse(&mut b, NotesOp::CreateFolder { parent: None, name: "  ".into() });
        refuse(&mut b, NotesOp::DeleteTag { id: "nope".into() });
        assert_eq!(b, before);
    }

    #[test]
    fn export_import_round_trip_through_both_text_forms() {
        let mut b = NoteBook::default();
        let top = folder(&mut b, None, "Intel");
        let sub = folder(&mut b, Some(&top), "Delve");
        let t = tag(&mut b, &top, NoteKind::Pilot, "Hunter");
        run(&mut b, NotesOp::SetEntry { folder: sub.clone(), subject: pilot(5, "Bob"), note: "cloaky".into(), tags: vec![t.clone(), "d:pilot:cyno".into()] });
        let e = b.export(&top).unwrap();
        for text in [to_json(&e), to_compressed(&e)] {
            assert_eq!(parse_export(&text).unwrap(), e);
        }
        let mut fresh = NoteBook::default();
        run(&mut fresh, NotesOp::Import { export: e, mode: ImportMode::Add, parent: None });
        assert_eq!(fresh.view("").pilot(5).unwrap().tags, vec!["d:pilot:cyno".to_owned(), t]);
    }

    #[test]
    fn export_carries_tags_borrowed_from_other_folders() {
        let mut b = NoteBook::default();
        let lib = folder(&mut b, None, "Tag library");
        let share = folder(&mut b, None, "Share");
        let t = tag(&mut b, &lib, NoteKind::System, "Hot");
        set_tag(&mut b, &share, Subject::System(DQ), &t, true);
        let e = b.export(&share).unwrap();
        assert_eq!(e.folder.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Hot"]);
    }

    #[test]
    fn importing_an_existing_folder_asks_then_overwrites_or_merges() {
        let mut b = NoteBook::default();
        let top = folder(&mut b, None, "Intel");
        run(&mut b, NotesOp::SetEntry { folder: top.clone(), subject: pilot(1, "Old"), note: "old only".into(), tags: vec![] });
        run(&mut b, NotesOp::SetEntry { folder: top.clone(), subject: pilot(2, "Both"), note: "mine".into(), tags: vec![] });
        let mut shared = b.export(&top).unwrap();
        shared.folder.name = "Intel v2".into();
        shared.folder.pilots.remove(&1);
        shared.folder.pilots.get_mut(&2).unwrap().note = "theirs".into();
        shared.folder.pilots.insert(3, Entry { name: "New".into(), note: "new".into(), ..Default::default() });

        assert_eq!(b.apply(NotesOp::Import { export: shared.clone(), mode: ImportMode::Add, parent: None }, 0, &test_system_name), Err("folder exists"));

        let mut merged = b.clone();
        run(&mut merged, NotesOp::Import { export: shared.clone(), mode: ImportMode::Merge, parent: None });
        let f = merged.find(&top).unwrap();
        assert_eq!(f.name, "Intel v2");
        assert_eq!(f.pilots.keys().copied().collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(f.pilots[&2].note, "theirs");

        let mut over = b.clone();
        run(&mut over, NotesOp::SetOnline { id: top.clone(), on: false });
        run(&mut over, NotesOp::Import { export: shared, mode: ImportMode::Overwrite, parent: None });
        let f = over.find(&top).unwrap();
        assert_eq!(f.pilots.keys().copied().collect::<Vec<_>>(), [2, 3]);
        assert!(!f.online, "overwrite keeps the local online switch");
        assert_eq!(over.folders.len(), 1);
    }

    #[test]
    fn import_never_duplicates_an_identity_that_lives_elsewhere() {
        let mut b = NoteBook::default();
        let a = folder(&mut b, None, "A");
        let t = tag(&mut b, &a, NoteKind::Pilot, "Hunter");
        let mut e = b.export(&a).unwrap();
        e.folder.id = new_id();
        let inner = Folder { id: a.clone(), ..Folder::new("Clash") };
        e.folder.children.push(inner);
        run(&mut b, NotesOp::Import { export: e, mode: ImportMode::Add, parent: None });
        let ids: Vec<&str> = b.all().iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids.iter().filter(|i| **i == a).count(), 1);
        let defs = b.all().iter().flat_map(|f| f.tags.iter()).filter(|x| x.id == t).count();
        assert_eq!(defs, 1);
    }

    #[test]
    fn import_drops_what_it_cannot_trust() {
        let mut f = Folder::new("x".repeat(200).as_str());
        f.systems.insert(1, Entry { name: "Nowhere".into(), note: "x".into(), ..Default::default() });
        f.pilots.insert(-4, Entry { name: "Bad".into(), note: "x".into(), ..Default::default() });
        f.pilots.insert(4, Entry { name: "Good".into(), note: "x".into(), tags: vec!["missing".into()], ..Default::default() });
        let export = FolderExport { format: EXPORT_FORMAT.into(), version: 1, folder: f };
        let mut b = NoteBook::default();
        let id = run(&mut b, NotesOp::Import { export, mode: ImportMode::Add, parent: None }).folder.unwrap();
        let f = b.find(&id).unwrap();
        assert_eq!(f.name, "Imported");
        assert!(f.systems.is_empty());
        assert_eq!(f.pilots.keys().copied().collect::<Vec<_>>(), [4]);
        assert!(f.pilots[&4].tags.is_empty());
        assert!(parse_export("garbage").is_err());
    }

    #[test]
    fn any_tag_matches_only_tagged_subjects() {
        let tagged = Merged { tags: vec!["t".into()], ..Default::default() };
        let noted = Merged { parts: vec![Part { note: "x".into(), ..Default::default() }], ..Default::default() };
        let any = [ANY_TAG.to_owned()];
        assert!(has_any(&tagged, &any));
        assert!(!has_any(&noted, &any));
        assert!(!has_any(&tagged, &["gone".to_owned()]), "a deleted tag's id matches nothing");
    }

    #[test]
    fn view_finds_pilots_by_name_and_survives_the_wire() {
        let mut b = NoteBook::default();
        set_tag(&mut b, "", pilot(5, "Hostile Pilot"), "d:pilot:cyno", true);
        let v = b.view("");
        assert!(v.pilot_by_name("hostile PILOT").is_some());
        let mut back: NotesView = serde_json::from_str(&serde_json::to_string(&v.subset([], [5])).unwrap()).unwrap();
        back.reindex();
        assert!(back.pilot_by_name("Hostile Pilot").is_some());
        assert!(v.matches(v.pilot(5).unwrap(), "cyn"));
    }

    #[test]
    fn uuids_look_like_v4() {
        let id = new_id();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert_ne!(id, new_id());
    }
}

#[cfg(test)]
mod fixture_tests {
    #[test]
    fn fixture_notebook_builds_and_hides_the_offline_folder() {
        let b = crate::uitest::fixtures::notebook();
        let v = b.view("");
        let dq = v.system(30_004_759).unwrap();
        assert_eq!(dq.parts.len(), 2);
        assert_eq!(dq.tags.len(), 3);
        assert!(v.system(30_003_704).is_none());
        assert!(v.pilot_by_name("Hostile Pilot").is_some());
    }
}
