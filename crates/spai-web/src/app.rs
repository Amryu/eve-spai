//! The web app: loads New Eden, then shows the wormhole map.

use std::sync::{Arc, Mutex};

use spai_core::universe::Universe;
use spai_ui::wh_graph::WhGraphView;

use crate::host::WebHost;

type Loading = Arc<Mutex<Option<Result<Universe, String>>>>;

pub struct WebApp {
    loading: Loading,
    host: Option<WebHost>,
    view: WhGraphView,
    error: Option<String>,
}

impl WebApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        spai_ui::theme::install_fonts_opts(&cc.egui_ctx, false);
        // The theme is EVE Spai's own, not the browser's light or dark preference.
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        spai_ui::theme::Theme::default().apply(&cc.egui_ctx);
        let loading: Loading = Default::default();
        let (slot, ctx) = (loading.clone(), cc.egui_ctx.clone());
        ehttp::fetch(ehttp::Request::get("universe.json.gz"), move |r| {
            let result = match r {
                Ok(resp) if resp.ok => Universe::from_gz(&resp.bytes),
                Ok(resp) => Err(format!("the universe file answered {} {}", resp.status, resp.status_text)),
                Err(e) => Err(e),
            };
            *slot.lock().unwrap() = Some(result);
            ctx.request_repaint();
        });
        WebApp { loading, host: None, view: WhGraphView::default(), error: None }
    }
}

impl eframe::App for WebApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.host.is_none() {
            match self.loading.lock().unwrap().take() {
                Some(Ok(u)) => self.host = Some(WebHost::new(Arc::new(u.systems()))),
                Some(Err(e)) => self.error = Some(e),
                None => {}
            }
        }
        egui::CentralPanel::default().show_inside(ui, |ui| match (&mut self.host, &self.error) {
            (Some(host), _) => spai_ui::wh_tab::show(&mut self.view, host, ui),
            (None, Some(e)) => {
                ui.label(egui::RichText::new(format!("New Eden did not load: {e}")).color(ui.visuals().error_fg_color));
            }
            (None, None) => {
                ui.centered_and_justified(|ui| ui.label(egui::RichText::new("Loading New Eden\u{2026}").weak()));
            }
        });
    }
}
