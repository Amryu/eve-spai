//! Hands freed heap back to the OS. glibc keeps what a thread freed in that thread's arena, so a
//! burst of parsing on a worker stays resident after the worker is done with it.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn release() {
    extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    // SAFETY: malloc_trim only returns free pages in every arena to the kernel.
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn release() {}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests {
    fn rss_mb() -> f64 {
        let pages: f64 = std::fs::read_to_string("/proc/self/statm").unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
        pages * 4096.0 / 1_048_576.0
    }

    #[test]
    fn a_workers_freed_burst_goes_back_to_the_os() {
        // Small allocations, as JSON parsing makes, on a thread of its own: its own arena. A few
        // stay alive throughout it, as a parse's results do, so the arena cannot shrink by itself.
        let survivors = std::thread::spawn(|| {
            let mut burst: Vec<Option<Box<[u8; 96]>>> = (0..1_000_000).map(|_| Some(Box::new([7u8; 96]))).collect();
            let survivors: Vec<Box<[u8; 96]>> = burst.iter_mut().step_by(64).filter_map(Option::take).collect();
            drop(burst);
            survivors
        })
        .join()
        .unwrap();
        let kept = rss_mb();
        super::release();
        let after = rss_mb();
        assert_eq!(survivors.len(), 1_000_000 / 64);
        assert!(kept - after > 30.0, "kept {kept:.0} MB, {after:.0} MB after the trim");
    }
}
