//! Numbers and names as the screens print them.

/// `1,234`, `12.3k`, `1.23M`.
pub fn num(v: f64) -> String {
    let v = if v == 0.0 || !v.is_finite() { 0.0 } else { v };
    let a = v.abs();
    if a >= 1.0e9 {
        format!("{:.2}B", v / 1.0e9)
    } else if a >= 1.0e6 {
        format!("{:.2}M", v / 1.0e6)
    } else if a >= 10_000.0 {
        format!("{:.1}k", v / 1.0e3)
    } else if a >= 100.0 {
        format!("{v:.0}")
    } else if a >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

/// Metres as `750 m` or `9.68 km`.
pub fn metres(v: f64) -> String {
    if v.abs() >= 1000.0 {
        format!("{:.2} km", v / 1000.0)
    } else {
        format!("{v:.0} m")
    }
}

pub fn pct(fraction: f64) -> String {
    format!("{:.1}%", fraction * 100.0)
}

/// Seconds as `3.2 s`, `1m 05s` or `2h 03m`.
pub fn secs(v: f64) -> String {
    if v < 60.0 {
        format!("{v:.1} s")
    } else if v < 3600.0 {
        format!("{}m {:02}s", (v / 60.0) as u64, (v % 60.0) as u64)
    } else {
        format!("{}h {:02}m", (v / 3600.0) as u64, ((v % 3600.0) / 60.0) as u64)
    }
}

/// At most `max` characters, the end replaced by an ellipsis.
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_at_a_glance() {
        assert_eq!(num(0.0), "0.00");
        assert_eq!(num(-0.0), "0.00");
        assert_eq!(num(f64::NAN), "0.00");
        assert_eq!(num(3.14159), "3.14");
        assert_eq!(num(42.42), "42.4");
        assert_eq!(num(365.4), "365");
        assert_eq!(num(12_345.0), "12.3k");
        assert_eq!(num(4_606_000.0), "4.61M");
        assert_eq!(num(-2_500_000_000.0), "-2.50B");
        assert_eq!(metres(750.0), "750 m");
        assert_eq!(metres(9675.0), "9.68 km");
        assert_eq!(pct(0.5106), "51.1%");
        assert_eq!(secs(3.25), "3.2 s");
        assert_eq!(secs(65.0), "1m 05s");
        assert_eq!(secs(7380.0), "2h 03m");
        assert_eq!(clip("Republic Fleet EMP S", 10), "Republic …");
        assert_eq!(clip("Rifter", 10), "Rifter");
    }
}
