use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use rusqlite::{params, Connection};

const BASE: &str = "https://www.fuzzwork.co.uk/dump/latest/csv";
/// The same files on eve-spai.com, refreshed daily (crates/server/deploy/sde-mirror.sh), behind
/// Cloudflare: tried first, as pilots far from the UK (China) got fuzzwork at a crawl.
const MIRROR: &str = "https://eve-spai.com/sde";
/// A source still this slow after [`SLOW_AFTER`] is left for the next one.
const SLOW_BYTES_PER_SEC: f64 = 50.0 * 1024.0;
const SLOW_AFTER: std::time::Duration = std::time::Duration::from_secs(20);
/// FC's static data export, which the map layout, gates and celestials come from.
pub const JSONL_URL: &str = "https://developers.eveonline.com/static-data/eve-online-static-data-latest-jsonl.zip";
/// The whole download on a slow line: the JSONL zip alone is about 100 MB.
const DOWNLOAD_TIMEOUT_SECS: u64 = 3 * 3600;

/// Dogma attribute ids we keep for ships (resonances, hp, drones, hardpoints,
/// speed, slots). Resist = 1 - resonance.
const SHIP_ATTRS: &[i64] = &[
    271, 272, 273, 274, // shield resonance: em, exp, kin, therm
    267, 268, 269, 270, // armor resonance
    113, 111, 109, 110, // hull resonance
    263, 265, 9, // shield hp, armor hp, structure hp
    283, 1271, // drone capacity, drone bandwidth
    101, 102, // launcher / turret hardpoints
    37,  // max velocity
    12, 13, 14, // low / med / hi slots
    600, 1281, // warp speed multiplier, base warp speed
];

#[derive(Clone, Debug, Default)]
pub enum SdeStatus {
    #[default]
    NotReady,
    Downloading(String),
    Ready,
    Failed(String),
}

pub type SharedStatus = Arc<Mutex<SdeStatus>>;

/// The build running now. Cancelling moves it on, so a download still blocked on the network
/// stops at its next read and never reports over a later build.
static RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Stops the download in progress. A build already past its downloads finishes.
pub fn cancel(status: &SharedStatus) {
    RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    *status.lock().unwrap() = SdeStatus::Failed("Download cancelled.".to_owned());
}

fn cancelled(run: u64) -> bool {
    RUN.load(std::sync::atomic::Ordering::SeqCst) != run
}

pub fn spawn_traits_bake(path: PathBuf, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(mut conn) = Connection::open(&path) else { return };
        crate::store::apply_pragmas(&conn);
        let baked: i64 = conn
            .query_row("SELECT COUNT(*) FROM sde_ship_traits", [], |r| r.get(0))
            .unwrap_or(0);
        if baked > 0 {
            return;
        }
        let Ok(client) = crate::http::client(60)
        else {
            return;
        };
        let Some(csv) = sources("invTraits.csv", &format!("{BASE}/invTraits.csv"))
            .into_iter()
            .find_map(|url| client.get(url).send().and_then(|r| r.error_for_status()).and_then(|r| r.text()).ok())
        else {
            return;
        };
        let Ok(tx) = conn.transaction() else { return };
        {
            let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_reader(csv.as_bytes());
            let Ok(mut stmt) = tx.prepare(
                "INSERT INTO sde_ship_traits(ship_id, skill_id, bonus, text) VALUES(?1,?2,?3,?4)",
            ) else {
                return;
            };
            // traitID(0), typeID(1), skillID(2), bonus(3), bonusText(4), unitID(5)
            for rec in rdr.records().flatten() {
                let type_id: i64 = rec.get(1).unwrap_or("").trim().parse().unwrap_or(0);
                let skill_id: i64 = rec.get(2).unwrap_or("").trim().parse().unwrap_or(-1);
                let bonus: f64 = rec.get(3).unwrap_or("").trim().parse().unwrap_or(0.0);
                let text = strip_html(rec.get(4).unwrap_or(""));
                if type_id != 0 && !text.is_empty() {
                    let _ = stmt.execute(params![type_id, skill_id, bonus, text]);
                }
            }
        }
        let _ = tx.commit();
        ctx.request_repaint();
    });
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
}

