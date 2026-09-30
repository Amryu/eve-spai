//! What the shared wormhole map sees of the web app: New Eden from the universe file, the group's
//! holes, and settings kept for this browser.

use std::collections::HashMap;
use std::sync::Arc;

use spai_core::geo::Systems;
use spai_core::wormholes::{DestClass, Life, Mass, Source, SystemSig, Wormhole};
use spai_ui::wh_graph::WhGraphView;
use spai_ui::wh_tab::{WhHost, WhPrefs};

/// A change made on the map, for the app to store and send.
#[derive(Clone, Debug)]
pub enum Edit {
    Save(Wormhole),
    Dead(String),
}

/// The hole being added or edited in the side panel.
#[derive(Clone, Debug, Default)]
pub struct HoleForm {
    /// The hole edited; `None` adds one.
    pub uid: Option<String>,
    pub sig: String,
    /// A system name, or a kind of space ("Highsec", "C3").
    pub far: String,
    pub far_sig: String,
    pub wh_type: String,
    pub life: Option<Life>,
    pub mass: Option<Mass>,
    pub note: String,
    pub error: Option<String>,
}

impl HoleForm {
    fn of(w: &Wormhole, here: i64, geo: &Systems) -> Self {
        let near = w.system_id == here;
        let (sig, far, far_sig) = if near { (&w.signature, w.dest_system_id, &w.dest_signature) } else { (&w.dest_signature, Some(w.system_id), &w.signature) };
        HoleForm {
            uid: Some(w.uid.clone()),
            sig: sig.clone().unwrap_or_default(),
            far: far.and_then(|id| geo.info_of(id)).map_or_else(|| if w.dest == DestClass::Unknown { String::new() } else { w.dest.label().to_owned() }, |i| i.name.clone()),
            far_sig: far_sig.clone().unwrap_or_default(),
            wh_type: w.wh_type.clone().unwrap_or_default(),
            life: w.life,
            mass: w.mass,
            note: w.note.clone().unwrap_or_default(),
            error: None,
        }
    }

    /// The hole as the form leaves it, from `was` when editing; seen from `here`.
    fn apply(&self, was: Option<&Wormhole>, here: i64, geo: &Systems, now: i64) -> Result<Wormhole, String> {
        let text = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_uppercase());
        let far_id = if self.far.trim().is_empty() { None } else { geo.lookup(self.far.trim()).map(|i| i.id) };
        let far_kind = far_id.is_none().then(|| DestClass::from_words(&self.far)).flatten();
        if !self.far.trim().is_empty() && far_id.is_none() && far_kind.is_none() {
            return Err(format!("No system or kind of space called {:?}.", self.far.trim()));
        }
        let mut w = was.cloned().unwrap_or_else(|| Wormhole {
            uid: spai_share::crypto::random32()[..16].iter().map(|b| format!("{b:02x}")).collect(),
            system_id: here,
            source: Source::Manual,
            reported_at: now,
            ..Default::default()
        });
        // Kept the way round it was stored: a hole seen from its far end is edited from there.
        let near = w.system_id == here;
        let (sig, far_sig) = (text(&self.sig), text(&self.far_sig));
        if near {
            w.signature = sig;
            w.dest_signature = far_sig;
            w.dest_system_id = far_id;
            w.dest = far_id.map_or(far_kind.unwrap_or(DestClass::Unknown), |id| spai_core::wormholes::dest_class(geo, id));
        } else {
            w.dest_signature = sig;
            w.signature = far_sig;
        }
        w.wh_type = text(&self.wh_type);
        if w.life != self.life || w.mass != self.mass {
            w.explicit_expiry = w.expiry_after_reading(self.life, now);
            w.observed_at = Some(now);
        }
        w.life = self.life;
        w.mass = self.mass;
        w.note = (!self.note.trim().is_empty()).then(|| self.note.trim().to_owned());
        w.updated_at = now;
        Ok(w)
    }
}

pub struct WebHost {
    pub geo: Arc<Systems>,
    pub holes: Vec<Wormhole>,
    pub prefs: WhPrefs,
    pub layout: HashMap<i64, egui::Pos2>,
    pub sigs: HashMap<i64, Vec<SystemSig>>,
    /// Settings or the layout changed since they were last saved.
    pub dirty: bool,
    /// The character may share into a group, so holes can be added and edited.
    pub can_edit: bool,
    pub form: Option<HoleForm>,
    /// Changes made since the app last took them.
    pub edits: Vec<Edit>,
    side: Side,
    pin_input: String,
}

/// The side panel's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Side {
    #[default]
    Holes,
    Signatures,
    Routes,
}

