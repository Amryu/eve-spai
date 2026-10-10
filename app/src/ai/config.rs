//! The assistant's settings. One sub-struct of `Settings`, so a field added or changed here can never
//! fail the parse of the whole file: every field defaults, and every enum reads an unknown value as
//! `Unknown` instead of failing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: ProviderKind,
    pub anthropic: ApiCfg,
    pub openai: OpenAiCfg,
    pub gemini: ApiCfg,
    pub claude_cli: CliCfg,
    pub codex_cli: CliCfg,
    /// The cheaper model watches run on; empty uses the provider's own model.
    pub watch_model: String,
    /// Data the assistant may read and things it may do, by permission key. Absent is denied.
    pub perms: BTreeMap<String, bool>,
    /// How far from your characters the situation summary looks.
    pub situation_jumps: u8,
    pub voice: VoiceSettings,
    pub caps: Caps,
    /// The user's own instructions, added to the assistant's.
    pub instructions: String,
    /// The language answers come in; "auto" follows the user's.
    pub language: String,
    pub glossary: super::glossary::Edits,
    /// Outside sources for the assistant to read and watch.
    pub feeds: Vec<super::feeds::FeedDef>,
    /// Action kinds (their Data access keys, like "actions.destination") the assistant may carry out
    /// without a click. Broadcasts and the watch question always ask.
    pub auto_actions: Vec<String>,
    /// The Assistant tab is in its own window, and where that window was left.
    pub popped: bool,
    pub popout_pos: Option<(f32, f32)>,
    pub popout_size: Option<(f32, f32)>,
}

/// Models offered for a service, as (what is sent, what is shown). An empty id is the service's own
/// default. OpenAI-compatible servers list their own.
pub fn model_choices(kind: ProviderKind) -> &'static [(&'static str, &'static str)] {
    match kind {
        ProviderKind::Anthropic => &[
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-sonnet-5-5", "Sonnet 5.5"),
            ("claude-fable-5-1", "Fable 5.1"),
            ("claude-haiku-4-5", "Haiku 4.5"),
        ],
        ProviderKind::ClaudeCli => &[("", "The program's default"), ("opus", "Opus 5.5"), ("sonnet", "Sonnet 5.5"), ("claude-fable-5-1", "Fable 5.1"), ("haiku", "Haiku 4.5")],
        ProviderKind::Gemini => &[("gemini-2.5-pro", "Gemini 2.5 Pro"), ("gemini-2.5-flash", "Gemini 2.5 Flash"), ("gemini-2.5-flash-lite", "Gemini 2.5 Flash-Lite")],
        ProviderKind::CodexCli => &[("", "The program's default")],
        ProviderKind::OpenaiCompat | ProviderKind::Unknown => &[],
    }
}

impl AiSettings {
    /// The model setting of the service in use.
    pub fn model_mut(&mut self) -> Option<&mut String> {
        match self.provider {
            ProviderKind::Anthropic => Some(&mut self.anthropic.model),
            ProviderKind::OpenaiCompat => Some(&mut self.openai.model),
            ProviderKind::Gemini => Some(&mut self.gemini.model),
            ProviderKind::ClaudeCli => Some(&mut self.claude_cli.model),
            ProviderKind::CodexCli => Some(&mut self.codex_cli.model),
            ProviderKind::Unknown => None,
        }
    }
}

/// Languages the assistant can be asked to answer in: (code, name as shown, name in English).
pub const LANGUAGES: [(&str, &str, &str); 7] = [
    ("auto", "Same as the question", ""),
    ("en", "English", "English"),
    ("de", "Deutsch", "German"),
    ("es", "Español", "Spanish"),
    ("fr", "Français", "French"),
    ("ru", "Русский", "Russian"),
    ("zh", "中文", "Chinese (Simplified)"),
];

