use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

const EVE_IMG_PREFIX: &str = "https://images.evetech.net/";
/// Fetch and decode threads. A feed scrolled fast asks for hundreds of portraits at once; one
/// thread each swamped the machine, and decoding on the UI thread stalled the frame instead.
const WORKERS: usize = 4;
const TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The shared fetch and decode pool.
fn pool() -> &'static std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>> {
    use std::sync::OnceLock;
    static POOL: OnceLock<std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..WORKERS {
            let rx = rx.clone();
            let _ = std::thread::Builder::new().name(format!("image-{i}")).spawn(move || {
                loop {
                    let job = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => break,
                    }
                }
            });
        }
        tx
    })
}

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
        let client = crate::http::client(20)
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
        let _ = pool().send(Box::new(move || {
            let result = load_blocking(&client, &uri, path.as_deref());
            if let Some(slot) = mem.lock().unwrap().get_mut(&uri) {
                *slot = Poll::Ready(result);
            }
            ctx.request_repaint();
        }));

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
    if !crate::disk::writes_allowed(crate::disk::Kind::Historic) {
        return;
    }
    let tmp = p.with_extension("tmp");
    match std::fs::write(&tmp, bytes) {
        Ok(()) => {
            let _ = std::fs::rename(&tmp, p);
        }
        // A leftover partial file would make a full disk grow the cache, and the orphan would
        // survive pruning for a full TTL on its fresh mtime.
        Err(e) => {
            crate::disk::note_io_error(&e);
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// Under pressure the cache sheds far more aggressively: everything here refetches over the
/// network, so it is the cheapest space in the profile to give back.
pub(crate) fn prune_for_level(level: crate::disk::Level) {
    let ttl = match level {
        crate::disk::Level::Normal => TTL,
        _ => Duration::from_secs(3 * 24 * 60 * 60),
    };
    if let Ok(dir) = crate::store::data_dir() {
        prune_with(&dir.join("image_cache"), ttl);
    }
}

fn prune_old(dir: &Path) {
    prune_with(dir, TTL);
}

fn prune_with(dir: &Path, ttl: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        // A `.tmp` orphan is a failed write, never a cache hit, so age it out immediately.
        let is_tmp = path.extension().is_some_and(|e| e == "tmp");
        let stale = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > ttl);
        if stale || is_tmp {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Decodes EVE images off the UI thread. egui_extras' loader decodes inside `load`, on the frame
/// that first shows the image, so a feed scrolled into a screenful of new portraits decoded all of
/// them between two frames.
#[derive(Default)]
struct EveImageDecoder {
    mem: Arc<Mutex<HashMap<String, Poll<Result<Arc<egui::ColorImage>, String>>>>>,
}

impl ImageLoader for EveImageDecoder {
    fn id(&self) -> &str {
        egui::generate_loader_id!(EveImageDecoder)
    }

    fn load(&self, ctx: &egui::Context, uri: &str, _: SizeHint) -> ImageLoadResult {
        if !uri.starts_with(EVE_IMG_PREFIX) {
            return Err(LoadError::NotSupported);
        }
        if let Some(entry) = self.mem.lock().unwrap_or_else(|e| e.into_inner()).get(uri).cloned() {
            return match entry {
                Poll::Ready(Ok(image)) => Ok(ImagePoll::Ready { image }),
                Poll::Ready(Err(err)) => Err(LoadError::Loading(err)),
                Poll::Pending => Ok(ImagePoll::Pending { size: None }),
            };
        }
        // The bytes come from the cache above, which fetches them on the same pool.
        let bytes = match ctx.try_load_bytes(uri)? {
            BytesPoll::Ready { bytes, .. } => bytes,
            BytesPoll::Pending { size } => return Ok(ImagePoll::Pending { size }),
        };
        {
            let mut mem = self.mem.lock().unwrap_or_else(|e| e.into_inner());
            // A portrait is ~16 KB decoded. This is a memory bound, not a hit-rate one: the bytes
            // stay cached, so a dropped image costs one decode.
            if mem.len() > 2000 {
                mem.clear();
            }
            mem.insert(uri.to_owned(), Poll::Pending);
        }
        let (uri, mem, ctx) = (uri.to_owned(), Arc::clone(&self.mem), ctx.clone());
        let _ = pool().send(Box::new(move || {
            let result = decode(&bytes).map(Arc::new);
            if let Some(slot) = mem.lock().unwrap_or_else(|e| e.into_inner()).get_mut(&uri) {
                *slot = Poll::Ready(result);
            }
            ctx.request_repaint();
        }));
        Ok(ImagePoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        let _ = self.mem.lock().unwrap_or_else(|e| e.into_inner()).remove(uri);
    }

    fn forget_all(&self) {
        self.mem.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    fn byte_size(&self) -> usize {
        self.mem
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|e| match e {
                Poll::Ready(Ok(img)) => img.width() * img.height() * 4,
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.mem.lock().unwrap_or_else(|e| e.into_inner()).values().any(|e| e.is_pending())
    }
}

fn decode(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, image.as_flat_samples().as_slice()))
}

/// Install egui's image loaders with the disk-caching EVE-image loader in front of the
/// default network loader. Loaders are tried last-registered-first, so registering ours
/// *after* `install_image_loaders` gives it precedence for the `images.evetech.net` host.
pub fn install_image_loaders_cached(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    ctx.add_bytes_loader(Arc::new(EveImageCache::new()));
    ctx.add_image_loader(Arc::new(EveImageDecoder::default()));
}

#[cfg(test)]
mod tests {
    use super::decode;

    /// A 1x1 red PNG, so the decode path is covered without a fixture file.
    const RED_DOT: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
        0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn a_png_decodes_to_pixels() {
        let img = decode(RED_DOT).expect("decode");
        assert_eq!(img.size, [1, 1]);
        assert_eq!(img.pixels[0], egui::Color32::from_rgb(255, 0, 0));
        assert!(decode(b"not an image").is_err());
    }
}