/// Builds the static data, from `zip` (the JSONL export the user downloaded) when given, else
/// downloading it.
pub fn spawn_download(path: PathBuf, status: SharedStatus, ctx: egui::Context, zip: Option<PathBuf>) {
    let me = RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        let set = |s: SdeStatus| {
            if !cancelled(me) {
                *status.lock().unwrap() = s;
                ctx.request_repaint();
            }
        };
        set(SdeStatus::Downloading(tr!("Connecting…").to_owned()));
        match run(&path, &set, zip.as_deref(), me) {
            Ok(()) => set(SdeStatus::Ready),
            Err(e) => set(SdeStatus::Failed(format!("{e:#}"))),
        }
    });
}

/// The body of the first of `urls` that delivers, with how much has arrived shown as it comes in.
/// A source that fails, stalls or crawls is left for the next.
fn download(client: &reqwest::blocking::Client, urls: &[String], what: &str, set: &impl Fn(SdeStatus), me: u64) -> Result<Vec<u8>> {
    let mut last = anyhow::anyhow!("no source for {what}");
    for url in urls {
        match download_from(client, url, what, set, me) {
            Ok(body) => return Ok(body),
            Err(e) if cancelled(me) => return Err(e),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// One source, read on its own thread so a read that hangs (the client allows hours for one) can be
/// given up on: no new bytes or a crawl past [`SLOW_AFTER`] ends it.
fn download_from(client: &reqwest::blocking::Client, url: &str, what: &str, set: &impl Fn(SdeStatus), me: u64) -> Result<Vec<u8>> {
    use std::io::Read as _;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    const MB: f64 = 1_048_576.0;
    set(SdeStatus::Downloading(trf!("Downloading {what}…", what = what)));
    let got = Arc::new(AtomicU64::new(0));
    let total = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let (client, url, what) = (client.clone(), url.to_owned(), what.to_owned());
        let (got, total, stop) = (got.clone(), total.clone(), stop.clone());
        std::thread::spawn(move || {
            let r = (|| -> Result<Vec<u8>> {
                let mut resp = client.get(&url).send()?.error_for_status().with_context(|| format!("fetching {what}"))?;
                total.store(resp.content_length().unwrap_or(0), Ordering::Relaxed);
                let mut body = Vec::with_capacity(resp.content_length().unwrap_or(0) as usize);
                let mut buf = vec![0u8; 256 * 1024];
                loop {
                    if stop.load(Ordering::Relaxed) {
                        anyhow::bail!("stopped");
                    }
                    let n = resp.read(&mut buf).with_context(|| format!("reading {what}"))?;
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&buf[..n]);
                    got.fetch_add(n as u64, Ordering::Relaxed);
                }
                Ok(body)
            })();
            let _ = tx.send(r);
        });
    }
    let started = std::time::Instant::now();
    let host = url.split('/').nth(2).unwrap_or(url).to_owned();
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(r) => return r,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("the download of {what} stopped"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if cancelled(me) {
            stop.store(true, Ordering::Relaxed);
            anyhow::bail!("download cancelled");
        }
        let (n, of) = (got.load(Ordering::Relaxed) as f64, total.load(Ordering::Relaxed) as f64);
        let rate = n / started.elapsed().as_secs_f64().max(0.001);
        let left = if of > 0.0 { of - n } else { f64::MAX };
        if started.elapsed() >= SLOW_AFTER && rate < SLOW_BYTES_PER_SEC && left > MB {
            stop.store(true, Ordering::Relaxed);
            anyhow::bail!("{what} from {host} was too slow ({:.0} KB/s)", rate / 1024.0);
        }
        let of = if of > 0.0 { format!(" / {:.1}", of / MB) } else { String::new() };
        set(SdeStatus::Downloading(trf!("Downloading {what} from {host}… {got}{of} MB at {rate} MB/s", what = what, host = host, got = format!("{:.1}", n / MB), of = of, rate = format!("{:.1}", rate / MB))));
    }
}

/// Whether a status line is a download still running, which can be cancelled: in the language shown.
pub fn cancellable(msg: &str) -> bool {
    let head = |t: &str| t.split('{').next().unwrap_or(t).trim_end().to_owned();
    msg.starts_with(tr!("Connecting…")) || msg.starts_with(&head(tr!("Downloading {what}…")))
}

/// Where a file can come from, nearest first.
fn sources(name: &str, original: &str) -> Vec<String> {
    vec![format!("{MIRROR}/{name}"), original.to_owned()]
}

/// Whether `bytes` is a zip holding the JSONL SDE's solar systems.
fn check_jsonl_zip(bytes: &[u8]) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| anyhow::anyhow!("{e}"))?;
    archive.by_name("mapSolarSystems.jsonl").map_err(|e| anyhow::anyhow!("mapSolarSystems.jsonl: {e}"))?;
    Ok(())
}

