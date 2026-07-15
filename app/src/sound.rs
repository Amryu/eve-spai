use std::path::{Path, PathBuf};
#[cfg(not(target_os = "windows"))]
use std::process::Command;

struct Seg {
    f0: f32,
    f1: f32,
    ms: u32,
}

struct Tone {
    segs: Vec<Seg>,
    amp: f32,
    harmonics: &'static [f32],
    detune: f32,
}

const BLIP: &[f32] = &[1.0, 0.3];
const BRASS: &[f32] = &[1.0, 0.8, 0.6, 0.45, 0.32, 0.22, 0.15, 0.1];

pub const PRESETS: &[&str] =
    &["info", "warning", "danger", "critical", "beep", "chime", "sweep", "horn"];

fn preset(name: &str) -> Option<Tone> {
    let s = |f0: f32, f1: f32, ms: u32| Seg { f0, f1, ms };
    let blip = |segs: Vec<Seg>, amp: f32| Tone { segs, amp, harmonics: BLIP, detune: 0.0 };
    Some(match name {
        "info" => blip(vec![s(740.0, 880.0, 90)], 0.22),
        "warning" => blip(vec![s(784.0, 784.0, 110), s(0.0, 0.0, 50), s(988.0, 988.0, 120)], 0.26),
        "danger" => blip(vec![s(960.0, 560.0, 160), s(0.0, 0.0, 70), s(960.0, 560.0, 160)], 0.30),
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
        "beep" => blip(vec![s(880.0, 880.0, 110)], 0.26),
        "chime" => blip(vec![s(1046.0, 1568.0, 220)], 0.24),
        "sweep" => blip(vec![s(400.0, 1400.0, 260)], 0.28),
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

static GATE: std::sync::Mutex<Option<(std::time::Instant, u8)>> = std::sync::Mutex::new(None);
const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(2);

pub fn play_prio(spec: &str, prio: u8, volume: f32) {
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
    play(spec, volume);
}

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

pub fn play(spec: &str, volume: f32) {
    let spec = spec.trim();
    if spec.is_empty() || spec.eq_ignore_ascii_case("off") {
        return;
    }
    let volume = volume.clamp(0.0, 1.0);
    if volume < 0.005 {
        return;
    }
    // Volume is applied by baking it into a WAV, which works everywhere including Windows PlaySoundW
    // (no per-sound gain): presets are synthesized at the target amplitude, and a custom WAV file is
    // rescaled sample-by-sample (no decoder dependency). A custom file we can't rescale in place
    // (mp3/ogg/flac, or an exotic WAV format) falls back to the player's own volume flag, honoured
    // on macOS/PulseAudio and full-volume on Windows.
    let (path, file_volume) = if Path::new(spec).is_file() {
        match scaled_wav_file(spec, volume) {
            Some(p) => (p, 1.0),
            None => (PathBuf::from(spec), volume),
        }
    } else if let Some(mut tone) = preset(spec) {
        tone.amp *= volume;
        match ensure_tone(spec, volume, &tone) {
            Some(p) => (p, 1.0),
            None => return,
        }
    } else {
        return;
    };
    std::thread::spawn(move || play_file(&path, file_volume));
}

fn play_file(path: &Path, volume: f32) {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("afplay");
        if volume < 0.999 {
            cmd.arg("-v").arg(format!("{volume:.3}"));
        }
        let _ = cmd.arg(path).status();
    }
    #[cfg(target_os = "windows")]
    {
        // winmm PlaySound — the canonical Windows WAV playback. Avoids PowerShell's startup
        // latency, console-window flash, and System.Media.SoundPlayer quirks, all of which
        // made the previous shell-out unreliable. It has no per-sound volume: presets bake gain
        // into the WAV, user files play at full (the `volume` arg only applies to files here).
        let _ = volume;
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        #[link(name = "winmm")]
        extern "system" {
            fn PlaySoundW(psz_sound: *const u16, hmod: *mut core::ffi::c_void, flags: u32) -> i32;
        }
        const SND_SYNC: u32 = 0x0000_0000;
        const SND_FILENAME: u32 = 0x0002_0000;
        unsafe {
            PlaySoundW(wide.as_ptr(), core::ptr::null_mut(), SND_SYNC | SND_FILENAME);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // paplay --volume is a linear 0..=65536 scale (65536 = 100%). aplay has no volume knob.
        let vol = (volume.clamp(0.0, 1.0) * 65536.0).round() as u32;
        let played = Command::new("paplay")
            .arg(format!("--volume={vol}"))
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !played {
            let _ = Command::new("aplay").arg("-q").arg(path).status();
        }
    }
}

fn ensure_tone(name: &str, volume: f32, tone: &Tone) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("eve-spai-sounds");
    let _ = std::fs::create_dir_all(&dir);
    // Version the file name so changes to the synth regenerate it; the volume bucket (in percent)
    // keys the baked gain so different volumes don't collide on one cached file.
    let pct = (volume * 100.0).round() as u32;
    let path = dir.join(format!("{name}-v5-{pct}.wav"));
    if path.is_file() {
        return Some(path);
    }
    std::fs::write(&path, wav(tone)).ok()?;
    Some(path)
}

/// Bake `volume` into a copy of a custom WAV file and cache it, returning the temp path. Returns
/// `None` for a non-WAV file, an unsupported WAV encoding, or at (near) full volume where scaling
/// is pointless — the caller then plays the original with the player's own volume flag.
fn scaled_wav_file(spec: &str, volume: f32) -> Option<PathBuf> {
    if volume > 0.999 {
        return None;
    }
    if !spec.to_ascii_lowercase().ends_with(".wav") {
        return None;
    }
    let meta = std::fs::metadata(spec).ok()?;
    // Cache key = path + size + mtime + volume bucket, so an edited source or a new volume misses.
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    spec.hash(&mut h);
    meta.len().hash(&mut h);
    mtime.hash(&mut h);
    ((volume * 100.0).round() as u32).hash(&mut h);
    let dir = std::env::temp_dir().join("eve-spai-sounds");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("custom-{:016x}.wav", h.finish()));
    if path.is_file() {
        return Some(path);
    }
    let bytes = std::fs::read(spec).ok()?;
    let scaled = scale_wav(&bytes, volume)?;
    std::fs::write(&path, scaled).ok()?;
    Some(path)
}

/// Multiply every PCM sample in a WAV by `volume` in place, preserving all headers/chunks. Handles
/// 16-bit int and 32-bit float samples (incl. WAVE_FORMAT_EXTENSIBLE 16-bit); returns `None` for
/// anything else so the caller can fall back.
fn scale_wav(bytes: &[u8], volume: f32) -> Option<Vec<u8>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut fmt_tag = 0u16;
    let mut bits = 0u16;
    let mut data: Option<(usize, usize)> = None;
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        let end = body.checked_add(size)?;
        if end > bytes.len() {
            break;
        }
        match &bytes[pos..pos + 4] {
            b"fmt " if size >= 16 => {
                fmt_tag = u16::from_le_bytes(bytes[body..body + 2].try_into().ok()?);
                bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().ok()?);
            }
            b"data" => data = Some((body, end)),
            _ => {}
        }
        pos = end + (size & 1); // chunks are word-aligned: skip the pad byte after an odd size
    }
    let (ds, de) = data?;
    let v = volume.clamp(0.0, 1.0);
    let mut out = bytes.to_vec();
    // 1 = PCM, 3 = IEEE float, 0xFFFE = EXTENSIBLE (treated as PCM for the 16-bit case).
    match (fmt_tag, bits) {
        (1, 16) | (0xFFFE, 16) => {
            for s in out[ds..de].chunks_exact_mut(2) {
                let scaled = i16::from_le_bytes([s[0], s[1]]) as f32 * v;
                let scaled = scaled.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                s.copy_from_slice(&scaled.to_le_bytes());
            }
        }
        (3, 32) => {
            for s in out[ds..de].chunks_exact_mut(4) {
                let scaled = f32::from_le_bytes([s[0], s[1], s[2], s[3]]) * v;
                s.copy_from_slice(&scaled.to_le_bytes());
            }
        }
        _ => return None,
    }
    Some(out)
}

