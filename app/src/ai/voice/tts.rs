//! Turning a sentence into sound: Piper on this computer, or OpenAI or ElevenLabs over the network.
//! Each hands back 16-bit mono samples, in pieces as they arrive where the service streams.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

pub trait Tts: Send {
    /// Speaks `text` in `lang`, handing samples to `out` at their rate as they come.
    fn say(&mut self, text: &str, lang: &str, cancel: &AtomicBool, out: &mut dyn FnMut(u32, &[i16])) -> anyhow::Result<()>;
}

/// Reads a raw 16-bit little-endian stream into `out` in pieces of about a fifth of a second.
fn stream_pcm(mut r: impl Read, rate: u32, cancel: &AtomicBool, out: &mut dyn FnMut(u32, &[i16])) -> anyhow::Result<()> {
    let piece = (rate as usize / 5) * 2;
    let mut buf = vec![0u8; piece];
    let mut have = 0;
    let mut odd: Option<u8> = None;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let n = r.read(&mut buf[have..])?;
        have += n;
        if have == piece || (n == 0 && have > 0) {
            let mut bytes: Vec<u8> = odd.take().into_iter().chain(buf[..have].iter().copied()).collect();
            if bytes.len() % 2 == 1 {
                odd = bytes.pop();
            }
            let pcm: Vec<i16> = bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            out(rate, &pcm);
            have = 0;
        }
        if n == 0 {
            return Ok(());
        }
    }
}

pub struct OpenAiTts {
    pub key: String,
    pub voice: String,
}

impl Tts for OpenAiTts {
    fn say(&mut self, text: &str, lang: &str, cancel: &AtomicBool, out: &mut dyn FnMut(u32, &[i16])) -> anyhow::Result<()> {
        let client = crate::http::client(30)?;
        let body = serde_json::json!({
            "model": "gpt-4o-mini-tts",
            "voice": if self.voice.trim().is_empty() { "alloy" } else { self.voice.trim() },
            "input": text,
            "response_format": "pcm",
            "instructions": format!("Speak briefly and clearly, like a fleet member on comms. Language: {lang}."),
        });
        let resp = client.post("https://api.openai.com/v1/audio/speech").bearer_auth(&self.key).json(&body).send()?;
        if !resp.status().is_success() {
            let status = resp.status();
            let t = resp.text().unwrap_or_default();
            anyhow::bail!("OpenAI speech answered {status}: {}", crate::ai::secrets::redact(&t.chars().take(200).collect::<String>(), &[self.key.clone()]));
        }
        stream_pcm(resp, 24_000, cancel, out)
    }
}

pub struct ElevenLabs {
    pub key: String,
    pub voice: String,
}

/// Rachel: on every ElevenLabs account, so a fresh key speaks without picking a voice first.
pub const ELEVENLABS_DEFAULT_VOICE: &str = "21m00Tcm4TlvDq8ikWAM";

impl Tts for ElevenLabs {
    fn say(&mut self, text: &str, _lang: &str, cancel: &AtomicBool, out: &mut dyn FnMut(u32, &[i16])) -> anyhow::Result<()> {
        let client = crate::http::client(30)?;
        let voice = if self.voice.trim().is_empty() { ELEVENLABS_DEFAULT_VOICE } else { self.voice.trim() };
        let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}/stream?output_format=pcm_22050");
        let body = serde_json::json!({"text": text, "model_id": "eleven_flash_v2_5"});
        let resp = client.post(url).header("xi-api-key", &self.key).json(&body).send()?;
        if !resp.status().is_success() {
            let status = resp.status();
            let t = resp.text().unwrap_or_default();
            anyhow::bail!("ElevenLabs answered {status}: {}", crate::ai::secrets::redact(&t.chars().take(200).collect::<String>(), &[self.key.clone()]));
        }
        stream_pcm(resp, 22_050, cancel, out)
    }
}

/// Piper kept running per voice, so a sentence costs the speech and not the model loading: a line
/// in, the path of the finished WAV out.
pub struct Piper {
    /// Voice per language, by id.
    pub voices: std::collections::BTreeMap<String, String>,
    warm: Option<(String, Child, ChildStdin, BufReader<ChildStdout>)>,
    out_dir: std::path::PathBuf,
}

