//! Local persistence (SQLite via rusqlite, bundled — no system dependency).
//!
//! Holds settings (key/value) and the baked EVE static data (SDE) tables
//! (docs/DESIGN.md §8).

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::settings::Settings;

/// Bump when the SDE schema/content changes, to force a re-download + re-bake.
pub const SDE_SCHEMA_VERSION: &str = "5";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sde_regions (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sde_constellations (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sde_systems (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    region_id        INTEGER,
    constellation_id INTEGER,
    faction_id       INTEGER,
    security         REAL,
    x REAL, y REAL, z REAL,
    x2d REAL, z2d REAL
);
CREATE INDEX IF NOT EXISTS idx_sde_systems_name ON sde_systems(name);
CREATE TABLE IF NOT EXISTS sde_jumps (from_id INTEGER, to_id INTEGER);
CREATE INDEX IF NOT EXISTS idx_sde_jumps_from ON sde_jumps(from_id);
CREATE TABLE IF NOT EXISTS sde_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sde_ships (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    group_name TEXT,
    mass       REAL,
    volume     REAL
);
CREATE INDEX IF NOT EXISTS idx_sde_ships_name ON sde_ships(name);
CREATE TABLE IF NOT EXISTS sde_ship_attrs (
    ship_id INTEGER,
    attr_id INTEGER,
    value   REAL,
    PRIMARY KEY (ship_id, attr_id)
);
CREATE TABLE IF NOT EXISTS characters (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    expires_at INTEGER,
    scopes     TEXT
);
";

/// A ship type with computed resist/tank/fitting stats.
#[derive(Clone, Debug)]
pub struct ShipDetails {
    pub name: String,
    pub group: String,
    /// Resist % in EVE display order: em, thermal, kinetic, explosive.
    pub shield_resist: [u32; 4],
    pub armor_resist: [u32; 4],
    pub hull_resist: [u32; 4],
    pub shield_hp: f64,
    pub armor_hp: f64,
    pub hull_hp: f64,
    pub drone_cap: f64,
    pub drone_bw: f64,
    pub turret_hardpoints: i64,
    pub launcher_hardpoints: i64,
    pub high_slots: i64,
    pub mid_slots: i64,
    pub low_slots: i64,
    pub max_velocity: f64,
}

/// A solar system with map coordinates.
#[derive(Clone, Debug)]
pub struct MapSystem {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub region_id: i64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// EVE's precomputed 2D "schematic" map position (in-game flattened layout).
    pub x2d: f64,
    pub z2d: f64,
}

/// A stored, SSO-authenticated character.
#[derive(Clone, Debug)]
pub struct CharacterRow {
    pub id: i64,
    pub name: String,
    pub expires_at: i64,
    pub scopes: String,
}

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl Store {
    pub fn open() -> Result<Self> {
        let dir = data_dir()?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("eve-spai.db");
        let conn = Connection::open(&path)?;
        conn.execute_batch(SCHEMA)?;
        // Add columns to pre-existing SDE tables (no-op if already there).
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN constellation_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN faction_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN x2d REAL", []);
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN z2d REAL", []);
        migrate_plaintext_tokens(&conn);
        Ok(Self { conn, path })
    }

    /// Path to the DB file (so background workers can open their own connection).
    pub fn path(&self) -> &Path {
        &self.path
    }

    // --- Settings ---

    pub fn load_settings(&self) -> Option<Settings> {
        let json: String = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'settings'", [], |r| {
                r.get(0)
            })
            .ok()?;
        serde_json::from_str(&json).ok()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![json],
        )?;
        Ok(())
    }

    // --- SDE ---

    /// True when the SDE is baked at the current schema version.
    pub fn sde_ready(&self) -> bool {
        let systems: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM sde_systems", [], |r| r.get(0))
            .unwrap_or(0);
        let schema: String = self
            .conn
            .query_row("SELECT value FROM sde_meta WHERE key = 'schema'", [], |r| r.get(0))
            .unwrap_or_default();
        systems > 0 && schema == SDE_SCHEMA_VERSION
    }

    /// System name search for the map (id, name, security).
    pub fn search_systems(&self, query: &str, limit: i64) -> Vec<(i64, String, f64)> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let pattern = format!("{q}%");
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT id, name, security FROM sde_systems WHERE name LIKE ?1 ORDER BY name LIMIT ?2",
        ) {
            if let Ok(rows) =
                stmt.query_map(params![pattern, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// Regions (id, name) for the map picker.
    pub fn regions(&self) -> Vec<(i64, String)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name FROM sde_regions ORDER BY name") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// The region a system belongs to.
    pub fn region_of_system(&self, id: i64) -> Option<i64> {
        self.conn
            .query_row("SELECT region_id FROM sde_systems WHERE id = ?1", params![id], |r| r.get(0))
            .ok()
    }

    /// Systems in a region with map coordinates (EVE x/z plane, top-down).
    pub fn region_systems(&self, region_id: i64) -> Vec<MapSystem> {
        self.map_systems("WHERE region_id = ?1", params![region_id])
    }

    /// All systems with map coordinates (universe view).
    pub fn all_map_systems(&self) -> Vec<MapSystem> {
        self.map_systems("", params![])
    }

    fn map_systems(&self, filter: &str, p: impl rusqlite::Params) -> Vec<MapSystem> {
        // Fall back to 3D x/z when a system has no 2D position (rare; filtered out).
        let sql = format!(
            "SELECT id, name, security, COALESCE(region_id,0), x, y, z, \
             COALESCE(x2d, x), COALESCE(z2d, z) FROM sde_systems {filter}"
        );
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(p, |r| {
                Ok(MapSystem {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    security: r.get(2)?,
                    region_id: r.get(3)?,
                    x: r.get(4)?,
                    y: r.get(5)?,
                    z: r.get(6)?,
                    x2d: r.get(7)?,
                    z2d: r.get(8)?,
                })
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// Lower-cased ship-name index for the intel parser (name -> (id, canonical)).
    pub fn ship_index(&self) -> std::collections::HashMap<String, (i64, String)> {
        let mut map = std::collections::HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name FROM sde_ships") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
                for (id, name) in rows.flatten() {
                    map.insert(name.to_lowercase(), (id, name));
                }
            }
        }
        // Nicknames / abbreviations / acronyms (e.g. "vaga", "cfi") as extra keys.
        for (slug, entry) in crate::shipnames::aliases(&map) {
            map.entry(slug).or_insert(entry);
        }
        map
    }

    /// Computed ship details (resists, hp, drones, hardpoints, speed).
    pub fn ship_details(&self, id: i64) -> Option<ShipDetails> {
        let (name, group): (String, String) = self
            .conn
            .query_row(
                "SELECT name, COALESCE(group_name,'') FROM sde_ships WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()?;

        let mut attr: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT attr_id, value FROM sde_ship_attrs WHERE ship_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))) {
                for (a, v) in rows.flatten() {
                    attr.insert(a, v);
                }
            }
        }
        // resist% = round((1 - resonance) * 100); ids in em, therm, kin, exp order.
        let resist = |ids: [i64; 4]| -> [u32; 4] {
            ids.map(|a| {
                let resonance = attr.get(&a).copied().unwrap_or(1.0);
                ((1.0 - resonance) * 100.0).round().clamp(0.0, 100.0) as u32
            })
        };
        let val = |a: i64| attr.get(&a).copied().unwrap_or(0.0);

        Some(ShipDetails {
            name,
            group,
            shield_resist: resist([271, 274, 273, 272]),
            armor_resist: resist([267, 270, 269, 268]),
            hull_resist: resist([113, 110, 109, 111]),
            shield_hp: val(263),
            armor_hp: val(265),
            hull_hp: val(9),
            drone_cap: val(283),
            drone_bw: val(1271),
            turret_hardpoints: val(102) as i64,
            launcher_hardpoints: val(101) as i64,
            high_slots: val(14) as i64,
            mid_slots: val(13) as i64,
            low_slots: val(12) as i64,
            max_velocity: val(37),
        })
    }

    /// Load the system graph (names + jump adjacency) for the intel parser.
    pub fn load_systems(&self) -> crate::geo::Systems {
        use std::collections::HashMap;

        let mut by_name: HashMap<String, crate::geo::SystemInfo> = HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT s.id, s.name, s.security, COALESCE(c.name,''), COALESCE(r.name,''), COALESCE(s.faction_id,0)
             FROM sde_systems s
             LEFT JOIN sde_constellations c ON c.id = s.constellation_id
             LEFT JOIN sde_regions r ON r.id = s.region_id",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok(crate::geo::SystemInfo {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    security: r.get(2)?,
                    constellation: r.get(3)?,
                    region: r.get(4)?,
                    faction: crate::factions::name(r.get::<_, i64>(5)?).to_owned(),
                })
            }) {
                for info in rows.flatten() {
                    by_name.insert(info.name.to_lowercase(), info);
                }
            }
        }

        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT from_id, to_id FROM sde_jumps") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
                for (a, b) in rows.flatten() {
                    adjacency.entry(a).or_default().push(b);
                }
            }
        }

        crate::geo::Systems::new(by_name, adjacency)
    }

    // --- Characters ---

    pub fn list_characters(&self) -> Vec<CharacterRow> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT id, name, COALESCE(expires_at, 0), COALESCE(scopes, '')
             FROM characters ORDER BY name",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok(CharacterRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    expires_at: row.get(2)?,
                    scopes: row.get(3)?,
                })
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn character_by_name(&self, name: &str) -> Option<CharacterRow> {
        self.list_characters().into_iter().find(|c| c.name == name)
    }

    pub fn update_token_expiry(&self, id: i64, expires_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE characters SET expires_at = ?1 WHERE id = ?2",
            params![expires_at, id],
        )?;
        Ok(())
    }

    pub fn remove_character(&self, id: i64) -> Result<()> {
        let _ = crate::tokens::delete(id);
        self.conn
            .execute("DELETE FROM characters WHERE id = ?1", params![id])?;
        Ok(())
    }
}

