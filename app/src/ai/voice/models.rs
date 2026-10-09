//! Piper, the local voice: its program and the voices, fetched on first use from pinned releases
//! and checked against known SHA-256 sums before anything is kept.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PIPER_RELEASE: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2";
const VOICES: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0";

/// (archive, sha256) of the Piper build for this platform.
fn piper_asset() -> Option<(&'static str, &'static str)> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some(("piper_linux_x86_64.tar.gz", "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992")),
        ("linux", "aarch64") => Some(("piper_linux_aarch64.tar.gz", "fea0fd2d87c54dbc7078d0f878289f404bd4d6eea6e7444a77835d1537ab88eb")),
        ("macos", "aarch64") => Some(("piper_macos_aarch64.tar.gz", "6b1eb03b3735946cb35216e063e7eebcc33a6bbf5dd96ec0217959bf1cdcb0cc")),
        ("macos", "x86_64") => Some(("piper_macos_x64.tar.gz", "ced85c0a3df13945b1e623b878a48fdc2854d5c485b4b67f62857cf551deaf8b")),
        ("windows", "x86_64") => Some(("piper_windows_amd64.zip", "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea")),
        _ => None,
    }
}

pub struct VoiceInfo {
    pub id: &'static str,
    pub lang: &'static str,
    pub label: &'static str,
    path: &'static str,
    onnx_sha: &'static str,
    json_sha: &'static str,
}

pub const CATALOG: &[VoiceInfo] = &[
    VoiceInfo { id: "en_US-lessac-medium", lang: "en", label: "English (US), Lessac", path: "en/en_US/lessac/medium", onnx_sha: "5efe09e69902187827af646e1a6e9d269dee769f9877d17b16b1b46eeaaf019f", json_sha: "efe19c417bed055f2d69908248c6ba650fa135bc868b0e6abb3da181dab690a0" },
    VoiceInfo { id: "en_GB-alan-medium", lang: "en", label: "English (UK), Alan", path: "en/en_GB/alan/medium", onnx_sha: "0a309668932205e762801f1efc2736cd4b0120329622adf62be09e56339d3330", json_sha: "c0f0d124e5895c00e7c03b35dcc8287f319a6998a365b182deb5c8e752ee8c1e" },
    VoiceInfo { id: "de_DE-thorsten-medium", lang: "de", label: "Deutsch, Thorsten", path: "de/de_DE/thorsten/medium", onnx_sha: "7e64762d8e5118bb578f2eea6207e1a35a8e0c30595010b666f983fc87bb7819", json_sha: "974adee790533adb273a1ac88f49027d2a1b8f0f2cf4905954a4791e79264e85" },
    VoiceInfo { id: "es_ES-davefx-medium", lang: "es", label: "Español, Davefx", path: "es/es_ES/davefx/medium", onnx_sha: "6658b03b1a6c316ee4c265a9896abc1393353c2d9e1bca7d66c2c442e222a917", json_sha: "0e0dda87c732f6f38771ff274a6380d9252f327dca77aa2963d5fbdf9ec54842" },
    VoiceInfo { id: "ru_RU-irina-medium", lang: "ru", label: "Русский, Irina", path: "ru/ru_RU/irina/medium", onnx_sha: "8ff38212d23da300bbe3705c645e6e5b9475f0bfde01558eb17813e22acaaaaa", json_sha: "c2ec28bb38e2b59e93b959b3e40348c1afebbd272f30fed5d41205d08e98a9d7" },
    VoiceInfo { id: "zh_CN-huayan-medium", lang: "zh", label: "中文, Huayan", path: "zh/zh_CN/huayan/medium", onnx_sha: "9929917bf8cabb26fd528ea44d3a6699c11e87317a14765312420be230be0f3d", json_sha: "d521dc45504a8ccc99e325822b35946dd701840bfb07e3dbb31a40929ed6a82b" },
];

pub fn voice(id: &str) -> Option<&'static VoiceInfo> {
    CATALOG.iter().find(|v| v.id == id)
}

/// The first voice listed for a language.
pub fn default_voice(lang: &str) -> Option<&'static VoiceInfo> {
    CATALOG.iter().find(|v| v.lang == lang)
}

pub fn root() -> anyhow::Result<PathBuf> {
    Ok(crate::store::data_dir()?.join("ai").join("piper"))
}