fn run(path: &PathBuf, set: &impl Fn(SdeStatus), zip: Option<&std::path::Path>, me: u64) -> Result<()> {
    let client = crate::http::client(DOWNLOAD_TIMEOUT_SECS)?;
    // A file picked by hand is checked before anything is downloaded.
    let local = match zip {
        Some(file) => {
            set(SdeStatus::Downloading(trf!("Reading {file}…", file = file.display())));
            let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
            check_jsonl_zip(&bytes).with_context(|| format!("{} is not the JSONL SDE zip", file.display()))?;
            Some(bytes)
        }
        None => None,
    };
    let fetch = |name: &str| -> Result<String> {
        let body = download(&client, &sources(name, &format!("{BASE}/{name}")), name, set, me)?;
        String::from_utf8(body).with_context(|| format!("reading {name}"))
    };

    let regions_csv = fetch("mapRegions.csv")?;
    let constellations_csv = fetch("mapConstellations.csv")?;
    let systems_csv = fetch("mapSolarSystems.csv")?;
    let jumps_csv = fetch("mapSolarSystemJumps.csv")?;
    let groups_csv = fetch("invGroups.csv")?;
    let types_csv = fetch("invTypes.csv")?;
    let attrs_csv = fetch("dgmTypeAttributes.csv")?;

    let zip_bytes = match local {
        Some(b) => b,
        None => download(&client, &sources("eve-online-static-data-latest-jsonl.zip", JSONL_URL), "the JSONL SDE", set, me)?,
    };
    // Past here the build writes the database; a cancel no longer stops it.
    if cancelled(me) {
        anyhow::bail!("download cancelled");
    }
    set(SdeStatus::Downloading(tr!("Building local database…").to_owned()));
    let mut conn = Connection::open(path)?;
    crate::store::apply_pragmas(&conn);
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM sde_regions", [])?;
    tx.execute("DELETE FROM sde_constellations", [])?;
    tx.execute("DELETE FROM sde_systems", [])?;
    tx.execute("DELETE FROM sde_jumps", [])?;

    // Regions: regionID(0), regionName(1)
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(regions_csv.as_bytes());
        let mut stmt = tx.prepare("INSERT OR REPLACE INTO sde_regions(id, name) VALUES(?1, ?2)")?;
        for rec in rdr.records() {
            let rec = rec?;
            let id: i64 = match rec.get(0).unwrap_or("").trim().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            stmt.execute(params![id, rec.get(1).unwrap_or("")])?;
        }
    }

    // Constellations: constellationID(1), constellationName(2)
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(constellations_csv.as_bytes());
        let mut stmt =
            tx.prepare("INSERT OR REPLACE INTO sde_constellations(id, name) VALUES(?1, ?2)")?;
        for rec in rdr.records() {
            let rec = rec?;
            let id: i64 = match rec.get(1).unwrap_or("").trim().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            stmt.execute(params![id, rec.get(2).unwrap_or("")])?;
        }
    }

    // Systems: regionID(0), constellationID(1), solarSystemID(2), name(3),
    // x(4), y(5), z(6), security(21)
    let mut systems = 0i64;
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(systems_csv.as_bytes());
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO sde_systems(id, name, region_id, constellation_id, faction_id, security, x, y, z)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for rec in rdr.records() {
            let rec = rec?;
            let id: i64 = match rec.get(2).unwrap_or("").trim().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            let region_id: i64 = rec.get(0).unwrap_or("").trim().parse().unwrap_or(0);
            let constellation_id: i64 = rec.get(1).unwrap_or("").trim().parse().unwrap_or(0);
            let name = rec.get(3).unwrap_or("");
            let security: f64 = rec.get(21).unwrap_or("").trim().parse().unwrap_or(0.0);
            // factionID(22) is blank for unclaimed systems.
            let faction_id: i64 = rec.get(22).unwrap_or("").trim().parse().unwrap_or(0);
            let x: f64 = rec.get(4).unwrap_or("").trim().parse().unwrap_or(0.0);
            let y: f64 = rec.get(5).unwrap_or("").trim().parse().unwrap_or(0.0);
            let z: f64 = rec.get(6).unwrap_or("").trim().parse().unwrap_or(0.0);
            stmt.execute(params![id, name, region_id, constellation_id, faction_id, security, x, y, z])?;
            systems += 1;
        }
    }

    // Jumps: fromSolarSystemID(2), toSolarSystemID(3)
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(jumps_csv.as_bytes());
        let mut stmt = tx.prepare("INSERT INTO sde_jumps(from_id, to_id) VALUES(?1, ?2)")?;
        for rec in rdr.records() {
            let rec = rec?;
            let from: i64 = rec.get(2).unwrap_or("").trim().parse().unwrap_or(0);
            let to: i64 = rec.get(3).unwrap_or("").trim().parse().unwrap_or(0);
            if from != 0 && to != 0 {
                stmt.execute(params![from, to])?;
            }
        }
    }

    set(SdeStatus::Downloading(tr!("Building ship data…").to_owned()));
    tx.execute("DELETE FROM sde_ships", [])?;
    tx.execute("DELETE FROM sde_ship_attrs", [])?;

    // Groups (0=groupID, 1=categoryID, 2=groupName). `ship_groups` keeps the ship category;
    // `all_groups` keeps every group so we can classify camp-relevant non-ship types too.
    let mut ship_groups: HashMap<i64, String> = HashMap::new();
    let mut all_groups: HashMap<i64, String> = HashMap::new();
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(groups_csv.as_bytes());
        for rec in rdr.records() {
            let rec = rec?;
            let cat: i64 = rec.get(1).unwrap_or("").trim().parse().unwrap_or(0);
            if let Ok(gid) = rec.get(0).unwrap_or("").trim().parse::<i64>() {
                let name = rec.get(2).unwrap_or("").to_owned();
                if cat == 6 {
                    ship_groups.insert(gid, name.clone());
                }
                all_groups.insert(gid, name);
            }
        }
    }

    // Ships: typeID(0), groupID(1), typeName(2), mass(4), volume(5).
    let mut ship_ids: HashSet<i64> = HashSet::new();
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(types_csv.as_bytes());
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO sde_ships(id, name, group_name, mass, volume) VALUES(?1,?2,?3,?4,?5)",
        )?;
        for rec in rdr.records() {
            let rec = rec?;
            let gid: i64 = rec.get(1).unwrap_or("").trim().parse().unwrap_or(0);
            let Some(group) = ship_groups.get(&gid) else {
                continue;
            };
            let Ok(id) = rec.get(0).unwrap_or("").trim().parse::<i64>() else {
                continue;
            };
            let mass: f64 = rec.get(4).unwrap_or("").trim().parse().unwrap_or(0.0);
            let volume: f64 = rec.get(5).unwrap_or("").trim().parse().unwrap_or(0.0);
            stmt.execute(params![id, rec.get(2).unwrap_or(""), group, mass, volume])?;
            ship_ids.insert(id);
        }
    }

    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(types_csv.as_bytes());
        let mut stmt =
            tx.prepare("INSERT OR REPLACE INTO sde_camp_types(id, kind) VALUES(?1, ?2)")?;
        for rec in rdr.records() {
            let rec = rec?;
            let gid: i64 = rec.get(1).unwrap_or("").trim().parse().unwrap_or(0);
            let Some(group) = all_groups.get(&gid) else { continue };
            let kind = match group.as_str() {
                "Interdictor" => "dic",
                "Heavy Interdiction Cruiser" => "hic",
                "Smart Bomb" => "smartbomb",
                "Mobile Warp Disruptor" => "bubble",
                _ => continue,
            };
            if let Ok(id) = rec.get(0).unwrap_or("").trim().parse::<i64>() {
                stmt.execute(params![id, kind])?;
            }
        }
    }

    // Ship attributes: typeID(0), attributeID(1), valueInt(2), valueFloat(3).
    {
        let needed: HashSet<i64> = SHIP_ATTRS.iter().copied().collect();
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(attrs_csv.as_bytes());
        let mut stmt =
            tx.prepare("INSERT OR REPLACE INTO sde_ship_attrs(ship_id, attr_id, value) VALUES(?1,?2,?3)")?;
        for rec in rdr.records() {
            let rec = rec?;
            let tid: i64 = rec.get(0).unwrap_or("").trim().parse().unwrap_or(0);
            let aid: i64 = rec.get(1).unwrap_or("").trim().parse().unwrap_or(0);
            if !ship_ids.contains(&tid) || !needed.contains(&aid) {
                continue;
            }
            let value: f64 = rec
                .get(3)
                .and_then(|v| v.trim().parse().ok())
                .or_else(|| rec.get(2).and_then(|v| v.trim().parse().ok()))
                .unwrap_or(0.0);
            stmt.execute(params![tid, aid, value])?;
        }
    }

    {
        use std::io::Read as _;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes.as_slice()))
            .map_err(|e| anyhow::anyhow!("opening JSONL SDE: {e}"))?;
        let mut jsonl = String::new();
        archive
            .by_name("mapSolarSystems.jsonl")
            .map_err(|e| anyhow::anyhow!("mapSolarSystems.jsonl: {e}"))?
            .read_to_string(&mut jsonl)
            .context("reading mapSolarSystems.jsonl")?;

        set(SdeStatus::Downloading(tr!("Building 2D map layout…").to_owned()));
        let mut stmt = tx.prepare("UPDATE sde_systems SET x2d = ?2, z2d = ?3 WHERE id = ?1")?;
        for line in jsonl.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let (Some(id), Some(p)) = (v.get("_key").and_then(|k| k.as_i64()), v.get("position2D"))
            else {
                continue;
            };
            if let (Some(x), Some(y)) =
                (p.get("x").and_then(|n| n.as_f64()), p.get("y").and_then(|n| n.as_f64()))
            {
                stmt.execute(params![id, x, y])?;
            }
        }
        drop(stmt);

        set(SdeStatus::Downloading(tr!("Indexing stargates…").to_owned()));
        let mut gates_jsonl = String::new();
        let _ = archive
            .by_name("mapStargates.jsonl")
            .map(|mut e| e.read_to_string(&mut gates_jsonl));
        if !gates_jsonl.is_empty() {
            tx.execute("DELETE FROM sde_stargates", [])?;
            let mut ins =
                tx.prepare("INSERT INTO sde_stargates(system_id, x, y, z) VALUES(?1,?2,?3,?4)")?;
            for line in gates_jsonl.lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
                let Some(sys) = v.get("solarSystemID").and_then(|s| s.as_i64()) else { continue };
                let Some(p) = v.get("position") else { continue };
                if let (Some(x), Some(y), Some(z)) = (
                    p.get("x").and_then(|n| n.as_f64()),
                    p.get("y").and_then(|n| n.as_f64()),
                    p.get("z").and_then(|n| n.as_f64()),
                ) {
                    ins.execute(params![sys, x, y, z])?;
                }
            }
        }

        set(SdeStatus::Downloading(tr!("Indexing ship translations…").to_owned()));
        let ship_ids: HashSet<i64> = {
            let mut s = HashSet::new();
            let mut q = tx.prepare("SELECT id FROM sde_ships")?;
            for id in q.query_map([], |r| r.get::<_, i64>(0))?.flatten() {
                s.insert(id);
            }
            s
        };
        let mut types_jsonl = String::new();
        let _ = archive
            .by_name("types.jsonl")
            .map(|mut e| e.read_to_string(&mut types_jsonl));
        if !types_jsonl.is_empty() {
            tx.execute("DELETE FROM sde_ship_i18n", [])?;
            tx.execute("DELETE FROM sde_ship_names", [])?;
            let mut ins = tx.prepare("INSERT INTO sde_ship_i18n(ship_id, name) VALUES(?1, ?2)")?;
            let mut by_lang = tx.prepare("INSERT OR REPLACE INTO sde_ship_names(ship_id, lang, name) VALUES(?1, ?2, ?3)")?;
            for line in types_jsonl.lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
                let Some(id) = v.get("_key").and_then(|k| k.as_i64()) else { continue };
                if !ship_ids.contains(&id) {
                    continue;
                }
                let Some(names) = v.get("name").and_then(|n| n.as_object()) else { continue };
                let en = names.get("en").and_then(|n| n.as_str()).unwrap_or("");
                for (lang, val) in names {
                    if lang == "en" {
                        continue;
                    }
                    if let Some(loc) = val.as_str() {
                        if !loc.is_empty() && loc != en {
                            ins.execute(params![id, loc])?;
                            by_lang.execute(params![id, lang, loc])?;
                        }
                    }
                }
            }
        }
    }

    let version = crate::clock::utc().format("%Y-%m-%d").to_string();
    tx.execute(
        "INSERT OR REPLACE INTO sde_meta(key, value) VALUES('version', ?1)",
        params![version],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO sde_meta(key, value) VALUES('schema', ?1)",
        params![crate::store::SDE_SCHEMA_VERSION],
    )?;
    tx.commit()?;

    if systems == 0 {
        anyhow::bail!("no systems parsed from SDE");
    }

    // Celestials go in after the main commit, in chunked transactions: holding the write lock
    // across the 224MB moon parse blocks every UI-thread write on busy_timeout.
    bake_celestials(&mut conn, &zip_bytes, set)?;

    Ok(())
}

