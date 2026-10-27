//! The page's end of the fleet worker (`src/bin/fleet-worker.rs`): requests out by id, each
//! reply to the run that asked for it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use spai_fit_ui::offload::{decode, encode, OffFrame, WorkReply, WorkRequest};
use wasm_bindgen::{closure::Closure, JsCast as _};

/// The worker's ids for "the data could not be loaded" and "listening", as `fleet-worker.rs`
/// sends them.
const FAILED: u32 = u32::MAX;
const READY: u32 = u32::MAX - 1;

type Replies = Rc<RefCell<HashMap<u32, Box<dyn FnMut(WorkReply)>>>>;

pub struct FleetWorker {
    worker: web_sys::Worker,
    replies: Replies,
    next: Cell<u32>,
    alive: Rc<Cell<bool>>,
    /// Requests held until the worker listens; none once it does.
    held: Rc<RefCell<Option<Vec<Vec<u8>>>>>,
}

impl FleetWorker {
    /// The worker next to the page's own script; none where the browser will not start it.
    pub fn start(ctx: &egui::Context, url: &str) -> Option<Rc<Self>> {
        let worker = web_sys::Worker::new(url).map_err(|e| log::warn!("fleet worker: {e:?}")).ok()?;
        let replies: Replies = Rc::default();
        let alive = Rc::new(Cell::new(true));
        let held: Rc<RefCell<Option<Vec<Vec<u8>>>>> = Rc::new(RefCell::new(Some(Vec::new())));
        // Gone: the runs it owes end, and the page works their fights out itself.
        let die = {
            let (replies, alive) = (replies.clone(), alive.clone());
            move |why: String| {
                log::warn!("fleet worker: {why}");
                alive.set(false);
                replies.borrow_mut().clear();
            }
        };
        let on_message = {
            let (replies, ctx, die, held, worker) = (replies.clone(), ctx.clone(), die.clone(), held.clone(), worker.clone());
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
                let bytes = js_sys::Uint8Array::new(&e.data()).to_vec();
                if let Some((FAILED, why)) = decode::<(u32, String)>(&bytes) {
                    die(why);
                } else if let Some((READY, ())) = decode::<(u32, ())>(&bytes) {
                    for r in held.borrow_mut().take().unwrap_or_default() {
                        let _ = worker.post_message(&js_sys::Uint8Array::from(r.as_slice()));
                    }
                } else if let Some((id, reply)) = decode::<(u32, WorkReply)>(&bytes) {
                    let last = !matches!(reply, WorkReply::Progress(_));
                    // Out of the map while it runs: a reply may ask for more work.
                    let taken = replies.borrow_mut().remove(&id);
                    if let Some(mut f) = taken {
                        f(reply);
                        if !last {
                            replies.borrow_mut().insert(id, f);
                        }
                    }
                } else {
                    die("a reply it could not read".to_owned());
                }
                ctx.request_repaint();
            })
        };
        let on_error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |e: web_sys::ErrorEvent| die(e.message()));
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_message.forget();
        on_error.forget();
        Some(Rc::new(FleetWorker { worker, replies, next: Cell::new(0), alive, held }))
    }
}

impl OffFrame for FleetWorker {
    fn run(&self, request: WorkRequest, reply: Box<dyn FnMut(WorkReply)>) {
        if !self.alive.get() {
            return;
        }
        let id = self.next.get();
        self.next.set((id + 1) % READY);
        self.replies.borrow_mut().insert(id, reply);
        let bytes = encode(&(id, request));
        if let Some(held) = self.held.borrow_mut().as_mut() {
            held.push(bytes);
        } else if self.worker.post_message(&js_sys::Uint8Array::from(bytes.as_slice())).is_err() {
            self.replies.borrow_mut().remove(&id);
        }
    }

    fn alive(&self) -> bool {
        self.alive.get()
    }
}
