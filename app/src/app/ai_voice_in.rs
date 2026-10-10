//! Asking by voice: the talk key (or the mic button, or `eve-spai --ptt`) records, letting go
//! turns the recording into a question. A short tone marks the start and the end, since the user is
//! usually in the game and cannot see the app.

use super::*;
use crate::ai::config::SttKind;

/// What started the recording; only the same thing ends it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Key,
    Button,
    Toggle,
}

#[derive(Default)]
pub enum Listen {
    #[default]
    Idle,
    Recording(crate::ai::voice::capture::Recording, Source),
    /// The recording is being turned into text.
    Transcribing(std::sync::mpsc::Receiver<anyhow::Result<Option<String>>>),
}

impl Listen {
    pub fn recording(&self) -> bool {
        matches!(self, Listen::Recording(..))
    }

    pub fn transcribing(&self) -> bool {
        matches!(self, Listen::Transcribing(_))
    }
}

impl SpaiApp {
    pub(crate) fn ai_stt_on(&self) -> bool {
        self.settings.ai.enabled && !matches!(self.settings.ai.voice.stt, SttKind::Off | SttKind::Unknown)
    }

    fn ai_stt_cfg(&self) -> crate::ai::voice::stt::SttCfg {
        let v = &self.settings.ai.voice;
        let key = match v.stt {
            SttKind::Openai => self.ai_secrets.get("openai:openai"),
            SttKind::Groq => self.ai_secrets.get("openai:groq"),
            SttKind::Local => self.ai_secrets.get("stt:local"),
            _ => None,
        };
        crate::ai::voice::stt::SttCfg { kind: v.stt, key, local_url: v.stt_url.clone(), local_model: v.whisper_model.clone(), whisper_file: v.whisper_file.clone() }
    }

    /// EVE words and the systems around the user, so the recogniser spells them right.
    fn ai_stt_hint(&self) -> String {
        let terms: Vec<String> = crate::ai::glossary::rows(&self.settings.ai.glossary).into_iter().map(|r| r.term).collect();
        let mut systems = Vec::new();
        if let (Some(g), Some(me)) = (self.systems.as_ref(), self.player_system()) {
            let mut near: Vec<(i64, u32)> = g.distances_from(me, 4).into_iter().collect();
            near.sort_by_key(|(_, j)| *j);
            systems = near.into_iter().filter_map(|(id, _)| g.info_of(id).map(|i| i.name.clone())).take(30).collect();
        }
        // The fleet dashboard's comms channels, as the user hears them called.
        let channels: Vec<String> = self.fleet.lock().unwrap_or_else(|e| e.into_inner()).seed.mumble_channels.iter().map(|c| c.name.clone()).filter(|n| !n.is_empty()).take(20).collect();
        let mut words = channels;
        words.extend(systems);
        crate::ai::voice::stt::hint(&terms, &words)
    }

    pub(crate) fn ai_listen_start(&mut self, source: Source) {
        if !self.ai_stt_on() || self.headless || !matches!(self.ai_listen, Listen::Idle) {
            return;
        }
        // Talking over the answer means the user wants to be heard, not to hear it.
        self.ai_voice_stop();
        match crate::ai::voice::capture::Recording::start(&self.settings.ai.voice.input_device) {
            Ok(r) => {
                crate::sound::play("info", 0.6);
                self.ai_listen = Listen::Recording(r, source);
            }
            Err(e) => self.toast_error(format!("Microphone: {e}")),
        }
    }

    pub(crate) fn ai_listen_stop(&mut self, source: Source) {
        let Listen::Recording(_, s) = &self.ai_listen else { return };
        if *s != source && source != Source::Toggle {
            return;
        }
        let Listen::Recording(rec, _) = std::mem::take(&mut self.ai_listen) else { return };
        crate::sound::play("beep", 0.4);
        // A cpal stream cannot change threads, and dropping one is instant; Linux's recorder process
        // is ended on the worker instead.
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        let rec = rec.stop();
        let cfg = self.ai_stt_cfg();
        let lang = self.settings.ai.language.clone();
        let hint = self.ai_stt_hint();
        let ctx = self.ui_ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("ai-stt".into()).spawn(move || {
            #[cfg(all(unix, not(target_os = "macos")))]
            let pcm = rec.stop();
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            let pcm = rec;
            let out = match crate::ai::voice::capture::trim(&pcm) {
                Some(speech) => crate::ai::voice::stt::transcribe(&cfg, &speech, &lang, &hint).map(|t| (!t.is_empty()).then_some(t)),
                None => Ok(None),
            };
            let _ = tx.send(out);
            ctx.request_repaint();
        });
        self.ai_listen = Listen::Transcribing(rx);
    }

    /// Runs the talk key and the recording through their states, once a frame.
    pub(crate) fn ai_listen_tick(&mut self) {
        crate::ai::voice::whisper::unload_idle();
        if !self.ai_stt_on() {
            if self.ai_listen.recording() {
                self.ai_listen = Listen::Idle;
            }
            return;
        }
        if self.ai_ptt.is_none() && !self.headless && crate::ai::ptt::SUPPORTED {
            self.ai_ptt = Some(crate::ai::ptt::spawn(self.ui_ctx.clone()));
        }
        let mut events = Vec::new();
        if let Some(p) = &self.ai_ptt {
            *p.bind.lock().unwrap_or_else(|e| e.into_inner()) = self.settings.ai.voice.ptt.clone();
            p.exclusive.store(self.settings.ai.voice.ptt_exclusive, std::sync::atomic::Ordering::Relaxed);
            events.extend(p.rx.try_iter());
        }
        for ev in events {
            match ev {
                crate::ai::ptt::PttEvent::Down => self.ai_listen_start(Source::Key),
                crate::ai::ptt::PttEvent::Up => self.ai_listen_stop(Source::Key),
                crate::ai::ptt::PttEvent::Bound(k) => {
                    self.settings.ai.voice.ptt = Some(k);
                    self.needs_save = true;
                }
            }
        }
        for _ in 0..crate::instance::take_ptt_toggles() {
            if self.ai_listen.recording() {
                self.ai_listen_stop(Source::Toggle);
            } else {
                self.ai_listen_start(Source::Toggle);
            }
        }
        if self.ai_listen.recording() {
            self.ui_ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        let done = match &self.ai_listen {
            Listen::Transcribing(rx) => rx.try_recv().ok(),
            _ => None,
        };
        if let Some(res) = done {
            self.ai_listen = Listen::Idle;
            match res {
                Ok(Some(text)) => {
                    let ctx = self.ui_ctx.clone();
                    let h = self.ai_handle(&ctx);
                    h.send(crate::ai::session::Command::Send { text, voice: true });
                }
                Ok(None) => self.toast("Nothing heard"),
                Err(e) => self.toast_error(format!("Speech recognition: {e}")),
            }
        }
    }

    /// Asks for the next key pressed to become the talk key.
    pub(crate) fn ai_ptt_bind(&mut self) {
        if self.ai_ptt.is_none() && !self.headless && crate::ai::ptt::SUPPORTED {
            self.ai_ptt = Some(crate::ai::ptt::spawn(self.ui_ctx.clone()));
        }
        if let Some(p) = &self.ai_ptt {
            p.binding.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub(crate) fn ai_ptt_binding(&self) -> bool {
        self.ai_ptt.as_ref().is_some_and(|p| p.binding.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub(crate) fn ai_ptt_problem(&self) -> Option<String> {
        self.ai_ptt.as_ref().and_then(|p| p.problem.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }
}
