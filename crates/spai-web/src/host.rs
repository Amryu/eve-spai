//! What the shared wormhole map sees of the web app: New Eden from the universe file, the group's
//! holes, and settings kept for this browser.

use std::collections::HashMap;
use std::sync::Arc;

use spai_core::geo::Systems;
use spai_core::wormholes::{SystemSig, Wormhole};
use spai_ui::wh_graph::WhGraphView;
use spai_ui::wh_tab::{WhHost, WhPrefs};

pub struct WebHost {
    pub geo: Arc<Systems>,
    pub holes: Vec<Wormhole>,
    pub prefs: WhPrefs,
    pub layout: HashMap<i64, egui::Pos2>,
    pub sigs: HashMap<i64, Vec<SystemSig>>,
    /// Settings or the layout changed since they were last saved.
    pub dirty: bool,
}

impl WebHost {
    pub fn new(geo: Arc<Systems>) -> Self {
        let prefs = WhPrefs { pin_jumps: 10, layout_style: "tree".into(), layout_pack: true, ..Default::default() };
        WebHost { geo, holes: Vec::new(), prefs, layout: HashMap::new(), sigs: HashMap::new(), dirty: false }
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
                ui.strong("Holes");
                let here: Vec<&Wormhole> = holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).collect();
                if here.is_empty() {
                    ui.label(egui::RichText::new("None known here").weak());
                }
                egui::Grid::new("wh_web_holes").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
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
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.strong(format!("Signatures ({})", sigs.len()));
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
            });
        });
    }
}
