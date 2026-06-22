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
CREATE TABLE IF NOT EXISTS characters (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    expires_at INTEGER,
    scopes     TEXT
);
";

/// A solar system row for lookups/UI.
#[derive(Clone, Debug)]
pub struct SystemRow {
    pub name: String,
    pub security: f64,
    pub region: String,
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

    /// Load the system graph (names + jump adjacency) for the intel parser.
    pub fn load_systems(&self) -> crate::geo::Systems {
        use std::collections::HashMap;

        let mut by_name: HashMap<String, crate::geo::SystemInfo> = HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name, security FROM sde_systems") {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))
            }) {
                for (id, name, security) in rows.flatten() {
                    by_name.insert(
                        name.to_lowercase(),
                        crate::geo::SystemInfo { id, name, security },
                    );
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
