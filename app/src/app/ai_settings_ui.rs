//! The assistant's settings: switching it on, the model and its key, limits, and the data it may
//! read, as a tree of checkboxes in its own window.

use spai_ui::i18n::Tr;

use super::*;
use crate::ai::config::{OpenAiPreset, ProviderKind};
use crate::ai::perms::{self, Node, Tri};

const EFFORTS: [(&str, &str); 4] = [("low", tr_noop!("Low (fastest)")), ("medium", tr_noop!("Medium")), ("high", tr_noop!("High")), ("max", tr_noop!("Max (slowest)"))];

impl SpaiApp {
    /// The keyring account holding the current provider's key, when it takes one.
    fn ai_key_account(&self) -> Option<String> {
        let a = &self.settings.ai;
        match a.provider {
            ProviderKind::Anthropic => Some("anthropic".into()),
            ProviderKind::OpenaiCompat if a.openai.preset.needs_key() => Some(a.openai.preset.key_account()),
            ProviderKind::Gemini => Some("gemini".into()),
            _ => None,
        }
    }

    pub(crate) fn assistant_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        use egui_phosphor::regular as icon;
        let mut changed = false;
        let head = ui.label(egui::RichText::new(tr!("Assistant")).strong());
        if std::mem::take(&mut self.settings_scroll_to_ai) {
            head.scroll_to_me(Some(egui::Align::TOP));
        }
        changed |= ui
            .checkbox(&mut self.settings.ai.enabled, tr!("AI assistant"))
            .on_hover_text(tr!("Adds the Assistant tab: ask about intel, kills, routes and wormholes, by text or voice"))
            .changed();
        if !self.settings.ai.enabled {
            return changed;
        }
        let account = self.ai_key_account();
        let stored = account.as_deref().is_some_and(|a| self.ai_secrets.has(a));
        egui::Grid::new("ai_settings").num_columns(2).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            let a = &mut self.settings.ai;
            ui.label(tr!("Model service"));
            egui::ComboBox::from_id_salt("ai_provider").selected_text(a.provider.label().tr()).width(280.0).show_ui(ui, |ui| {
                for p in ProviderKind::CHOICES {
                    changed |= ui.menu_value(&mut a.provider, p, p.label().tr()).changed();
                }
            });
            ui.end_row();
            match a.provider {
                ProviderKind::Anthropic => {
                    ui.label(tr!("Model"));
                    changed |= ui.add(egui::TextEdit::singleline(&mut a.anthropic.model).hint_text(tr!("claude-opus-5-5")).desired_width(280.0)).changed();
                    ui.end_row();
                    ui.label(tr!("Effort")).on_hover_text(tr!("How much the model thinks before answering: lower answers sooner"));
                    let cur = EFFORTS.iter().find(|(k, _)| *k == a.anthropic.effort).map_or(tr!("Default"), |(_, l)| l.tr());
                    egui::ComboBox::from_id_salt("ai_effort").selected_text(cur).width(280.0).show_ui(ui, |ui| {
                        for (k, l) in EFFORTS {
                            changed |= ui.menu_value(&mut a.anthropic.effort, k.to_owned(), l.tr()).changed();
                        }
                    });
                    ui.end_row();
                }
                ProviderKind::OpenaiCompat => {
                    ui.label(tr!("Server"));
                    egui::ComboBox::from_id_salt("ai_preset").selected_text(a.openai.preset.label().tr()).width(280.0).show_ui(ui, |ui| {
                        for p in OpenAiPreset::CHOICES {
                            changed |= ui.menu_value(&mut a.openai.preset, p, p.label().tr()).changed();
                        }
                    });
                    ui.end_row();
                    ui.label(tr!("Address"));
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut a.openai.base_url).hint_text(a.openai.preset.base_url()).desired_width(280.0))
                        .on_hover_text(tr!("Leave empty for the server's usual address"))
                        .changed();
                    ui.end_row();
                    ui.label(tr!("Model"));
                    changed |= ui.add(egui::TextEdit::singleline(&mut a.openai.model).hint_text(tr!("e.g. qwen3:14b or gpt-5-mini")).desired_width(280.0)).changed();
                    ui.end_row();
                    ui.label(tr!("Tool calls"));
                    changed |= ui
                        .checkbox(&mut a.openai.tools, tr!("The model can look things up"))
                        .on_hover_text(tr!("Turn off for models without tool calling: they then answer from the summary alone"))
                        .changed();
                    ui.end_row();
                }
                ProviderKind::Gemini => {
                    ui.label(tr!("Model"));
                    changed |= ui.add(egui::TextEdit::singleline(&mut a.gemini.model).hint_text(tr!("gemini-2.5-flash")).desired_width(280.0)).changed();
                    ui.end_row();
                    ui.label(tr!("Effort")).on_hover_text(tr!("How much the model thinks before answering: lower answers sooner"));
                    let cur = EFFORTS.iter().find(|(k, _)| *k == a.gemini.effort).map_or(tr!("Default"), |(_, l)| l.tr());
                    egui::ComboBox::from_id_salt("ai_gemini_effort").selected_text(cur).width(280.0).show_ui(ui, |ui| {
                        for (k, l) in EFFORTS {
                            changed |= ui.menu_value(&mut a.gemini.effort, k.to_owned(), l.tr()).changed();
                        }
                    });
                    ui.end_row();
                }
                ProviderKind::ClaudeCli | ProviderKind::CodexCli => {
                    let claude = a.provider == ProviderKind::ClaudeCli;
                    let cfg = if claude { &mut a.claude_cli } else { &mut a.codex_cli };
                    let default = if claude { "claude" } else { "codex" };
                    ui.label(tr!("Program"));
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut cfg.path).hint_text(default).desired_width(200.0)).changed();
                        let prog = if cfg.path.trim().is_empty() { default } else { cfg.path.trim() };
                        if crate::ai::cli::find(prog).is_some() {
                            ui.label(egui::RichText::new(trf!("{icon}  Found", icon = icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        } else {
                            ui.label(egui::RichText::new(trf!("{icon}  Not found", icon = icon::WARNING)).color(crate::theme::standing::WARNING))
                                .on_hover_text(if claude { tr!("Install Claude Code and sign in with `claude` once.") } else { tr!("Install Codex and sign in with `codex login` once.") });
                        }
                    });
                    ui.end_row();
                    ui.label(tr!("Model"));
                    if claude {
                        // The program takes these short names and any full model id.
                        let claude_models = crate::ai::config::model_choices(ProviderKind::ClaudeCli);
                        let known = claude_models.iter().find(|(k, _)| *k == cfg.model.trim());
                        let other = known.is_none() || self.ai_cli_other_model;
                        let shown = if other { "Other\u{2026}".to_owned() } else { known.map_or("", |(_, l)| l).to_owned() };
                        ui.vertical(|ui| {
                            egui::ComboBox::from_id_salt("ai_claude_model").selected_text(shown).width(280.0).show_ui(ui, |ui| {
                                for &(k, l) in claude_models {
                                    if ui.menu_value(&mut cfg.model, k.to_owned(), l).changed() {
                                        self.ai_cli_other_model = false;
                                        changed = true;
                                    }
                                }
                                if ui.add(egui::Button::new(tr!("Other\u{2026}")).selected(other)).clicked() {
                                    self.ai_cli_other_model = true;
                                    ui.close();
                                }
                            });
                            if other {
                                changed |= ui
                                    .add(egui::TextEdit::singleline(&mut cfg.model).hint_text(tr!("a model id, e.g. claude-opus-5-5")).desired_width(280.0))
                                    .changed();
                            }
                        });
                    } else {
                        changed |= ui
                            .add(egui::TextEdit::singleline(&mut cfg.model).hint_text(tr!("the program's own default")).desired_width(280.0))
                            .changed();
                    }
                    ui.end_row();
                    ui.label("");
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(tr!("Uses your subscription through the program you are signed in to. It reaches only EVE Spai's tools, never files or commands."))
                                .weak(),
                        )
                        .wrap(),
                    );
                    ui.end_row();
                }
                ProviderKind::Unknown => {
                    ui.label("");
                    ui.label(egui::RichText::new(tr!("Not available in this version yet")).weak());
                    ui.end_row();
                }
            }
            if let Some(acc) = &account {
                ui.label(tr!("API key"));
                ui.horizontal(|ui| {
                    if stored {
                        ui.label(egui::RichText::new(trf!("{icon}  Stored in the keychain", icon = icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        if ui.button(tr!("Remove")).clicked() {
                            self.ai_secrets.delete(acc);
                        }
                    } else {
                        ui.add(egui::TextEdit::singleline(&mut self.ai_key_input).password(true).hint_text(tr!("Paste the key")).desired_width(200.0));
                        if ui.add_enabled(!self.ai_key_input.trim().is_empty(), egui::Button::new(tr!("Save"))).clicked() {
                            match self.ai_secrets.set(acc, &self.ai_key_input) {
                                Ok(()) => self.toast(tr!("Key stored in the keychain")),
                                Err(e) => self.toast_error(trf!("Could not store the key: {e}", e = e)),
                            }
                            self.ai_key_input.clear();
                        }
                    }
                });
                ui.end_row();
            }
            let a = &mut self.settings.ai;
            ui.label(tr!("Limits")).on_hover_text(tr!("The assistant pauses when either is reached"));
            ui.horizontal(|ui| {
                changed |= ui.add(egui::DragValue::new(&mut a.caps.max_calls_per_hour).range(0..=10_000).suffix(" calls/hour")).changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut a.caps.max_tokens_per_day).range(0..=100_000_000).speed(10_000.0).suffix(" tokens/day"))
                    .changed();
            });
            ui.end_row();
            ui.label(tr!("Nearby means"));
            changed |= ui
                .add(egui::DragValue::new(&mut a.situation_jumps).range(1..=20).suffix(" jumps"))
                .on_hover_text(tr!("How far from your characters the assistant's standing summary looks"))
                .changed();
            ui.end_row();
        });
        egui::Grid::new("ai_settings_lang").num_columns(2).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            let a = &mut self.settings.ai;
            ui.label(tr!("Language"));
            let cur = crate::ai::config::LANGUAGES.iter().find(|(c, _, _)| *c == a.language).map_or("Same as the question", |(_, l, _)| l);
            egui::ComboBox::from_id_salt("ai_language").selected_text(cur).width(280.0).show_ui(ui, |ui| {
                for (code, label, _) in crate::ai::config::LANGUAGES {
                    changed |= ui.menu_value(&mut a.language, code.to_owned(), label).changed();
                }
            });
            ui.end_row();
            ui.label(tr!("Your instructions")).on_hover_text(tr!("Added to the assistant's own; yours win where they differ"));
            changed |= ui
                .add(
                    egui::TextEdit::multiline(&mut a.instructions)
                        .desired_rows(3)
                        .desired_width(280.0)
                        .hint_text(tr!("e.g. I fly with Goonswarm out of 1DQ1-A. Keep answers to two sentences.")),
                )
                .changed();
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button(trf!("{icon}  Data access\u{2026}", icon = icon::KEY)).on_hover_text(tr!("What the assistant may read and do")).clicked() {
                self.ai_perms_open = true;
            }
            if ui.button(trf!("{icon}  Glossary\u{2026}", icon = icon::BOOK_OPEN)).on_hover_text(tr!("How the assistant reads EVE terms")).clicked() {
                self.ai_glossary_open = true;
            }
            if ui.button(trf!("{icon}  Feeds\u{2026}", icon = icon::RSS)).on_hover_text(tr!("Outside sources for the assistant to read and watch")).clicked() {
                self.ai_feeds_open = true;
            }
        });
        changed |= self.ai_voice_settings(ui);
        changed |= self.ai_voice_in_settings(ui);
        if changed {
            self.ai_push_facts(true);
        }
        changed
    }

    /// Spoken replies: which voice, how loud, and what each engine needs.
    fn ai_voice_settings(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::ai::config::TtsKind;
        use egui_phosphor::regular as icon;
        let mut changed = false;
        ui.add_space(6.0);
        ui.label(egui::RichText::new(tr!("Voice")).strong());
        let label = |k: TtsKind| match k {
            TtsKind::Off => tr!("Off"),
            TtsKind::Piper => tr!("Piper, on this computer"),
            TtsKind::Openai => "OpenAI",
            TtsKind::Elevenlabs => "ElevenLabs",
            TtsKind::Unknown => tr!("Unknown"),
        };
        let secrets = self.ai_secrets.clone();
        let mut note: Option<Result<String, String>> = None;
        let mut reread = false;
        egui::Grid::new("ai_voice_settings").num_columns(2).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            let key_input = &mut self.ai_voice_key_input;
            let v = &mut self.settings.ai.voice;
            let mut key_row = |ui: &mut egui::Ui, acc: &str| {
                ui.label(tr!("API key"));
                ui.horizontal(|ui| {
                    if secrets.has(acc) {
                        ui.label(egui::RichText::new(trf!("{icon}  Stored in the keychain", icon = icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        if ui.button(tr!("Remove")).clicked() {
                            secrets.delete(acc);
                            reread = true;
                        }
                    } else {
                        ui.add(egui::TextEdit::singleline(key_input).password(true).hint_text(tr!("Paste the key")).desired_width(200.0));
                        if ui.add_enabled(!key_input.trim().is_empty(), egui::Button::new(tr!("Save"))).clicked() {
                            note = Some(secrets.set(acc, key_input.trim()).map(|_| tr!("Key stored in the keychain").to_owned()).map_err(|e| trf!("Could not store the key: {e}", e = e)));
                            key_input.clear();
                            reread = true;
                        }
                    }
                });
                ui.end_row();
            };
            ui.label(tr!("Speech"));
            egui::ComboBox::from_id_salt("ai_tts").selected_text(label(v.tts)).width(280.0).show_ui(ui, |ui| {
                for k in [TtsKind::Off, TtsKind::Piper, TtsKind::Openai, TtsKind::Elevenlabs] {
                    changed |= ui.menu_value(&mut v.tts, k, label(k)).changed();
                }
            });
            ui.end_row();
            if v.tts == TtsKind::Off {
                return;
            }
            ui.label("");
            changed |= ui
                .checkbox(&mut v.speak_replies, tr!("Speak every answer"))
                .on_hover_text(tr!("Answers to a push-to-talk question are spoken either way, and so are watch matches when this is on"))
                .changed();
            ui.end_row();
            ui.label(tr!("Volume"));
            changed |= ui.add(egui::Slider::new(&mut v.volume, 0.0..=1.0).show_value(false)).changed();
            ui.end_row();
            match v.tts {
                TtsKind::Openai => {
                    ui.label(tr!("Voice"));
                    let voices = ["alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse"];
                    egui::ComboBox::from_id_salt("ai_openai_voice").selected_text(v.cloud_voice.clone()).width(280.0).show_ui(ui, |ui| {
                        for n in voices {
                            changed |= ui.menu_value(&mut v.cloud_voice, n.to_owned(), n).changed();
                        }
                    });
                    ui.end_row();
                    key_row(ui, "openai:openai");
                }
                TtsKind::Elevenlabs => {
                    ui.label(tr!("Voice id"));
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut v.elevenlabs_voice).hint_text(tr!("a stock voice when empty")).desired_width(280.0))
                        .on_hover_text(tr!("From your ElevenLabs voice library. The multilingual model speaks every answer language."))
                        .changed();
                    ui.end_row();
                    key_row(ui, "elevenlabs");
                }
                _ => {}
            }
        });
        if self.settings.ai.voice.tts == crate::ai::config::TtsKind::Off {
            return changed;
        }
        match note {
            Some(Ok(m)) => self.toast(m),
            Some(Err(m)) => self.toast_error(m),
            None => {}
        }
        if reread {
            self.ai_voice_cfg_at = None;
        }
        if self.settings.ai.voice.tts == crate::ai::config::TtsKind::Piper {
            changed |= self.ai_piper_voices_ui(ui);
        }
        ui.horizontal(|ui| {
            if ui.button(trf!("{icon}  Try it", icon = icon::SPEAKER_HIGH)).on_hover_text(tr!("Says a sample answer in the answer language")).clicked() {
                let lang = self.settings.ai.language.clone();
                self.ai_voice_test(&lang);
            }
        });
        changed
    }

    /// Asking by voice: who turns speech into text, the talk key, the microphone.
    fn ai_voice_in_settings(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::ai::config::SttKind;
        use egui_phosphor::regular as icon;
        let mut changed = false;
        ui.add_space(6.0);
        ui.label(egui::RichText::new(tr!("Voice questions")).strong());
        let label = |k: SttKind| match k {
            SttKind::Off => tr!("Off"),
            SttKind::Local => tr!("A speech server on this computer"),
            SttKind::Openai => "OpenAI",
            SttKind::Groq => "Groq",
            SttKind::Whisper => tr!("Whisper, on this computer"),
            SttKind::Unknown => tr!("Unknown"),
        };
        let secrets = self.ai_secrets.clone();
        let mut note: Option<Result<String, String>> = None;
        let binding = self.ai_ptt_binding();
        let mut bind = false;
        let problem = self.ai_ptt_problem();
        if self.ai_mics.is_none() && self.settings.ai.voice.stt != SttKind::Off && !self.headless {
            self.ai_mics = Some(crate::ai::voice::capture::devices());
        }
        let mics = self.ai_mics.clone().unwrap_or_default();
        egui::Grid::new("ai_voice_in").num_columns(2).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            let key_input = &mut self.ai_stt_key_input;
            let v = &mut self.settings.ai.voice;
            ui.label(tr!("Recognition"));
            egui::ComboBox::from_id_salt("ai_stt").selected_text(label(v.stt)).width(280.0).show_ui(ui, |ui| {
                let kinds: &[SttKind] = if crate::ai::voice::whisper::AVAILABLE {
                    &[SttKind::Off, SttKind::Whisper, SttKind::Local, SttKind::Openai, SttKind::Groq]
                } else {
                    &[SttKind::Off, SttKind::Local, SttKind::Openai, SttKind::Groq]
                };
                for &k in kinds {
                    changed |= ui.menu_value(&mut v.stt, k, label(k)).changed();
                }
            });
            ui.end_row();
            if v.stt == SttKind::Off {
                return;
            }
            let account = match v.stt {
                SttKind::Local => {
                    ui.label(tr!("Address"));
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut v.stt_url).hint_text("http://localhost:8000/v1").desired_width(280.0))
                        .on_hover_text(tr!("Any server with OpenAI's transcription API: speaches, faster-whisper-server, LocalAI"))
                        .changed();
                    ui.end_row();
                    ui.label(tr!("Model"));
                    changed |= ui.add(egui::TextEdit::singleline(&mut v.whisper_model).hint_text(tr!("Systran/faster-whisper-small")).desired_width(280.0)).changed();
                    ui.end_row();
                    None
                }
                SttKind::Openai => Some("openai:openai"),
                SttKind::Groq => Some("openai:groq"),
                _ => None,
            };
            if let Some(acc) = account {
                ui.label(tr!("API key"));
                ui.horizontal(|ui| {
                    if secrets.has(acc) {
                        ui.label(egui::RichText::new(trf!("{icon}  Stored in the keychain", icon = icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        if ui.button(tr!("Remove")).clicked() {
                            secrets.delete(acc);
                        }
                    } else {
                        ui.add(egui::TextEdit::singleline(key_input).password(true).hint_text(tr!("Paste the key")).desired_width(200.0));
                        if ui.add_enabled(!key_input.trim().is_empty(), egui::Button::new(tr!("Save"))).clicked() {
                            note = Some(secrets.set(acc, key_input.trim()).map(|_| tr!("Key stored in the keychain").to_owned()).map_err(|e| trf!("Could not store the key: {e}", e = e)));
                            key_input.clear();
                        }
                    }
                });
                ui.end_row();
            }
            ui.label(tr!("Talk key"));
            ui.horizontal(|ui| {
                if !crate::ai::ptt::SUPPORTED {
                    ui.label(egui::RichText::new(tr!("Bind `eve-spai --ptt` to a system shortcut to start and stop listening")).weak());
                    return;
                }
                let shown = if binding {
                    tr!("Press a key or a side mouse button\u{2026}").to_owned()
                } else {
                    v.ptt.as_ref().filter(|k| k.platform == std::env::consts::OS).map_or(tr!("Not set").to_owned(), |k| k.label.clone())
                };
                if ui.button(format!("{}  {shown}", icon::KEYBOARD)).on_hover_text(tr!("Click, then press the key to hold while you talk")).clicked() {
                    bind = true;
                }
                if v.ptt.is_some() && ui.button(icon::X).on_hover_text(tr!("No talk key")).clicked() {
                    v.ptt = None;
                    changed = true;
                }
            });
            ui.end_row();
            if crate::ai::ptt::SUPPORTED {
                ui.label("");
                changed |= ui
                    .checkbox(&mut v.ptt_exclusive, tr!("Keep the key from the game"))
                    .on_hover_text(tr!("Off: the game sees the key too, handy when it is also your voice chat key"))
                    .changed();
                ui.end_row();
            }
            ui.label(tr!("Microphone"));
            let cur = if v.input_device.is_empty() { tr!("System default").to_owned() } else { v.input_device.clone() };
            egui::ComboBox::from_id_salt("ai_mic").selected_text(cur).width(280.0).truncate().show_ui(ui, |ui| {
                changed |= ui.menu_value(&mut v.input_device, String::new(), tr!("System default")).changed();
                for m in &mics {
                    changed |= ui.menu_value(&mut v.input_device, m.clone(), m).changed();
                }
            });
            ui.end_row();
        });
        if self.settings.ai.voice.stt == SttKind::Off {
            return changed;
        }
        if self.settings.ai.voice.stt == SttKind::Whisper {
            changed |= self.ai_whisper_models_ui(ui);
        }
        if let Some(p) = problem {
            ui.label(egui::RichText::new(p).color(crate::theme::standing::WARNING));
        } else if cfg!(target_os = "linux") && crate::ai::ptt::SUPPORTED {
            ui.label(egui::RichText::new(tr!("The talk key works while the game or another X11 window has focus; in EVE Spai, hold the mic button.")).weak());
        }
        match note {
            Some(Ok(m)) => self.toast(m),
            Some(Err(m)) => self.toast_error(m),
            None => {}
        }
        if bind {
            self.ai_ptt_bind();
        }
        if binding {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        changed
    }

    /// The Whisper model in use, picked from all of them, with what each costs and whether it is here.
    fn ai_whisper_models_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::ai::voice::whisper::{self, MODELS};
        use egui_phosphor::regular as icon;
        let mut changed = false;
        let progress = self.ai_whisper_progress.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let cur = self.settings.ai.voice.whisper_file.clone();
        let shown = |m: &whisper::Model| trf!("{name}  \u{b7}  {size}  \u{b7}  about {mb} MB memory", name = m.label().tr(), size = super::fmt_bytes(m.bytes), mb = m.ram_mb());
        let mut get: Option<String> = None;
        egui::Grid::new("ai_whisper").num_columns(2).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            ui.label(tr!("Whisper model")).on_hover_text(
                tr!("Bigger models hear better and take longer: tiny and base answer in well under a second, small in one or two, \
                 medium and large in several on a CPU. q5 and q8 are smaller copies that sound nearly the same. \
                 English-only models hear every language as English."),
            );
            let sel = whisper::model(&cur).map_or(cur.clone(), &shown);
            let v = &mut self.settings.ai.voice;
            egui::ComboBox::from_id_salt("ai_whisper_model").selected_text(sel).width(420.0).truncate().height(420.0).show_ui(ui, |ui| {
                for m in MODELS {
                    let mark = if whisper::installed(m.file) { format!("{}  ", icon::CHECK) } else { "      ".to_owned() };
                    changed |= ui.menu_value(&mut v.whisper_file, m.file.to_owned(), format!("{mark}{}", shown(m))).changed();
                }
            });
            ui.end_row();
            ui.label("");
            ui.horizontal(|ui| {
                if whisper::installed(&cur) {
                    let loaded = whisper::loaded().as_deref() == Some(cur.as_str());
                    ui.label(egui::RichText::new(if loaded { tr!("Downloaded, loaded now") } else { tr!("Downloaded") }).weak());
                    if ui.button(trf!("{icon}  Remove", icon = icon::TRASH)).on_hover_text(tr!("Delete the file to free the disk space")).clicked() {
                        whisper::remove(&cur);
                    }
                } else if let Some(m) = whisper::model(&cur) {
                    if ui.add_enabled(!progress.busy, egui::Button::new(trf!("{icon}  Get, {v}", icon = icon::DOWNLOAD_SIMPLE, v = super::fmt_bytes(m.bytes)))).clicked() {
                        get = Some(cur.clone());
                    }
                }
            });
            ui.end_row();
        });
        if progress.busy {
            let frac = if progress.total > 0 { progress.done as f32 / progress.total as f32 } else { 0.0 };
            ui.add(egui::ProgressBar::new(frac).text(format!("{}  {}", progress.what, super::fmt_bytes(progress.done))));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        } else if let Some(e) = &progress.error {
            ui.label(egui::RichText::new(e).color(crate::theme::standing::WARNING));
        }
        ui.label(egui::RichText::new(tr!("The model loads when you ask and is let go after five idle minutes.")).weak());
        if let Some(f) = get {
            crate::ai::voice::whisper::install(f, self.ai_whisper_progress.clone(), Some(self.ui_ctx.clone()));
        }
        changed
    }

    /// One row per answer language: its Piper voice and whether it is installed.
    fn ai_piper_voices_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::ai::voice::models;
        let mut changed = false;
        let progress = self.ai_piper_progress.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut fetch: Vec<String> = Vec::new();
        let program = models::has_program();
        egui::Grid::new("ai_piper_voices").num_columns(3).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            for (code, name, _) in crate::ai::config::LANGUAGES.iter().filter(|(c, _, _)| *c != "auto") {
                let chosen = self.settings.ai.voice.piper_voices.get(*code).cloned().or_else(|| models::default_voice(code).map(|v| v.id.to_owned())).unwrap_or_default();
                ui.label(*name);
                let shown = models::voice(&chosen).map_or(chosen.clone(), |v| v.label.to_owned());
                egui::ComboBox::from_id_salt(("piper_voice", *code)).selected_text(shown).width(200.0).show_ui(ui, |ui| {
                    let mut pick = chosen.clone();
                    for v in models::CATALOG.iter().filter(|v| v.lang == *code) {
                        if ui.menu_value(&mut pick, v.id.to_owned(), v.label).on_hover_text(trf!("Recordings licensed {v}", v = v.license)).changed() {
                            self.settings.ai.voice.piper_voices.insert((*code).to_owned(), pick.clone());
                            changed = true;
                        }
                    }
                });
                if models::has_voice(&chosen) && program {
                    ui.label(egui::RichText::new(tr!("Installed")).weak());
                } else if ui.add_enabled(!progress.busy, egui::Button::new(trf!("{icon}  Get, 63 MB", icon = egui_phosphor::regular::DOWNLOAD_SIMPLE))).clicked() {
                    fetch.push(chosen);
                }
                ui.end_row();
            }
        });
        if progress.busy {
            let frac = if progress.total > 0 { progress.done as f32 / progress.total as f32 } else { 0.0 };
            ui.add(egui::ProgressBar::new(frac).text(format!("{}  {}", progress.what, super::fmt_bytes(progress.done))));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        } else if let Some(e) = &progress.error {
            ui.label(egui::RichText::new(e).color(crate::theme::standing::WARNING));
        } else if !program {
            ui.label(egui::RichText::new(tr!("Piper itself (about 25 MB) comes with the first voice.")).weak());
        }
        if !fetch.is_empty() {
            models::install_missing(fetch, self.ai_piper_progress.clone(), Some(self.ui_ctx.clone()));
        }
        changed
    }

    pub(crate) fn ai_perms_window(&mut self, ctx: &egui::Context) {
        if !self.ai_perms_open {
            return;
        }
        let unlocked = perms::Unlocked { fleet: self.fleet_on(), rescue: self.rescue_on() };
        let channels = self.settings.intel_channels.clone();
        let feeds: Vec<(String, String)> = self.settings.ai.feeds.iter().map(|f| (f.perm_key(), f.name.clone())).collect();
        let feed_keys: Vec<String> = feeds.iter().map(|(k, _)| k.clone()).collect();
        let dynamic = move |key: &str| -> Vec<String> {
            match key {
                "intel.chatlogs" => channels.iter().map(|c| perms::channel_key(c)).collect(),
                "feeds" => feed_keys.clone(),
                _ => Vec::new(),
            }
        };
        let mut changed = false;
        let mut perms_map = std::mem::take(&mut self.settings.ai.perms);
        let mut auto = std::mem::take(&mut self.settings.ai.auto_actions);
        let channels = self.settings.intel_channels.clone();
        let keep = Self::dialog_viewport(ctx, "ai_perms", tr!("EVE Spai - Assistant data access"), [460.0, 640.0], |ui| {
            ui.label(egui::RichText::new(tr!("The assistant reads only what is ticked. Actions ask you first unless you let a kind through.")).weak());
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for node in perms::TREE {
                    changed |= perm_node(ui, node, &mut perms_map, unlocked, &dynamic, &channels, &feeds, &mut auto);
                }
            });
        });
        self.settings.ai.perms = perms_map;
        self.settings.ai.auto_actions = auto;
        if changed {
            self.needs_save = true;
            self.ai_push_facts(true);
        }
        if !keep {
            self.ai_perms_open = false;
        }
    }
}

