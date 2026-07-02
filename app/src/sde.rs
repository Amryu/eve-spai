//! EVE Static Data Export (SDE) — downloaded on first run and baked into the local
//! SQLite DB (docs/DESIGN.md §4, §7.1 E2). M1 imports the slice the intel/map
//! features need: solar systems (id, name, region, security, coordinates),
//! regions, and the system jump graph.
//!
//! Source: Fuzzwork's CSV conversion of the SDE. Columns are read positionally so
//! a leading BOM or added columns don't break parsing.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use rusqlite::{params, Connection};

const BASE: &str = "https://www.fuzzwork.co.uk/dump/latest/csv";

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

/// Shared, observable state of the SDE download/bake.
#[derive(Clone, Debug, Default)]
pub enum SdeStatus {
    #[default]
    NotReady,
    Downloading(String),
    Ready,
    Failed(String),
}

pub type SharedStatus = Arc<Mutex<SdeStatus>>;

/// Bake ship role bonuses (invTraits) lazily — small, separate from the main SDE
/// so it doesn't force a re-download. No-op if already baked.
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
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
            .timeout(std::time::Duration::from_secs(60))
            .build()
        else {
            return;
        };
        let Ok(csv) = client
            .get(format!("{BASE}/invTraits.csv"))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text())
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

/// Strip simple HTML (`<a href=…>Name</a>` → `Name`) from SDE bonus text.
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

/// Kick off a background download + bake. Updates `status` and repaints the UI as
/// it progresses. Safe to call again to refresh.
pub fn spawn_download(path: PathBuf, status: SharedStatus, ctx: egui::Context) {
    std::thread::spawn(move || {
        let set = |s: SdeStatus| {
            *status.lock().unwrap() = s;
            ctx.request_repaint();
        };
        set(SdeStatus::Downloading("Connecting…".to_owned()));
        match run(&path, &set) {
            Ok(()) => set(SdeStatus::Ready),
            Err(e) => set(SdeStatus::Failed(format!("{e:#}"))),
        }
    });
}

fn run(path: &PathBuf, set: &impl Fn(SdeStatus)) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
        .timeout(std::time::Duration::from_secs(180))
        .build()?;
    let fetch = |name: &str| -> Result<String> {
        set(SdeStatus::Downloading(format!("Downloading {name}…")));
        let url = format!("{BASE}/{name}");
        client
            .get(&url)
            .send()?
            .error_for_status()
            .with_context(|| format!("fetching {name}"))?
            .text()
            .with_context(|| format!("reading {name}"))
    };

    let regions_csv = fetch("mapRegions.csv")?;
    let constellations_csv = fetch("mapConstellations.csv")?;
    let systems_csv = fetch("mapSolarSystems.csv")?;
    let jumps_csv = fetch("mapSolarSystemJumps.csv")?;
    let groups_csv = fetch("invGroups.csv")?;
    let types_csv = fetch("invTypes.csv")?;
    let attrs_csv = fetch("dgmTypeAttributes.csv")?;

    set(SdeStatus::Downloading("Building local database…".to_owned()));
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

    // --- Ships (category 6) + selected dogma attributes ---
    set(SdeStatus::Downloading("Building ship data…".to_owned()));
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

    // Camp-relevant types for gate-camp signals: interdictors, HICs, smartbombs, and
    // anchorable warp-disruption bubbles. Classified by their group name.
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

    // --- EVE's flattened 2D map positions (position2D) from the modern JSONL SDE ---
    // (Fuzzwork's CSV only has 3D x/y/z.) Used for the in-game-style map layout.
    set(SdeStatus::Downloading("Downloading 2D map layout…".to_owned()));
    // Kept alive past the main commit so the celestial phase can re-open the archive
    // without re-downloading the ~99MB zip (Bytes clone is a cheap refcount bump).
    let zip_bytes = client
        .get("https://developers.eveonline.com/static-data/eve-online-static-data-latest-jsonl.zip")
        .send()?
        .error_for_status()
        .context("fetching JSONL SDE")?
        .bytes()
        .context("reading JSONL SDE")?;
    {
        use std::io::Read as _;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes.clone()))
            .map_err(|e| anyhow::anyhow!("opening JSONL SDE: {e}"))?;
        let mut jsonl = String::new();
        archive
            .by_name("mapSolarSystems.jsonl")
            .map_err(|e| anyhow::anyhow!("mapSolarSystems.jsonl: {e}"))?
            .read_to_string(&mut jsonl)
            .context("reading mapSolarSystems.jsonl")?;

        set(SdeStatus::Downloading("Building 2D map layout…".to_owned()));
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

        // Stargate positions per system (for on-gate gate-camp detection). Schema:
        // { "_key": int, "solarSystemID": int, "position": {x,y,z} }.
        set(SdeStatus::Downloading("Indexing stargates…".to_owned()));
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

        // Localized ship names (so intel from non-English clients resolves, e.g. a
        // Chinese hull name). Stream types.jsonl (large) and keep only ship types.
        set(SdeStatus::Downloading("Indexing ship translations…".to_owned()));
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
            let mut ins = tx.prepare("INSERT INTO sde_ship_i18n(ship_id, name) VALUES(?1, ?2)")?;
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
                        // Skip languages that just repeat the English name.
                        if !loc.is_empty() && loc != en {
                            ins.execute(params![id, loc])?;
                        }
                    }
                }
            }
        }
    }

    let version = chrono::Utc::now().format("%Y-%m-%d").to_string();
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

    // Celestials are populated AFTER the main commit, in their own chunked transactions
    // (see `bake_celestials`). Holding the single main write-transaction across the 224MB
    // moon parse kept the WAL write-lock for the whole bake, so every UI-thread write
    // blocked on busy_timeout and the app appeared frozen. The core SDE is committed by
    // now, so the UI is usable while celestials trickle in.
    bake_celestials(&mut conn, zip_bytes.as_ref(), set)?;

    Ok(())
}

