//! The wall clock, read in one place so a test render can stop it: screenshots are compared pixel
//! for pixel, and fixtures and the UI both place things relative to now.

#[cfg(test)]
thread_local! {
    static FROZEN: std::cell::Cell<Option<i64>> = const { std::cell::Cell::new(None) };
}

/// Stops this thread's clock at `secs` (unix time), or starts it again with `None`.
#[cfg(test)]
pub fn freeze(secs: Option<i64>) {
    FROZEN.with(|f| f.set(secs));
}

/// This thread's stopped clock, if it is stopped.
#[cfg(test)]
pub fn frozen() -> Option<i64> {
    FROZEN.with(|f| f.get())
}

pub fn utc() -> chrono::DateTime<chrono::Utc> {
    #[cfg(test)]
    if let Some(s) = FROZEN.with(|f| f.get()) {
        return chrono::DateTime::from_timestamp(s, 0).expect("a sane frozen time");
    }
    chrono::Utc::now()
}

pub fn system() -> std::time::SystemTime {
    #[cfg(test)]
    if let Some(s) = FROZEN.with(|f| f.get()) {
        return std::time::UNIX_EPOCH + std::time::Duration::from_secs(s as u64);
    }
    std::time::SystemTime::now()
}

/// Seconds for an animation, from egui's clock; still while this thread's clock is stopped.
pub fn anim(ui: &egui::Ui) -> f64 {
    #[cfg(test)]
    if frozen().is_some() {
        return 0.0;
    }
    ui.input(|i| i.time)
}
