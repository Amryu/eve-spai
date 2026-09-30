//! What the shared wormhole map sees of the web app: New Eden from the universe file, the group's
//! holes, and settings kept for this browser.

use std::collections::HashMap;
use std::sync::Arc;

use spai_core::geo::Systems;
use spai_core::wormholes::Wormhole;
use spai_ui::wh_graph::WhGraphView;
use spai_ui::wh_tab::{WhHost, WhPrefs};

pub struct WebHost {
    pub geo: Arc<Systems>,
    pub holes: Vec<Wormhole>,
    pub prefs: WhPrefs,
    pub layout: HashMap<i64, egui::Pos2>,
}

impl WebHost {
    pub fn new(geo: Arc<Systems>) -> Self {
        let prefs = WhPrefs { pin_jumps: 10, layout_style: "tree".into(), layout_pack: true, ..Default::default() };
        WebHost { geo, holes: Vec::new(), prefs, layout: HashMap::new() }
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
    }

    fn saved_layout(&self) -> HashMap<i64, egui::Pos2> {
        self.layout.clone()
    }

    fn save_layout(&mut self, id: i64, at: egui::Pos2) {
        self.layout.insert(id, at);
    }

    fn clear_layout(&mut self) {
        self.layout.clear();
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
        egui::Panel::right("wh_web_side").resizable(true).default_size(280.0).show_inside(ui, |ui| {
            ui.heading(&info.name);
            ui.label(egui::RichText::new(format!("{} \u{b7} {:.1}", info.region, info.security)).weak());
            ui.separator();
            let here: Vec<&Wormhole> = holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).collect();
            if here.is_empty() {
                ui.label(egui::RichText::new("No holes known here").weak());
            }
            for w in here {
                let far = if w.system_id == sel { w.dest_system_id } else { Some(w.system_id) };
                let far = far.and_then(|id| geo.info_of(id)).map_or_else(|| w.dest.label().to_owned(), |i| i.name.clone());
                ui.label(format!("{} \u{2192} {far}", w.signature.as_deref().unwrap_or("?")));
            }
        });
    }
}
