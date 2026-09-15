//! The note editor and the notes manager windows.
//!
//! Both render from `Arc` snapshots of the book and view and hand back a list of actions, which the
//! window function applies once the viewport closure has released its borrows of `self`.

use egui_phosphor::regular as icon;

use super::{selectable_chip, tag_chip, IntelClick, NoteDraft, SpaiApp};
use crate::notes::{Folder, FolderExport, ImportMode, NoteBook, NoteKind, NotesOp, NotesView, Subject};

pub(crate) enum EditorAction {
    NewTag,
    Save,
    Remove,
    Manage,
    FolderChanged,
}

#[derive(Clone)]
pub(crate) struct TagEdit {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) color: [u8; 3],
    pub(crate) folder: String,
}

pub(crate) enum Confirm {
    Folder { id: String, name: String, items: usize },
    Tag { id: String, name: String, uses: usize },
}

pub(crate) struct Import {
    pub(crate) text: String,
    pub(crate) error: Option<String>,
    /// Parsed, and its folder already exists here: waiting for Overwrite or Merge.
    pub(crate) clash: Option<FolderExport>,
    pub(crate) into_selected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailTab {
    Notes,
    Tags,
}

pub(crate) struct NotesManager {
    pub(crate) kind: NoteKind,
    pub(crate) folder: String,
    pub(crate) tab: DetailTab,
    /// Set when a rename starts, so the name box takes focus once instead of every frame.
    pub(crate) rename_focus: bool,
    /// Folders the user expanded or collapsed by hand. Others follow their online state.
    pub(crate) open: std::collections::HashMap<String, bool>,
    pub(crate) search: String,
    pub(crate) rename: Option<(String, String)>,
    pub(crate) tag_edit: Option<TagEdit>,
    pub(crate) new_tag: String,
    pub(crate) new_color: [u8; 3],
    pub(crate) import: Option<Import>,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) status: Option<String>,
}

impl NotesManager {
    pub(crate) fn new(kind: NoteKind, folder: String) -> Self {
        NotesManager {
            kind,
            folder,
            tab: DetailTab::Notes,
            rename_focus: false,
            open: std::collections::HashMap::new(),
            search: String::new(),
            rename: None,
            tag_edit: None,
            new_tag: String::new(),
            new_color: crate::notes::default_color(0),
            import: None,
            confirm: None,
            status: None,
        }
    }
}

pub(crate) enum ManagerAction {
    Op(NotesOp),
    /// A new folder, renamed in place as soon as it exists.
    CreateFolder { parent: Option<String> },
    SetTarget(String),
    Edit { folder: String, subject: Subject },
    Go(Subject),
    ExportFile(String),
    Copy { id: String, compressed: bool },
    OpenImportFile,
    Import { export: FolderExport, mode: ImportMode },
    DefaultColor { id: String, color: Option<[u8; 3]> },
}

impl SpaiApp {
    pub(crate) fn open_notes_manager(&mut self, kind: NoteKind) {
        let folder = self.notes_view.target.clone();
        self.notes_manager = Some(NotesManager::new(kind, folder));
        self.focus_window = Some(egui::ViewportId::from_hash_of("notes_manager"));
    }

    pub(crate) fn note_editor_window(&mut self, ctx: &egui::Context) {
        if let Some(s) = self.note_editor_pending.take() {
            self.open_note_editor(s);
        }
        let Some(mut draft) = self.note_editor.take() else { return };
        let book = self.notes.clone();
        let view = self.notes_view.clone();
        let title = subject_title(&draft.subject, &self.systems);
        let error = self.notes_error.clone();
        let mut actions = Vec::new();
        let mut keep = Self::dialog_viewport(ctx, "note_editor", "EVE Spai - Notes and tags", [460.0, 600.0], |ui| {
            editor_body(ui, &mut draft, &book, &view, &title, error.as_deref(), &mut actions);
        });
        for a in actions {
            match a {
                EditorAction::NewTag => {
                    let op = NotesOp::PutTag {
                        folder: draft.folder.clone(),
                        id: None,
                        kind: draft.subject.kind(),
                        name: draft.new_tag.clone(),
                        color: draft.new_color,
                    };
                    if let Ok(applied) = self.apply_notes_op(op) {
                        if let Some(f) = applied.folder {
                            draft.folder = f;
                        }
                        if let Some(t) = applied.tag {
                            draft.tags.push(t);
                        }
                        draft.new_tag.clear();
                        draft.new_color = crate::notes::default_color(draft.new_color.iter().map(|c| *c as usize).sum());
                    }
                }
                EditorAction::Save => {
                    let op = NotesOp::SetEntry {
                        folder: draft.folder.clone(),
                        subject: draft.subject.clone(),
                        note: draft.note.clone(),
                        tags: draft.tags.clone(),
                    };
                    if self.apply_notes_op(op).is_ok() {
                        keep = false;
                    }
                }
                EditorAction::Remove => {
                    let op = NotesOp::RemoveEntry { folder: draft.folder.clone(), subject: draft.subject.clone() };
                    if self.apply_notes_op(op).is_ok() {
                        draft.load(&self.notes);
                    }
                }
                EditorAction::Manage => {
                    self.open_notes_manager(draft.subject.kind());
                    if let Some(m) = &mut self.notes_manager {
                        m.folder = draft.folder.clone();
                    }
                }
                EditorAction::FolderChanged => draft.load(&self.notes),
            }
        }
        if keep {
            self.note_editor = Some(draft);
        } else {
            self.notes_error = None;
        }
    }

