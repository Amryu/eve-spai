//! The microphone, open only while the talk key is held. 16 kHz mono, what speech recognition wants.
//! Linux records through PipeWire's or PulseAudio's recorder, so nothing new is linked; Windows and
//! macOS go through cpal.

pub const RATE: u32 = 16_000;

/// Input devices by name, for the settings. Empty when they cannot be listed; the default is used.
pub fn devices() -> Vec<String> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let out = std::process::Command::new("pactl").args(["list", "short", "sources"]).output();
        out.ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.split('\t').nth(1).map(str::to_owned))
                    .filter(|n| !n.ends_with(".monitor"))
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        cpal::default_host().input_devices().map(|d| d.filter_map(|x| x.name().ok()).collect()).unwrap_or_default()
    }
}

/// A recording in progress; [`Recording::stop`] gives the samples.
pub struct Recording {
    #[cfg(all(unix, not(target_os = "macos")))]
    child: std::process::Child,
    #[cfg(all(unix, not(target_os = "macos")))]
    reader: Option<std::thread::JoinHandle<Vec<u8>>>,
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    inner: desktop::Rec,
}

impl Recording {
    /// Starts recording from `device`, or the default when empty.
    pub fn start(device: &str) -> anyhow::Result<Self> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use std::process::{Command, Stdio};
            let rate = RATE.to_string();
            let mut pw = Command::new("pw-record");
            pw.args(["-a", "--rate", &rate, "--channels", "1", "--format", "s16"]);
            if !device.is_empty() {
                pw.args(["--target", device]);
            }
            let child = pw.arg("-").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().or_else(|_| {
                let mut pa = Command::new("parec");
                pa.args(["--raw", &format!("--rate={RATE}"), "--format=s16le", "--channels=1"]);
                if !device.is_empty() {
                    pa.arg(format!("--device={device}"));
                }
                pa.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
            });
            let mut child = child.map_err(|_| anyhow::anyhow!("no recorder found: install PipeWire's pw-record or PulseAudio's parec"))?;
            let mut out = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no output"))?;
            let reader = std::thread::spawn(move || {
                let mut b = Vec::new();
                let _ = std::io::Read::read_to_end(&mut out, &mut b);
                b
            });
            Ok(Self { child, reader: Some(reader) })
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Ok(Self { inner: desktop::Rec::start(device)? })
        }
    }

    /// Ends the recording: the samples at [`RATE`].
    #[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(unused_mut))]
    pub fn stop(mut self) -> Vec<i16> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let bytes = self.reader.take().and_then(|r| r.join().ok()).unwrap_or_default();
            bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            self.inner.stop()
        }
    }
}

/// Linear resampling to [`RATE`], plenty for speech going to a recogniser.
#[cfg_attr(all(unix, not(target_os = "macos")), allow(dead_code))]
pub fn resample(pcm: &[f32], from: u32) -> Vec<i16> {
    if pcm.is_empty() || from == 0 {
        return Vec::new();
    }
    let step = from as f64 / RATE as f64;
    let n = (pcm.len() as f64 / step) as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * step;
            let j = x as usize;
            let f = (x - j as f64) as f32;
            let a = pcm[j.min(pcm.len() - 1)];
            let b = pcm[(j + 1).min(pcm.len() - 1)];
            ((a + (b - a) * f).clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        })
        .collect()
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
mod desktop {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};

    pub struct Rec {
        stream: cpal::Stream,
        buf: Arc<Mutex<Vec<f32>>>,
        rate: u32,
    }

    impl Rec {
        pub fn start(device: &str) -> anyhow::Result<Self> {
            let host = cpal::default_host();
            let dev = if device.is_empty() {
                host.default_input_device()
            } else {
                host.input_devices()?.find(|d| d.name().is_ok_and(|n| n == device)).or_else(|| host.default_input_device())
            }
            .ok_or_else(|| anyhow::anyhow!("no microphone found"))?;
            let cfg = dev.default_input_config()?;
            let channels = cfg.channels() as usize;
            let rate = cfg.sample_rate().0;
            let buf: Arc<Mutex<Vec<f32>>> = Default::default();
            let sink = buf.clone();
            let err = |e| eprintln!("[voice] input stream: {e}");
            let stream = match cfg.sample_format() {
                cpal::SampleFormat::F32 => dev.build_input_stream(
                    &cfg.into(),
                    move |d: &[f32], _| sink.lock().unwrap_or_else(|e| e.into_inner()).extend(d.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32)),
                    err,
                    None,
                )?,
                cpal::SampleFormat::I16 => dev.build_input_stream(
                    &cfg.into(),
                    move |d: &[i16], _| {
                        sink.lock().unwrap_or_else(|e| e.into_inner()).extend(d.chunks(channels).map(|f| f.iter().map(|s| *s as f32 / i16::MAX as f32).sum::<f32>() / channels as f32))
                    },
                    err,
                    None,
                )?,
                other => anyhow::bail!("the microphone gives {other:?} samples, which are not supported"),
            };
            stream.play()?;
            Ok(Self { stream, buf, rate })
        }

        pub fn stop(self) -> Vec<i16> {
            drop(self.stream);
            let pcm = std::mem::take(&mut *self.buf.lock().unwrap_or_else(|e| e.into_inner()));
            super::resample(&pcm, self.rate)
        }
    }
}

/// The spoken part of a recording, with the silence around it cut, or None when nobody spoke.
pub fn trim(pcm: &[i16]) -> Option<Vec<i16>> {
    let frame = RATE as usize / 50;
    let energy: Vec<f32> = pcm.chunks(frame).map(|f| (f.iter().map(|s| (*s as f32).powi(2)).sum::<f32>() / f.len() as f32).sqrt()).collect();
    let peak = energy.iter().cloned().fold(0.0, f32::max);
    // Quieter than this is the room, not a voice.
    if peak < 300.0 {
        return None;
    }
    let gate = (peak * 0.08).max(150.0);
    let first = energy.iter().position(|e| *e >= gate)?;
    let last = energy.iter().rposition(|e| *e >= gate)?;
    // A little of the quiet either side, so the first and last sounds are not clipped.
    let pad = 10;
    let from = first.saturating_sub(pad) * frame;
    let to = ((last + 1 + pad) * frame).min(pcm.len());
    let speech = &pcm[from..to];
    (speech.len() >= RATE as usize * 3 / 10).then(|| speech.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(secs: f32, amp: f32) -> Vec<i16> {
        (0..(RATE as f32 * secs) as usize).map(|i| ((i as f32 * 0.07).sin() * amp) as i16).collect()
    }

    #[test]
    fn silence_is_cut_and_nothing_said_is_nothing() {
        let mut rec = vec![0i16; RATE as usize];
        rec.extend(tone(1.0, 8000.0));
        rec.extend(vec![0i16; RATE as usize * 2]);
        let speech = trim(&rec).unwrap();
        assert!(speech.len() < RATE as usize * 2, "the quiet around it goes: {}", speech.len());
        assert!(speech.len() >= RATE as usize);
        assert!(trim(&vec![20i16; RATE as usize * 2]).is_none(), "room noise only");
        assert!(trim(&tone(0.1, 8000.0)).is_none(), "a click is not a question");
    }

    #[test]
    fn resampling_keeps_the_length_in_time() {
        let pcm: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let out = resample(&pcm, 48_000);
        assert_eq!(out.len(), RATE as usize);
        assert!(out.iter().any(|s| *s > 10_000));
    }
}
