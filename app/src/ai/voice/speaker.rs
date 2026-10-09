//! The voice: sentences go in as the answer is written and are spoken in order on their own thread.
//! A stop drops whatever is queued or playing, at once.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};

use super::playback::{gain, Player};
use super::tts::{ElevenLabs, OpenAiTts, Piper, Tts};
use crate::ai::config::TtsKind;

/// What the voice needs from the settings and the keychain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoiceCfg {
    pub kind: TtsKind,
    pub openai_key: Option<String>,
    pub openai_voice: String,
    pub elevenlabs_key: Option<String>,
    pub elevenlabs_voice: String,
    pub piper_voices: std::collections::BTreeMap<String, String>,
    pub volume: f32,
    /// The answer language when fixed in the settings; "auto" detects it per sentence.
    pub language: String,
}

enum Cmd {
    Say(u64, String),
    Stop,
}

#[derive(Clone)]
pub struct Speaker {
    tx: Sender<Cmd>,
    /// Bumped by every stop; sentences queued before it are dropped.
    gen: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    pub speaking: Arc<AtomicBool>,
    /// The last thing that went wrong, for the UI to show once.
    pub error: Arc<Mutex<Option<String>>>,
    cfg: Arc<Mutex<VoiceCfg>>,
}

impl Speaker {
    pub fn spawn(cfg: VoiceCfg) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let s = Speaker {
            tx,
            gen: Default::default(),
            cancel: Default::default(),
            speaking: Default::default(),
            error: Default::default(),
            cfg: Arc::new(Mutex::new(cfg)),
        };
        let worker = s.clone();
        let _ = std::thread::Builder::new().name("ai-voice".into()).spawn(move || worker.run(rx));
        s
    }

    pub fn configure(&self, cfg: VoiceCfg) {
        *self.cfg.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
    }

    pub fn say(&self, text: String) {
        if !text.trim().is_empty() {
            let _ = self.tx.send(Cmd::Say(self.gen.load(Ordering::Relaxed), text));
        }
    }

    pub fn stop(&self) {
        self.gen.fetch_add(1, Ordering::Relaxed);
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Cmd::Stop);
    }

    fn engine(cfg: &VoiceCfg) -> anyhow::Result<Option<Box<dyn Tts>>> {
        Ok(match cfg.kind {
            TtsKind::Off | TtsKind::Unknown => None,
            TtsKind::Openai => Some(Box::new(OpenAiTts {
                key: cfg.openai_key.clone().ok_or_else(|| anyhow::anyhow!("no OpenAI API key for the voice yet"))?,
                voice: cfg.openai_voice.clone(),
            })),
            TtsKind::Elevenlabs => Some(Box::new(ElevenLabs {
                key: cfg.elevenlabs_key.clone().ok_or_else(|| anyhow::anyhow!("no ElevenLabs API key yet"))?,
                voice: cfg.elevenlabs_voice.clone(),
            })),
            TtsKind::Piper => Some(Box::new(Piper::new(cfg.piper_voices.clone())?)),
        })
    }

    fn run(self, rx: Receiver<Cmd>) {
        let mut player = Player::default();
        let mut engine: Option<(VoiceCfg, Box<dyn Tts>)> = None;
        loop {
            let cmd = match rx.recv_timeout(std::time::Duration::from_millis(400)) {
                Ok(c) => c,
                // Nothing more to say for now: let the last sentence play out.
                Err(RecvTimeoutError::Timeout) => {
                    if self.speaking.load(Ordering::Relaxed) {
                        player.finish();
                        self.speaking.store(false, Ordering::Relaxed);
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            };
            match cmd {
                Cmd::Stop => {
                    player.stop();
                    self.speaking.store(false, Ordering::Relaxed);
                    self.cancel.store(false, Ordering::Relaxed);
                }
                Cmd::Say(g, text) => {
                    if g != self.gen.load(Ordering::Relaxed) {
                        continue;
                    }
                    let cfg = self.cfg.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    if engine.as_ref().is_none_or(|(c, _)| *c != cfg) {
                        engine = match Self::engine(&cfg) {
                            Ok(Some(e)) => Some((cfg.clone(), e)),
                            Ok(None) => None,
                            Err(e) => {
                                *self.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
                                None
                            }
                        };
                    }
                    let Some((_, tts)) = engine.as_mut() else { continue };
                    let lang = if cfg.language.is_empty() || cfg.language == "auto" { super::lang::detect(&text) } else { cfg.language.as_str() };
                    let g_now = &self.gen;
                    let vol = cfg.volume * crate::sound::master();
                    self.speaking.store(true, Ordering::Relaxed);
                    let res = tts.say(&text, lang, &self.cancel, &mut |rate, pcm| {
                        if g_now.load(Ordering::Relaxed) != g || vol < 0.005 {
                            return;
                        }
                        let mut pcm = pcm.to_vec();
                        gain(&mut pcm, vol);
                        player.play(rate, &pcm);
                    });
                    if let Err(e) = res {
                        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
                        // A broken engine is rebuilt for the next sentence.
                        engine = None;
                    }
                }
            }
        }
    }
}