    pub(crate) fn notes_manager_window(&mut self, ctx: &egui::Context) {
        let Some(mut m) = self.notes_manager.take() else { return };
        let book = self.notes.clone();
        let view = self.notes_view.clone();
        let error = self.notes_error.clone();
        let systems = self.systems.clone();
        let mut actions = Vec::new();
        let title = match m.kind {
            NoteKind::System => "EVE Spai - System notes and tags",
            NoteKind::Pilot => "EVE Spai - Pilot notes and tags",
        };
        let keep = Self::dialog_viewport(ctx, "notes_manager", title, [860.0, 620.0], |ui| {
            manager_body(ui, &mut m, &book, &view, &systems, error.as_deref(), &mut actions);
        });
        for a in actions {
            self.manager_action(ctx, &mut m, a);
        }
        if keep {
            self.notes_manager = Some(m);
        } else {
            self.notes_error = None;
        }
    }

    fn manager_action(&mut self, ctx: &egui::Context, m: &mut NotesManager, a: ManagerAction) {
        m.status = None;
        match a {
            ManagerAction::Op(op) => {
                if let NotesOp::SetOnline { id, .. } = &op {
                    // Going offline folds the folder; coming back online unfolds it.
                    m.open.remove(id);
                }
                let _ = self.apply_notes_op(op);
            }
            ManagerAction::CreateFolder { parent } => {
                let op = NotesOp::CreateFolder { parent, name: "New folder".into() };
                if let Ok(applied) = self.apply_notes_op(op) {
                    if let Some(id) = applied.folder {
                        m.folder = id.clone();
                        m.rename = Some((id, "New folder".into()));
                        m.rename_focus = true;
                    }
                }
            }
            ManagerAction::SetTarget(id) => self.set_notes_target(id),
            ManagerAction::DefaultColor { id, color } => self.set_default_tag_color(id, color),
            ManagerAction::Edit { folder, subject } => {
                self.open_note_editor(subject);
                if let Some(d) = &mut self.note_editor {
                    d.folder = folder;
                    d.load(&self.notes);
                }
            }
            ManagerAction::Go(subject) => match subject {
                Subject::System(id) => self.open_system(id),
                Subject::Pilot { name, .. } => self.pending_overlay_clicks.push(IntelClick::Pilot(name)),
            },
            ManagerAction::ExportFile(id) => {
                let Some(export) = self.notes.export(&id) else { return };
                let file = format!("{}.spainotes.json", export.folder.name.replace(['/', '\\', ':'], "_"));
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(file)
                    .add_filter("EVE Spai notes", &["json"])
                    .save_file()
                {
                    m.status = Some(match std::fs::write(&path, crate::notes::to_json(&export)) {
                        Ok(()) => format!("Exported to {}", path.display()),
                        Err(e) => format!("Export failed: {e}"),
                    });
                }
            }
            ManagerAction::Copy { id, compressed } => {
                let Some(export) = self.notes.export(&id) else { return };
                let text =
                    if compressed { crate::notes::to_compressed(&export) } else { crate::notes::to_json(&export) };
                ctx.copy_text(text);
                m.status = Some(format!(
                    "Copied \"{}\" {}",
                    export.folder.name,
                    if compressed { "compressed" } else { "as JSON" }
                ));
            }
            ManagerAction::OpenImportFile => {
                if let Some(path) = rfd::FileDialog::new().add_filter("EVE Spai notes", &["json", "txt"]).pick_file() {
                    let imp = m.import.get_or_insert_with(|| Import {
                        text: String::new(),
                        error: None,
                        clash: None,
                        into_selected: false,
                    });
                    match std::fs::read_to_string(&path) {
                        Ok(t) => {
                            imp.text = t;
                            imp.error = None;
                        }
                        Err(e) => imp.error = Some(format!("Could not read the file: {e}")),
                    }
                }
            }
            ManagerAction::Import { export, mode } => {
                let parent = m.import.as_ref().filter(|i| i.into_selected).map(|_| m.folder.clone());
                let name = export.folder.name.clone();
                match self.apply_notes_op(NotesOp::Import { export, mode, parent }) {
                    Ok(applied) => {
                        if let Some(f) = applied.folder {
                            m.folder = f;
                        }
                        m.import = None;
                        m.status = Some(format!("Imported \"{name}\""));
                    }
                    Err(e) => {
                        if let Some(imp) = &mut m.import {
                            imp.error = Some(e);
                        }
                    }
                }
            }
        }
    }
}

