//! The wall clock, read in one place so a test render can stop it: screenshots are compared pixel
//! for pixel, and fixtures and the UI both place things relative to now. `web-time` because
//! `std::time::SystemTime::now` panics in the browser.

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static FROZEN: std::cell::Cell<Option<i64>> = const { std::cell::Cell::new(None) };
}

/// Stops this thread's clock at `secs` (unix time), or starts it again with `None`.
#[cfg(any(test, feature = "test-support"))]
pub fn freeze(secs: Option<i64>) {
    FROZEN.with(|f| f.set(secs));
}

/// This thread's stopped clock, if it is stopped.
#[cfg(any(test, feature = "test-support"))]
pub fn frozen() -> Option<i64> {
    FROZEN.with(|f| f.get())
}

pub fn system() -> web_time::SystemTime {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(s) = frozen() {
        return web_time::UNIX_EPOCH + std::time::Duration::from_secs(s as u64);
    }
    web_time::SystemTime::now()
}

pub fn utc() -> chrono::DateTime<chrono::Utc> {
    let d = system().duration_since(web_time::UNIX_EPOCH).unwrap_or_default();
    chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos()).expect("a time after 1970")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_stopped_clock_reads_the_same_everywhere() {
        super::freeze(Some(1_780_315_200));
        assert_eq!(super::utc().timestamp(), 1_780_315_200);
        assert_eq!(super::system().duration_since(web_time::UNIX_EPOCH).unwrap().as_secs(), 1_780_315_200);
        super::freeze(None);
        assert!(super::utc().timestamp() > 1_780_315_200 - 400 * 86_400);
    }
}