fn wav(tone: &Tone) -> Vec<u8> {
    const RATE: u32 = 44_100;
    let Tone { segs, amp, harmonics, detune } = tone;
    let (amp, detune) = (*amp, *detune);
    let hsum: f32 = harmonics.iter().sum::<f32>().max(1e-3);
    let norm = hsum * if detune != 0.0 { 1.2 } else { 1.0 };
    let ratio = 2f32.powf(detune / 1200.0);
    let mut samples: Vec<i16> = Vec::new();
    for seg in segs {
        let n = (RATE as u64 * seg.ms as u64 / 1000) as usize;
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
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
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
        assert!(peak > 20_000, "peak={peak}");
        let clipped = samples.iter().filter(|s| s.unsigned_abs() >= 32_760).count();
        assert!(clipped < samples.len() / 20, "too much clipping: {clipped}/{}", samples.len());
    }

    #[test]
    fn scale_wav_halves_16bit_pcm_and_keeps_headers() {
        let src = wav(&preset("horn").unwrap());
        let scaled = scale_wav(&src, 0.5).expect("16-bit PCM should scale");
        assert_eq!(scaled.len(), src.len(), "headers/size preserved");
        assert_eq!(&scaled[..44], &src[..44], "fmt/data headers untouched");
        let peak = |b: &[u8]| {
            b[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs() as i32).max().unwrap()
        };
        let (a, b) = (peak(&src), peak(&scaled));
        assert!((b as f32 / a as f32 - 0.5).abs() < 0.02, "peak roughly halved: {a} -> {b}");
    }

    #[test]
    fn scale_wav_rejects_non_wav() {
        assert!(scale_wav(b"not a wav file at all!!", 0.5).is_none());
    }

    #[test]
    fn cooldown_and_severity_breakthrough() {
        let t0 = Instant::now();
        assert!(gate_allows(None, t0, 0));
        assert!(!gate_allows(Some((t0, 1)), t0 + Duration::from_millis(500), 1));
        assert!(!gate_allows(Some((t0, 1)), t0 + Duration::from_millis(500), 0));
        assert!(gate_allows(Some((t0, 1)), t0 + Duration::from_millis(500), 2));
        assert!(gate_allows(Some((t0, 3)), t0 + COOLDOWN, 0));
        assert!(!gate_allows(Some((t0, 2)), t0 + Duration::from_millis(100), 2));
        assert!(gate_allows(Some((t0, 2)), t0 + Duration::from_millis(100), 3));
    }
}