impl WebHost {
    pub fn new(geo: Arc<Systems>) -> Self {
        let prefs = WhPrefs { pin_jumps: 10, layout_style: "tree".into(), layout_pack: true, ..Default::default() };
        WebHost { geo, holes: Vec::new(), prefs, layout: HashMap::new(), sigs: HashMap::new(), dirty: false, can_edit: false, form: None, edits: Vec::new(), side: Side::default(), pin_input: String::new() }
    }
}

/// Pinned on first use: the chains hang from these and routes are measured to them.
pub const DEFAULT_PINS: [&str; 3] = ["C-J6MT", "C-N4OD", "4-HWWF"];

impl WebHost {
    /// The Routes tab: how far each pinned system is from the selected one, and the pins
    /// themselves to add and remove.
    fn routes(&mut self, ui: &mut egui::Ui, geo: &Systems, sel: i64, sel_name: &str) {
        let pinned = self.prefs.route_pins.iter().any(|p| p.eq_ignore_ascii_case(sel_name));
        let label = if pinned { format!("Unpin {sel_name}") } else { format!("Pin {sel_name}") };
        let mut change: Option<(String, bool)> = None;
        if ui.button(format!("{}  {label}", egui_phosphor::regular::PUSH_PIN)).clicked() {
            change = Some((sel_name.to_owned(), !pinned));
        }
        ui.add_space(4.0);
        if self.prefs.route_pins.is_empty() {
            ui.label(egui::RichText::new("No pinned systems. Pin the systems you stage in: chains hang from them and routes are measured to them.").weak());
        }
        egui::Grid::new("wh_web_pins").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
            for name in &self.prefs.route_pins {
                let info = geo.lookup(name);
                ui.label(egui::RichText::new(name).strong());
                let jumps = info.and_then(|i| geo.jumps(sel, i.id, 60));
                ui.label(egui::RichText::new(match jumps {
                    Some(0) => "here".to_owned(),
                    Some(1) => "1 jump".to_owned(),
                    Some(n) => format!("{n} jumps"),
                    None => "no route".to_owned(),
                }).weak())
                .on_hover_text("By gate and Ansiblex; holes are on the map");
                if ui.small_button(egui_phosphor::regular::X).on_hover_text("Unpin").clicked() {
                    change = Some((name.clone(), false));
                }
                ui.end_row();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.pin_input).hint_text("System to pin").desired_width(160.0));
            let go = ui.button("Pin").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                match geo.lookup(self.pin_input.trim()).or_else(|| geo.lookup_prefix(self.pin_input.trim())) {
                    Some(i) => {
                        change = Some((i.name.clone(), true));
                        self.pin_input.clear();
                    }
                    None => self.pin_input = format!("{} (no such system)", self.pin_input.trim()),
                }
            }
        });
        if let Some((name, on)) = change {
            self.prefs.route_pins.retain(|p| !p.eq_ignore_ascii_case(&name));
            if on {
                self.prefs.route_pins.push(name);
            }
            self.dirty = true;
        }
    }
}

impl WhHost for WebHost {
    fn systems(&self) -> Option<Arc<Systems>> {
        Some(self.geo.clone())
    }

    fn holes(&self, now: i64) -> Vec<Wormhole> {
        self.holes.iter().filter(|w| !w.is_expired(now)).cloned().collect()
    }

    fn characters(&self) -> HashMap<String, (i64, bool)> {
        HashMap::new()
    }

    fn prefs(&self) -> WhPrefs {
        self.prefs.clone()
    }

    fn set_prefs(&mut self, prefs: WhPrefs) {
        self.prefs = prefs;
        self.dirty = true;
    }

    fn saved_layout(&self) -> HashMap<i64, egui::Pos2> {
        self.layout.clone()
    }

    fn save_layout(&mut self, id: i64, at: egui::Pos2) {
        self.layout.insert(id, at);
        self.dirty = true;
    }

    fn clear_layout(&mut self) {
        self.layout.clear();
        self.dirty = true;
    }

    fn blocked(&self, _: &Wormhole, _: i64) -> bool {
        false
    }

    fn disabled(&self, _: &Wormhole) -> bool {
        false
    }

    fn disabled_count(&self) -> usize {
        0
    }

    fn disabled_systems(&self) -> Vec<i64> {
        Vec::new()
    }

    fn clear_disabled(&mut self) {}

    fn group_name(&self, _: &str) -> Option<String> {
        None
    }

    fn open_system(&mut self, _: i64) {}

    fn system_menu(&mut self, _: &mut egui::Ui, _: i64) {}

    fn toolbar(&mut self, _: &mut WhGraphView, _: &mut egui::Ui) {}

