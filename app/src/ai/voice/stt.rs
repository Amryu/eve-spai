//! Speech to text: a recording becomes the question. OpenAI, Groq, or a speech server the user runs
//! on their own computer that speaks OpenAI's transcription API (speaches, faster-whisper-server,
//! LocalAI). The answer language and EVE's vocabulary go along as hints, so "Muninn" and "1DQ1-A"
//! come back spelled right.

use crate::ai::config::SttKind;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SttCfg {
    pub kind: SttKind,
    pub key: Option<String>,
    /// The local server's address, ending in /v1.
    pub local_url: String,
    /// The model the local server is asked for.
    pub local_model: String,
    /// The Whisper model file, when Whisper runs in the app.
    pub whisper_file: String,
}

impl SttCfg {
    fn endpoint(&self) -> anyhow::Result<(String, String)> {
        Ok(match self.kind {
            SttKind::Openai => ("https://api.openai.com/v1".into(), "gpt-4o-mini-transcribe".into()),
            SttKind::Groq => ("https://api.groq.com/openai/v1".into(), "whisper-large-v3-turbo".into()),
            SttKind::Local => {
                let url = self.local_url.trim().trim_end_matches('/');
                if url.is_empty() {
                    anyhow::bail!("set the speech server's address in Settings, Assistant");
                }
                (url.to_owned(), if self.local_model.trim().is_empty() { "whisper-1".into() } else { self.local_model.trim().to_owned() })
            }
            SttKind::Whisper => unreachable!("handled before"),
            SttKind::Off | SttKind::Unknown => anyhow::bail!("speech recognition is off"),
        })
    }
}

/// What was said in `pcm` (16 kHz mono). `lang` is the answer language when fixed, else empty;
/// `hint` is words likely to come up.
pub fn transcribe(cfg: &SttCfg, pcm: &[i16], lang: &str, hint: &str) -> anyhow::Result<String> {
    if cfg.kind == SttKind::Whisper {
        return super::whisper::transcribe(&cfg.whisper_file, pcm, lang, hint);
    }
    let (base, model) = cfg.endpoint()?;
    let wav = super::playback::wav(super::capture::RATE, pcm);
    let part = reqwest::blocking::multipart::Part::bytes(wav).file_name("question.wav").mime_str("audio/wav")?;
    let mut form = reqwest::blocking::multipart::Form::new().part("file", part).text("model", model).text("response_format", "json");
    if !lang.is_empty() && lang != "auto" {
        form = form.text("language", lang.to_owned());
    }
    if !hint.is_empty() {
        form = form.text("prompt", hint.to_owned());
    }
    let client = crate::http::client(60)?;
    let mut req = client.post(format!("{base}/audio/transcriptions")).multipart(form);
    if let Some(k) = cfg.key.as_deref().filter(|k| !k.is_empty()) {
        req = req.bearer_auth(k);
    }
    let resp = req.send()?;
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if !status.is_success() {
        let secrets: Vec<String> = cfg.key.iter().cloned().collect();
        anyhow::bail!("speech recognition answered {status}: {}", crate::ai::secrets::redact(&body.chars().take(200).collect::<String>(), &secrets));
    }
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| anyhow::anyhow!("speech recognition sent no text"))?;
    Ok(tidy(v["text"].as_str().unwrap_or_default().trim()))
}

/// The words a question is likely to hold that a recogniser would not know: EVE terms and the
/// systems near the user. Kept short; recognisers only read the first couple of hundred tokens.
pub fn hint(glossary_terms: &[String], systems: &[String]) -> String {
    // Comms channels first: "op 11" is otherwise heard as "upper level" or "set up 11".
    const OPS: &str = "Op 1, Op 2, Op 3, Op 4, Op 5, Op 6, Op 7, Op 8, Op 9, Op 10, Op 11, Op 12, op11, Capital Comms";
    let mut words: Vec<&str> = vec![OPS];
    words.extend(systems.iter().map(String::as_str).take(30));
    words.extend(glossary_terms.iter().map(String::as_str).take(40));
    let mut s = String::from("EVE Online intel: ");
    for w in words {
        if s.len() + w.len() > 600 {
            break;
        }
        s.push_str(w);
        s.push_str(", ");
    }
    s.trim_end_matches([',', ' ']).to_owned()
}

/// "op eleven", "opp 11", "op11" as "Op 11": how the comms channels are written.
pub fn tidy(text: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(?:op|opp|ops)\.?\s*-?\s*(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|\d{1,2})\b").expect("pattern")
    });
    re.replace_all(text, |c: &regex::Captures| {
        let n = match c[1].to_lowercase().as_str() {
            "one" => "1",
            "two" => "2",
            "three" => "3",
            "four" => "4",
            "five" => "5",
            "six" => "6",
            "seven" => "7",
            "eight" => "8",
            "nine" => "9",
            "ten" => "10",
            "eleven" => "11",
            "twelve" => "12",
            d => return format!("Op {d}"),
        };
        format!("Op {n}")
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_and_hints() {
        let local = SttCfg { kind: SttKind::Local, local_url: "http://localhost:8000/v1/".into(), ..Default::default() };
        assert_eq!(local.endpoint().unwrap(), ("http://localhost:8000/v1".into(), "whisper-1".into()));
        assert!(SttCfg { kind: SttKind::Local, ..Default::default() }.endpoint().is_err());
        assert_eq!(SttCfg { kind: SttKind::Groq, ..Default::default() }.endpoint().unwrap().1, "whisper-large-v3-turbo");
        let h = hint(&["Cyno".into(), "Ansiblex".into()], &["1DQ1-A".into()]);
        assert!(h.starts_with("EVE Online intel: Op 1, Op 2") && h.ends_with("1DQ1-A, Cyno, Ansiblex"), "{h}");
        assert_eq!(tidy("move me to op eleven and opp 4, then op11"), "move me to Op 11 and Op 4, then Op 11");
        assert_eq!(tidy("the opening operation"), "the opening operation", "only whole words");
        let long: Vec<String> = (0..500).map(|i| format!("Term{i}")).collect();
        assert!(hint(&long, &[]).len() <= 620);
    }
}
