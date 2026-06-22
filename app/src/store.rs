//! Local persistence (SQLite via rusqlite, bundled — no system dependency).
//!
//! Holds settings (key/value) and the baked EVE static data (SDE) tables
//! (docs/DESIGN.md §8).

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::settings::Settings;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sde_regions (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sde_systems (
    id        INTEGER PRIMARY KEY,
    name      TEXT NOT NULL,
    region_id INTEGER,
    security  REAL,
    x REAL, y REAL, z REAL
);
CREATE INDEX IF NOT EXISTS idx_sde_systems_name ON sde_systems(name);
CREATE TABLE IF NOT EXISTS sde_jumps (from_id INTEGER, to_id INTEGER);
CREATE INDEX IF NOT EXISTS idx_sde_jumps_from ON sde_jumps(from_id);
CREATE TABLE IF NOT EXISTS sde_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

/// A solar system row for lookups/UI.
#[derive(Clone, Debug)]
pub struct SystemRow {
    pub name: String,
    pub security: f64,
    pub region: String,
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

    /// Returns `(systems, regions, version)` if the SDE has been baked, else None.
    pub fn sde_summary(&self) -> Option<(i64, i64, String)> {
        let systems: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM sde_systems", [], |r| r.get(0))
            .ok()?;
        if systems == 0 {
            return None;
        }
        let regions: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM sde_regions", [], |r| r.get(0))
            .unwrap_or(0);
        let version: String = self
            .conn
            .query_row("SELECT value FROM sde_meta WHERE key = 'version'", [], |r| {
                r.get(0)
            })
            .unwrap_or_default();
        Some((systems, regions, version))
    }

    /// Case-insensitive prefix search over system names.
    pub fn find_systems(&self, query: &str, limit: i64) -> Vec<SystemRow> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let pattern = format!("{q}%");
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT s.name, s.security, COALESCE(r.name, '')
             FROM sde_systems s LEFT JOIN sde_regions r ON r.id = s.region_id
             WHERE s.name LIKE ?1 ORDER BY s.name LIMIT ?2",
        ) {
            if let Ok(rows) = stmt.query_map(params![pattern, limit], |row| {
                Ok(SystemRow {
                    name: row.get(0)?,
                    security: row.get(1)?,
                    region: row.get(2)?,
                })
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }
}

fn data_dir() -> Result<PathBuf> {
    let pd = directories::ProjectDirs::from("online", "EveSpai", "eve-spai")
        .ok_or_else(|| anyhow!("could not resolve a data directory"))?;
    Ok(pd.data_dir().to_path_buf())
}