    fn side_panel(
        &mut self,
        view: &mut WhGraphView,
        ui: &mut egui::Ui,
        geo: &Arc<Systems>,
        holes: &[Wormhole],
        _: &HashMap<String, (i64, bool)>,
        _: i64,
    ) {
        let Some(sel) = view.selected else { return };
        let Some(info) = geo.info_of(sel) else { return };
        let now = spai_core::clock::utc().timestamp();
        let sigs = self.sigs.get(&sel).cloned().unwrap_or_default();
        egui::Panel::right("wh_web_side").resizable(true).default_size(320.0).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading(&info.name);
                ui.label(egui::RichText::new(format!("{} \u{b7} {:.1}", info.region, info.security)).weak());
                ui.separator();
                if let Some(form) = &mut self.form {
                    let was = form.uid.as_ref().and_then(|u| holes.iter().find(|w| &w.uid == u)).cloned();
                    match hole_form(ui, form, was.is_some()) {
                        FormAct::Save => match form.apply(was.as_ref(), sel, geo, now) {
                            Ok(w) => {
                                self.edits.push(Edit::Save(w));
                                self.form = None;
                            }
                            Err(e) => form.error = Some(e),
                        },
                        FormAct::Dead => {
                            if let Some(w) = was {
                                self.edits.push(Edit::Dead(w.uid));
                            }
                            self.form = None;
                        }
                        FormAct::Cancel => self.form = None,
                        FormAct::None => {}
                    }
                    return;
                }
                let here_n = holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).count();
                ui.horizontal(|ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    ui.menu_value(&mut self.side, Side::Holes, format!("Holes ({here_n})"));
                    ui.menu_value(&mut self.side, Side::Signatures, format!("Signatures ({})", sigs.len()));
                    ui.menu_value(&mut self.side, Side::Routes, "Routes");
                });
                ui.separator();
                match self.side {
                    Side::Holes => {
                ui.horizontal(|ui| {
                    ui.strong("Holes");
                    if self.can_edit && ui.small_button(format!("{}  Add", egui_phosphor::regular::PLUS)).clicked() {
                        self.form = Some(HoleForm::default());
                    }
                });
                let here: Vec<&Wormhole> = holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).collect();
                if here.is_empty() {
                    ui.label(egui::RichText::new("None known here").weak());
                }
                let mut edit: Option<HoleForm> = None;
                egui::Grid::new("wh_web_holes").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                    for w in here {
                        let near = w.system_id == sel;
                        let (sig, far) = if near { (&w.signature, w.dest_system_id) } else { (&w.dest_signature, Some(w.system_id)) };
                        let far = far.and_then(|id| geo.info_of(id)).map_or_else(|| w.dest.label().to_owned(), |i| i.name.clone());
                        ui.monospace(sig.as_deref().unwrap_or("?"));
                        ui.label(format!("{} {far}", egui_phosphor::regular::ARROW_RIGHT));
                        let mut facts: Vec<String> = [w.wh_type.clone(), w.effective_size().map(|s| s.label().to_owned()), w.mass.map(|m| m.short().to_owned())].into_iter().flatten().collect();
                        if let Some(h) = w.hours_left(now) {
                            facts.push(format!("{h}h left"));
                        }
                        ui.label(egui::RichText::new(facts.join(" \u{b7} ")).weak());
                        if self.can_edit && ui.small_button(egui_phosphor::regular::PENCIL_SIMPLE).on_hover_text("Edit this hole").clicked() {
                            edit = Some(HoleForm::of(w, sel, geo));
                        }
                        ui.end_row();
                    }
                });
                if edit.is_some() {
                    self.form = edit;
                }
                    }
                    Side::Signatures => {
                if sigs.is_empty() {
                    ui.label(egui::RichText::new("No probe scan shared for this system").weak());
                }
                egui::Grid::new("wh_web_sigs").num_columns(3).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
                    let mut sigs = sigs.clone();
                    sigs.sort_by(|a, b| a.sig.cmp(&b.sig));
                    for s in &sigs {
                        let aged = spai_ui::widgets::age_color(ui.visuals(), now, s.updated_at);
                        let tint = |t: egui::RichText| match aged {
                            Some(c) => t.color(c),
                            None => t,
                        };
                        ui.label(tint(egui::RichText::new(&s.sig).monospace())).on_hover_text(&s.kind);
                        ui.label(tint(egui::RichText::new(spai_ui::widgets::found_at(s.added_at, now, true))))
                            .on_hover_text(spai_ui::widgets::found_hover(s.added_at, now, true));
                        let what = if s.name.is_empty() { spai_ui::wh_graph::short_group(&s.group).to_owned() } else { s.name.clone() };
                        ui.label(tint(egui::RichText::new(what)));
                        ui.end_row();
                    }
                });
                    }
                    Side::Routes => self.routes(ui, geo, sel, &info.name),
                }
            });
        });
    }
}

