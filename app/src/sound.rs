//! Alert sounds. To avoid an ALSA build dependency we synthesise short sci-fi
//! warning tones on demand and play them through the system player (paplay /
//! aplay). A preset is a built-in tone name, "off", or a path to a sound file.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One swept segment: glide from `f0` to `f1` over `ms` (f0=0 ⇒ silence/gap).
struct Seg {
    f0: f32,
    f1: f32,
    ms: u32,
}

/// Built-in presets → (segments, peak amplitude 0..1). Kept gentle ("not overly
/// intrusive") with smooth sweeps + a soft harmonic for a spaceship-console feel.
fn preset(name: &str) -> Option<(Vec<Seg>, f32)> {
    let s = |f0: f32, f1: f32, ms: u32| Seg { f0, f1, ms };
    Some(match name {
        // Soft single blip.
        "info" => (vec![s(740.0, 880.0, 90)], 0.22),
        // Calm two-tone "ping… pong".
        "warning" => (vec![s(784.0, 784.0, 110), s(0.0, 0.0, 50), s(988.0, 988.0, 120)], 0.26),
        // Descending console alert, twice.
        "danger" => (
            vec![s(960.0, 560.0, 160), s(0.0, 0.0, 70), s(960.0, 560.0, 160)],
            0.30,
        ),
        // Urgent rising sweep, three pulses.
        "critical" => (
            vec![
                s(620.0, 1180.0, 130),
                s(0.0, 0.0, 45),
                s(620.0, 1180.0, 130),
                s(0.0, 0.0, 45),
                s(620.0, 1180.0, 150),
            ],
            0.34,
        ),
        // Generic extras.
        "beep" => (vec![s(880.0, 880.0, 110)], 0.26),
        "chime" => (vec![s(1046.0, 1568.0, 220)], 0.24),
        "sweep" => (vec![s(400.0, 1400.0, 260)], 0.28),
        // Sci-fi wake-up horn: two low brassy blasts that bend up — attention-getting
        // for fleet pings without being shrill (the synth adds a 2nd harmonic).
        "horn" => (
            vec![
                s(180.0, 262.0, 300),
                s(262.0, 262.0, 240),
                s(0.0, 0.0, 90),
                s(196.0, 330.0, 340),
                s(330.0, 330.0, 300),
            ],
            0.45,
        ),
        _ => return None,
    })
}

/// Play a preset (built-in name), a file path, or do nothing for "off"/empty.
pub fn play(spec: &str) {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("off") {
        return;
    }
    let path = if Path::new(spec).is_file() {
        PathBuf::from(spec)
    } else if let Some((segs, amp)) = preset(spec) {
        match ensure_tone(spec, &segs, amp) {
            Some(p) => p,
            None => return,
        }
    } else {
        return;
    };
    std::thread::spawn(move || {
        let played = Command::new("paplay").arg(&path).status().map(|s| s.success()).unwrap_or(false);
        if !played {
            let _ = Command::new("aplay").arg("-q").arg(&path).status();
        }
    });
}

/// Generate (once) a WAV for a built-in preset in the scratch dir; return its path.
fn ensure_tone(name: &str, segs: &[Seg], amp: f32) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("eve-spai-sounds");
    let _ = std::fs::create_dir_all(&dir);
    // Version the file name so changes to the synth regenerate it.
    let path = dir.join(format!("{name}-v2.wav"));
    if path.is_file() {
        return Some(path);
    }
    std::fs::write(&path, wav(segs, amp)).ok()?;
    Some(path)
}

/// 16-bit mono 44.1 kHz WAV of the swept segments (fundamental + soft 2nd harmonic,
/// each segment faded in/out to avoid clicks).
fn wav(segs: &[Seg], amp: f32) -> Vec<u8> {
    const RATE: u32 = 44_100;
    let mut samples: Vec<i16> = Vec::new();
    for seg in segs {
        let n = (RATE as u64 * seg.ms as u64 / 1000) as usize;
        // Silence segment.
        if seg.f0 <= 0.0 {
            samples.resize(samples.len() + n, 0);
            continue;
        }
        let mut phase = 0.0f32;
        let mut phase2 = 0.0f32;
        for i in 0..n {
            let frac = i as f32 / n.max(1) as f32;
            let f = seg.f0 + (seg.f1 - seg.f0) * frac;
            phase += std::f32::consts::TAU * f / RATE as f32;
            phase2 += std::f32::consts::TAU * (f * 2.0) / RATE as f32;
            // Fade in/out (raised-cosine-ish) to avoid clicks.
            let env = (frac.min(1.0 - frac) * 10.0).clamp(0.0, 1.0);
            let v = (phase.sin() + 0.3 * phase2.sin()) / 1.3 * amp * env;
            samples.push((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
        }
    }

    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
