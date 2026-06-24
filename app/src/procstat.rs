//! Self process resource usage: resident memory and CPU%.
//!
//! Cheap to poll every frame — it only re-samples about once a second and computes CPU%
//! from the change in consumed CPU time over wall-clock time. Linux reads /proc; Windows
//! uses GetProcessMemoryInfo + GetProcessTimes.

use std::time::{Duration, Instant};

// Standard on Linux: 100 scheduler ticks/sec and 4 KiB pages.
#[cfg(target_os = "linux")]
const CLK_TCK: f64 = 100.0;
#[cfg(target_os = "linux")]
const PAGE: u64 = 4096;
const INTERVAL: Duration = Duration::from_millis(1000);

pub struct Monitor {
    ncpu: f64,
    prev: Option<(u64, Instant)>,
    next: Option<Instant>,
    pub rss_bytes: u64,
    pub cpu_percent: f32,
}

impl Monitor {
    pub fn new() -> Self {
        let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
        Self { ncpu, prev: None, next: None, rss_bytes: 0, cpu_percent: 0.0 }
    }

    /// Refresh the figures (no-op until ~1 s since the last refresh).
    pub fn tick(&mut self) {
        let now = Instant::now();
        if self.next.is_some_and(|t| now < t) {
            return;
        }
        self.next = Some(now + INTERVAL);
        #[cfg(target_os = "linux")]
        self.tick_linux(now);
        #[cfg(target_os = "windows")]
        self.tick_windows(now);
    }

    /// Update cpu_percent from the cumulative CPU time, expressed in `units_per_sec` units.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    fn record_cpu(&mut self, cpu_total: u64, units_per_sec: f64, now: Instant) {
        if let Some((prev_total, prev_t)) = self.prev {
            let d = cpu_total.saturating_sub(prev_total) as f64;
            let dt = now.duration_since(prev_t).as_secs_f64().max(1e-3);
            let pct = (d / units_per_sec) / dt / self.ncpu * 100.0;
            self.cpu_percent = pct.clamp(0.0, 100.0 * self.ncpu) as f32;
        }
        self.prev = Some((cpu_total, now));
    }

    #[cfg(target_os = "linux")]
    fn tick_linux(&mut self, now: Instant) {
        // RSS: 2nd field of /proc/self/statm is resident pages.
        if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(pages) = s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()) {
                self.rss_bytes = pages * PAGE;
            }
        }
        // CPU: utime+stime (fields 14,15 of /proc/self/stat). The comm field (2nd,
        // parenthesised) can contain spaces, so index from the last ')'.
        if let Ok(s) = std::fs::read_to_string("/proc/self/stat") {
            if let Some(rp) = s.rfind(')') {
                let f: Vec<&str> = s[rp + 1..].split_whitespace().collect();
                let utime = f.get(11).and_then(|v| v.parse::<u64>().ok());
                let stime = f.get(12).and_then(|v| v.parse::<u64>().ok());
                if let (Some(u), Some(st)) = (utime, stime) {
                    self.record_cpu(u + st, CLK_TCK, now);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn tick_windows(&mut self, now: Instant) {
        use core::ffi::c_void;
        #[repr(C)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_nonpaged_pool_usage: usize,
            quota_nonpaged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct Filetime {
            low: u32,
            high: u32,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut c_void;
            fn GetProcessTimes(
                process: *mut c_void,
                creation: *mut Filetime,
                exit: *mut Filetime,
                kernel: *mut Filetime,
                user: *mut Filetime,
            ) -> i32;
        }
        #[link(name = "psapi")]
        extern "system" {
            fn GetProcessMemoryInfo(
                process: *mut c_void,
                counters: *mut ProcessMemoryCounters,
                cb: u32,
            ) -> i32;
        }
        unsafe {
            let process = GetCurrentProcess();
            let mut pmc: ProcessMemoryCounters = core::mem::zeroed();
            pmc.cb = core::mem::size_of::<ProcessMemoryCounters>() as u32;
            if GetProcessMemoryInfo(process, &mut pmc, pmc.cb) != 0 {
                self.rss_bytes = pmc.working_set_size as u64;
            }
            let (mut c, mut e, mut k, mut u) = (
                Filetime::default(),
                Filetime::default(),
                Filetime::default(),
                Filetime::default(),
            );
            if GetProcessTimes(process, &mut c, &mut e, &mut k, &mut u) != 0 {
                let kt = ((k.high as u64) << 32) | k.low as u64;
                let ut = ((u.high as u64) << 32) | u.low as u64;
                // FILETIME ticks are 100 ns → 1e7 per second.
                self.record_cpu(kt + ut, 1e7, now);
            }
        }
    }

    /// RSS as a compact "123 MB" / "1.2 GB" string.
    pub fn rss_human(&self) -> String {
        let mb = self.rss_bytes as f64 / (1024.0 * 1024.0);
        if mb >= 1024.0 {
            format!("{:.1} GB", mb / 1024.0)
        } else {
            format!("{mb:.0} MB")
        }
    }
}