/// One-time migration: if an older DB stored tokens in plaintext columns, move them
/// into the keychain and drop the columns (scrubbing the DB file with VACUUM).
fn migrate_plaintext_tokens(conn: &Connection) {
    let has_legacy = conn
        .prepare("PRAGMA table_info(characters)")
        .and_then(|mut stmt| {
            let cols: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(cols.iter().any(|c| c == "refresh_token"))
        })
        .unwrap_or(false);
    if !has_legacy {
        return;
    }

    let rows: Vec<(i64, Option<String>, Option<String>)> = conn
        .prepare("SELECT id, refresh_token, access_token FROM characters")
        .and_then(|mut stmt| {
            let v = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(v)
        })
        .unwrap_or_default();

    let mut all_migrated = true;
    for (id, refresh, access) in rows {
        if let Some(refresh) = refresh.filter(|s| !s.is_empty()) {
            let tokens = crate::tokens::Tokens {
                refresh_token: refresh,
                access_token: access.unwrap_or_default(),
            };
            if let Err(e) = crate::tokens::save(id, &tokens) {
                eprintln!("keychain migration failed for character {id}: {e:#}");
                all_migrated = false;
            }
        }
    }

    // Only remove the plaintext once everything is safely in the keychain.
    if all_migrated {
        let _ = conn.execute("ALTER TABLE characters DROP COLUMN access_token", []);
        let _ = conn.execute("ALTER TABLE characters DROP COLUMN refresh_token", []);
        let _ = conn.execute("VACUUM", []);
    }
}

fn data_dir() -> Result<PathBuf> {
    let pd = directories::ProjectDirs::from("online", "EveSpai", "eve-spai")
        .ok_or_else(|| anyhow!("could not resolve a data directory"))?;
    Ok(pd.data_dir().to_path_buf())
}
