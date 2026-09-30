//! The wall clock lives in `spai_core`, so the web app and its tests stop it the same way.

pub use spai_core::clock::{system, utc};
#[cfg(test)]
pub use spai_core::clock::{freeze, frozen};

/// Seconds for an animation, from egui's clock; still while this thread's clock is stopped.
pub fn anim(ui: &egui::Ui) -> f64 {
    #[cfg(test)]
    if frozen().is_some() {
        return 0.0;
    }
    ui.input(|i| i.time)
}
