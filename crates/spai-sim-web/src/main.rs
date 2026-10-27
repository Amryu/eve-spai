//! EVE Spai's fitting simulator in the browser. Built with trunk (`trunk serve` in this
//! directory); served at eve-spai.com/sim/.

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod worker;
#[allow(dead_code)]
mod store;

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().and_then(|w| w.document()).expect("a document");
        let canvas = document.get_element_by_id("spai").expect("the canvas").dyn_into::<web_sys::HtmlCanvasElement>().expect("a canvas");
        let started = eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(app::SimWebApp::new(cc)))))
            .await;
        if let Some(el) = document.get_element_by_id("loading") {
            match started {
                Ok(()) => el.remove(),
                Err(e) => el.set_text_content(Some(&format!("EVE Spai could not start: {e:?}"))),
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("spai-sim-web runs in the browser: `trunk serve` in crates/spai-sim-web");
}