/// One node of the tree and its children. Returns whether anything changed.
fn perm_node(
    ui: &mut egui::Ui,
    node: &Node,
    map: &mut std::collections::BTreeMap<String, bool>,
    u: perms::Unlocked,
    dynamic: &dyn Fn(&str) -> Vec<String>,
    channels: &[String],
    feeds: &[(String, String)],
    auto: &mut Vec<String>,
) -> bool {
    if !perms::visible(node, u) {
        return false;
    }
    let mut changed = false;
    let leaves = perms::descendants(node, dynamic);
    let state = perms::state(map, node.key, &leaves, u);
    let locked = node.key == "sde";
    let mut on = state == Tri::On || locked;
    let row = |ui: &mut egui::Ui, on: &mut bool| {
        let r = ui.add_enabled(!locked, egui::Checkbox::new(on, spai_ui::i18n::t(node.label)).indeterminate(state == Tri::Mixed));
        if node.hint.is_empty() { r } else { r.on_hover_text(spai_ui::i18n::t(node.hint)) }
    };
    let kids = dynamic(node.key);
    if node.children.is_empty() && kids.is_empty() {
        // Indented by the width of a branch's arrow, so every checkbox in a level lines up.
        ui.horizontal(|ui| {
            ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x * 0.5);
            if row(ui, &mut on).changed() {
                perms::set(map, node.key, on);
                changed = true;
            }
            // Actions can be let through without a click, one kind at a time.
            if on && node.key.starts_with("actions.") {
                let mut free = auto.iter().any(|a| a == node.key);
                let shown = if free { tr!("without asking") } else { tr!("asks first") };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| egui::ComboBox::from_id_salt(("ai_auto", node.key)).selected_text(shown).width(130.0).show_ui(ui, |ui| {
                    let a = ui.menu_value(&mut free, false, tr!("asks first")).changed();
                    let b = ui.menu_value(&mut free, true, tr!("without asking")).on_hover_text(if node.key == "actions.jabber" { tr!("Broadcasts (!bping, !bcast) still ask") } else { "" }).changed();
                    if a || b {
                        auto.retain(|k| k != node.key);
                        if free {
                            auto.push(node.key.to_owned());
                        }
                        changed = true;
                    }
                }));
            }
        });
        return changed;
    }
    let id = ui.make_persistent_id(("ai_perm", node.key));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            if row(ui, &mut on).changed() {
                perms::set(map, node.key, on);
                changed = true;
            }
        })
        .body(|ui| {
            for c in node.children {
                changed |= perm_node(ui, c, map, u, dynamic, channels, feeds, auto);
            }
            let extra: Vec<(String, String)> = match node.key {
                "intel.chatlogs" => channels.iter().map(|ch| (perms::channel_key(ch), ch.clone())).collect(),
                "feeds" => feeds.to_vec(),
                _ => Vec::new(),
            };
            {
                for (key, ch) in extra {
                    let mut on = perms::allowed(map, &key, u);
                    ui.horizontal(|ui| {
                        ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x * 0.5);
                        if ui.checkbox(&mut on, "").changed() {
                            map.insert(key.clone(), on);
                            changed = true;
                        }
                        ui.add(egui::Label::new(&ch).truncate()).on_hover_text(&ch);
                    });
                }
            }
        });
    changed
}