fn bake_celestials(conn: &mut Connection, zip_bytes: &[u8], set: &impl Fn(SdeStatus)) -> Result<()> {
    use std::io::{BufRead, BufReader, Read as _};

    const CHUNK: usize = 30_000;

    set(SdeStatus::Downloading(tr!("Indexing celestials…").to_owned()));

    let sys_names: HashMap<i64, String> = {
        let mut m = HashMap::new();
        let mut q = conn.prepare("SELECT id, name FROM sde_systems")?;
        for row in q
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .flatten()
        {
            m.insert(row.0, row.1);
        }
        m
    };

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| anyhow::anyhow!("re-opening JSONL SDE for celestials: {e}"))?;

    let mut op_names: HashMap<i64, String> = HashMap::new();
    {
        let mut ops = String::new();
        let _ = archive.by_name("stationOperations.jsonl").map(|mut e| e.read_to_string(&mut ops));
        for line in ops.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let Some(id) = v.get("_key").and_then(|k| k.as_i64()) else { continue };
            if let Some(n) = v
                .get("operationName")
                .and_then(|o| o.get("en"))
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty())
            {
                op_names.insert(id, n.to_owned());
            }
        }
    }

    conn.execute("DELETE FROM sde_celestials", [])?;

    let mut buf: Vec<(i64, String, [f64; 3])> = Vec::with_capacity(CHUNK);
    let mut total = 0usize;

    if let Ok(entry) = archive.by_name("mapStargates.jsonl") {
        for line in BufReader::new(entry).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            let Some(sys) = v.get("solarSystemID").and_then(|s| s.as_i64()) else { continue };
            let Some(p) = v.get("position").and_then(json_pos) else { continue };
            let dest = v
                .get("destination")
                .and_then(|d| d.get("solarSystemID"))
                .and_then(|s| s.as_i64());
            let name = match dest.and_then(|d| sys_names.get(&d)) {
                Some(n) => format!("{n} gate"),
                None => "gate".to_owned(),
            };
            buf.push((sys, name, p));
            flush_if_full(conn, &mut buf, &mut total, CHUNK, set)?;
        }
    }

    if let Ok(entry) = archive.by_name("mapPlanets.jsonl") {
        for line in BufReader::new(entry).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            let Some(sys) = v.get("solarSystemID").and_then(|s| s.as_i64()) else { continue };
            let Some(sname) = sys_names.get(&sys) else { continue };
            let Some(p) = v.get("position").and_then(json_pos) else { continue };
            let idx = v.get("celestialIndex").and_then(|n| n.as_i64()).unwrap_or(0);
            buf.push((sys, format!("{sname} {}", roman(idx)), p));
            flush_if_full(conn, &mut buf, &mut total, CHUNK, set)?;
        }
    }

    // Moons are 224MB, so stream them: "<system> <roman(celestialIndex)> - Moon <orbitIndex>".
    if let Ok(entry) = archive.by_name("mapMoons.jsonl") {
        for line in BufReader::new(entry).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            let Some(sys) = v.get("solarSystemID").and_then(|s| s.as_i64()) else { continue };
            let Some(sname) = sys_names.get(&sys) else { continue };
            let Some(p) = v.get("position").and_then(json_pos) else { continue };
            let idx = v.get("celestialIndex").and_then(|n| n.as_i64()).unwrap_or(0);
            let orbit = v.get("orbitIndex").and_then(|n| n.as_i64()).unwrap_or(0);
            buf.push((sys, format!("{sname} {} - Moon {orbit}", roman(idx)), p));
            flush_if_full(conn, &mut buf, &mut total, CHUNK, set)?;
        }
    }

    if let Ok(entry) = archive.by_name("npcStations.jsonl") {
        for line in BufReader::new(entry).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            let Some(sys) = v.get("solarSystemID").and_then(|s| s.as_i64()) else { continue };
            let Some(sname) = sys_names.get(&sys) else { continue };
            let Some(p) = v.get("position").and_then(json_pos) else { continue };
            let name = v
                .get("operationID")
                .and_then(|o| o.as_i64())
                .and_then(|o| op_names.get(&o))
                .cloned()
                .unwrap_or_else(|| format!("{sname} station"));
            buf.push((sys, name, p));
            flush_if_full(conn, &mut buf, &mut total, CHUNK, set)?;
        }
    }

    total += buf.len();
    commit_chunk(conn, &mut buf)?;
    set(SdeStatus::Downloading(trf!("Indexing celestials… {total}", total = total)));
    Ok(())
}

