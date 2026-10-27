//! The fleet work of the page, in a web worker: the page sends `(id, WorkRequest)`, the worker
//! answers `(id, WorkReply)`, both MessagePack. It loads the fitting data itself, and holds what
//! comes before it is in.

#[cfg(target_arch = "wasm32")]
mod worker {
    use std::cell::RefCell;
    use std::sync::Arc;

    use spai_fit_ui::offload::{decode, encode, WorkReply, WorkRequest, Worker};
    use wasm_bindgen::{closure::Closure, JsCast as _};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    /// What the page reads instead of a reply when the data could not be loaded.
    pub const FAILED: u32 = u32::MAX;
    /// Sent once the worker listens: a message before it is lost.
    pub const READY: u32 = u32::MAX - 1;

    thread_local! {
        static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
        static WAITING: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
    }

    fn scope() -> DedicatedWorkerGlobalScope {
        js_sys::global().unchecked_into()
    }

    fn post(bytes: &[u8]) {
        let _ = scope().post_message(&js_sys::Uint8Array::from(bytes));
    }

    fn handle(bytes: &[u8]) {
        let Some((id, request)) = decode::<(u32, WorkRequest)>(bytes) else {
            post(&encode(&(FAILED, "a request it could not read")));
            return;
        };
        WORKER.with_borrow_mut(|w| {
            if let Some(w) = w {
                w.serve(request, &mut |r: WorkReply| post(&encode(&(id, r))));
            }
        });
    }

    async fn load() -> Result<spai_fit::Dogma, String> {
        let scope = scope();
        let url = format!("{}/sde/{}", scope.location().origin(), spai_fit::dogma::blob::file_name());
        let resp: web_sys::Response = JsFuture::from(scope.fetch_with_str(&url)).await.map_err(|e| format!("{e:?}"))?.unchecked_into();
        if !resp.ok() {
            return Err(format!("{} {}", resp.status(), resp.status_text()));
        }
        let buf = JsFuture::from(resp.array_buffer().map_err(|e| format!("{e:?}"))?).await.map_err(|e| format!("{e:?}"))?;
        spai_fit::dogma::blob::decode(&js_sys::Uint8Array::new(&buf).to_vec()).map_err(|e| format!("{e:#}"))
    }

    pub fn start() {
        // A panic leaves the worker unusable: the page is told, and works the fleets out itself.
        std::panic::set_hook(Box::new(|info| post(&encode(&(FAILED, info.to_string())))));
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(|e: MessageEvent| {
            let bytes = js_sys::Uint8Array::new(&e.data()).to_vec();
            if WORKER.with_borrow(Option::is_some) {
                handle(&bytes);
            } else {
                WAITING.with_borrow_mut(|w| w.push(bytes));
            }
        });
        scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        post(&encode(&(READY, ())));
        wasm_bindgen_futures::spawn_local(async {
            match load().await {
                Ok(d) => {
                    WORKER.set(Some(Worker::new(Arc::new(d))));
                    for bytes in WAITING.take() {
                        handle(&bytes);
                    }
                }
                Err(e) => post(&encode(&(FAILED, e))),
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    worker::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