impl AiSettings {
    /// The instruction telling the model which language to answer in.
    pub fn language_rule(&self) -> String {
        match LANGUAGES.iter().find(|(c, _, _)| *c == self.language).filter(|(c, _, _)| *c != "auto") {
            Some((_, _, en)) => format!("Always answer in {en}, whatever language the question or the data is in. Keep EVE names (systems, ships, pilots) as the game spells them."),
            None => "Answer in the language the user writes in. Keep EVE names (systems, ships, pilots) as the game spells them.".into(),
        }
    }
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: ProviderKind::Anthropic,
            anthropic: ApiCfg { model: "claude-opus-5-5".into(), effort: "low".into() },
            openai: OpenAiCfg::default(),
            gemini: ApiCfg { model: "gemini-2.5-flash".into(), effort: String::new() },
            claude_cli: CliCfg { path: "claude".into(), model: String::new() },
            codex_cli: CliCfg { path: "codex".into(), model: String::new() },
            watch_model: "claude-haiku-4-5".into(),
            perms: BTreeMap::new(),
            situation_jumps: 8,
            voice: VoiceSettings::default(),
            caps: Caps::default(),
            instructions: String::new(),
            language: "auto".into(),
            glossary: Default::default(),
            feeds: Vec::new(),
            auto_actions: Vec::new(),
            popped: false,
            popout_pos: None,
            popout_size: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Anthropic,
    OpenaiCompat,
    Gemini,
    ClaudeCli,
    CodexCli,
    #[serde(other)]
    Unknown,
}

