//! Self process resource usage (Linux /proc): resident memory and CPU%.
//!
//! Cheap to poll every frame — it only re-reads /proc about once a second and
//! computes CPU% from the change in consumed CPU time over wall-clock time.

use std::time::{Duration, Instant};

// Standard on Linux: 100 scheduler ticks/sec and 4 KiB pages. (Reading the exact
// values needs libc; these hold on every realistic target here.)
const CLK_TCK: f64 = 100.0;
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

        // Resident set size: 2nd field of /proc/self/statm is resident pages.
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
                    let jiffies = u + st;
                    if let Some((pj, pt)) = self.prev {
                        let dj = jiffies.saturating_sub(pj) as f64;
                        let dt = now.duration_since(pt).as_secs_f64().max(1e-3);
                        let pct = (dj / CLK_TCK) / dt / self.ncpu * 100.0;
                        self.cpu_percent = pct.clamp(0.0, 100.0 * self.ncpu) as f32;
                    }
                    self.prev = Some((jiffies, now));
                }
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
