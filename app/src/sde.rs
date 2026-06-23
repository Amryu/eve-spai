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
        let baked: i64 = conn
            .query_row("SELECT COUNT(*) FROM sde_ship_traits", [], |r| r.get(0))
            .unwrap_or(0);
        if baked > 0 {
            return;
        }
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent("eve-spai/0.1 (EVE intel tool)")
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
        .user_agent("eve-spai/0.1 (EVE intel tool)")
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

    // Ship groups (categoryID 0=groupID, 1=categoryID, 2=groupName).
    let mut ship_groups: HashMap<i64, String> = HashMap::new();
    {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(groups_csv.as_bytes());
        for rec in rdr.records() {
            let rec = rec?;
            let cat: i64 = rec.get(1).unwrap_or("").trim().parse().unwrap_or(0);
            if cat != 6 {
                continue;
            }
            if let Ok(gid) = rec.get(0).unwrap_or("").trim().parse::<i64>() {
                ship_groups.insert(gid, rec.get(2).unwrap_or("").to_owned());
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
    {
        use std::io::Read as _;
        let zip_bytes = client
            .get("https://developers.eveonline.com/static-data/eve-online-static-data-latest-jsonl.zip")
            .send()?
            .error_for_status()
            .context("fetching JSONL SDE")?
            .bytes()
            .context("reading JSONL SDE")?;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
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
    Ok(())
}
