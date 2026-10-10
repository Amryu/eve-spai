//! Piper, the local voice: its program and the voices, fetched on first use from pinned releases
//! and checked against known SHA-256 sums before anything is kept.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PIPER_RELEASE: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2";
/// Pinned to one commit of the collection, so a voice cannot change under its checksum.
const VOICES: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/c10ece1aade47bb51c153c893d14e5bf8e5b7117";

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
    /// The license of the recordings the voice was trained on. Only voices free to use are listed:
    /// Lessac (research only) and voices of unknown origin were taken out.
    pub license: &'static str,
    path: &'static str,
    onnx_sha: &'static str,
    json_sha: &'static str,
}

pub const CATALOG: &[VoiceInfo] = &[
    VoiceInfo { id: "en_US-joe-medium", lang: "en", label: "English (US), Joe", license: "CC0", path: "en/en_US/joe/medium", onnx_sha: "58afce0321b8d9c46d7cdf9c16500cc55a793b4220212dba6b70fb788b3baf06", json_sha: "3d6d5410b3795cb1950595247ef8f06190719e6fdbfa3a2356d8ec368e1aad33" },
    VoiceInfo { id: "en_GB-cori-medium", lang: "en", label: "English (UK), Cori", license: "Public domain", path: "en/en_GB/cori/medium", onnx_sha: "1899f98e5fb8310154f3c2973f4b8a929ba7245e722b3d3a85680b833d95f10d", json_sha: "e262c16d7f192f69d4edd6b4ef8a5915379e67495fcc402f1ab15eeb33da3d36" },
    VoiceInfo { id: "en_US-libritts_r-medium", lang: "en", label: "English (US), LibriTTS", license: "CC BY 4.0", path: "en/en_US/libritts_r/medium", onnx_sha: "10bb85e071d616fcf4071f369f1799d0491492ab3c5d552ec19fb548fac13195", json_sha: "b471dc60d2d8335e819c393d196d6fbf792817f40051257b269878505bc9afb3" },
    VoiceInfo { id: "de_DE-thorsten-medium", lang: "de", label: "Deutsch, Thorsten", license: "CC0", path: "de/de_DE/thorsten/medium", onnx_sha: "7e64762d8e5118bb578f2eea6207e1a35a8e0c30595010b666f983fc87bb7819", json_sha: "974adee790533adb273a1ac88f49027d2a1b8f0f2cf4905954a4791e79264e85" },
    VoiceInfo { id: "es_ES-davefx-medium", lang: "es", label: "Español, Davefx", license: "CC0", path: "es/es_ES/davefx/medium", onnx_sha: "6658b03b1a6c316ee4c265a9896abc1393353c2d9e1bca7d66c2c442e222a917", json_sha: "0e0dda87c732f6f38771ff274a6380d9252f327dca77aa2963d5fbdf9ec54842" },
    VoiceInfo { id: "fr_FR-siwis-medium", lang: "fr", label: "Français, Siwis", license: "CC BY 4.0", path: "fr/fr_FR/siwis/medium", onnx_sha: "641d1ab097da2b81128c076810edb052b385decc8be3381814802a64a73baf99", json_sha: "39479916c2db192b5ac9764daddd0c744d83e023ad890c6976c0633ae4df8959" },
    VoiceInfo { id: "fr_FR-gilles-low", lang: "fr", label: "Français, Gilles", license: "CC0", path: "fr/fr_FR/gilles/low", onnx_sha: "5cd711846720e261c2a176f6924c198a7424d0a75dd4b0a5357a5fb9cb739285", json_sha: "5a47cc0789e91267d17666bbec842dd92950669271a09023eb6970ee364cf88a" },
    VoiceInfo { id: "ru_RU-denis-medium", lang: "ru", label: "Русский, Denis", license: "CC0", path: "ru/ru_RU/denis/medium", onnx_sha: "15fab56e11a097858ee115545d0f697fc2a316c41a291a5362349fb870411b0a", json_sha: "831c860dac0b5073eaa81610a0a638ec23d90a6cf8e5f871b4485c2cec3767c8" },
    VoiceInfo { id: "ru_RU-dmitri-medium", lang: "ru", label: "Русский, Dmitri", license: "CC0", path: "ru/ru_RU/dmitri/medium", onnx_sha: "f073356ebc4bd0f80c5af58df2953a5988bd5bdab1eb38635ce960b071fbefcb", json_sha: "667ef3117bc642c2892dff7690d8bdc8ca4228aeaa783b2dc1416df632855e0d" },
    VoiceInfo { id: "zh_CN-chaowen-medium", lang: "zh", label: "中文, Chaowen", license: "CC0", path: "zh/zh_CN/chaowen/medium", onnx_sha: "820d64ac16048fbcf38dd0823d37fab5f5e0c2bd71b01ca5a50f553fac19e746", json_sha: "a6bb2caafa0645642f13cbf7e2f6fbbb16fded66e51109fc26d622f6472fa16f" },
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
pub(crate) fn fetch_checked(url: &str, sha: &str, what: &str, progress: &SharedProgress) -> anyhow::Result<Vec<u8>> {
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
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
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
        for lang in ["en", "de", "es", "fr", "ru", "zh"] {
            assert!(default_voice(lang).is_some(), "{lang}");
        }
        for v in CATALOG {
            assert_eq!(v.onnx_sha.len(), 64);
            assert_eq!(v.json_sha.len(), 64);
            let quality = v.path.rsplit('/').next().unwrap();
            assert!(v.id.ends_with(&format!("-{quality}")) && v.path.contains(v.id.split('-').nth(1).unwrap()), "{}", v.id);
        }
        assert!(piper_asset().is_some(), "this build host has a Piper");
        assert_eq!(sha_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
}
