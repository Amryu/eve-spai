//! Alert sounds. To avoid an ALSA build dependency we generate short WAV tones on
//! demand and play them through the system player (paplay / aplay). A preset is a
//! built-in tone name, "off", or a path to a custom sound file.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Built-in tone presets (name → (frequency Hz, duration ms, repeats)).
fn preset_tone(name: &str) -> Option<(f32, u32, u32)> {
    Some(match name {
        "info" => (520.0, 120, 1),
        "warning" => (660.0, 180, 1),
        "danger" => (840.0, 160, 2),
        "critical" => (1040.0, 150, 3),
        "beep" => (800.0, 120, 1),
        "chime" => (988.0, 250, 1),
        _ => return None,
    })
}

/// Play a preset (built-in name), a file path, or do nothing for "off"/empty.
pub fn play(preset: &str) {
    let preset = preset.trim();
    if preset.is_empty() || preset.eq_ignore_ascii_case("off") {
        return;
    }
    let path = if Path::new(preset).is_file() {
        PathBuf::from(preset)
    } else if let Some((freq, dur, reps)) = preset_tone(preset) {
        match ensure_tone(preset, freq, dur, reps) {
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

/// Generate (once) a WAV for a built-in tone in the scratch dir; return its path.
fn ensure_tone(name: &str, freq: f32, dur_ms: u32, reps: u32) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("eve-spai-sounds");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.wav"));
    if path.is_file() {
        return Some(path);
    }
    let bytes = wav_tone(freq, dur_ms, reps);
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// A 16-bit mono 44.1 kHz WAV of `reps` sine bursts (with short gaps), faded in/out.
fn wav_tone(freq: f32, dur_ms: u32, reps: u32) -> Vec<u8> {
    const RATE: u32 = 44_100;
    let burst = (RATE as u64 * dur_ms as u64 / 1000) as usize;
    let gap = burst / 3;
    let total = (burst + gap) * reps as usize;
    let mut samples: Vec<i16> = Vec::with_capacity(total);
    for _ in 0..reps {
        for i in 0..burst {
            let t = i as f32 / RATE as f32;
            // Linear fade in/out to avoid clicks.
            let env = {
                let f = (i as f32 / burst as f32).min(1.0 - i as f32 / burst as f32) * 8.0;
                f.clamp(0.0, 1.0)
            };
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5 * env;
            samples.push((s * i16::MAX as f32) as i16);
        }
        for _ in 0..gap {
            samples.push(0);
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
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