fn flush_if_full(
    conn: &mut Connection,
    buf: &mut Vec<(i64, String, [f64; 3])>,
    total: &mut usize,
    chunk: usize,
    set: &impl Fn(SdeStatus),
) -> Result<()> {
    if buf.len() >= chunk {
        *total += buf.len();
        commit_chunk(conn, buf)?;
        set(SdeStatus::Downloading(trf!("Indexing celestials… {total}", total = total)));
    }
    Ok(())
}

fn commit_chunk(conn: &mut Connection, buf: &mut Vec<(i64, String, [f64; 3])>) -> Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction()?;
    {
        let mut ins =
            tx.prepare("INSERT INTO sde_celestials(system_id, name, x, y, z) VALUES(?1,?2,?3,?4,?5)")?;
        for (sys, name, p) in buf.iter() {
            ins.execute(params![sys, name, p[0], p[1], p[2]])?;
        }
    }
    tx.commit()?;
    buf.clear();
    Ok(())
}

fn json_pos(p: &serde_json::Value) -> Option<[f64; 3]> {
    Some([p.get("x")?.as_f64()?, p.get("y")?.as_f64()?, p.get("z")?.as_f64()?])
}

fn roman(n: i64) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let table = [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut n = n;
    let mut out = String::new();
    for (v, s) in table {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::roman;

    #[test]
    fn roman_numerals() {
        assert_eq!(roman(1), "I");
        assert_eq!(roman(4), "IV");
        assert_eq!(roman(9), "IX");
        assert_eq!(roman(13), "XIII");
    }
}

#[cfg(test)]
mod download_tests {
    use super::check_jsonl_zip;

    fn zip_with(name: &str) -> Vec<u8> {
        use std::io::Write as _;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"{}\n").unwrap();
        w.finish().unwrap().into_inner()
    }

    /// The whole build from a zip on disk, into a scratch profile: `SDE_TEST_DIR` and `SDE_TEST_ZIP`.
    #[test]
    #[ignore]
    fn builds_from_a_downloaded_zip() {
        let dir = std::env::var("SDE_TEST_DIR").expect("SDE_TEST_DIR");
        let zip = std::env::var("SDE_TEST_ZIP").expect("SDE_TEST_ZIP");
        std::env::set_var("EVE_SPAI_DATA_DIR", &dir);
        let path = crate::store::Store::open().unwrap().path().to_path_buf();
        let me = super::RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let set = |s: super::SdeStatus| eprintln!("{s:?}");
        super::run(&path, &set, Some(std::path::Path::new(&zip)), me).unwrap();
        assert!(crate::store::Store::open().unwrap().sde_ready());
    }

    /// A source that answers with an error is left for the next one, which delivers.
    #[test]
    fn a_failing_source_falls_through_to_the_next() {
        let serve = |status: u16, body: &'static str| {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let url = format!("http://{}/file.csv", server.server_addr().to_ip().unwrap());
            std::thread::spawn(move || {
                if let Ok(req) = server.recv() {
                    let _ = req.respond(tiny_http::Response::from_string(body).with_status_code(status));
                }
            });
            url
        };
        let (down, up) = (serve(404, ""), serve(200, "regionID,regionName"));
        let client = crate::http::client(30).unwrap();
        let me = super::RUN.load(std::sync::atomic::Ordering::SeqCst);
        let body = super::download(&client, &[down, up], "file.csv", &|_| {}, me).unwrap();
        assert_eq!(body, b"regionID,regionName");
    }

    /// A live download stopped once it has started: `SDE_TEST_DIR`.
    #[test]
    #[ignore]
    fn a_download_stops_when_cancelled() {
        let dir = std::env::var("SDE_TEST_DIR").expect("SDE_TEST_DIR");
        std::env::set_var("EVE_SPAI_DATA_DIR", &dir);
        let path = crate::store::Store::open().unwrap().path().to_path_buf();
        let status: super::SharedStatus = Default::default();
        let me = super::RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let st = status.clone();
        let worker = std::thread::spawn(move || {
            let set = |s: super::SdeStatus| {
                eprintln!("{s:?}");
                *st.lock().unwrap() = s;
            };
            super::run(&path, &set, None, me)
        });
        loop {
            let now = status.lock().unwrap().clone();
            if matches!(&now, super::SdeStatus::Downloading(m) if m.contains("invTypes") && m.contains("MB")) {
                break;
            }
            assert!(!worker.is_finished(), "ended before it could be cancelled: {now:?}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let at = std::time::Instant::now();
        super::cancel(&status);
        let err = worker.join().unwrap().unwrap_err();
        assert!(format!("{err:#}").contains("cancelled"), "{err:#}");
        assert!(at.elapsed() < std::time::Duration::from_secs(5));
        assert!(!crate::store::Store::open().unwrap().sde_ready(), "nothing built");
    }

    #[test]
    fn only_the_jsonl_export_is_taken_as_the_sde() {
        assert!(check_jsonl_zip(&zip_with("mapSolarSystems.jsonl")).is_ok());
        assert!(check_jsonl_zip(&zip_with("invTypes.csv")).is_err(), "another zip");
        assert!(check_jsonl_zip(b"not a zip").is_err());
    }
}