impl Piper {
    pub fn new(voices: std::collections::BTreeMap<String, String>) -> anyhow::Result<Self> {
        let out_dir = super::models::root()?.join("out");
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir)?;
        Ok(Self { voices, warm: None, out_dir })
    }

    /// The voice for `lang`: the one chosen for it, else its default, else any installed one.
    fn voice_for(&self, lang: &str) -> Option<String> {
        let chosen = self.voices.get(lang).cloned().filter(|v| super::models::has_voice(v));
        chosen
            .or_else(|| super::models::default_voice(lang).map(|v| v.id.to_owned()).filter(|v| super::models::has_voice(v)))
            .or_else(|| self.voices.values().find(|v| super::models::has_voice(v)).cloned())
            .or_else(|| super::models::CATALOG.iter().map(|v| v.id.to_owned()).find(|v| super::models::has_voice(v)))
    }

    fn start(&mut self, voice: &str) -> anyhow::Result<()> {
        if self.warm.as_ref().is_some_and(|w| w.0 == voice) {
            return Ok(());
        }
        self.shutdown();
        let mut cmd = Command::new(super::models::program_path()?);
        cmd.arg("-q").arg("-m").arg(super::models::voice_path(voice)?).arg("-d").arg(&self.out_dir);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no input"))?;
        let stdout = BufReader::new(child.stdout.take().ok_or_else(|| anyhow::anyhow!("no output"))?);
        self.warm = Some((voice.to_owned(), child, stdin, stdout));
        Ok(())
    }

    fn shutdown(&mut self) {
        if let Some((_, mut c, stdin, _)) = self.warm.take() {
            drop(stdin);
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Piper {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Tts for Piper {
    fn say(&mut self, text: &str, lang: &str, cancel: &AtomicBool, out: &mut dyn FnMut(u32, &[i16])) -> anyhow::Result<()> {
        let voice = self.voice_for(lang).ok_or_else(|| anyhow::anyhow!("no Piper voice is installed yet"))?;
        self.start(&voice)?;
        let line = text.replace(['\n', '\r'], " ");
        let path = {
            let (_, _, stdin, stdout) = self.warm.as_mut().expect("started");
            writeln!(stdin, "{line}").and_then(|_| stdin.flush())?;
            let mut p = String::new();
            if stdout.read_line(&mut p)? == 0 {
                self.shutdown();
                anyhow::bail!("Piper stopped");
            }
            std::path::PathBuf::from(p.trim())
        };
        let bytes = std::fs::read(&path);
        let _ = std::fs::remove_file(&path);
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let (rate, pcm) = super::playback::read_wav(&bytes?).ok_or_else(|| anyhow::anyhow!("Piper wrote no usable audio"))?;
        out(rate, &pcm);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_streams_come_out_in_whole_samples() {
        let bytes: Vec<u8> = (0..10_001u32).flat_map(|i| (i as i16).to_le_bytes()).chain([7u8]).collect();
        let mut got: Vec<i16> = Vec::new();
        let mut pieces = 0;
        stream_pcm(bytes.as_slice(), 22_050, &AtomicBool::new(false), &mut |r, p| {
            assert_eq!(r, 22_050);
            pieces += 1;
            got.extend_from_slice(p);
        })
        .unwrap();
        assert_eq!(got.len(), 10_001);
        assert_eq!(got[10_000], 10_000);
        assert!(pieces > 1, "streamed in pieces");
    }
}

/// Runs the real Piper from `EVE_SPAI_DATA_DIR`:
/// `EVE_SPAI_DATA_DIR=<dir with ai/piper> cargo test --bin eve-spai piper_live -- --ignored`.
#[cfg(test)]
#[test]
#[ignore = "needs Piper and a voice installed under EVE_SPAI_DATA_DIR"]
fn piper_live() {
    let mut p = Piper::new(Default::default()).unwrap();
    for line in ["The Frat gang went south.", "Die Frat-Gang ist in QX-LIJ."] {
        let mut n = 0;
        p.say(line, "de", &AtomicBool::new(false), &mut |rate, pcm| {
            assert_eq!(rate, 22_050);
            n += pcm.len();
        })
        .unwrap();
        assert!(n > 22_050 / 2, "{line}: {n} samples");
    }
}