impl SpaiApp {
    /// Outside feeds: the list with their state, and one at a time being added or edited.
    pub(crate) fn ai_feeds_window(&mut self, ctx: &egui::Context) {
        use crate::ai::feeds::{FeedDef, FeedKind, MIN_INTERVAL};
        use egui_phosphor::regular as icon;
        if !self.ai_feeds_open {
            return;
        }
        let mut changed = false;
        let mut feeds = std::mem::take(&mut self.settings.ai.feeds);
        let mut edit = self.ai_feed_edit.take();
        let mut secret = std::mem::take(&mut self.ai_feed_secret);
        let secrets = self.ai_secrets.clone();
        let (errors, counts) = {
            let st = self.ai_feeds.lock().unwrap_or_else(|e| e.into_inner());
            let mut counts: std::collections::HashMap<u64, usize> = Default::default();
            for i in &st.items {
                *counts.entry(i.feed).or_default() += 1;
            }
            (st.errors.clone(), counts)
        };
        let mut note: Option<String> = None;
        let keep = Self::dialog_viewport(ctx, "ai_feeds", tr!("EVE Spai - Assistant feeds"), [560.0, 600.0], |ui| {
            ui.label(egui::RichText::new(tr!("News, timers, broadcasts: anything with a feed. The assistant reads the ones allowed under Data access and watches them for you.")).weak());
            ui.add_space(4.0);
            if let Some(e) = edit.as_mut() {
                egui::Grid::new("ai_feed_form").num_columns(2).spacing([12.0, 6.0]).min_col_width(100.0).show(ui, |ui| {
                    ui.label(tr!("Name"));
                    ui.add(egui::TextEdit::singleline(&mut e.name).hint_text(tr!("Timers board")).desired_width(360.0));
                    ui.end_row();
                    ui.label(tr!("Address"));
                    ui.add(egui::TextEdit::singleline(&mut e.url).hint_text("https://\u{2026}").desired_width(360.0));
                    ui.end_row();
                    ui.label(tr!("Kind"));
                    egui::ComboBox::from_id_salt("ai_feed_kind").selected_text(e.kind.label().tr()).width(200.0).show_ui(ui, |ui| {
                        for k in FeedKind::CHOICES {
                            ui.menu_value(&mut e.kind, k, k.label().tr());
                        }
                    });
                    ui.end_row();
                    if e.kind == FeedKind::Json {
                        ui.label(tr!("Items at")).on_hover_text(tr!("Where the list is, as dot-separated keys; empty when the answer is the list"));
                        ui.add(egui::TextEdit::singleline(&mut e.items_path).hint_text(tr!("data.items")).desired_width(200.0));
                        ui.end_row();
                        for (label, f) in [(tr!("Text field"), &mut e.text_field), (tr!("Title field"), &mut e.title_field), (tr!("Time field"), &mut e.time_field), (tr!("Link field"), &mut e.link_field)] {
                            ui.label(label);
                            ui.add(egui::TextEdit::singleline(f).desired_width(200.0));
                            ui.end_row();
                        }
                    }
                    ui.label(tr!("Check every"));
                    ui.add(egui::DragValue::new(&mut e.interval).range(MIN_INTERVAL..=86_400).suffix(" s"));
                    ui.end_row();
                    ui.label(tr!("Header")).on_hover_text(tr!("For feeds that want a key: the header name here, its value goes to the keychain"));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut e.auth_header).hint_text(tr!("none, or e.g. Authorization")).desired_width(160.0));
                        if !e.auth_header.trim().is_empty() {
                            if secrets.has(&e.secret_account()) {
                                ui.label(egui::RichText::new(trf!("{icon}  Value stored", icon = icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                            } else {
                                ui.add(egui::TextEdit::singleline(&mut secret).password(true).hint_text(tr!("its value")).desired_width(150.0));
                            }
                        }
                    });
                    ui.end_row();
                });
                let ok = !e.name.trim().is_empty() && (e.url.starts_with("https://") || e.url.starts_with("http://"));
                let mut done = None;
                ui.horizontal(|ui| {
                    if ui.add_enabled(ok, egui::Button::new(trf!("{icon}  Save", icon = icon::CHECK))).clicked() {
                        done = Some(true);
                    }
                    if ui.button(tr!("Cancel")).clicked() {
                        done = Some(false);
                    }
                });
                match done {
                    Some(true) => {
                        let e = edit.take().expect("editing");
                        if !secret.trim().is_empty() {
                            if let Err(err) = secrets.set(&e.secret_account(), secret.trim()) {
                                note = Some(trf!("Could not store the header value: {err}", err = err));
                            }
                            secret.clear();
                        }
                        match feeds.iter_mut().find(|f| f.id == e.id) {
                            Some(slot) => *slot = e,
                            None => feeds.push(e),
                        }
                        changed = true;
                    }
                    Some(false) => {
                        edit = None;
                        secret.clear();
                    }
                    None => {}
                }
                ui.separator();
            } else if ui.button(trf!("{icon}  Add a feed", icon = icon::PLUS)).clicked() {
                let id = feeds.iter().map(|f| f.id).max().unwrap_or(0) + 1;
                edit = Some(FeedDef { id, ..Default::default() });
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if feeds.is_empty() {
                    ui.label(egui::RichText::new(tr!("No feeds yet.")).weak());
                }
                let mut remove = None;
                for f in feeds.iter_mut() {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut f.enabled, "").on_hover_text(tr!("Checked: polled")).changed();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(icon::TRASH).on_hover_text(tr!("Delete")).clicked() {
                                remove = Some(f.id);
                            }
                            if ui.button(icon::PENCIL_SIMPLE).on_hover_text(tr!("Edit")).clicked() {
                                edit = Some(f.clone());
                            }
                            match errors.get(&f.id) {
                                Some(e) => {
                                    ui.label(egui::RichText::new(icon::WARNING).color(crate::theme::standing::WARNING)).on_hover_text(e);
                                }
                                None => {
                                    ui.label(egui::RichText::new(trf!("{v} items", v = counts.get(&f.id).copied().unwrap_or(0))).weak());
                                }
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(egui::RichText::new(&f.name).strong()).truncate()).on_hover_text(format!("{}\n{}", f.name, f.url));
                            });
                        });
                    });
                }
                if let Some(id) = remove {
                    if let Some(f) = feeds.iter().find(|f| f.id == id) {
                        secrets.delete(&f.secret_account());
                    }
                    feeds.retain(|f| f.id != id);
                    changed = true;
                }
            });
        });
        self.settings.ai.feeds = feeds;
        self.ai_feed_edit = edit;
        self.ai_feed_secret = secret;
        if let Some(n) = note {
            self.toast_error(n);
        }
        if changed {
            self.needs_save = true;
            self.ai_push_facts(true);
        }
        if !keep {
            self.ai_feeds_open = false;
        }
    }

    /// The glossary: every base entry editable and resettable, the user's own entries, and a way
    /// back to the shipped list.
    pub(crate) fn ai_glossary_window(&mut self, ctx: &egui::Context) {
        use crate::ai::glossary::{rows, Entry};
        use egui_phosphor::regular as icon;
        if !self.ai_glossary_open {
            return;
        }
        let mut changed = false;
        let mut edits = std::mem::take(&mut self.settings.ai.glossary);
        let mut filter = std::mem::take(&mut self.ai_glossary_filter);
        let mut new = std::mem::take(&mut self.ai_glossary_new);
        let mut reset_all = self.ai_glossary_reset_all;
        let keep = Self::dialog_viewport(ctx, "ai_glossary", tr!("EVE Spai - Assistant glossary"), [620.0, 700.0], |ui| {
            ui.label(egui::RichText::new(tr!("How the assistant reads EVE terms. Change any meaning, clear one to hide it, or add your own.")).weak());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(icon::MAGNIFYING_GLASS);
                ui.add(egui::TextEdit::singleline(&mut filter).hint_text(tr!("Find a term")).desired_width(200.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if reset_all {
                        if ui.button(tr!("Cancel")).clicked() {
                            reset_all = false;
                        }
                        if ui.button(egui::RichText::new(tr!("Reset all")).color(crate::theme::standing::WARNING)).clicked() {
                            edits.overrides.clear();
                            reset_all = false;
                            changed = true;
                        }
                        ui.label(tr!("Put every base entry back?"));
                    } else if ui
                        .add_enabled(!edits.overrides.is_empty(), egui::Button::new(trf!("{icon}  Reset all", icon = icon::ARROW_COUNTER_CLOCKWISE)))
                        .on_hover_text(tr!("Every base entry back to the shipped meaning; your own entries stay"))
                        .clicked()
                    {
                        reset_all = true;
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut new.0).hint_text(tr!("New term")).desired_width(140.0));
                // The button first from the right, so its label decides its width in any language.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!new.0.trim().is_empty() && !new.1.trim().is_empty(), egui::Button::new(trf!("{icon}  Add", icon = icon::PLUS))).clicked() {
                        edits.custom.push(Entry { term: new.0.trim().to_owned(), meaning: new.1.trim().to_owned() });
                        new = Default::default();
                        changed = true;
                    }
                    ui.add(egui::TextEdit::singleline(&mut new.1).hint_text(tr!("What it means")).desired_width(ui.available_width()));
                });
            });
            ui.add_space(4.0);
            let f = filter.trim().to_lowercase();
            let mut remove_custom: Option<usize> = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut custom_i = 0usize;
                for r in rows(&edits) {
                    let ci = if r.custom {
                        custom_i += 1;
                        Some(custom_i - 1)
                    } else {
                        None
                    };
                    if !f.is_empty() && !r.term.to_lowercase().contains(&f) && !r.meaning.to_lowercase().contains(&f) {
                        continue;
                    }
                    let hidden = r.meaning.trim().is_empty();
                    ui.horizontal(|ui| {
                        let term = if r.custom { egui::RichText::new(&r.term).strong().color(ui.visuals().hyperlink_color) } else { egui::RichText::new(&r.term).strong() };
                        ui.scope(|ui| {
                            ui.set_width(120.0);
                            ui.add(egui::Label::new(term).truncate()).on_hover_text(if r.custom { tr!("Your own entry") } else { tr!("From the shipped glossary") });
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(i) = ci {
                                if ui.button(icon::TRASH).on_hover_text(tr!("Delete")).clicked() {
                                    remove_custom = Some(i);
                                }
                            } else if r.changed() || hidden {
                                if ui.button(icon::ARROW_COUNTER_CLOCKWISE).on_hover_text(trf!("Back to: {v}", v = r.base.unwrap_or_default())).clicked() {
                                    edits.overrides.remove(&r.term);
                                    changed = true;
                                }
                            } else if ui.button(icon::EYE_SLASH).on_hover_text(tr!("Hide this term from the assistant")).clicked() {
                                edits.overrides.insert(r.term.clone(), String::new());
                                changed = true;
                            }
                            let mut meaning = r.meaning.clone();
                            let edit = egui::TextEdit::singleline(&mut meaning)
                                .desired_width(ui.available_width())
                                .hint_text(tr!("Hidden: the assistant does not get this term"));
                            if ui.add(edit).on_hover_text(&r.meaning).changed() {
                                match ci {
                                    Some(i) => edits.custom[i].meaning = meaning.clone(),
                                    None if Some(meaning.as_str()) == r.base => {
                                        edits.overrides.remove(&r.term);
                                    }
                                    None => {
                                        edits.overrides.insert(r.term.clone(), meaning.clone());
                                    }
                                }
                                changed = true;
                            }
                        });
                    });
                }
            });
            if let Some(i) = remove_custom {
                edits.custom.remove(i);
                changed = true;
            }
        });
        self.settings.ai.glossary = edits;
        self.ai_glossary_filter = filter;
        self.ai_glossary_new = new;
        self.ai_glossary_reset_all = reset_all;
        if changed {
            self.needs_save = true;
            self.ai_push_facts(true);
        }
        if !keep {
            self.ai_glossary_open = false;
            self.ai_glossary_reset_all = false;
        }
    }
}