pub fn program_path() -> anyhow::Result<PathBuf> {
    let exe = if cfg!(windows) { "piper.exe" } else { "piper" };
    Ok(root()?.join("bin").join("piper").join(exe))
}

pub fn voice_path(id: &str) -> anyhow::Result<PathBuf> {
    Ok(root()?.join("voices").join(format!("{id}.onnx")))
}

pub fn has_program() -> bool {
    program_path().is_ok_and(|p| p.is_file())
}

pub fn has_voice(id: &str) -> bool {
    voice_path(id).is_ok_and(|p| p.is_file() && p.with_extension("onnx.json").is_file())
}

/// A download in progress or its outcome, for the settings to show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub what: String,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
    pub busy: bool,
}

pub type SharedProgress = Arc<Mutex<Progress>>;

fn sha_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// Fetches `url` whole, reporting progress, and returns it only if its SHA-256 is `sha`.
fn fetch_checked(url: &str, sha: &str, what: &str, progress: &SharedProgress) -> anyhow::Result<Vec<u8>> {
    let client = crate::http::client(60)?;
    let mut resp = client.get(url).timeout(std::time::Duration::from_secs(1800)).send()?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    {
        let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
        p.what = what.to_owned();
        p.done = 0;
        p.total = total;
    }
    let mut buf = Vec::with_capacity(total as usize);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        progress.lock().unwrap_or_else(|e| e.into_inner()).done = buf.len() as u64;
    }
    let got = sha_hex(&buf);
    if got != sha {
        anyhow::bail!("{what} did not match its checksum; nothing was kept");
    }
    Ok(buf)
}

/// Writes through a temporary name so a half-written file is never taken for a whole one.
fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn install_program(progress: &SharedProgress) -> anyhow::Result<()> {
    let (asset, sha) = piper_asset().ok_or_else(|| anyhow::anyhow!("Piper has no build for this computer"))?;
    let bytes = fetch_checked(&format!("{PIPER_RELEASE}/{asset}"), sha, "Piper", progress)?;
    let bin = root()?.join("bin");
    let staging = root()?.join("bin.part");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    if asset.ends_with(".zip") {
        zip::ZipArchive::new(std::io::Cursor::new(bytes))?.extract(&staging)?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(std::io::Cursor::new(bytes))).unpack(&staging)?;
    }
    let _ = std::fs::remove_dir_all(&bin);
    std::fs::rename(&staging, &bin)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(program_path()?, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

pub fn install_voice(id: &str, progress: &SharedProgress) -> anyhow::Result<()> {
    let v = voice(id).ok_or_else(|| anyhow::anyhow!("unknown voice {id}"))?;
    let json = fetch_checked(&format!("{VOICES}/{}/{}.onnx.json", v.path, v.id), v.json_sha, v.label, progress)?;
    let onnx = fetch_checked(&format!("{VOICES}/{}/{}.onnx", v.path, v.id), v.onnx_sha, v.label, progress)?;
    let path = voice_path(id)?;
    write_atomic(&path.with_extension("onnx.json"), &json)?;
    write_atomic(&path, &onnx)?;
    Ok(())
}

/// Gets the program and `voices` that are missing, on a thread, reporting into `progress`.
pub fn install_missing(voices: Vec<String>, progress: SharedProgress, ctx: Option<egui::Context>) {
    {
        let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
        if p.busy {
            return;
        }
        *p = Progress { busy: true, ..Default::default() };
    }
    let _ = std::thread::Builder::new().name("piper-download".into()).spawn(move || {
        let mut result = Ok(());
        if !has_program() {
            result = install_program(&progress);
        }
        for v in voices.iter().filter(|v| !has_voice(v)) {
            if result.is_err() {
                break;
            }
            result = install_voice(v, &progress);
        }
        let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
        p.busy = false;
        p.error = result.err().map(|e| e.to_string());
        drop(p);
        if let Some(c) = ctx {
            c.request_repaint();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_voice_and_sums_are_well_formed() {
        for lang in ["en", "de", "es", "ru", "zh"] {
            assert!(default_voice(lang).is_some(), "{lang}");
        }
        for v in CATALOG {
            assert_eq!(v.onnx_sha.len(), 64);
            assert_eq!(v.json_sha.len(), 64);
            assert!(v.path.ends_with("/medium") && v.id.ends_with("-medium"));
        }
        assert!(piper_asset().is_some(), "this build host has a Piper");
        assert_eq!(sha_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
}