fn subject_title(s: &Subject, systems: &Option<std::sync::Arc<crate::geo::Systems>>) -> String {
    match s {
        Subject::System(id) => systems
            .as_ref()
            .and_then(|g| g.info_of(*id))
            .map(|i| format!("{}  {}", icon::PLANET, i.name))
            .unwrap_or_else(|| format!("System {id}")),
        Subject::Pilot { name, .. } => format!("{}  {name}", icon::USER),
    }
}

fn folder_choices(book: &NoteBook) -> Vec<(String, String)> {
    book.all().into_iter().map(|f| (f.id.clone(), book.path(&f.id).join(" / "))).collect()
}

pub(crate) fn editor_body(
    ui: &mut egui::Ui,
    d: &mut NoteDraft,
    book: &NoteBook,
    view: &NotesView,
    title: &str,
    error: Option<&str>,
    actions: &mut Vec<EditorAction>,
) {
    let kind = d.subject.kind();
    ui.heading(title);
    ui.add_space(4.0);

    let choices = folder_choices(book);
    let current = choices
        .iter()
        .find(|(id, _)| *id == d.folder)
        .map(|(_, p)| p.clone())
        .unwrap_or_else(|| format!("{} (made on save)", crate::notes::DEFAULT_FOLDER));
    ui.horizontal(|ui| {
        ui.label(format!("{}  Folder", icon::FOLDER));
        egui::ComboBox::from_id_salt("note_editor_folder").selected_text(current).width(260.0).show_ui(ui, |ui| {
            for (id, path) in &choices {
                if ui.add(egui::Button::new(path.as_str()).selected(*id == d.folder)).clicked() {
                    d.folder = id.clone();
                    actions.push(EditorAction::FolderChanged);
                    ui.close();
                }
            }
        });
    });
    ui.add_space(6.0);

    ui.label(format!("{}  Note", icon::NOTE));
    ui.add(
        egui::TextEdit::multiline(&mut d.note)
            .desired_rows(5)
            .desired_width(f32::INFINITY)
            .hint_text("What should you remember about this?"),
    );
    let n = d.note.chars().count();
    let counter = egui::RichText::new(format!("{n} / {}", crate::notes::NOTE_MAX));
    ui.label(if n > crate::notes::NOTE_MAX { counter.color(crate::theme::standing::HOSTILE) } else { counter.weak() });
    ui.add_space(6.0);

    ui.label(format!("{}  Tags", icon::TAG));
    ui.horizontal_wrapped(|ui| {
        for t in view.tags(kind) {
            let on = d.tags.contains(&t.id);
            let text = egui::RichText::new(&t.name).color(crate::notes::color32(t.color));
            if selectable_chip(ui, on, text).clicked() {
                if on {
                    d.tags.retain(|x| *x != t.id);
                } else {
                    d.tags.push(t.id.clone());
                }
            }
        }
        // Tags from offline folders stay on the entry; they can be removed here but not added.
        let hidden: Vec<String> = d.tags.iter().filter(|id| view.tag(id).is_none()).cloned().collect();
        for id in hidden {
            let name = book.tag(&id).map(|t| t.name.clone()).unwrap_or_else(|| "deleted tag".into());
            if selectable_chip(ui, true, egui::RichText::new(format!("{name} (offline)")).weak())
                .on_hover_text("Defined in an offline folder. Click to remove it from this entry.")
                .clicked()
            {
                d.tags.retain(|x| *x != id);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut d.new_tag).hint_text("New tag").desired_width(180.0));
        ui.color_edit_button_srgb(&mut d.new_color);
        let ok = !d.new_tag.trim().is_empty();
        if ui.add_enabled(ok, egui::Button::new(format!("{}  Add tag", icon::PLUS))).clicked() {
            actions.push(EditorAction::NewTag);
        }
    });

    if let Some(m) = view.entry(&d.subject) {
        let others: Vec<_> = m.parts.iter().filter(|p| p.folder != d.folder).collect();
        if !others.is_empty() {
            ui.add_space(8.0);
            ui.separator();
            ui.label(egui::RichText::new("In other folders").strong());
            for p in others {
                ui.label(egui::RichText::new(format!("{}  {}", icon::FOLDER, p.path)).weak());
                ui.horizontal_wrapped(|ui| {
                    for t in p.tags.iter().filter_map(|t| view.tag(t)) {
                        tag_chip(ui, t, false);
                    }
                });
                if !p.note.is_empty() {
                    ui.add(egui::Label::new(p.note.as_str()).wrap());
                }
            }
        }
    }

    ui.add_space(8.0);
    if let Some(e) = error {
        ui.colored_label(crate::theme::standing::HOSTILE, e);
    }
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Save", icon::CHECK)).clicked() {
            actions.push(EditorAction::Save);
        }
        let saved = book.find(&d.folder).is_some_and(|f| f.entries(kind).contains_key(&d.subject.key()));
        if ui.add_enabled(saved, egui::Button::new(format!("{}  Remove from folder", icon::TRASH))).clicked() {
            actions.push(EditorAction::Remove);
        }
        if ui.button(format!("{}  Manage…", icon::FOLDER_OPEN)).clicked() {
            actions.push(EditorAction::Manage);
        }
    });
}

