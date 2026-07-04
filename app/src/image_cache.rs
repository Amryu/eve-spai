use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

const EVE_IMG_PREFIX: &str = "https://images.evetech.net/";
const TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Clone)]
struct Img {
    bytes: Arc<[u8]>,
    mime: Option<String>,
}

type Entry = Poll<Result<Img, String>>;

struct EveImageCache {
    mem: Arc<Mutex<HashMap<String, Entry>>>,
    dir: Option<PathBuf>,
    client: reqwest::blocking::Client,
}

impl EveImageCache {
    fn new() -> Self {
        let dir = crate::store::data_dir().ok().map(|d| d.join("image_cache"));
        if let Some(d) = &dir {
            let _ = std::fs::create_dir_all(d);
            prune_old(d);
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "eve-spai/",
                env!("CARGO_PKG_VERSION"),
                " (EVE intel tool; image cache)"
            ))
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self { mem: Arc::new(Mutex::new(HashMap::new())), dir, client }
    }

    fn path_for(&self, uri: &str) -> Option<PathBuf> {
        let mut h = DefaultHasher::new();
        uri.hash(&mut h);
        self.dir.as_ref().map(|d| d.join(format!("{:016x}", h.finish())))
    }
}

impl BytesLoader for EveImageCache {
    fn id(&self) -> &str {
        egui::generate_loader_id!(EveImageCache)
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !uri.starts_with(EVE_IMG_PREFIX) {
            return Err(LoadError::NotSupported);
        }

        let mut mem = self.mem.lock().unwrap();
        if let Some(entry) = mem.get(uri).cloned() {
            return match entry {
                Poll::Ready(Ok(img)) => Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(img.bytes),
                    mime: img.mime,
                }),
                Poll::Ready(Err(err)) => Err(LoadError::Loading(err)),
                Poll::Pending => Ok(BytesPoll::Pending { size: None }),
            };
        }

        mem.insert(uri.to_owned(), Poll::Pending);
        drop(mem);

        let uri = uri.to_owned();
        let path = self.path_for(&uri);
        let client = self.client.clone();
        let mem = Arc::clone(&self.mem);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = load_blocking(&client, &uri, path.as_deref());
            if let Some(slot) = mem.lock().unwrap().get_mut(&uri) {
                *slot = Poll::Ready(result);
            }
            ctx.request_repaint();
        });

        Ok(BytesPoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        let _ = self.mem.lock().unwrap().remove(uri);
    }

    fn forget_all(&self) {
        self.mem.lock().unwrap().clear();
    }

    fn byte_size(&self) -> usize {
        self.mem
            .lock()
            .unwrap()
            .values()
            .map(|e| match e {
                Poll::Ready(Ok(img)) => img.bytes.len() + img.mime.as_ref().map_or(0, |m| m.len()),
                Poll::Ready(Err(err)) => err.len(),
                Poll::Pending => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.mem.lock().unwrap().values().any(|e| matches!(e, Poll::Pending))
    }
}

fn load_blocking(
    client: &reqwest::blocking::Client,
    uri: &str,
    path: Option<&Path>,
) -> Result<Img, String> {
    if let Some(p) = path {
        if let Some(bytes) = read_if_fresh(p) {
            return Ok(Img { bytes: bytes.into(), mime: None });
        }
    }

    let fetched = (|| -> Result<(Vec<u8>, Option<String>), String> {
        let resp = client.get(uri).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
        Ok((bytes, mime))
    })();

    match fetched {
        Ok((bytes, mime)) => {
            if let Some(p) = path {
                write_atomic(p, &bytes);
            }
            Ok(Img { bytes: bytes.into(), mime })
        }
        Err(err) => {
            if let Some(p) = path {
                if let Ok(bytes) = std::fs::read(p) {
                    return Ok(Img { bytes: bytes.into(), mime: None });
                }
            }
            Err(format!("{uri}: {err}"))
        }
    }
}

fn read_if_fresh(p: &Path) -> Option<Vec<u8>> {
    let age = std::fs::metadata(p).ok()?.modified().ok()?.elapsed().ok()?;
    (age < TTL).then(|| std::fs::read(p).ok()).flatten()
}

/// Write bytes to `p` atomically (temp file + rename) so a crash never leaves a partial image.
fn write_atomic(p: &Path, bytes: &[u8]) {
    let tmp = p.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, p);
    }
}

fn prune_old(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > TTL);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// default network loader. Loaders are tried last-registered-first, so registering ours
/// *after* `install_image_loaders` gives it precedence for the `images.evetech.net` host.
pub fn install_image_loaders_cached(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    ctx.add_bytes_loader(Arc::new(EveImageCache::new()));
}
