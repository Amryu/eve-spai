//! Alert sounds. To avoid an ALSA build dependency we synthesise short sci-fi
//! warning tones on demand and play them through the system player (paplay /
//! aplay). A preset is a built-in tone name, "off", or a path to a sound file.

use std::path::{Path, PathBuf};
#[cfg(not(target_os = "windows"))]
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
        // Sci-fi wake-up horn: two low, steady brassy blasts (a fourth apart) with only
        // a gentle rise — low frequencies + a soft 2nd harmonic read as a horn, not a
        // shrill sweep.
        "horn" => (
            vec![
                s(147.0, 160.0, 360),
                s(0.0, 0.0, 80),
                s(196.0, 208.0, 520),
            ],
            0.38,
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
    std::thread::spawn(move || play_file(&path));
}

/// Play a WAV file through the platform's audio player.
fn play_file(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("afplay").arg(path).status();
    }
    #[cfg(target_os = "windows")]
    {
        // winmm PlaySound — the canonical Windows WAV playback. Avoids PowerShell's startup
        // latency, console-window flash, and System.Media.SoundPlayer quirks, all of which
        // made the previous shell-out unreliable.
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        #[link(name = "winmm")]
        extern "system" {
            fn PlaySoundW(psz_sound: *const u16, hmod: *mut core::ffi::c_void, flags: u32) -> i32;
        }
        const SND_SYNC: u32 = 0x0000_0000;
        const SND_FILENAME: u32 = 0x0002_0000;
        // SND_SYNC: block until done (we are on a dedicated thread).
        unsafe {
            PlaySoundW(wide.as_ptr(), core::ptr::null_mut(), SND_SYNC | SND_FILENAME);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // PulseAudio/PipeWire first, then ALSA.
        let played =
            Command::new("paplay").arg(path).status().map(|s| s.success()).unwrap_or(false);
        if !played {
            let _ = Command::new("aplay").arg("-q").arg(path).status();
        }
    }
}

/// Generate (once) a WAV for a built-in preset in the scratch dir; return its path.
fn ensure_tone(name: &str, segs: &[Seg], amp: f32) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("eve-spai-sounds");
    let _ = std::fs::create_dir_all(&dir);
    // Version the file name so changes to the synth regenerate it.
    let path = dir.join(format!("{name}-v3.wav"));
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
