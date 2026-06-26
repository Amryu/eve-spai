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

/// A playable tone: swept segments, peak amplitude, the additive-harmonic weights of its
/// voice (harmonic 1, 2, 3, …), and a detune in cents for a second layer (0 = none).
struct Tone {
    segs: Vec<Seg>,
    amp: f32,
    harmonics: &'static [f32],
    detune: f32,
}

/// The console-blip voice all the alert tones use: fundamental + a soft 2nd harmonic.
const BLIP: &[f32] = &[1.0, 0.3];
/// A brass-like voice (saw-ish harmonic stack) for the synth-horn fleet call.
const BRASS: &[f32] = &[1.0, 0.8, 0.6, 0.45, 0.32, 0.22, 0.15, 0.1];

/// Built-in presets. Kept gentle ("not overly intrusive") with smooth sweeps + a soft
/// harmonic for a spaceship-console feel — except the fleet "horn", a louder synth-brass stab.
fn preset(name: &str) -> Option<Tone> {
    let s = |f0: f32, f1: f32, ms: u32| Seg { f0, f1, ms };
    let blip = |segs: Vec<Seg>, amp: f32| Tone { segs, amp, harmonics: BLIP, detune: 0.0 };
    Some(match name {
        // Soft single blip.
        "info" => blip(vec![s(740.0, 880.0, 90)], 0.22),
        // Calm two-tone "ping… pong".
        "warning" => blip(vec![s(784.0, 784.0, 110), s(0.0, 0.0, 50), s(988.0, 988.0, 120)], 0.26),
        // Descending console alert, twice.
        "danger" => blip(vec![s(960.0, 560.0, 160), s(0.0, 0.0, 70), s(960.0, 560.0, 160)], 0.30),
        // Urgent rising sweep, three pulses.
        "critical" => blip(
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
        "beep" => blip(vec![s(880.0, 880.0, 110)], 0.26),
        "chime" => blip(vec![s(1046.0, 1568.0, 220)], 0.24),
        "sweep" => blip(vec![s(400.0, 1400.0, 260)], 0.28),
        // Fleet call: a synth-brass fanfare (D4 → A4 → D5) — a detuned saw-ish voice in the
        // speakers' efficient mid range, near full amplitude, so it reads as a confident synth
        // "horn" rather than a literal brass sample.
        "horn" => Tone {
            segs: vec![
                s(293.66, 293.66, 200),
                s(0.0, 0.0, 45),
                s(440.0, 440.0, 200),
                s(0.0, 0.0, 45),
                s(587.33, 587.33, 600),
            ],
            amp: 0.9,
            harmonics: BRASS,
            detune: 9.0,
        },
        _ => return None,
    })
}

/// Rate-limit gate: (instant of the last played sound, its priority).
static GATE: std::sync::Mutex<Option<(std::time::Instant, u8)>> = std::sync::Mutex::new(None);
const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(2);

/// Play `spec` with a priority (higher = more severe), subject to a 2-second cooldown so a
/// burst of alerts doesn't stack into noise. Within the cooldown only a *strictly higher*
/// priority breaks through; doing so re-arms the cooldown at that higher level, so the next
/// sound must be higher still. "off"/empty never plays and never arms the cooldown.
pub fn play_prio(spec: &str, prio: u8) {
    let s = spec.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("off") {
        return;
    }
    {
        let mut g = GATE.lock().unwrap();
        let now = std::time::Instant::now();
        if !gate_allows(*g, now, prio) {
            return;
        }
        *g = Some((now, prio));
    }
    play(spec);
}

/// Whether a sound of `prio` may play now, given the last (instant, priority) it played.
/// Allowed if no prior sound, the cooldown has elapsed, or `prio` strictly exceeds the last.
fn gate_allows(
    state: Option<(std::time::Instant, u8)>,
    now: std::time::Instant,
    prio: u8,
) -> bool {
    match state {
        Some((last, sev)) => now.duration_since(last) >= COOLDOWN || prio > sev,
        None => true,
    }
}

/// Play a preset (built-in name), a file path, or do nothing for "off"/empty. Ungated — used
/// for the settings "Test" buttons; operational alerts go through [`play_prio`].
pub fn play(spec: &str) {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("off") {
        return;
    }
    let path = if Path::new(spec).is_file() {
        PathBuf::from(spec)
    } else if let Some(tone) = preset(spec) {
        match ensure_tone(spec, &tone) {
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
fn ensure_tone(name: &str, tone: &Tone) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("eve-spai-sounds");
    let _ = std::fs::create_dir_all(&dir);
    // Version the file name so changes to the synth regenerate it.
    let path = dir.join(format!("{name}-v5.wav"));
    if path.is_file() {
        return Some(path);
    }
    std::fs::write(&path, wav(tone)).ok()?;
    Some(path)
}

/// 16-bit mono 44.1 kHz WAV of the swept segments: additive synthesis over the voice's
/// harmonic weights, optionally doubled by a slightly detuned layer for a fatter synth
/// timbre, each segment faded in/out to avoid clicks.
fn wav(tone: &Tone) -> Vec<u8> {
    const RATE: u32 = 44_100;
    let Tone { segs, amp, harmonics, detune } = tone;
    let (amp, detune) = (*amp, *detune);
    // Normalise toward the harmonic energy. The detuned layer adds some headroom pressure, but
    // harmonics rarely align in phase, so we divide by less than the worst case and let the
    // final clamp shave the occasional peak (a touch of grit suits the synth-horn stab).
    let hsum: f32 = harmonics.iter().sum::<f32>().max(1e-3);
    let norm = hsum * if detune != 0.0 { 1.2 } else { 1.0 };
    let ratio = 2f32.powf(detune / 1200.0); // cents → frequency multiplier
    let mut samples: Vec<i16> = Vec::new();
    for seg in segs {
        let n = (RATE as u64 * seg.ms as u64 / 1000) as usize;
        // Silence segment.
        if seg.f0 <= 0.0 {
            samples.resize(samples.len() + n, 0);
            continue;
        }
        let mut ph = vec![0.0f32; harmonics.len()];
        let mut phd = vec![0.0f32; harmonics.len()];
        for i in 0..n {
            let frac = i as f32 / n.max(1) as f32;
            let f = seg.f0 + (seg.f1 - seg.f0) * frac;
            let mut v = 0.0f32;
            for (h, w) in harmonics.iter().enumerate() {
                let hf = f * (h as f32 + 1.0);
                ph[h] += std::f32::consts::TAU * hf / RATE as f32;
                v += w * ph[h].sin();
                if detune != 0.0 {
                    phd[h] += std::f32::consts::TAU * hf * ratio / RATE as f32;
                    v += 0.7 * w * phd[h].sin();
                }
            }
            // Fade in/out (raised-cosine-ish) to avoid clicks.
            let env = (frac.min(1.0 - frac) * 10.0).clamp(0.0, 1.0);
            let v = v / norm * amp * env;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn horn_synthesizes_reasonably() {
        let tone = preset("horn").unwrap();
        let bytes = wav(&tone);
        let samples: Vec<i16> =
            bytes[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert!(!samples.is_empty());
        let peak = samples.iter().map(|s| s.unsigned_abs() as i32).max().unwrap();
        // Loud (near amp 0.9) so it grabs attention …
        assert!(peak > 20_000, "peak={peak}");
        // … but not a wall of saturated maxima (which would sound like harsh distortion).
        let clipped = samples.iter().filter(|s| s.unsigned_abs() >= 32_760).count();
        assert!(clipped < samples.len() / 20, "too much clipping: {clipped}/{}", samples.len());
    }

    #[test]
    fn cooldown_and_severity_breakthrough() {
        let t0 = Instant::now();
        // No prior sound → always allowed.
        assert!(gate_allows(None, t0, 0));
        // Within the cooldown: equal or lower priority is suppressed.
        assert!(!gate_allows(Some((t0, 1)), t0 + Duration::from_millis(500), 1));
        assert!(!gate_allows(Some((t0, 1)), t0 + Duration::from_millis(500), 0));
        // Within the cooldown: a strictly higher priority breaks through.
        assert!(gate_allows(Some((t0, 1)), t0 + Duration::from_millis(500), 2));
        // After the cooldown elapses: anything plays again.
        assert!(gate_allows(Some((t0, 3)), t0 + COOLDOWN, 0));
        // Re-armed at the breakthrough level: a 2 can't follow a 2, but a 3 can.
        assert!(!gate_allows(Some((t0, 2)), t0 + Duration::from_millis(100), 2));
        assert!(gate_allows(Some((t0, 2)), t0 + Duration::from_millis(100), 3));
    }
}