enum FormAct {
    None,
    Save,
    Dead,
    Cancel,
}

fn hole_form(ui: &mut egui::Ui, f: &mut HoleForm, editing: bool) -> FormAct {
    let mut act = FormAct::None;
    ui.strong(if editing { "Edit hole" } else { "Add a hole" });
    egui::Grid::new("wh_web_form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Signature");
        ui.add(egui::TextEdit::singleline(&mut f.sig).hint_text("ABC-123").desired_width(120.0));
        ui.end_row();
        ui.label("Leads to");
        ui.add(egui::TextEdit::singleline(&mut f.far).hint_text("System, or Highsec, C3\u{2026}").desired_width(180.0));
        ui.end_row();
        ui.label("Far signature");
        ui.add(egui::TextEdit::singleline(&mut f.far_sig).hint_text("XYZ-789").desired_width(120.0));
        ui.end_row();
        ui.label("Type");
        ui.add(egui::TextEdit::singleline(&mut f.wh_type).hint_text("K162, B274\u{2026}").desired_width(80.0));
        ui.end_row();
        ui.label("Life");
        egui::ComboBox::from_id_salt("wh_web_life").width(180.0).selected_text(f.life.map_or("Not known", Life::label)).show_ui(ui, |ui| {
            ui.selectable_value(&mut f.life, None, "Not known");
            for l in Life::ALL {
                ui.selectable_value(&mut f.life, Some(l), l.label());
            }
        });
        ui.end_row();
        ui.label("Mass");
        egui::ComboBox::from_id_salt("wh_web_mass").width(180.0).selected_text(f.mass.map_or("Not known", Mass::label)).show_ui(ui, |ui| {
            ui.selectable_value(&mut f.mass, None, "Not known");
            for m in Mass::ALL {
                ui.selectable_value(&mut f.mass, Some(m), m.label());
            }
        });
        ui.end_row();
        ui.label("Note");
        ui.add(egui::TextEdit::singleline(&mut f.note).desired_width(180.0));
        ui.end_row();
    });
    if let Some(e) = &f.error {
        ui.label(egui::RichText::new(e).color(ui.visuals().error_fg_color));
    }
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Save", egui_phosphor::regular::CHECK)).clicked() {
            act = FormAct::Save;
        }
        if ui.button("Cancel").clicked() {
            act = FormAct::Cancel;
        }
        if editing && ui.button(format!("{}  Collapsed", egui_phosphor::regular::X_CIRCLE)).on_hover_text("It is gone: take it off the map for everyone").clicked() {
            act = FormAct::Dead;
        }
    });
    act
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo() -> Systems {
        spai_core::test_support::small_universe(&[(31_000_200, "J100200".into(), -1.0, "A-R00001".into())])
    }

    #[test]
    fn a_new_hole_takes_the_form_and_a_fresh_id() {
        let g = geo();
        let f = HoleForm { sig: "abc-123".into(), far: "Jita".into(), life: Some(Life::UnderDay), ..Default::default() };
        let w = f.apply(None, 31_000_200, &g, 1_000).unwrap();
        assert_eq!((w.signature.as_deref(), w.dest_system_id, w.dest), (Some("ABC-123"), Some(30_000_142), DestClass::Highsec));
        assert_eq!(w.uid.len(), 32);
        assert_eq!(w.explicit_expiry, Some(1_000 + 86_400));
        assert!(HoleForm { far: "Nowhere".into(), ..Default::default() }.apply(None, 31_000_200, &g, 1_000).is_err());
    }

    #[test]
    fn a_hole_edited_from_its_far_end_stays_the_way_round_it_was() {
        let g = geo();
        let was = Wormhole { uid: "u".into(), system_id: 30_000_142, signature: Some("AAA-111".into()), dest_system_id: Some(31_000_200), dest_signature: Some("BBB-222".into()), ..Default::default() };
        let mut f = HoleForm::of(&was, 31_000_200, &g);
        assert_eq!((f.sig.as_str(), f.far.as_str(), f.far_sig.as_str()), ("BBB-222", "Jita", "AAA-111"));
        f.sig = "BBB-223".into();
        f.mass = Some(Mass::Critical);
        let w = f.apply(Some(&was), 31_000_200, &g, 5_000).unwrap();
        assert_eq!((w.system_id, w.signature.as_deref(), w.dest_signature.as_deref()), (30_000_142, Some("AAA-111"), Some("BBB-223")));
        assert_eq!((w.mass, w.observed_at), (Some(Mass::Critical), Some(5_000)));
    }
}
