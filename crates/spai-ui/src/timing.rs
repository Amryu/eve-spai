//! Named spans of a frame for the desktop's slow-frame log, from code outside the app crate. Off
//! unless the app switches them on; the web build never does, so it never reads a clock it lacks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static ON: AtomicBool = AtomicBool::new(false);

thread_local! {
    static SPANS: std::cell::RefCell<Vec<(&'static str, Duration)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

/// Times the rest of the enclosing scope as a named part of the current frame.
pub struct Span(Option<(&'static str, std::time::Instant)>);

pub fn span(name: &'static str) -> Span {
    Span(ON.load(Ordering::Relaxed).then(|| (name, std::time::Instant::now())))
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, at)) = self.0.take() {
            SPANS.with(|s| s.borrow_mut().push((name, at.elapsed())));
        }
    }
}

/// This frame's spans, emptied.
pub fn take() -> Vec<(&'static str, Duration)> {
    SPANS.with(|s| std::mem::take(&mut *s.borrow_mut()))
}
