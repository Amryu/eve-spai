//! Small note and tag widgets shared by cards, tooltips, the map menu and the note editor.

use super::*;

/// The note editor's working copy of one subject in one folder.
pub(crate) struct NoteDraft {
    pub(crate) subject: crate::notes::Subject,
    /// Empty means the Default folder, which the first save creates.
    pub(crate) folder: String,
    pub(crate) note: String,
    pub(crate) tags: Vec<String>,
    pub(crate) new_tag: String,
    pub(crate) new_color: [u8; 3],
}

impl NoteDraft {
    /// Reads this folder's entry, so switching folders shows what is already saved there.
    pub(crate) fn load(&mut self, book: &crate::notes::NoteBook) {
        let e = book.find(&self.folder).and_then(|f| f.entries(self.subject.kind()).get(&self.subject.key()));
        self.note = e.map(|e| e.note.clone()).unwrap_or_default();
        self.tags = e.map(|e| e.tags.clone()).unwrap_or_default();
    }
}

/// A subject's tags and notes, resolved so a compact tip can carry them into its own viewport.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NoteTip {
    pub(crate) tags: Vec<crate::notes::Tag>,
    /// (folder path, note), only folders that have a note.
    pub(crate) notes: Vec<(String, String)>,
}

impl NoteTip {
    pub(crate) fn of(view: &crate::notes::NotesView, m: Option<&crate::notes::Merged>) -> Option<Self> {
        let m = m?;
        Some(NoteTip {
            tags: view.tags_of(m).cloned().collect(),
            notes: m.parts.iter().filter(|p| !p.note.is_empty()).map(|p| (p.path.clone(), p.note.clone())).collect(),
        })
    }
}

pub(crate) fn note_tip_ui(ui: &mut egui::Ui, n: &NoteTip) {
    if !n.tags.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for t in &n.tags {
                tag_chip(ui, t, false);
            }
        });
    }
    for (path, note) in &n.notes {
        ui.label(egui::RichText::new(format!("{}  {path}", egui_phosphor::regular::NOTE)).weak());
        ui.add(egui::Label::new(note.as_str()).wrap());
    }
}

/// One user tag as a chip, coloured by the tag. Hover only: the menu on the chip it follows is the
/// way to change it.
pub(crate) fn tag_chip(ui: &mut egui::Ui, t: &crate::notes::Tag, compact: bool) -> egui::Response {
    let col = crate::notes::color32(t.color);
    if compact {
        return ui
            .add(egui::Label::new(egui::RichText::new(egui_phosphor::regular::TAG).color(col)).sense(egui::Sense::hover()))
            .on_hover_text(&t.name);
    }
    let fill = egui::Color32::from_rgba_unmultiplied(t.color[0], t.color[1], t.color[2], 40);
    ui.add(
        egui::Button::new(egui::RichText::new(&t.name).color(col).strong())
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, col.gamma_multiply(0.6)))
            .sense(egui::Sense::hover()),
    )
}

/// The right-click menu on a system or pilot: tag toggles for the quick-edit folder and the full
/// editor. Unticking only touches that folder, so the menu names the other folders that still set it.
pub(crate) fn notes_quick_menu(
    ui: &mut egui::Ui,
    view: &crate::notes::NotesView,
    folder_label: &str,
    subject: &crate::notes::Subject,
) -> Option<IntelClick> {
    use egui_phosphor::regular as icon;
    let mut out = notes_tag_toggles(ui, view, folder_label, subject);
    ui.separator();
    if ui.button(format!("{}  Edit note and tags…", icon::NOTE_PENCIL)).clicked() {
        out = Some(IntelClick::Annotate(subject.clone()));
        ui.close();
    }
    out
}

/// The tag checkboxes of [`notes_quick_menu`], alone, for menus that nest them.
pub(crate) fn notes_tag_toggles(
    ui: &mut egui::Ui,
    view: &crate::notes::NotesView,
    folder_label: &str,
    subject: &crate::notes::Subject,
) -> Option<IntelClick> {
    use egui_phosphor::regular as icon;
    let m = view.entry(subject);
    let mine = m.and_then(|m| m.part_in(&view.target));
    let mut out = None;
    ui.label(egui::RichText::new(format!("{}  Tags in {folder_label}", icon::TAG)).strong());
    // The alert window can be a couple of hundred pixels tall, and a popup is clipped to its window,
    // so the list scrolls within whatever the window leaves after the header and the editor button.
    let room = ui.ctx().content_rect().height() - 4.0 * ui.spacing().interact_size.y;
    egui::ScrollArea::vertical().max_height(room.clamp(48.0, 360.0)).show(ui, |ui| {
        for t in view.tags(subject.kind()) {
            let mut on = mine.is_some_and(|p| p.tags.contains(&t.id));
            let elsewhere: Vec<&str> = m
                .map(|m| {
                    m.parts
                        .iter()
                        .filter(|p| p.folder != view.target && p.tags.contains(&t.id))
                        .map(|p| p.path.as_str())
                        .collect()
                })
                .unwrap_or_default();
            let mut resp = ui.checkbox(&mut on, egui::RichText::new(&t.name).color(crate::notes::color32(t.color)));
            if !elsewhere.is_empty() {
                resp = resp.on_hover_text(format!("Also set in {}", elsewhere.join(", ")));
                ui.label(egui::RichText::new(format!("also in {}", elsewhere.join(", "))).weak());
            }
            if resp.changed() {
                out = Some(IntelClick::Notes(crate::notes::NotesOp::SetTag {
                    folder: view.target.clone(),
                    subject: subject.clone(),
                    tag: t.id.clone(),
                    on,
                }));
                ui.close();
            }
        }
    });
    out
}

/// The quick-edit folder's name for menus. Before anything is written there is no Default folder yet,
/// and the first write makes it.
pub(crate) fn notes_folder_label(view: &crate::notes::NotesView) -> String {
    if view.target.is_empty() {
        return crate::notes::DEFAULT_FOLDER.to_owned();
    }
    view.target_path.clone()
}