pub(crate) fn manager_body(
    ui: &mut egui::Ui,
    m: &mut NotesManager,
    book: &NoteBook,
    view: &NotesView,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    error: Option<&str>,
    actions: &mut Vec<ManagerAction>,
) {
    if book.find(&m.folder).is_none() {
        m.folder = book.folders.first().map(|f| f.id.clone()).unwrap_or_default();
    }
    confirm_modal(ui, m, actions);

    egui::Panel::left("notes_folder_tree").resizable(true).default_size(250.0).show_inside(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (k, label) in [(NoteKind::System, "Systems"), (NoteKind::Pilot, "Pilots")] {
                if selectable_chip(ui, m.kind == k, label).clicked() {
                    m.kind = k;
                    m.tag_edit = None;
                }
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button(format!("{}  Folder", icon::FOLDER_PLUS)).on_hover_text("New top-level folder").clicked() {
                actions.push(ManagerAction::CreateFolder { parent: None });
            }
            if ui.button(format!("{}  Import…", icon::DOWNLOAD_SIMPLE)).clicked() {
                m.import.get_or_insert_with(|| Import { text: String::new(), error: None, clash: None, into_selected: false });
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt("notes_tree_scroll").show(ui, |ui| {
            if book.folders.is_empty() {
                ui.label(egui::RichText::new("No folders yet. The first note you save goes into a Default folder.").weak());
            }
            folder_rows(ui, &book.folders, 0, true, m, book, &view.target, actions, &[]);
        });
    });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("notes_detail_scroll").show(ui, |ui| {
            if let Some(s) = &m.status {
                ui.label(egui::RichText::new(s).weak());
            }
            if let Some(e) = error {
                ui.colored_label(crate::theme::standing::HOSTILE, e);
            }
            if m.import.is_some() {
                import_section(ui, m, book, actions);
                ui.separator();
            }
            let Some(folder) = book.find(&m.folder) else { return };
            folder_detail(ui, m, folder, book, view, systems, actions);
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn folder_rows(
    ui: &mut egui::Ui,
    folders: &[Folder],
    depth: usize,
    top: bool,
    m: &mut NotesManager,
    book: &NoteBook,
    target: &str,
    actions: &mut Vec<ManagerAction>,
    rails: &[bool],
) {
    const INDENT: f32 = 22.0;
    let caret_w = ui.spacing().interact_size.y;
    for (i, f) in folders.iter().enumerate() {
        let last = i + 1 == folders.len();
        // Offline folders fold away by default, since nothing in them is in use.
        let open = m.open.get(&f.id).copied().unwrap_or(!top || f.online);
        let row = ui.horizontal(|ui| {
            ui.add_space(depth as f32 * INDENT);
            if f.children.is_empty() {
                ui.add_space(caret_w);
            } else {
                let caret = if open { icon::CARET_DOWN } else { icon::CARET_RIGHT };
                let hint = if open { "Collapse" } else { "Expand" };
                if ui
                    .add_sized([caret_w, caret_w], egui::Button::new(caret).frame(false))
                    .on_hover_text(hint)
                    .clicked()
                {
                    m.open.insert(f.id.clone(), !open);
                }
            }
            if top {
                let mut on = f.online;
                if ui.checkbox(&mut on, "").on_hover_text("Online: shown and used by rules").changed() {
                    actions.push(ManagerAction::Op(NotesOp::SetOnline { id: f.id.clone(), on }));
                }
            }
            let selected = m.folder == f.id;
            let glyph = if selected { icon::FOLDER_OPEN } else { icon::FOLDER };
            let mut text = egui::RichText::new(format!("{glyph}  {}", f.name));
            if top && !f.online {
                text = text.weak().italics();
            }
            let r = selectable_chip(ui, selected, text);
            if r.clicked() {
                m.folder = f.id.clone();
                m.tag_edit = None;
            }
            if f.id == target {
                ui.label(egui::RichText::new(icon::CROSSHAIR_SIMPLE).weak()).on_hover_text("Quick edits go here");
            }
            if top && !f.online {
                ui.label(egui::RichText::new("offline").weak());
            }
            if !open && !f.children.is_empty() {
                ui.label(egui::RichText::new(format!("+{}", f.children.len())).weak())
                    .on_hover_text("Subfolders hidden");
            }
            r.context_menu(|ui| folder_menu(ui, f, top, m, book, actions));
        });
        if depth > 0 {
            // Tree guides under each ancestor's caret, drawn only where a later sibling still hangs off
            // that line, and stretched over the row gap so they join up.
            let rect = row.response.rect;
            let gap = ui.spacing().item_spacing.y;
            let (top, mid, bottom) = (rect.top() - gap, rect.center().y, rect.bottom() + gap);
            let stroke = egui::Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.7));
            let x_at = |level: usize| rect.left() + level as f32 * INDENT + caret_w * 0.5;
            let p = ui.painter();
            for (level, on) in rails.iter().enumerate() {
                if *on {
                    p.line_segment([egui::pos2(x_at(level), top), egui::pos2(x_at(level), bottom)], stroke);
                }
            }
            let x = x_at(depth - 1);
            p.line_segment([egui::pos2(x, top), egui::pos2(x, if last { mid } else { bottom })], stroke);
            p.line_segment([egui::pos2(x, mid), egui::pos2(rect.left() + depth as f32 * INDENT, mid)], stroke);
        }
        if open {
            let mut child_rails = rails.to_vec();
            if depth > 0 {
                child_rails.push(!last);
            }
            folder_rows(ui, &f.children, depth + 1, false, m, book, target, actions, &child_rails);
        }
    }
}

fn folder_menu(
    ui: &mut egui::Ui,
    f: &Folder,
    top: bool,
    m: &mut NotesManager,
    book: &NoteBook,
    actions: &mut Vec<ManagerAction>,
) {
    let id = f.id.clone();
    let mut pick = None;
    if ui.button(format!("{}  New subfolder", icon::FOLDER_PLUS)).clicked() {
        pick = Some(ManagerAction::CreateFolder { parent: Some(id.clone()) });
    }
    if ui.button(format!("{}  Rename", icon::PENCIL_SIMPLE)).clicked() {
        m.folder = id.clone();
        m.rename = Some((id.clone(), f.name.clone()));
        m.rename_focus = true;
        ui.close();
    }
    if ui.button(format!("{}  Use for quick edits", icon::CROSSHAIR_SIMPLE)).clicked() {
        pick = Some(ManagerAction::SetTarget(id.clone()));
    }
    if top {
        let (label, on) = if f.online {
            (format!("{}  Take offline", icon::CLOUD_SLASH), false)
        } else {
            (format!("{}  Bring online", icon::CLOUD), true)
        };
        if ui.button(label).clicked() {
            pick = Some(ManagerAction::Op(NotesOp::SetOnline { id: id.clone(), on }));
        }
    }
    ui.separator();
    if ui.button(format!("{}  Export to file…", icon::FILE_ARROW_DOWN)).clicked() {
        pick = Some(ManagerAction::ExportFile(id.clone()));
    }
    if ui.button(format!("{}  Copy as JSON", icon::COPY)).clicked() {
        pick = Some(ManagerAction::Copy { id: id.clone(), compressed: false });
    }
    if ui.button(format!("{}  Copy compressed", icon::COPY)).clicked() {
        pick = Some(ManagerAction::Copy { id: id.clone(), compressed: true });
    }
    ui.separator();
    if !top && ui.button(format!("{}  Move to top level", icon::ARROW_RIGHT)).clicked() {
        pick = Some(ManagerAction::Op(NotesOp::MoveFolder { id: id.clone(), parent: None }));
    }
    let mut inside = Vec::new();
    collect_ids(f, &mut inside);
    ui.menu_button(format!("{}  Move into", icon::FOLDER), |ui| {
        for (other, path) in folder_choices(book) {
            if inside.contains(&other) {
                continue;
            }
            if ui.button(path).clicked() {
                pick = Some(ManagerAction::Op(NotesOp::MoveFolder { id: id.clone(), parent: Some(other) }));
            }
        }
    });
    ui.separator();
    if ui.button(format!("{}  Delete…", icon::TRASH)).clicked() {
        m.confirm = Some(Confirm::Folder { id: id.clone(), name: f.name.clone(), items: count_items(f) });
        ui.close();
    }
    if let Some(p) = pick {
        actions.push(p);
        ui.close();
    }
}

fn collect_ids(f: &Folder, out: &mut Vec<String>) {
    out.push(f.id.clone());
    for c in &f.children {
        collect_ids(c, out);
    }
}

fn count_items(f: &Folder) -> usize {
    f.tags.len() + f.systems.len() + f.pilots.len() + f.children.iter().map(count_items).sum::<usize>()
}

fn folder_detail(
    ui: &mut egui::Ui,
    m: &mut NotesManager,
    folder: &Folder,
    book: &NoteBook,
    view: &NotesView,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    actions: &mut Vec<ManagerAction>,
) {
    let kind = m.kind;
    let top = book.is_top(&folder.id);
    let path = book.path(&folder.id);
    if path.len() > 1 {
        ui.label(egui::RichText::new(path[..path.len() - 1].join(" / ")).weak());
    }
    let renaming = m.rename.as_ref().is_some_and(|(id, _)| *id == folder.id);
    if renaming {
        let mut done = None;
        ui.horizontal(|ui| {
            let (_, text) = m.rename.as_mut().expect("renaming");
            let r = ui.add(
                egui::TextEdit::singleline(text)
                    .font(egui::TextStyle::Heading)
                    .desired_width(320.0),
            );
            if std::mem::take(&mut m.rename_focus) {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                done = Some(true);
            }
            if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = Some(false);
            }
            if ui.button(icon::CHECK).on_hover_text("Rename").clicked() {
                done = Some(true);
            }
            if ui.button(icon::X).on_hover_text("Cancel").clicked() {
                done = Some(false);
            }
        });
        match done {
            Some(true) => {
                let (id, name) = m.rename.take().expect("renaming");
                actions.push(ManagerAction::Op(NotesOp::RenameFolder { id, name }));
            }
            Some(false) => m.rename = None,
            None => {}
        }
    } else {
        ui.horizontal(|ui| {
            ui.heading(&folder.name);
            if ui.button(icon::PENCIL_SIMPLE).on_hover_text("Rename folder").clicked() {
                m.rename = Some((folder.id.clone(), folder.name.clone()));
                m.rename_focus = true;
            }
            ui.menu_button(icon::EXPORT, |ui| {
                if ui.button(format!("{}  Export to file…", icon::FILE_ARROW_DOWN)).clicked() {
                    actions.push(ManagerAction::ExportFile(folder.id.clone()));
                    ui.close();
                }
                if ui.button(format!("{}  Copy as JSON", icon::COPY)).clicked() {
                    actions.push(ManagerAction::Copy { id: folder.id.clone(), compressed: false });
                    ui.close();
                }
                if ui.button(format!("{}  Copy compressed", icon::COPY)).clicked() {
                    actions.push(ManagerAction::Copy { id: folder.id.clone(), compressed: true });
                    ui.close();
                }
            })
            .response
            .on_hover_text("Export folder");
            if ui.button(icon::FOLDER_PLUS).on_hover_text("New subfolder").clicked() {
                actions.push(ManagerAction::CreateFolder { parent: Some(folder.id.clone()) });
            }
            if ui.button(icon::TRASH).on_hover_text("Delete folder").clicked() {
                m.confirm = Some(Confirm::Folder { id: folder.id.clone(), name: folder.name.clone(), items: count_items(folder) });
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        if top {
            let mut on = folder.online;
            if ui
                .checkbox(&mut on, "Online")
                .on_hover_text("Offline folders, their subfolders and their tags are ignored everywhere")
                .changed()
            {
                actions.push(ManagerAction::Op(NotesOp::SetOnline { id: folder.id.clone(), on }));
            }
        } else {
            ui.label(egui::RichText::new("Online state follows the top-level folder").weak());
        }
        if view.target == folder.id {
            ui.label(egui::RichText::new(format!("{}  Quick edits go here", icon::CROSSHAIR_SIMPLE)).weak());
        } else if ui.button(format!("{}  Use for quick edits", icon::CROSSHAIR_SIMPLE)).clicked() {
            actions.push(ManagerAction::SetTarget(folder.id.clone()));
        }
    });
    ui.add_space(8.0);

    let noun = match kind {
        NoteKind::System => "System",
        NoteKind::Pilot => "Pilot",
    };
    let n_tags = folder.tags.iter().filter(|t| t.kind == kind).count();
    let n_notes = folder.entries(kind).len();
    ui.horizontal(|ui| {
        for (tab, label) in [
            (DetailTab::Notes, format!("{}  {noun} notes ({n_notes})", icon::NOTE)),
            (DetailTab::Tags, format!("{}  {noun} tags ({n_tags})", icon::TAG)),
        ] {
            if selectable_chip(ui, m.tab == tab, label).clicked() {
                m.tab = tab;
            }
        }
    });
    ui.separator();
    if m.tab == DetailTab::Tags {
        tags_tab(ui, m, folder, book, view, kind, noun, actions);
    } else {
        notes_tab(ui, m, folder, book, kind, noun, systems, actions);
    }
}

#[allow(clippy::too_many_arguments)]
fn tags_tab(
    ui: &mut egui::Ui,
    m: &mut NotesManager,
    folder: &Folder,
    book: &NoteBook,
    view: &NotesView,
    kind: NoteKind,
    noun: &str,
    actions: &mut Vec<ManagerAction>,
) {
    let tags: Vec<_> = folder.tags.iter().filter(|t| t.kind == kind).collect();
    if tags.is_empty() {
        ui.label(egui::RichText::new("None yet.").weak());
    }
    for t in tags {
        let editing = m.tag_edit.as_ref().is_some_and(|e| e.id == t.id);
        if editing {
            let choices = folder_choices(book);
            let e = m.tag_edit.as_mut().expect("editing");
            let mut done = None;
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(160.0));
                ui.color_edit_button_srgb(&mut e.color);
                let path = choices.iter().find(|(id, _)| *id == e.folder).map(|(_, p)| p.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt(("tag_folder", &t.id)).selected_text(path).show_ui(ui, |ui| {
                    for (id, p) in &choices {
                        if ui.add(egui::Button::new(p.as_str()).selected(*id == e.folder)).clicked() {
                            e.folder = id.clone();
                        }
                    }
                });
                if ui.button(format!("{}  Save", icon::CHECK)).clicked() {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    done = Some(false);
                }
            });
            if done == Some(true) {
                let e = m.tag_edit.take().expect("editing");
                actions.push(ManagerAction::Op(NotesOp::PutTag {
                    folder: String::new(),
                    id: Some(e.id.clone()),
                    kind,
                    name: e.name,
                    color: e.color,
                }));
                if e.folder != folder.id {
                    actions.push(ManagerAction::Op(NotesOp::MoveTag { id: e.id, to: e.folder }));
                }
            } else if done == Some(false) {
                m.tag_edit = None;
            }
            continue;
        }
        let uses: usize = book.all().iter().map(|f| f.entries(kind).values().filter(|e| e.tags.contains(&t.id)).count()).sum();
        ui.horizontal(|ui| {
            tag_chip(ui, t, false);
            ui.label(egui::RichText::new(format!("{uses} used")).weak());
            if ui.button(icon::PENCIL_SIMPLE).on_hover_text("Rename, recolour or move").clicked() {
                m.tag_edit = Some(TagEdit { id: t.id.clone(), name: t.name.clone(), color: t.color, folder: folder.id.clone() });
            }
            if ui.button(icon::TRASH).on_hover_text("Delete tag").clicked() {
                m.confirm = Some(Confirm::Tag { id: t.id.clone(), name: t.name.clone(), uses });
            }
        });
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut m.new_tag).hint_text("New tag").desired_width(160.0));
        ui.color_edit_button_srgb(&mut m.new_color);
        if ui.add_enabled(!m.new_tag.trim().is_empty(), egui::Button::new(format!("{}  Add", icon::PLUS))).clicked() {
            actions.push(ManagerAction::Op(NotesOp::PutTag {
                folder: folder.id.clone(),
                id: None,
                kind,
                name: std::mem::take(&mut m.new_tag),
                color: m.new_color,
            }));
            m.new_color = crate::notes::default_color(folder.tags.len() + 1);
        }
    });
    ui.add_space(8.0);
    egui::CollapsingHeader::new(format!("Built-in {} tags", noun.to_lowercase()))
        .id_salt(("builtin", noun))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Shared by every folder. Colours are yours to change.").weak());
            egui::Grid::new(("builtin_grid", noun)).num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                for shipped in crate::notes::default_tags().iter().filter(|t| t.kind == kind) {
                    let current = view.tag(&shipped.id).cloned().unwrap_or_else(|| shipped.clone());
                    tag_chip(ui, &current, false);
                    let mut c = current.color;
                    if ui.color_edit_button_srgb(&mut c).changed() {
                        actions.push(ManagerAction::DefaultColor { id: shipped.id.clone(), color: Some(c) });
                    }
                    if current.color != shipped.color {
                        if ui.button("Reset").on_hover_text("Back to the shipped colour").clicked() {
                            actions.push(ManagerAction::DefaultColor { id: shipped.id.clone(), color: None });
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn notes_tab(
    ui: &mut egui::Ui,
    m: &mut NotesManager,
    folder: &Folder,
    book: &NoteBook,
    kind: NoteKind,
    noun: &str,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    actions: &mut Vec<ManagerAction>,
) {
    let entries = folder.entries(kind);
    if entries.is_empty() {
        ui.label(egui::RichText::new(format!("No {} notes in this folder.", noun.to_lowercase())).weak());
    }
    ui.horizontal(|ui| {
        ui.label(icon::MAGNIFYING_GLASS);
        ui.add(egui::TextEdit::singleline(&mut m.search).hint_text("Name, note or tag").desired_width(220.0));
    });
    let needle = m.search.trim().to_lowercase();
    let choices = folder_choices(book);
    for (key, e) in entries {
        let tag_names: Vec<&crate::notes::Tag> = e.tags.iter().filter_map(|t| book.tag(t)).collect();
        if !needle.is_empty()
            && !e.name.to_lowercase().contains(&needle)
            && !e.note.to_lowercase().contains(&needle)
            && !tag_names.iter().any(|t| t.name.to_lowercase().contains(&needle))
        {
            continue;
        }
        let subject = match kind {
            NoteKind::System => Subject::System(*key),
            NoteKind::Pilot => Subject::Pilot { id: *key, name: e.name.clone() },
        };
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let name = match kind {
                NoteKind::System => systems
                    .as_ref()
                    .and_then(|g| g.info_of(*key))
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| e.name.clone()),
                NoteKind::Pilot => e.name.clone(),
            };
            ui.label(egui::RichText::new(name).strong());
            for t in &tag_names {
                tag_chip(ui, t, false);
            }
        });
        if !e.note.is_empty() {
            let first: String = e.note.lines().next().unwrap_or_default().chars().take(120).collect();
            let more = e.note.chars().count() > first.chars().count();
            ui.label(egui::RichText::new(if more { format!("{first}…") } else { first }).weak());
        }
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Edit", icon::PENCIL_SIMPLE)).clicked() {
                actions.push(ManagerAction::Edit { folder: folder.id.clone(), subject: subject.clone() });
            }
            if ui.button(format!("{}  Show", icon::ARROW_SQUARE_OUT)).clicked() {
                actions.push(ManagerAction::Go(subject.clone()));
            }
            ui.menu_button(format!("{}  Move to", icon::FOLDER), |ui| {
                for (id, path) in &choices {
                    if *id == folder.id {
                        continue;
                    }
                    if ui.button(path.as_str()).clicked() {
                        actions.push(ManagerAction::Op(NotesOp::MoveEntry {
                            from: folder.id.clone(),
                            to: id.clone(),
                            subject: subject.clone(),
                        }));
                        ui.close();
                    }
                }
            });
            if ui.button(format!("{}  Remove", icon::TRASH)).clicked() {
                actions.push(ManagerAction::Op(NotesOp::RemoveEntry { folder: folder.id.clone(), subject: subject.clone() }));
            }
        });
    }
}

