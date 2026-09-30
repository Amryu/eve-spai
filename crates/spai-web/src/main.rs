//! EVE Spai's wormhole map in the browser. Built with trunk (`trunk serve` in this directory).

#[allow(dead_code)]
mod accounts;
#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod auth;
#[cfg(target_arch = "wasm32")]
mod sync;
#[cfg(target_arch = "wasm32")]
mod page;
#[allow(dead_code)]
mod group;
#[allow(dead_code)]
mod host;
#[allow(dead_code)]
mod sso;
#[allow(dead_code)]
mod planner;
#[allow(dead_code)]
mod starmap;
#[allow(dead_code)]
mod store;

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().and_then(|w| w.document()).expect("a document");
        let canvas = document.get_element_by_id("spai").expect("the canvas").dyn_into::<web_sys::HtmlCanvasElement>().expect("a canvas");
        let started = eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(app::WebApp::new(cc)))))
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
    eprintln!("spai-web runs in the browser: `trunk serve` in crates/spai-web");
}
