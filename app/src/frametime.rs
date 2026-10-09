//! Opt-in slow-frame log, `EVE_SPAI_FRAME_LOG=1`: a frame whose UI code or painting ran long is
//! printed with the time each step took, so a stall can be pinned on what caused it.

use std::time::{Duration, Instant};

/// A frame slower than this is logged.
const SLOW: Duration = Duration::from_millis(20);

pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("EVE_SPAI_FRAME_LOG").is_some())
}

pub struct FrameTimer {
    on: bool,
    start: Instant,
    last: Instant,
    steps: Vec<(&'static str, Duration)>,
}

impl FrameTimer {
    pub fn start() -> Self {
        let now = Instant::now();
        FrameTimer { on: enabled(), start: now, last: now, steps: Vec::new() }
    }

    /// The time since the previous mark, under `name`.
    pub fn mark(&mut self, name: &'static str) {
        if !self.on {
            return;
        }
        let now = Instant::now();
        self.steps.push((name, now - self.last));
        self.last = now;
    }

    /// Logs the frame if it was slow. `prev_cpu` is eframe's figure for the previous frame, painting
    /// and texture uploads included, which the UI code's own time leaves out.
    pub fn finish(self, view: &str, prev_cpu: Option<f32>) {
        if !self.on {
            return;
        }
        let total = self.start.elapsed();
        let spans = take_spans();
        let prev = prev_cpu.map(Duration::from_secs_f32).unwrap_or_default();
        if total < SLOW && prev < SLOW {
            return;
        }
        let mut steps: Vec<_> = self.steps.into_iter().chain(spans).filter(|(_, d)| *d >= Duration::from_millis(1)).collect();
        steps.sort_by(|a, b| b.1.cmp(&a.1));
        let parts: Vec<String> = steps.iter().map(|(n, d)| format!("{n} {:.1}", d.as_secs_f64() * 1e3)).collect();
        eprintln!(
            "[frame] {view}: ui {:.1} ms, previous frame with painting {:.1} ms; {}",
            total.as_secs_f64() * 1e3,
            prev.as_secs_f64() * 1e3,
            parts.join(", ")
        );
    }
}

thread_local! {
    static SPANS: std::cell::RefCell<Vec<(&'static str, Duration)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Times the rest of the enclosing scope as a named part of the current frame.
pub struct Span(Option<(&'static str, Instant)>);

pub fn span(name: &'static str) -> Span {
    Span(enabled().then(|| (name, Instant::now())))
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, at)) = self.0.take() {
            SPANS.with(|s| s.borrow_mut().push((name, at.elapsed())));
        }
    }
}

fn take_spans() -> Vec<(&'static str, Duration)> {
    SPANS.with(|s| std::mem::take(&mut *s.borrow_mut()))
}