fn import_section(ui: &mut egui::Ui, m: &mut NotesManager, book: &NoteBook, actions: &mut Vec<ManagerAction>) {
    let selected_path = book.path(&m.folder).join(" / ");
    let Some(imp) = m.import.as_mut() else { return };
    ui.label(egui::RichText::new(format!("{}  Import a folder", icon::DOWNLOAD_SIMPLE)).strong());
    ui.label(egui::RichText::new("Paste exported JSON or compressed text, or open a file.").weak());
    ui.add(egui::TextEdit::multiline(&mut imp.text).desired_rows(4).desired_width(f32::INFINITY).code_editor());
    if !selected_path.is_empty() {
        ui.checkbox(&mut imp.into_selected, format!("Put a new folder inside \"{selected_path}\""));
    }
    if let Some(e) = &imp.error {
        ui.colored_label(crate::theme::standing::HOSTILE, e);
    }
    let mut close = false;
    if let Some(export) = imp.clash.clone() {
        ui.colored_label(
            crate::theme::standing::WARNING,
            format!("\"{}\" is already here. Overwrite drops the existing folder; merge keeps both and the import wins where they overlap.", export.folder.name),
        );
        ui.horizontal(|ui| {
            if ui.button("Overwrite").clicked() {
                actions.push(ManagerAction::Import { export: export.clone(), mode: ImportMode::Overwrite });
            }
            if ui.button("Merge").clicked() {
                actions.push(ManagerAction::Import { export: export.clone(), mode: ImportMode::Merge });
            }
            if ui.button("Cancel").clicked() {
                imp.clash = None;
            }
        });
        return;
    }
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Open file…", icon::FILE_ARROW_UP)).clicked() {
            actions.push(ManagerAction::OpenImportFile);
        }
        if ui.add_enabled(!imp.text.trim().is_empty(), egui::Button::new(format!("{}  Import", icon::CHECK))).clicked() {
            match crate::notes::parse_export(&imp.text) {
                Ok(export) => {
                    imp.error = None;
                    if book.find(&export.folder.id).is_some() {
                        imp.clash = Some(export);
                    } else {
                        actions.push(ManagerAction::Import { export, mode: ImportMode::Add });
                    }
                }
                Err(e) => imp.error = Some(e),
            }
        }
        if ui.button("Cancel").clicked() {
            close = true;
        }
    });
    if close {
        m.import = None;
    }
}