impl ProviderKind {
    pub const CHOICES: [ProviderKind; 5] =
        [ProviderKind::Anthropic, ProviderKind::OpenaiCompat, ProviderKind::Gemini, ProviderKind::ClaudeCli, ProviderKind::CodexCli];

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Claude (Anthropic API)",
            ProviderKind::OpenaiCompat => "OpenAI-compatible",
            ProviderKind::Gemini => "Gemini (Google API)",
            ProviderKind::ClaudeCli => "Claude subscription (claude CLI)",
            ProviderKind::CodexCli => "ChatGPT subscription (codex CLI)",
            ProviderKind::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiCfg {
    pub model: String,
    /// How hard the model thinks before answering, where the provider has such a knob ("low" is
    /// fastest). Empty leaves the provider's default.
    pub effort: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiCfg {
    pub preset: OpenAiPreset,
    /// Used when the preset is `Custom`, or to override a preset's address.
    pub base_url: String,
    pub model: String,
    /// The model takes tool calls. Off, it only gets the situation summary in its prompt.
    pub tools: bool,
}

impl Default for OpenAiCfg {
    fn default() -> Self {
        Self { preset: OpenAiPreset::Ollama, base_url: String::new(), model: String::new(), tools: true }
    }
}

impl OpenAiCfg {
    /// The address requests go to: the override when set, else the preset's own.
    pub fn base(&self) -> String {
        let b = self.base_url.trim();
        if !b.is_empty() {
            return b.trim_end_matches('/').to_owned();
        }
        self.preset.base_url().to_owned()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiPreset {
    Openai,
    Openrouter,
    Groq,
    Mistral,
    #[default]
    Ollama,
    LmStudio,
    LlamaCpp,
    Vllm,
    Custom,
    #[serde(other)]
    Unknown,
}

impl OpenAiPreset {
    pub const CHOICES: [OpenAiPreset; 9] = [
        OpenAiPreset::Openai,
        OpenAiPreset::Openrouter,
        OpenAiPreset::Groq,
        OpenAiPreset::Mistral,
        OpenAiPreset::Ollama,
        OpenAiPreset::LmStudio,
        OpenAiPreset::LlamaCpp,
        OpenAiPreset::Vllm,
        OpenAiPreset::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OpenAiPreset::Openai => "OpenAI",
            OpenAiPreset::Openrouter => "OpenRouter",
            OpenAiPreset::Groq => "Groq",
            OpenAiPreset::Mistral => "Mistral",
            OpenAiPreset::Ollama => "Ollama (local)",
            OpenAiPreset::LmStudio => "LM Studio (local)",
            OpenAiPreset::LlamaCpp => "llama.cpp server (local)",
            OpenAiPreset::Vllm => "vLLM (local)",
            OpenAiPreset::Custom | OpenAiPreset::Unknown => "Custom address",
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            OpenAiPreset::Openai => "https://api.openai.com/v1",
            OpenAiPreset::Openrouter => "https://openrouter.ai/api/v1",
            OpenAiPreset::Groq => "https://api.groq.com/openai/v1",
            OpenAiPreset::Mistral => "https://api.mistral.ai/v1",
            OpenAiPreset::Ollama => "http://localhost:11434/v1",
            OpenAiPreset::LmStudio => "http://localhost:1234/v1",
            OpenAiPreset::LlamaCpp => "http://localhost:8080/v1",
            OpenAiPreset::Vllm => "http://localhost:8000/v1",
            OpenAiPreset::Custom | OpenAiPreset::Unknown => "",
        }
    }

    /// Whether requests carry an API key. The local servers take none.
    pub fn needs_key(self) -> bool {
        matches!(self, OpenAiPreset::Openai | OpenAiPreset::Openrouter | OpenAiPreset::Groq | OpenAiPreset::Mistral | OpenAiPreset::Custom)
    }

    /// The keyring account its key is kept under.
    pub fn key_account(self) -> String {
        format!("openai:{}", serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CliCfg {
    /// The program, by name on PATH or a full path.
    pub path: String,
    /// Passed as `--model` when set.
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceSettings {
    pub stt: SttKind,
    pub tts: TtsKind,
    /// The model asked of a local speech server.
    pub whisper_model: String,
    /// A local speech server speaking OpenAI's transcription API, ending in /v1.
    pub stt_url: String,
    /// The Whisper model file run inside the app.
    pub whisper_file: String,
    pub piper_voice: String,
    /// The Piper voice per answer language, by voice id; a language left out takes its default.
    pub piper_voices: std::collections::BTreeMap<String, String>,
    /// The OpenAI voice.
    pub cloud_voice: String,
    /// The ElevenLabs voice id; empty takes a stock one.
    pub elevenlabs_voice: String,
    /// Speak replies aloud. Answers to a push-to-talk question are spoken regardless.
    pub speak_replies: bool,
    pub volume: f32,
    /// Empty uses the system default input.
    pub input_device: String,
    pub ptt: Option<KeyBind>,
    /// Exclusive takes the key from the game; otherwise the game sees it too.
    pub ptt_exclusive: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            stt: SttKind::Off,
            tts: TtsKind::Off,
            whisper_model: "Systran/faster-whisper-small".into(),
            stt_url: "http://localhost:8000/v1".into(),
            whisper_file: crate::ai::voice::whisper::DEFAULT_MODEL.into(),
            piper_voice: "en_US-joe-medium".into(),
            piper_voices: Default::default(),
            cloud_voice: "alloy".into(),
            elevenlabs_voice: String::new(),
            speak_replies: false,
            volume: 1.0,
            input_device: String::new(),
            ptt: None,
            ptt_exclusive: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SttKind {
    #[default]
    Off,
    Local,
    Openai,
    Groq,
    /// Whisper running inside the app.
    Whisper,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsKind {
    #[default]
    Off,
    Piper,
    Openai,
    Elevenlabs,
    #[serde(other)]
    Unknown,
}

/// A key held to talk, as the platform's listener reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBind {
    pub code: u32,
    pub label: String,
    /// The platform the code belongs to; a bind from another OS is ignored.
    pub platform: String,
}

impl Default for KeyBind {
    fn default() -> Self {
        Self { code: 0, label: String::new(), platform: std::env::consts::OS.to_owned() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Caps {
    pub max_calls_per_hour: u32,
    pub max_tokens_per_day: u64,
    pub watch_min_interval_s: u32,
}

impl Default for Caps {
    fn default() -> Self {
        Self { max_calls_per_hour: 120, max_tokens_per_day: 2_000_000, watch_min_interval_s: 60 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_values_and_missing_fields_still_parse() {
        let s: AiSettings = serde_json::from_str(r#"{"enabled":true,"provider":"some_future_kind","openai":{"preset":"nope"},"extra":1}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.provider, ProviderKind::Unknown);
        assert_eq!(s.openai.preset, OpenAiPreset::Unknown);
        assert_eq!(s.voice, VoiceSettings::default());
        let back: AiSettings = serde_json::from_str(&serde_json::to_string(&AiSettings::default()).unwrap()).unwrap();
        assert_eq!(back, AiSettings::default());
    }

    #[test]
    fn a_preset_address_gives_way_to_an_override() {
        let mut c = OpenAiCfg::default();
        assert_eq!(c.base(), "http://localhost:11434/v1");
        c.base_url = "http://box:9000/v1/".into();
        assert_eq!(c.base(), "http://box:9000/v1");
        assert_eq!(OpenAiPreset::LmStudio.key_account(), "openai:lm_studio");
    }
}