/// Populate `sde_celestials` (~432k rows) in ~30k-row chunked transactions, streaming the
/// large celestial files (the 224MB moon file line-by-line — never read whole). Runs AFTER
/// the main SDE commit, so it is the only writer and each chunk releases the WAL write-lock
/// so concurrent UI-thread writes interleave and the app stays responsive.
fn bake_celestials(conn: &mut Connection, zip_bytes: &[u8], set: &impl Fn(SdeStatus)) -> Result<()> {
    use std::io::{BufRead, BufReader, Read as _};

    const CHUNK: usize = 30_000;

    set(SdeStatus::Downloading("Indexing celestials…".to_owned()));

    // system id -> name (sde_systems is committed by now), for the display labels.
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

    // Operation id -> English operation name (small 140KB file, read whole).
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

    // Clear once, in its own tiny transaction.
    conn.execute("DELETE FROM sde_celestials", [])?;

    // Rows buffer -> flushed to a fresh transaction every CHUNK; `total` drives the status.
    let mut buf: Vec<(i64, String, [f64; 3])> = Vec::with_capacity(CHUNK);
    let mut total = 0usize;

    // Gates (3MB): name = "<destination system> gate" (fallback "gate").
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

    // Planets (50MB): name = "<system> <roman(celestialIndex)>" e.g. "Jita IV".
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

    // Moons (224MB — MUST stream): "<system> <roman(celestialIndex)> - Moon <orbitIndex>".
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

    // Stations (2MB): the operation name when known, else "<system> station".
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

    // Final partial chunk.
    total += buf.len();
    commit_chunk(conn, &mut buf)?;
    set(SdeStatus::Downloading(format!("Indexing celestials… {total}")));
    Ok(())
}

/// Flush the buffer to a fresh transaction once it reaches `chunk`, updating the status.
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
        set(SdeStatus::Downloading(format!("Indexing celestials… {total}")));
    }
    Ok(())
}

/// Insert one buffered batch of celestials in a single transaction, then clear the buffer.
/// The short-lived transaction releases the WAL write-lock between chunks.
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

/// Extract an `{x, y, z}` position object into `[x, y, z]`, or `None` if any is missing.
fn json_pos(p: &serde_json::Value) -> Option<[f64; 3]> {
    Some([p.get("x")?.as_f64()?, p.get("y")?.as_f64()?, p.get("z")?.as_f64()?])
}

/// Roman numeral for a celestial index (1 -> "I"). Planets reach ~13; handles any
/// positive value. Non-positive input falls back to the decimal string.
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