fn confirm_modal(ui: &mut egui::Ui, m: &mut NotesManager, actions: &mut Vec<ManagerAction>) {
    let Some(c) = &m.confirm else { return };
    let (heading, body, op) = match c {
        Confirm::Folder { id, name, items } => (
            format!("Delete \"{name}\"?"),
            format!("The folder, its subfolders and their {items} tags and notes are deleted. Export it first to keep a copy."),
            NotesOp::DeleteFolder { id: id.clone() },
        ),
        Confirm::Tag { id, name, uses } => (
            format!("Delete tag \"{name}\"?"),
            format!("It is removed from {uses} entries. Alert rules that name it stop matching."),
            NotesOp::DeleteTag { id: id.clone() },
        ),
    };
    let mut answer = None;
    let resp = egui::Modal::new(egui::Id::new("notes_confirm")).show(ui.ctx(), |ui| {
        ui.set_max_width(380.0);
        ui.heading(heading);
        ui.add_space(4.0);
        ui.label(body);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Delete", icon::TRASH)).clicked() {
                answer = Some(true);
            }
            if ui.button("Cancel").clicked() {
                answer = Some(false);
            }
        });
    });
    if answer == Some(true) {
        actions.push(ManagerAction::Op(op));
    }
    if answer.is_some() || resp.should_close() {
        m.confirm = None;
    }
}
