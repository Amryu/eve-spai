//! Local persistence (SQLite via rusqlite, bundled — no system dependency).
//!
//! Holds settings (key/value) and the baked EVE static data (SDE) tables
//! (docs/DESIGN.md §8).

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::settings::Settings;

/// Bump when the SDE schema/content changes, to force a re-download + re-bake.
pub const SDE_SCHEMA_VERSION: &str = "7";

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
CREATE TABLE IF NOT EXISTS sde_ship_traits (
    ship_id  INTEGER,
    skill_id INTEGER,
    bonus    REAL,
    text     TEXT
);
CREATE INDEX IF NOT EXISTS idx_sde_traits_ship ON sde_ship_traits(ship_id);
CREATE TABLE IF NOT EXISTS known_pilots (
    name_lc   TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    char_id   INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sde_ship_i18n (
    ship_id INTEGER,
    name    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sde_ship_i18n ON sde_ship_i18n(ship_id);
CREATE TABLE IF NOT EXISTS characters (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    expires_at INTEGER,
    scopes     TEXT
);
CREATE TABLE IF NOT EXISTS pings (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    ts   INTEGER NOT NULL,
    json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pings_ts ON pings(ts);
CREATE TABLE IF NOT EXISTS chats (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    jid      TEXT NOT NULL,
    sender   TEXT NOT NULL,
    body     TEXT NOT NULL,
    time     INTEGER NOT NULL,
    outgoing INTEGER NOT NULL,
    UNIQUE(jid, time, sender, body)
);
CREATE INDEX IF NOT EXISTS idx_chats_jid ON chats(jid, time);
CREATE TABLE IF NOT EXISTS wormholes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    dedup           TEXT NOT NULL UNIQUE,
    system_id       INTEGER NOT NULL,
    signature       TEXT,
    wh_type         TEXT,
    dest_class      TEXT NOT NULL,
    dest_system_id  INTEGER,
    dest_signature  TEXT,
    dest_wh_type    TEXT,
    size            TEXT,
    is_drifter      INTEGER NOT NULL DEFAULT 0,
    reported_at     INTEGER NOT NULL,
    explicit_expiry INTEGER,
    source          TEXT NOT NULL,
    updated_at      INTEGER NOT NULL
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
    /// Warp speed in AU/s.
    pub warp_speed: f64,
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
        let _ = conn.execute("ALTER TABLE wormholes ADD COLUMN dest_signature TEXT", []);
        let _ = conn.execute("ALTER TABLE wormholes ADD COLUMN dest_wh_type TEXT", []);
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
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(i64, i64, String, f64)> = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name, security FROM sde_systems") {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))
            }) {
                for (id, name, sec) in rows.flatten() {
                    if let Some(sc) = fuzzy_score(&name.to_lowercase(), &q) {
                        scored.push((sc, id, name, sec));
                    }
                }
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        scored.into_iter().take(limit as usize).map(|(_, id, n, sec)| (id, n, sec)).collect()
    }

    /// Region name search (id, name).
    pub fn search_regions(&self, query: &str, limit: i64) -> Vec<(i64, String)> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let q = q.to_lowercase();
        let mut scored: Vec<(i64, i64, String)> = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name FROM sde_regions") {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            {
                for (id, name) in rows.flatten() {
                    if let Some(sc) = fuzzy_score(&name.to_lowercase(), &q) {
                        scored.push((sc, id, name));
                    }
                }
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        scored.into_iter().take(limit as usize).map(|(_, id, n)| (id, n)).collect()
    }

    /// Constellation name search (constellation id, name, its region id).
    pub fn search_constellations(&self, query: &str, limit: i64) -> Vec<(i64, String, i64)> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let q = q.to_lowercase();
        let mut scored: Vec<(i64, i64, String, i64)> = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT c.id, c.name,
                    (SELECT region_id FROM sde_systems WHERE constellation_id = c.id LIMIT 1)
             FROM sde_constellations c",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0)))
            }) {
                for (id, name, region) in rows.flatten() {
                    if let Some(sc) = fuzzy_score(&name.to_lowercase(), &q) {
                        scored.push((sc, id, name, region));
                    }
                }
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        scored.into_iter().take(limit as usize).map(|(_, id, n, reg)| (id, n, reg)).collect()
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

    /// Systems in a constellation.
    pub fn constellation_systems(&self, cid: i64) -> Vec<MapSystem> {
        self.map_systems("WHERE constellation_id = ?1", params![cid])
    }

    fn name_of(&self, sql: &str, id: i64) -> Option<String> {
        self.conn.query_row(sql, params![id], |r| r.get(0)).ok()
    }

    pub fn region_name(&self, id: i64) -> Option<String> {
        self.name_of("SELECT name FROM sde_regions WHERE id = ?1", id)
    }

    pub fn constellation_name(&self, id: i64) -> Option<String> {
        self.name_of("SELECT name FROM sde_constellations WHERE id = ?1", id)
    }

    /// The constellation (id, name) a system belongs to.
    pub fn constellation_of_system(&self, id: i64) -> Option<(i64, String)> {
        self.conn
            .query_row(
                "SELECT c.id, c.name FROM sde_systems s \
                 JOIN sde_constellations c ON c.id = s.constellation_id WHERE s.id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()
    }

    /// The region a constellation belongs to.
    pub fn region_of_constellation(&self, cid: i64) -> Option<i64> {
        self.conn
            .query_row(
                "SELECT region_id FROM sde_systems WHERE constellation_id = ?1 LIMIT 1",
                params![cid],
                |r| r.get(0),
            )
            .ok()
    }

    fn id_name_list(&self, sql: &str, id: i64) -> Vec<(i64, String)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(sql) {
            if let Ok(rows) = stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?))) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// Constellations within a region.
    pub fn constellations_in_region(&self, rid: i64) -> Vec<(i64, String)> {
        self.id_name_list(
            "SELECT DISTINCT c.id, c.name FROM sde_systems s \
             JOIN sde_constellations c ON c.id = s.constellation_id \
             WHERE s.region_id = ?1 ORDER BY c.name",
            rid,
        )
    }

    /// Constellations gate-adjacent to this one.
    pub fn constellation_neighbours(&self, cid: i64) -> Vec<(i64, String)> {
        self.id_name_list(
            "SELECT DISTINCT c.id, c.name FROM sde_jumps j \
             JOIN sde_systems a ON a.id = j.from_id \
             JOIN sde_systems b ON b.id = j.to_id \
             JOIN sde_constellations c ON c.id = b.constellation_id \
             WHERE a.constellation_id = ?1 AND b.constellation_id <> ?1 ORDER BY c.name",
            cid,
        )
    }

    /// Regions gate-adjacent to this one.
    pub fn region_neighbours(&self, rid: i64) -> Vec<(i64, String)> {
        self.id_name_list(
            "SELECT DISTINCT r.id, r.name FROM sde_jumps j \
             JOIN sde_systems a ON a.id = j.from_id \
             JOIN sde_systems b ON b.id = j.to_id \
             JOIN sde_regions r ON r.id = b.region_id \
             WHERE a.region_id = ?1 AND b.region_id <> ?1 ORDER BY r.name",
            rid,
        )
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
        // Localized hull names (zh/ru/…) as exact-match keys.
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT t.name, s.id, s.name FROM sde_ship_i18n t JOIN sde_ships s ON s.id = t.ship_id",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
            }) {
                for (loc, id, en) in rows.flatten() {
                    map.entry(loc.to_lowercase()).or_insert((id, en));
                }
            }
        }
        map
    }

    /// All known (ESI-confirmed) pilot names → character id, keyed lower-case.
    pub fn known_pilots(&self) -> std::collections::HashMap<String, i64> {
        let mut out = std::collections::HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT name_lc, char_id FROM known_pilots WHERE char_id != 0") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// Multi-word spans confirmed by ESI to NOT be characters (stored with char_id 0),
    /// so the over-glued-name cover doesn't re-block on them after a restart.
    pub fn known_negatives(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT name_lc FROM known_pilots WHERE char_id = 0") {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// Remember an ESI-confirmed pilot (char_id > 0) or non-name (char_id 0).
    pub fn add_known_pilot(&self, name: &str, char_id: i64) {
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO known_pilots(name_lc, name, char_id) VALUES(?1, ?2, ?3)",
            params![name.to_lowercase(), name, char_id],
        );
    }

    // --- Fleet pings (persisted indefinitely) ------------------------------

    /// Persist a parsed ping (serialised JSON).
    pub fn add_ping(&self, ts: i64, json: &str) {
        let _ = self
            .conn
            .execute("INSERT INTO pings(ts, json) VALUES(?1, ?2)", params![ts, json]);
    }

    /// Load the most recent `limit` pings (oldest first, for display order).
    pub fn load_pings(&self, limit: i64) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT json FROM (SELECT id, json FROM pings ORDER BY ts DESC, id DESC LIMIT ?1)
             ORDER BY id ASC",
        ) {
            if let Ok(rows) = stmt.query_map(params![limit], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    // --- Conversations (persisted, de-duplicated) --------------------------

    /// Delete all stored messages for a conversation (e.g. an invalid-JID DM).
    pub fn delete_chat_jid(&self, jid: &str) {
        let _ = self.conn.execute("DELETE FROM chats WHERE jid = ?1", params![jid]);
    }

    /// Persist one chat message (de-duplicated by jid+time+sender+body).
    pub fn add_chat(&self, jid: &str, sender: &str, body: &str, time: i64, outgoing: bool) {
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO chats(jid, sender, body, time, outgoing) VALUES(?1,?2,?3,?4,?5)",
            params![jid, sender, body, time, outgoing as i64],
        );
    }

    /// Load the most recent `limit` messages (oldest first): (jid, sender, body, time, outgoing).
    pub fn load_chats(&self, limit: i64) -> Vec<(String, String, String, i64, bool)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT jid, sender, body, time, outgoing FROM
                (SELECT * FROM chats ORDER BY time DESC, id DESC LIMIT ?1)
             ORDER BY time ASC, id ASC",
        ) {
            if let Ok(rows) = stmt.query_map(params![limit], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    // --- Wormholes ---------------------------------------------------------

    const WH_COLS: &'static str = "id, system_id, signature, wh_type, dest_class,
        dest_system_id, dest_signature, dest_wh_type, size, is_drifter, reported_at,
        explicit_expiry, source, updated_at";

    /// Insert a wormhole connection, or merge a report into the existing connection.
    /// A signature that matches *either* endpoint of a known connection pairs with it
    /// (so a hole reported from both sides is one connection, not two); otherwise the
    /// (system, type, dest) dedup key is used. Returns the row id.
    pub fn upsert_wormhole(&self, incoming: &crate::wormholes::Wormhole) -> i64 {
        // Signature-based pairing against either endpoint.
        if let Some(sig) = incoming.signature.as_deref().filter(|s| !s.is_empty()) {
            if let Some(mut near) = self.wormhole_where(
                "system_id=?1 AND signature=?2",
                params![incoming.system_id, sig],
            ) {
                near.merge_from(incoming);
                self.write_wormhole(&near);
                return near.id;
            }
            if let Some(mut owner) = self.wormhole_where(
                "dest_system_id=?1 AND dest_signature=?2",
                params![incoming.system_id, sig],
            ) {
                // Incoming's near side IS this connection's far side → confirm it.
                owner.confirm_far(incoming);
                self.write_wormhole(&owner);
                return owner.id;
            }
        }
        let key = incoming.dedup_key();
        if let Some(mut existing) = self.wormhole_where("dedup=?1", params![key]) {
            existing.merge_from(incoming);
            self.write_wormhole(&existing);
            return existing.id;
        }
        let _ = self.conn.execute(
            "INSERT INTO wormholes(dedup, system_id, signature, wh_type, dest_class,
                dest_system_id, dest_signature, dest_wh_type, size, is_drifter, reported_at,
                explicit_expiry, source, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                key, incoming.system_id, incoming.signature, incoming.wh_type,
                incoming.dest.code(), incoming.dest_system_id, incoming.dest_signature,
                incoming.dest_wh_type, incoming.size.map(|s| s.code()), incoming.is_drifter as i64,
                incoming.reported_at, incoming.explicit_expiry, incoming.source.code(),
                incoming.updated_at,
            ],
        );
        self.conn.last_insert_rowid()
    }

    fn write_wormhole(&self, w: &crate::wormholes::Wormhole) {
        let _ = self.conn.execute(
            "UPDATE wormholes SET system_id=?2, signature=?3, wh_type=?4, dest_class=?5,
                dest_system_id=?6, dest_signature=?7, dest_wh_type=?8, size=?9, is_drifter=?10,
                reported_at=?11, explicit_expiry=?12, source=?13, updated_at=?14 WHERE id=?1",
            params![
                w.id, w.system_id, w.signature, w.wh_type, w.dest.code(), w.dest_system_id,
                w.dest_signature, w.dest_wh_type, w.size.map(|s| s.code()), w.is_drifter as i64,
                w.reported_at, w.explicit_expiry, w.source.code(), w.updated_at,
            ],
        );
    }

    fn wormhole_where(
        &self,
        cond: &str,
        params: impl rusqlite::Params,
    ) -> Option<crate::wormholes::Wormhole> {
        self.conn
            .query_row(
                &format!("SELECT {} FROM wormholes WHERE {cond}", Self::WH_COLS),
                params,
                Self::row_to_wormhole,
            )
            .ok()
    }

    /// All known wormholes (callers prune/filter by expiry as needed).
    pub fn wormholes(&self) -> Vec<crate::wormholes::Wormhole> {
        let mut out = Vec::new();
        if let Ok(mut stmt) =
            self.conn.prepare(&format!("SELECT {} FROM wormholes", Self::WH_COLS))
        {
            if let Ok(rows) = stmt.query_map([], Self::row_to_wormhole) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// Drop wormholes past their (explicit or derived) lifetime.
    pub fn prune_wormholes(&self, now: i64) {
        let _ = self.conn.execute(
            "DELETE FROM wormholes WHERE
                COALESCE(explicit_expiry, reported_at + (CASE WHEN is_drifter THEN 86400 ELSE 172800 END)) <= ?1",
            params![now],
        );
    }

    fn row_to_wormhole(row: &rusqlite::Row) -> rusqlite::Result<crate::wormholes::Wormhole> {
        use crate::wormholes::{DestClass, ShipSize, Source, Wormhole};
        let dest_code: String = row.get(4)?;
        let size_code: Option<String> = row.get(8)?;
        let source_code: String = row.get(12)?;
        Ok(Wormhole {
            id: row.get(0)?,
            system_id: row.get(1)?,
            signature: row.get(2)?,
            wh_type: row.get(3)?,
            dest: DestClass::from_code(&dest_code),
            dest_system_id: row.get(5)?,
            dest_signature: row.get(6)?,
            dest_wh_type: row.get(7)?,
            size: size_code.and_then(|c| ShipSize::from_code(&c)),
            is_drifter: row.get::<_, i64>(9)? != 0,
            reported_at: row.get(10)?,
            explicit_expiry: row.get(11)?,
            source: Source::from_code(&source_code),
            updated_at: row.get(13)?,
        })
    }

    /// Whether ship traits (role bonuses) have been baked.
    pub fn traits_baked(&self) -> bool {
        self.conn
            .query_row("SELECT COUNT(*) FROM sde_ship_traits", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            > 0
    }

    /// Role bonuses for a ship (skill_id, bonus value, text). skill_id -1 = role.
    pub fn ship_traits(&self, id: i64) -> Vec<(i64, f64, String)> {
        let mut out = Vec::new();
        // Natural SDE order (specialized skills first; role bonuses placed last in UI).
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT skill_id, bonus, text FROM sde_ship_traits WHERE ship_id = ?1 ORDER BY rowid",
        ) {
            if let Ok(rows) =
                stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            {
                out.extend(rows.flatten());
            }
        }
        out
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
            // warpSpeedMultiplier (600) × baseWarpSpeed (1281, default 1) = AU/s, plus
            // any always-on role bonus to warp speed from the hull's traits.
            warp_speed: {
                let base = val(1281);
                let mult = val(600);
                let raw = if base > 0.0 { base * mult } else { mult };
                let role: f64 = self
                    .ship_traits(id)
                    .iter()
                    .filter(|(_, _, t)| t.to_lowercase().contains("warp speed"))
                    .map(|(_, b, _)| b)
                    .sum();
                raw * (1.0 + role / 100.0)
            },
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

/// Fuzzy name match score (higher = better) or None. exact > prefix > substring > trigram.
fn fuzzy_score(name_lc: &str, q: &str) -> Option<i64> {
    if name_lc == q {
        return Some(10_000);
    }
    if name_lc.starts_with(q) {
        return Some(5_000 - name_lc.len() as i64);
    }
    if let Some(pos) = name_lc.find(q) {
        return Some(2_000 - pos as i64 - name_lc.len() as i64);
    }
    // Trigram overlap for typo tolerance: fraction of the query's trigrams in the name.
    let qt = trigrams(q);
    if qt.is_empty() {
        return None;
    }
    let nt = trigrams(name_lc);
    let shared = qt.iter().filter(|t| nt.contains(*t)).count();
    let frac = shared as f64 / qt.len() as f64;
    (frac >= 0.5).then(|| (frac * 1_000.0) as i64 - name_lc.len() as i64)
}

/// Boundary-padded 3-grams of a lower-cased string.
fn trigrams(s: &str) -> std::collections::HashSet<[u8; 3]> {
    let padded = format!("  {s} ");
    let b = padded.as_bytes();
    let mut set = std::collections::HashSet::new();
    for w in b.windows(3) {
        set.insert([w[0], w[1], w[2]]);
    }
    set
}
