//! Whisper on this computer: every model whisper.cpp publishes, downloaded on demand and checked
//! against its pinned sum. The model is loaded when a question is asked and let go after a few
//! idle minutes, so it only holds memory while the user is talking to the assistant.

#[cfg(feature = "local-stt")]
use std::sync::Mutex;
#[cfg(feature = "local-stt")]
use std::time::Instant;
use std::time::Duration;

/// Pinned to one commit of the model repository, so a file cannot change under its checksum.
const BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1";

/// How long a loaded model is kept after its last question.
#[cfg_attr(not(feature = "local-stt"), allow(dead_code))]
pub const IDLE_UNLOAD: Duration = Duration::from_secs(300);

pub struct Model {
    pub file: &'static str,
    pub bytes: u64,
    sha: &'static str,
}

impl Model {
    /// English-only models are smaller for their quality but hear every language as English.
    pub fn english_only(&self) -> bool {
        self.file.contains(".en")
    }

    pub fn label(&self) -> String {
        let name = self.file.trim_start_matches("ggml-").trim_end_matches(".bin");
        let (base, quant) = match name.rsplit_once('-') {
            Some((b, q)) if q.starts_with('q') => (b, Some(q)),
            _ => (name, None),
        };
        let base = base.trim_end_matches(".en");
        let mut s = base.to_owned();
        if self.english_only() {
            s.push_str(", English only");
        }
        if let Some(q) = quant {
            s.push_str(&format!(", {q}"));
        }
        s
    }

    /// Memory while loaded: the weights plus whisper.cpp's working buffers, roughly.
    pub fn ram_mb(&self) -> u64 {
        self.bytes / 1_000_000 + if self.file.contains("large") { 300 } else { 150 }
    }
}

