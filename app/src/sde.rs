//! EVE Static Data Export (SDE) — downloaded on first run and baked into the local
//! SQLite DB (docs/DESIGN.md §4, §7.1 E2). M1 imports the slice the intel/map
//! features need: solar systems (id, name, region, security, coordinates),
//! regions, and the system jump graph.
//!
//! Source: Fuzzwork's CSV conversion of the SDE. Columns are read positionally so
//! a leading BOM or added columns don't break parsing.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use rusqlite::{params, Connection};

const BASE: &str = "https://www.fuzzwork.co.uk/dump/latest/csv";

/// Shared, observable state of the SDE download/bake.
#[derive(Clone, Debug, Default)]
pub enum SdeStatus {
    #[default]
    NotReady,
    Downloading(String),
    Ready {
        systems: i64,
        regions: i64,
        version: String,
    },
    Failed(String),
}

pub type SharedStatus = Arc<Mutex<SdeStatus>>;

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
            Ok((systems, regions, version)) => set(SdeStatus::Ready {
                systems,
                regions,
                version,
            }),
            Err(e) => set(SdeStatus::Failed(format!("{e:#}"))),
        }
    });
}

fn run(path: &PathBuf, set: &impl Fn(SdeStatus)) -> Result<(i64, i64, String)> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("eve-spai/0.1 (EVE intel tool)")
        .timeout(std::time::Duration::from_secs(120))
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

    let regions: i64 = tx.query_row("SELECT COUNT(*) FROM sde_regions", [], |r| r.get(0))?;
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
    Ok((systems, regions, version))
}