pub const MODELS: &[Model] = &[
    Model { file: "ggml-tiny.bin", bytes: 77_691_713, sha: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21" },
    Model { file: "ggml-tiny-q8_0.bin", bytes: 43_537_433, sha: "c2085835d3f50733e2ff6e4b41ae8a2b8d8110461e18821b09a15c40c42d1cca" },
    Model { file: "ggml-tiny-q5_1.bin", bytes: 32_152_673, sha: "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7" },
    Model { file: "ggml-tiny.en.bin", bytes: 77_704_715, sha: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f" },
    Model { file: "ggml-tiny.en-q8_0.bin", bytes: 43_550_795, sha: "5bc2b3860aa151a4c6e7bb095e1fcce7cf12c7b020ca08dcec0c6d018bb7dd94" },
    Model { file: "ggml-tiny.en-q5_1.bin", bytes: 32_166_155, sha: "c77c5766f1cef09b6b7d47f21b546cbddd4157886b3b5d6d4f709e91e66c7c2b" },
    Model { file: "ggml-base.bin", bytes: 147_951_465, sha: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe" },
    Model { file: "ggml-base-q8_0.bin", bytes: 81_768_585, sha: "c577b9a86e7e048a0b7eada054f4dd79a56bbfa911fbdacf900ac5b567cbb7d9" },
    Model { file: "ggml-base-q5_1.bin", bytes: 59_707_625, sha: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898" },
    Model { file: "ggml-base.en.bin", bytes: 147_964_211, sha: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002" },
    Model { file: "ggml-base.en-q8_0.bin", bytes: 81_781_811, sha: "a4d4a0768075e13cfd7e19df3ae2dbc4a68d37d36a7dad45e8410c9a34f8c87e" },
    Model { file: "ggml-base.en-q5_1.bin", bytes: 59_721_011, sha: "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f" },
    Model { file: "ggml-small.bin", bytes: 487_601_967, sha: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b" },
    Model { file: "ggml-small-q8_0.bin", bytes: 264_464_607, sha: "49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f" },
    Model { file: "ggml-small-q5_1.bin", bytes: 190_085_487, sha: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb" },
    Model { file: "ggml-small.en.bin", bytes: 487_614_201, sha: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d" },
    Model { file: "ggml-small.en-q8_0.bin", bytes: 264_477_561, sha: "67a179f608ea6114bd3fdb9060e762b588a3fb3bd00c4387971be4d177958067" },
    Model { file: "ggml-small.en-q5_1.bin", bytes: 190_098_681, sha: "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30" },
    Model { file: "ggml-medium.bin", bytes: 1_533_763_059, sha: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208" },
    Model { file: "ggml-medium-q8_0.bin", bytes: 823_369_779, sha: "42a1ffcbe4167d224232443396968db4d02d4e8e87e213d3ee2e03095dea6502" },
    Model { file: "ggml-medium-q5_0.bin", bytes: 539_212_467, sha: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f" },
    Model { file: "ggml-medium.en.bin", bytes: 1_533_774_781, sha: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356" },
    Model { file: "ggml-medium.en-q8_0.bin", bytes: 823_382_461, sha: "43fa2cd084de5a04399a896a9a7a786064e221365c01700cea4666005218f11c" },
    Model { file: "ggml-medium.en-q5_0.bin", bytes: 539_225_533, sha: "76733e26ad8fe1c7a5bf7531a9d41917b2adc0f20f2e4f5531688a8c6cd88eb0" },
    Model { file: "ggml-large-v1.bin", bytes: 3_094_623_691, sha: "7d99f41a10525d0206bddadd86760181fa920438b6b33237e3118ff6c83bb53d" },
    Model { file: "ggml-large-v2.bin", bytes: 3_094_623_691, sha: "9a423fe4d40c82774b6af34115b8b935f34152246eb19e80e376071d3f999487" },
    Model { file: "ggml-large-v2-q8_0.bin", bytes: 1_656_129_691, sha: "fef54e6d898246a65c8285bfa83bd1807e27fadf54d5d4e81754c47634737e8c" },
    Model { file: "ggml-large-v2-q5_0.bin", bytes: 1_080_732_091, sha: "3a214837221e4530dbc1fe8d734f302af393eb30bd0ed046042ebf4baf70f6f2" },
    Model { file: "ggml-large-v3.bin", bytes: 3_095_033_483, sha: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2" },
    Model { file: "ggml-large-v3-q5_0.bin", bytes: 1_081_140_203, sha: "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1" },
    Model { file: "ggml-large-v3-turbo.bin", bytes: 1_624_555_275, sha: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69" },
    Model { file: "ggml-large-v3-turbo-q8_0.bin", bytes: 874_188_075, sha: "317eb69c11673c9de1e1f0d459b253999804ec71ac4c23c17ecf5fbe24e259a1" },
    Model { file: "ggml-large-v3-turbo-q5_0.bin", bytes: 574_041_195, sha: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2" },
];

pub const DEFAULT_MODEL: &str = "ggml-small-q5_1.bin";

pub fn model(file: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.file == file)
}

pub fn path(file: &str) -> anyhow::Result<std::path::PathBuf> {
    Ok(crate::store::data_dir()?.join("ai").join("whisper").join(file))
}

pub fn installed(file: &str) -> bool {
    path(file).is_ok_and(|p| p.is_file())
}

pub fn remove(file: &str) {
    if let Ok(p) = path(file) {
        let _ = std::fs::remove_file(p);
    }
}

/// Whether this build carries Whisper.
pub const AVAILABLE: bool = cfg!(feature = "local-stt");

pub fn install(file: String, progress: super::models::SharedProgress, ctx: Option<egui::Context>) {
    {
        let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
        if p.busy {
            return;
        }
        *p = super::models::Progress { busy: true, ..Default::default() };
    }
    let _ = std::thread::Builder::new().name("whisper-download".into()).spawn(move || {
        let res = (|| -> anyhow::Result<()> {
            let m = model(&file).ok_or_else(|| anyhow::anyhow!("unknown model {file}"))?;
            let bytes = super::models::fetch_checked(&format!("{BASE}/{}", m.file), m.sha, &m.label(), &progress)?;
            super::models::write_atomic(&path(m.file)?, &bytes)
        })();
        let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
        p.busy = false;
        p.error = res.err().map(|e| e.to_string());
        drop(p);
        if let Some(c) = ctx {
            c.request_repaint();
        }
    });
}

/// The model last loaded, readable while a transcription holds [`LOADED`].
#[cfg(feature = "local-stt")]
static LAST: Mutex<Option<String>> = Mutex::new(None);

#[cfg(feature = "local-stt")]
static LOADED: Mutex<Option<(String, whisper_rs::WhisperContext, Instant)>> = Mutex::new(None);

/// Lets go of a model nobody has asked anything of for a while.
pub fn unload_idle() {
    #[cfg(feature = "local-stt")]
    {
        // Never waits: the lock is held for the whole of a load or a transcription, and this runs
        // on the UI thread.
        let Ok(mut g) = LOADED.try_lock() else { return };
        if g.as_ref().is_some_and(|(_, _, used)| used.elapsed() >= IDLE_UNLOAD) {
            *g = None;
        }
    }
}

/// Whether a model is in memory now.
pub fn loaded() -> Option<String> {
    #[cfg(feature = "local-stt")]
    {
        // Busy counts as loaded; asking must not wait for a transcription to finish.
        match LOADED.try_lock() {
            Ok(g) => g.as_ref().map(|(f, _, _)| f.clone()),
            Err(std::sync::TryLockError::WouldBlock) => LAST.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner().as_ref().map(|(f, _, _)| f.clone()),
        }
    }
    #[cfg(not(feature = "local-stt"))]
    {
        None
    }
}

/// Threads for one question: enough to be quick, few enough to leave the game its share.
fn threads() -> i32 {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    (n / 2).clamp(1, 4) as i32
}

/// What was said in `pcm` (16 kHz mono).
pub fn transcribe(file: &str, pcm: &[i16], lang: &str, hint: &str) -> anyhow::Result<String> {
    #[cfg(not(feature = "local-stt"))]
    {
        let _ = (file, pcm, lang, hint, threads());
        anyhow::bail!("this build has no Whisper; pick another recogniser")
    }
    #[cfg(feature = "local-stt")]
    {
        use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
        static HOOKS: std::sync::Once = std::sync::Once::new();
        // whisper.cpp prints to stderr by default; this routes it to the log crate instead.
        HOOKS.call_once(whisper_rs::install_logging_hooks);
        let m = model(file).ok_or_else(|| anyhow::anyhow!("unknown Whisper model {file}"))?;
        let p = path(m.file)?;
        if !p.is_file() {
            anyhow::bail!("the Whisper model {} is not downloaded yet", m.label());
        }
        let mut g = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_ref().is_none_or(|(f, _, _)| f != m.file) {
            *g = None;
            let ctx = WhisperContext::new_with_params(&p, WhisperContextParameters::default()).map_err(|e| anyhow::anyhow!("could not load {}: {e}", m.label()))?;
            *g = Some((m.file.to_owned(), ctx, Instant::now()));
            *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(m.file.to_owned());
        }
        let (_, ctx, used) = g.as_mut().expect("loaded");
        *used = Instant::now();
        let mut state = ctx.create_state().map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(threads());
        let language = if m.english_only() { "en" } else if lang.is_empty() || lang == "auto" { "auto" } else { lang };
        params.set_language(Some(language));
        params.set_no_context(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        if !hint.is_empty() {
            params.set_initial_prompt(hint);
        }
        let audio: Vec<f32> = pcm.iter().map(|s| *s as f32 / 32768.0).collect();
        state.full(params, &audio).map_err(|e| anyhow::anyhow!("Whisper failed: {e}"))?;
        let mut text = String::new();
        for seg in state.as_iter() {
            if let Ok(t) = seg.to_str_lossy() {
                text.push_str(&t);
            }
        }
        Ok(text.trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_is_listed_once_with_a_sum_and_a_readable_name() {
        assert_eq!(MODELS.len(), 33);
        let mut files: Vec<&str> = MODELS.iter().map(|m| m.file).collect();
        files.sort();
        files.dedup();
        assert_eq!(files.len(), 33);
        assert!(MODELS.iter().all(|m| m.sha.len() == 64 && m.bytes > 30_000_000));
        assert!(model(DEFAULT_MODEL).is_some_and(|m| !m.english_only()), "the default hears every answer language");
        assert_eq!(model("ggml-small.en-q5_1.bin").unwrap().label(), "small, English only, q5_1");
        assert_eq!(model("ggml-large-v3-turbo-q5_0.bin").unwrap().label(), "large-v3-turbo, q5_0");
        assert_eq!(model("ggml-tiny.bin").unwrap().label(), "tiny");
    }
}

/// Transcribes a real recording with a real model:
/// `EVE_SPAI_DATA_DIR=<dir with ai/whisper/<model>> WHISPER_WAV=<16 kHz wav> cargo test --release --bin eve-spai whisper_live -- --ignored --nocapture`.
#[cfg(all(test, feature = "local-stt"))]
#[test]
#[ignore = "needs a downloaded Whisper model and a recording"]
fn whisper_live() {
    let file = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "ggml-tiny.en-q5_1.bin".into());
    let wav = std::fs::read(std::env::var("WHISPER_WAV").expect("WHISPER_WAV")).unwrap();
    let (rate, pcm) = super::playback::read_wav(&wav).unwrap();
    let pcm = if rate == super::capture::RATE { pcm } else { super::capture::resample(&pcm.iter().map(|s| *s as f32 / 32768.0).collect::<Vec<_>>(), rate) };
    let t0 = Instant::now();
    let text = transcribe(&file, &pcm, "auto", "EVE Online intel: 1DQ1-A, Muninn").unwrap();
    println!("{file}: {:?} in {:?}", text, t0.elapsed());
    assert!(!text.is_empty());
}
