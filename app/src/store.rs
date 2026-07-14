use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::settings::Settings;

/// Bump when the SDE schema/content changes, to force a re-download + re-bake.
pub const SDE_SCHEMA_VERSION: &str = "9";

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
-- Stargate positions per system (for on-gate kill detection).
CREATE TABLE IF NOT EXISTS sde_stargates (system_id INTEGER, x REAL, y REAL, z REAL);
CREATE INDEX IF NOT EXISTS idx_sde_stargates_sys ON sde_stargates(system_id);
-- Named celestials (planets, moons, stations, gates) per system, for the
-- \"kill happened near <celestial>\" card. name is the display label.
CREATE TABLE IF NOT EXISTS sde_celestials (system_id INTEGER, name TEXT, x REAL, y REAL, z REAL);
CREATE INDEX IF NOT EXISTS idx_sde_celestials_sys ON sde_celestials(system_id);
-- Camp-relevant type ids by kind ('dic'|'hic'|'smartbomb'|'bubble'), for camp signals.
CREATE TABLE IF NOT EXISTS sde_camp_types (id INTEGER PRIMARY KEY, kind TEXT NOT NULL);
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
    updated_at      INTEGER NOT NULL,
    dead            INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS kill_intel (
    killmail_id  INTEGER PRIMARY KEY,
    system_id    INTEGER NOT NULL,
    ship_type_id INTEGER NOT NULL,
    time         INTEGER NOT NULL,
    value        REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_kill_intel_time ON kill_intel(time);
-- Battle engagements (one killmail each), persisted so clustered battles survive a restart.
-- The full Engagement is kept as JSON; the columns are for windowed load + prune.
CREATE TABLE IF NOT EXISTS engagements (
    kill_id   INTEGER PRIMARY KEY,
    time      INTEGER NOT NULL,
    system_id INTEGER NOT NULL,
    json      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_engagements_time ON engagements(time);
-- User overrides for battle reports: per-kill group re-tag / exclusion, persisted so a
-- manually corrected battle stays corrected across restarts.
CREATE TABLE IF NOT EXISTS battle_overrides (
    kill_id   INTEGER PRIMARY KEY,
    group_tag INTEGER,
    excluded  INTEGER NOT NULL DEFAULT 0
);
-- Per-kill characters marked as scrubs (non-combatants / pod-only) in a battle.
CREATE TABLE IF NOT EXISTS battle_scrubs (
    kill_id INTEGER NOT NULL,
    char_id INTEGER NOT NULL,
    PRIMARY KEY (kill_id, char_id)
);
-- Enriched killmail details (zKill + ESI), so a reloaded card doesn't re-fetch them.
CREATE TABLE IF NOT EXISTS kill_details (
    kill_id             INTEGER PRIMARY KEY,
    hash                TEXT,
    victim_char         INTEGER,
    victim_ship         INTEGER,
    victim_corp         INTEGER,
    victim_alliance     INTEGER,
    system_id           INTEGER NOT NULL,
    value               REAL NOT NULL,
    time                TEXT NOT NULL,
    final_blow_char     INTEGER,
    final_blow_corp     INTEGER,
    final_blow_alliance INTEGER,
    final_blow_ship     INTEGER,
    attacker_count      INTEGER NOT NULL,
    attacker_alliances  TEXT,
    near_name           TEXT,
    near_dist           REAL
);
-- Per-character zKill-activity + account-age cache (4h TTL on active_recent; birthday
-- is fetched once). Persisted so a restart doesn't re-storm zKill. Consumed in Phase 2.
CREATE TABLE IF NOT EXISTS pilot_activity (
    char_id          INTEGER PRIMARY KEY,
    active_recent    INTEGER NOT NULL,
    birthday         INTEGER,
    last_corp_change INTEGER,
    fetched_at       INTEGER NOT NULL
);
-- Per-pilot revival expiry (Phase 2): a pilot revived by wide roaming (or that is still
-- being mentioned) stays kept until this instant, refreshed on every fresh intel mention.
-- Name lower-cased.
CREATE TABLE IF NOT EXISTS pilot_revival (
    name          TEXT PRIMARY KEY,
    revived_until INTEGER NOT NULL
);
-- The user's manual verdict on an activity-flagged pilot: hidden=1 means 'not a pilot' (hide +
-- free its tokens), hidden=0 means 'real' (always show, clears the uncertainty).
CREATE TABLE IF NOT EXISTS pilot_verdict (
    name_lc TEXT PRIMARY KEY,
    hidden  INTEGER NOT NULL
);
";

#[derive(Clone, Debug)]
pub struct ShipDetails {
    pub name: String,
    pub group: String,
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
    pub warp_speed: f64,
}

#[derive(Clone, Debug)]
pub struct MapSystem {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub region_id: i64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub x2d: f64,
    pub z2d: f64,
}

#[derive(Clone, Debug)]
pub struct CharacterRow {
    pub id: i64,
    pub name: String,
    pub expires_at: i64,
    pub scopes: String,
}

struct SysRow {
    id: i64,
    name: String,
    lower: String,
    sec: f64,
    tri: std::collections::HashSet<[u8; 3]>,
}

struct PlaceCache {
    constellations: Vec<(i64, String, String, i64)>,
    regions: Vec<(i64, String, String)>,
}

pub struct Store {
    conn: Connection,
    path: PathBuf,
    sys_cache: std::cell::RefCell<Option<Vec<SysRow>>>,
    place_cache: std::cell::RefCell<Option<PlaceCache>>,
}

impl Store {
    pub fn open() -> Result<Self> {
        let dir = data_dir()?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("eve-spai.db");
        let conn = Connection::open(&path)?;
        apply_pragmas(&conn);
        conn.execute_batch(SCHEMA)?;
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN constellation_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN faction_id INTEGER", []);
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN x2d REAL", []);
        let _ = conn.execute("ALTER TABLE sde_systems ADD COLUMN z2d REAL", []);
        let _ = conn.execute("ALTER TABLE wormholes ADD COLUMN dest_signature TEXT", []);
        let _ = conn.execute("ALTER TABLE wormholes ADD COLUMN dest_wh_type TEXT", []);
        let _ = conn.execute("ALTER TABLE wormholes ADD COLUMN dead INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE pilot_activity ADD COLUMN last_corp_change INTEGER", []);
        let _ = conn.execute("ALTER TABLE kill_details ADD COLUMN near_name TEXT", []);
        let _ = conn.execute("ALTER TABLE kill_details ADD COLUMN near_dist REAL", []);
        // One-time: after the demotion-logic overhaul (90-day young-account grace, player-corp-change
        // signal, and a true-90-day activity window), wipe the persisted activity/demotion cache once
        // so every pilot is re-fetched and re-judged under the new rules instead of keeping a stale
        // "demoted" verdict (which was wrongly hiding real, recently-active pilots).
        let cleared: Option<String> = conn
            .query_row("SELECT value FROM kv WHERE key = 'activity_cache_reset_v2'", [], |r| r.get(0))
            .ok();
        if cleared.is_none() {
            let _ = conn.execute("DELETE FROM pilot_activity", []);
            let _ = conn.execute(
                "INSERT INTO kv (key, value) VALUES ('activity_cache_reset_v2', '1')
                 ON CONFLICT(key) DO UPDATE SET value = '1'",
                [],
            );
        }
        migrate_plaintext_tokens(&conn);
        Ok(Self {
            conn,
            path,
            sys_cache: std::cell::RefCell::new(None),
            place_cache: std::cell::RefCell::new(None),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_settings(&self) -> Option<Settings> {
        let json: String = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'settings'", [], |r| {
                r.get(0)
            })
            .ok()?;
        match serde_json::from_str(&json) {
            Ok(s) => Some(s),
            Err(e) => {
                // One unknown enum variant fails the whole parse; don't silently wipe the
                // user's config. Log it and stash the raw blob so the next save (with
                // defaults) doesn't overwrite the only copy.
                eprintln!("[settings] stored settings didn't parse, using defaults: {e}");
                let _ = self.conn.execute(
                    "INSERT INTO kv (key, value) VALUES ('settings.bad', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = ?1",
                    params![json],
                );
                None
            }
        }
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

    pub fn kv_get(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| r.get(0)).ok()
    }
    pub fn kv_set(&self, key: &str, value: &str) {
        let _ = self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        );
    }
    pub fn kv_delete(&self, key: &str) {
        let _ = self.conn.execute("DELETE FROM kv WHERE key = ?1", params![key]);
    }

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

    fn ensure_sys_cache(&self) {
        if self.sys_cache.borrow().is_some() {
            return;
        }
        let mut rows = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name, security FROM sde_systems") {
            if let Ok(qr) = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))
            }) {
                for (id, name, sec) in qr.flatten() {
                    let lower = name.to_lowercase();
                    let tri = trigrams(&lower);
                    rows.push(SysRow { id, name, lower, sec, tri });
                }
            }
        }
        *self.sys_cache.borrow_mut() = Some(rows);
    }

    fn ensure_place_cache(&self) {
        if self.place_cache.borrow().is_some() {
            return;
        }
        let mut cid_region: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        if let Ok(mut stmt) =
            self.conn.prepare("SELECT constellation_id, region_id FROM sde_systems")
        {
            if let Ok(qr) = stmt.query_map([], |r| {
                Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?))
            }) {
                for (cid, reg) in qr.flatten() {
                    if let (Some(cid), Some(reg)) = (cid, reg) {
                        cid_region.entry(cid).or_insert(reg);
                    }
                }
            }
        }
        let mut constellations = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name FROM sde_constellations") {
            if let Ok(qr) =
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            {
                for (id, name) in qr.flatten() {
                    let lower = name.to_lowercase();
                    let region = cid_region.get(&id).copied().unwrap_or(0);
                    constellations.push((id, name, lower, region));
                }
            }
        }
        let mut regions = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name FROM sde_regions") {
            if let Ok(qr) =
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            {
                for (id, name) in qr.flatten() {
                    let lower = name.to_lowercase();
                    regions.push((id, name, lower));
                }
            }
        }
        *self.place_cache.borrow_mut() = Some(PlaceCache { constellations, regions });
    }

    pub fn search_systems(&self, query: &str, limit: i64) -> Vec<(i64, String, f64)> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let qt = trigrams(&q);
        self.ensure_sys_cache();
        let cache = self.sys_cache.borrow();
        let rows = match cache.as_ref() {
            Some(r) => r,
            None => return Vec::new(),
        };
        let mut scored: Vec<(i64, i64, String, f64)> = Vec::new();
        for r in rows {
            if let Some(sc) = score_cached(&r.lower, &r.tri, &q, &qt) {
                scored.push((sc, r.id, r.name.clone(), r.sec));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        scored.into_iter().take(limit as usize).map(|(_, id, n, sec)| (id, n, sec)).collect()
    }

    pub fn search_regions(&self, query: &str, limit: i64) -> Vec<(i64, String)> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let q = q.to_lowercase();
        self.ensure_place_cache();
        let cache = self.place_cache.borrow();
        let Some(pc) = cache.as_ref() else { return Vec::new() };
        let mut scored: Vec<(i64, i64, String)> = Vec::new();
        for (id, name, lower) in &pc.regions {
            if let Some(sc) = fuzzy_score(lower, &q) {
                scored.push((sc, *id, name.clone()));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        scored.into_iter().take(limit as usize).map(|(_, id, n)| (id, n)).collect()
    }

    pub fn search_constellations(&self, query: &str, limit: i64) -> Vec<(i64, String, i64)> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let q = q.to_lowercase();
        self.ensure_place_cache();
        let cache = self.place_cache.borrow();
        let Some(pc) = cache.as_ref() else { return Vec::new() };
        let mut scored: Vec<(i64, i64, String, i64)> = Vec::new();
        for (id, name, lower, region) in &pc.constellations {
            if let Some(sc) = fuzzy_score(lower, &q) {
                scored.push((sc, *id, name.clone(), *region));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        scored.into_iter().take(limit as usize).map(|(_, id, n, reg)| (id, n, reg)).collect()
    }

    pub fn regions(&self) -> Vec<(i64, String)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name FROM sde_regions ORDER BY name") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    /// Every navigable k-space system as `(region name, constellation name, system name)`, ordered
    /// for tree building (region → constellation → system). Powers the alert-rule systems picker.
    /// Excludes the same space the map hides: wormholes + abyssal + the non-Pochven Triglavian
    /// regions (Yasna Zakh/Zarzakh, Exordium) via `region_id <= 10000070`, and the digit-named Jove
    /// regions via the name filter (mirrors `is_hidden_region`). Pochven (10000070) is kept.
    pub fn all_systems_geo(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT r.name, c.name, s.name
             FROM sde_systems s
             JOIN sde_constellations c ON c.id = s.constellation_id
             JOIN sde_regions r ON r.id = s.region_id
             WHERE s.region_id BETWEEN 10000001 AND 10000070
               AND r.name NOT GLOB '*[0-9]*'
             ORDER BY r.name, c.name, s.name",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn region_of_system(&self, id: i64) -> Option<i64> {
        self.conn
            .query_row("SELECT region_id FROM sde_systems WHERE id = ?1", params![id], |r| r.get(0))
            .ok()
    }

    pub fn region_systems(&self, region_id: i64) -> Vec<MapSystem> {
        self.map_systems("WHERE region_id = ?1", params![region_id])
    }

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

    pub fn constellations_in_region(&self, rid: i64) -> Vec<(i64, String)> {
        self.id_name_list(
            "SELECT DISTINCT c.id, c.name FROM sde_systems s \
             JOIN sde_constellations c ON c.id = s.constellation_id \
             WHERE s.region_id = ?1 ORDER BY c.name",
            rid,
        )
    }

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

    pub fn all_map_systems(&self) -> Vec<MapSystem> {
        self.map_systems("", params![])
    }

    fn map_systems(&self, filter: &str, p: impl rusqlite::Params) -> Vec<MapSystem> {
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

    pub fn ship_index(&self) -> std::collections::HashMap<String, (i64, String)> {
        let mut map = std::collections::HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, name FROM sde_ships") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
                for (id, name) in rows.flatten() {
                    map.insert(name.to_lowercase(), (id, name));
                }
            }
        }
        for (slug, entry) in crate::shipnames::aliases(&map) {
            map.entry(slug).or_insert(entry);
        }
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

    pub fn all_ships(&self) -> Vec<(i64, String, String)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT id, name, COALESCE(group_name, '') FROM sde_ships ORDER BY group_name, name",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn ship_sizes(&self) -> std::collections::HashMap<i64, crate::settings::ShipSize> {
        let mut map = std::collections::HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, COALESCE(group_name,'') FROM sde_ships") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
                for (id, group) in rows.flatten() {
                    map.insert(id, crate::settings::ShipSize::from_group(&group));
                }
            }
        }
        map
    }

    pub fn known_pilots(&self) -> std::collections::HashMap<String, i64> {
        let mut out = std::collections::HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT name_lc, char_id FROM known_pilots WHERE char_id != 0") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn known_pilot_names(&self) -> Vec<(String, i64)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT name, char_id FROM known_pilots WHERE char_id != 0 ORDER BY name")
        {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    #[allow(dead_code)]
    pub fn known_negatives(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT name_lc FROM known_pilots WHERE char_id = 0") {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn add_known_pilot(&self, name: &str, char_id: i64) {
        // Upgrade a previously-stored negative (char_id 0) once ESI confirms a real
        // character with the same name, but never downgrade a confirmed pilot back to 0
        // (the WHERE guards that). Plain OR IGNORE left the stale 0 row forever, hiding the
        // pilot from `known_pilots`, which filters char_id != 0.
        let _ = self.conn.execute(
            "INSERT INTO known_pilots(name_lc, name, char_id) VALUES(?1, ?2, ?3)
             ON CONFLICT(name_lc) DO UPDATE SET char_id=excluded.char_id, name=excluded.name
             WHERE excluded.char_id != 0",
            params![name.to_lowercase(), name, char_id],
        );
    }

    pub fn add_ping(&self, ts: i64, json: &str) {
        let _ = self
            .conn
            .execute("INSERT INTO pings(ts, json) VALUES(?1, ?2)", params![ts, json]);
    }

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

    pub fn add_kill_intel(&self, killmail_id: i64, system_id: i64, ship_type_id: i64, time: i64, value: f64) {
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO kill_intel(killmail_id, system_id, ship_type_id, time, value)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![killmail_id, system_id, ship_type_id, time, value],
        );
    }

    pub fn load_kill_intel(&self, since: i64) -> Vec<(i64, i64, i64, i64, f64)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT killmail_id, system_id, ship_type_id, time, value FROM kill_intel
             WHERE time >= ?1 ORDER BY time ASC",
        ) {
            if let Ok(rows) = stmt.query_map(params![since], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, f64>(4)?,
                ))
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn prune_kill_intel(&self, before: i64) {
        let _ = self.conn.execute("DELETE FROM kill_intel WHERE time < ?1", params![before]);
        let _ = self.conn.execute(
            "DELETE FROM kill_details WHERE kill_id NOT IN (SELECT killmail_id FROM kill_intel)",
            [],
        );
    }

    pub fn save_engagement(&self, e: &crate::battle::Engagement) {
        if let Ok(json) = serde_json::to_string(e) {
            let _ = self.conn.execute(
                "INSERT OR REPLACE INTO engagements(kill_id, time, system_id, json)
                 VALUES(?1, ?2, ?3, ?4)",
                params![e.kill_id, e.time, e.system_id, json],
            );
        }
    }

    pub fn load_engagements(&self, since: i64) -> Vec<crate::battle::Engagement> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT json FROM engagements WHERE time >= ?1 ORDER BY time ASC")
        {
            if let Ok(rows) = stmt.query_map(params![since], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten().filter_map(|j| serde_json::from_str(&j).ok()));
            }
        }
        out
    }

    #[allow(dead_code)]
    pub fn prune_engagements(&self, before: i64) {
        let _ = self.conn.execute("DELETE FROM engagements WHERE time < ?1", params![before]);
    }

    #[allow(dead_code)]
    pub fn set_battle_tag(&self, kill_id: i64, tag: Option<i64>) {
        let _ = self.conn.execute(
            "INSERT INTO battle_overrides(kill_id, group_tag, excluded) VALUES(?1, ?2, 0)
             ON CONFLICT(kill_id) DO UPDATE SET group_tag=?2",
            params![kill_id, tag],
        );
    }

    #[allow(dead_code)]
    pub fn set_battle_excluded(&self, kill_id: i64, excluded: bool) {
        let _ = self.conn.execute(
            "INSERT INTO battle_overrides(kill_id, group_tag, excluded) VALUES(?1, NULL, ?2)
             ON CONFLICT(kill_id) DO UPDATE SET excluded=?2",
            params![kill_id, excluded as i64],
        );
    }

    #[allow(dead_code)]
    pub fn clear_battle_override(&self, kill_id: i64) {
        let _ = self.conn.execute("DELETE FROM battle_overrides WHERE kill_id=?1", params![kill_id]);
    }

    #[allow(dead_code)]
    pub fn next_battle_tag(&self) -> i64 {
        self.conn
            .query_row("SELECT COALESCE(MAX(group_tag),0)+1 FROM battle_overrides", [], |r| r.get(0))
            .unwrap_or(1)
    }

    #[allow(dead_code)]
    pub fn set_scrub(&self, kill_id: i64, char_id: i64, on: bool) {
        let _ = if on {
            self.conn.execute(
                "INSERT OR IGNORE INTO battle_scrubs(kill_id, char_id) VALUES(?1, ?2)",
                params![kill_id, char_id],
            )
        } else {
            self.conn.execute(
                "DELETE FROM battle_scrubs WHERE kill_id=?1 AND char_id=?2",
                params![kill_id, char_id],
            )
        };
    }

    #[allow(dead_code)]
    pub fn load_battle_overrides(&self) -> crate::battle::Overrides {
        let mut o = crate::battle::Overrides::default();
        if let Ok(mut stmt) =
            self.conn.prepare("SELECT kill_id, group_tag, excluded FROM battle_overrides")
        {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, i64>(2)?))
            }) {
                for (kill_id, tag, excluded) in rows.flatten() {
                    if let Some(tag) = tag {
                        o.tag.insert(kill_id, tag);
                    }
                    if excluded != 0 {
                        o.excluded.insert(kill_id);
                    }
                }
            }
        }
        if let Ok(mut stmt) = self.conn.prepare("SELECT kill_id, char_id FROM battle_scrubs") {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
                for pair in rows.flatten() {
                    o.scrubs.insert(pair);
                }
            }
        }
        o
    }

    #[allow(dead_code)]
    pub fn list_excluded_engagements(&self) -> Vec<crate::battle::Engagement> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT e.json FROM engagements e JOIN battle_overrides o ON e.kill_id=o.kill_id
             WHERE o.excluded=1 ORDER BY e.time DESC",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten().filter_map(|j| serde_json::from_str(&j).ok()));
            }
        }
        out
    }

    #[allow(dead_code)]
    pub fn list_scrubs(&self) -> Vec<(i64, i64)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) =
            self.conn.prepare("SELECT kill_id, char_id FROM battle_scrubs ORDER BY kill_id")
        {
            if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn count_excluded(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM battle_overrides WHERE excluded=1", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize
    }
    pub fn count_scrubs(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM battle_scrubs", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    pub fn save_kill_details(&self, k: &crate::kills::KillInfo) {
        let alliances: String =
            k.attacker_alliances.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(",");
        let (near_name, near_dist) = match &k.near_celestial {
            Some((n, d)) => (Some(n.clone()), Some(*d)),
            None => (None, None),
        };
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO kill_details
                (kill_id, hash, victim_char, victim_ship, victim_corp, victim_alliance,
                 system_id, value, time, final_blow_char, final_blow_corp, final_blow_alliance,
                 final_blow_ship, attacker_count, attacker_alliances, near_name, near_dist)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                k.kill_id, k.hash, k.victim_char, k.victim_ship, k.victim_corp, k.victim_alliance,
                k.system_id, k.value, k.time, k.final_blow_char, k.final_blow_corp,
                k.final_blow_alliance, k.final_blow_ship, k.attacker_count as i64, alliances,
                near_name, near_dist,
            ],
        );
    }

    pub fn load_kill_details(&self) -> Vec<crate::kills::KillInfo> {
        let mut out = Vec::new();
        let Ok(mut stmt) = self.conn.prepare(
            "SELECT kill_id, hash, victim_char, victim_ship, victim_corp, victim_alliance,
                    system_id, value, time, final_blow_char, final_blow_corp, final_blow_alliance,
                    final_blow_ship, attacker_count, attacker_alliances, near_name, near_dist
             FROM kill_details",
        ) else {
            return out;
        };
        let rows = stmt.query_map([], |r| {
            let alliances: Option<String> = r.get(14)?;
            let attacker_alliances = alliances
                .unwrap_or_default()
                .split(',')
                .filter_map(|s| s.parse::<i64>().ok())
                .collect();
            let near_celestial = match (r.get::<_, Option<String>>(15)?, r.get::<_, Option<f64>>(16)?) {
                (Some(n), Some(d)) => Some((n, d)),
                _ => None,
            };
            Ok(crate::kills::KillInfo {
                kill_id: r.get(0)?,
                hash: r.get(1)?,
                victim_char: r.get(2)?,
                victim_ship: r.get(3)?,
                victim_corp: r.get(4)?,
                victim_alliance: r.get(5)?,
                system_id: r.get(6)?,
                value: r.get(7)?,
                time: r.get(8)?,
                final_blow_char: r.get(9)?,
                final_blow_corp: r.get(10)?,
                final_blow_alliance: r.get(11)?,
                final_blow_ship: r.get(12)?,
                attacker_count: r.get::<_, i64>(13)? as usize,
                attacker_alliances,
                near_celestial,
            })
        });
        if let Ok(rows) = rows {
            out.extend(rows.flatten());
        }
        out
    }

    pub fn save_pilot_activity(
        &self,
        char_id: i64,
        active_recent: bool,
        birthday: Option<i64>,
        last_corp_change: Option<i64>,
        fetched_at: i64,
    ) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO
                pilot_activity(char_id, active_recent, birthday, last_corp_change, fetched_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![char_id, active_recent as i64, birthday, last_corp_change, fetched_at],
        );
    }

    pub fn pilot_activity(&self) -> Vec<(i64, bool, Option<i64>, Option<i64>, i64)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT char_id, active_recent, birthday, last_corp_change, fetched_at
             FROM pilot_activity",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)? != 0,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn load_revivals(&self) -> Vec<(String, i64)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) =
            self.conn.prepare("SELECT name, revived_until FROM pilot_revival")
        {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn set_revival(&self, name: &str, revived_until: i64) {
        let _ = self.conn.execute(
            "INSERT INTO pilot_revival(name, revived_until) VALUES(?1, ?2)
             ON CONFLICT(name) DO UPDATE SET revived_until=?2",
            params![name.to_lowercase(), revived_until],
        );
    }

    pub fn load_pilot_verdicts(&self) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT name_lc, hidden FROM pilot_verdict") {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)))
            {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn set_pilot_verdict(&self, name: &str, hidden: bool) {
        let _ = self.conn.execute(
            "INSERT INTO pilot_verdict(name_lc, hidden) VALUES(?1, ?2)
             ON CONFLICT(name_lc) DO UPDATE SET hidden=?2",
            params![name.to_lowercase(), hidden as i64],
        );
    }

    pub fn delete_chat_jid(&self, jid: &str) {
        let _ = self.conn.execute("DELETE FROM chats WHERE jid = ?1", params![jid]);
    }

    pub fn add_chat(&self, jid: &str, sender: &str, body: &str, time: i64, outgoing: bool) {
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO chats(jid, sender, body, time, outgoing) VALUES(?1,?2,?3,?4,?5)",
            params![jid, sender, body, time, outgoing as i64],
        );
    }

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

    const WH_COLS: &'static str = "id, system_id, signature, wh_type, dest_class,
        dest_system_id, dest_signature, dest_wh_type, size, is_drifter, reported_at,
        explicit_expiry, source, updated_at";

    pub fn upsert_wormhole(&self, incoming: &crate::wormholes::Wormhole) -> i64 {
        // The find-then-insert/update below is a read-modify-write; two scout/watcher
        // connections could both miss the row and both INSERT, and the loser's INSERT would
        // hit the dedup UNIQUE constraint and be silently dropped. BEGIN IMMEDIATE takes the
        // write lock up front so concurrent upserts serialize. The body has no panic paths,
        // so the connection can't be left mid-transaction.
        let in_tx = self.conn.execute_batch("BEGIN IMMEDIATE").is_ok();
        let id = self.upsert_wormhole_locked(incoming);
        if in_tx {
            let _ = self.conn.execute_batch("COMMIT");
        }
        id
    }

    fn upsert_wormhole_locked(&self, incoming: &crate::wormholes::Wormhole) -> i64 {
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

    /// A hole the user has marked dead stays in the table (an EVE-Scout resync would otherwise just
    /// re-add it) but is invisible to everything downstream: overlay, routing, waypoints.
    pub fn kill_wormhole(&self, id: i64) {
        let _ = self.conn.execute("UPDATE wormholes SET dead = 1 WHERE id = ?1", params![id]);
    }

    pub fn wormholes(&self) -> Vec<crate::wormholes::Wormhole> {
        let mut out = Vec::new();
        if let Ok(mut stmt) =
            self.conn.prepare(&format!("SELECT {} FROM wormholes WHERE dead = 0", Self::WH_COLS))
        {
            if let Ok(rows) = stmt.query_map([], Self::row_to_wormhole) {
                out.extend(rows.flatten());
            }
        }
        out
    }

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

    pub fn traits_baked(&self) -> bool {
        self.conn
            .query_row("SELECT COUNT(*) FROM sde_ship_traits", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
            > 0
    }

    /// Role bonuses for a ship (skill_id, bonus value, text). skill_id -1 = role.
    pub fn ship_traits(&self, id: i64) -> Vec<(i64, f64, String)> {
        let mut out = Vec::new();
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

        let mut systems = crate::geo::Systems::new(by_name, adjacency);

        let mut stargates: HashMap<i64, Vec<[f64; 3]>> = HashMap::new();
        if let Ok(mut stmt) =
            self.conn.prepare("SELECT system_id, x, y, z FROM sde_stargates")
        {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?, r.get::<_, f64>(2)?, r.get::<_, f64>(3)?))
            }) {
                for (sys, x, y, z) in rows.flatten() {
                    stargates.entry(sys).or_default().push([x, y, z]);
                }
            }
        }
        systems.set_stargates(stargates);
        systems
    }

    #[allow(dead_code)]
    pub fn nearest_celestial(&self, system_id: i64, pos: [f64; 3]) -> Option<(String, f64)> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, x, y, z FROM sde_celestials WHERE system_id = ?1")
            .ok()?;
        let rows = stmt
            .query_map(params![system_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            })
            .ok()?;
        let mut best: Option<(String, f64)> = None;
        for (name, x, y, z) in rows.flatten() {
            let d2 = (x - pos[0]).powi(2) + (y - pos[1]).powi(2) + (z - pos[2]).powi(2);
            if best.as_ref().is_none_or(|(_, b)| d2 < *b) {
                best = Some((name, d2));
            }
        }
        best.map(|(name, d2)| (name, d2.sqrt()))
    }

    pub fn load_camp_types(&self) -> crate::camp::CampTypes {
        let mut t = crate::camp::CampTypes::default();
        if let Ok(mut stmt) = self.conn.prepare("SELECT id, kind FROM sde_camp_types") {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            {
                for (id, kind) in rows.flatten() {
                    match kind.as_str() {
                        "dic" | "hic" => {
                            t.dic_hic.insert(id);
                        }
                        "smartbomb" => {
                            t.smartbomb.insert(id);
                        }
                        "bubble" => {
                            t.bubble.insert(id);
                        }
                        _ => {}
                    }
                }
            }
        }
        t
    }

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

    pub fn token_expiry(&self, id: i64) -> Option<i64> {
        self.conn
            .query_row("SELECT expires_at FROM characters WHERE id = ?1", params![id], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .ok()
            .flatten()
    }

    pub fn remove_character(&self, id: i64) -> Result<()> {
        let _ = crate::tokens::delete(id);
        self.kv_delete(&format!("access:{id}"));
        self.conn
            .execute("DELETE FROM characters WHERE id = ?1", params![id])?;
        Ok(())
    }
}

/// Enable WAL and a busy timeout on a freshly opened connection. Many threads each
/// open their own connection to the same DB file (see `path`), so without WAL +
/// `busy_timeout` a colliding write fails with `SQLITE_BUSY` and — since most mutations
/// ignore the result — is silently dropped. WAL is a persistent DB property; the timeout
/// is per-connection, so every open must set it.
pub(crate) fn apply_pragmas(conn: &Connection) {
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "synchronous", "NORMAL");
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
            if let Err(e) = crate::tokens::save_refresh(id, &refresh) {
                eprintln!("keychain migration failed for character {id}: {e:#}");
                all_migrated = false;
            } else {
                let _ = conn.execute(
                    "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                    params![format!("access:{id}"), access.unwrap_or_default()],
                );
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

pub fn data_dir() -> Result<PathBuf> {
    let pd = directories::ProjectDirs::from("online", "EveSpai", "eve-spai")
        .ok_or_else(|| anyhow!("could not resolve a data directory"))?;
    Ok(pd.data_dir().to_path_buf())
}

fn score_cached(
    lower: &str,
    tri: &std::collections::HashSet<[u8; 3]>,
    q: &str,
    qt: &std::collections::HashSet<[u8; 3]>,
) -> Option<i64> {
    if lower == q {
        return Some(10_000);
    }
    if lower.starts_with(q) {
        return Some(5_000 - lower.len() as i64);
    }
    if let Some(pos) = lower.find(q) {
        return Some(2_000 - pos as i64 - lower.len() as i64);
    }
    if qt.is_empty() {
        return None;
    }
    let shared = qt.iter().filter(|t| tri.contains(*t)).count();
    let frac = shared as f64 / qt.len() as f64;
    (frac >= 0.5).then(|| (frac * 1_000.0) as i64 - lower.len() as i64)
}

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
    let qt = trigrams(q);
    if qt.is_empty() {
        return None;
    }
    let nt = trigrams(name_lc);
    let shared = qt.iter().filter(|t| nt.contains(*t)).count();
    let frac = shared as f64 / qt.len() as f64;
    (frac >= 0.5).then(|| (frac * 1_000.0) as i64 - name_lc.len() as i64)
}

fn trigrams(s: &str) -> std::collections::HashSet<[u8; 3]> {
    let padded = format!("  {s} ");
    let b = padded.as_bytes();
    let mut set = std::collections::HashSet::new();
    for w in b.windows(3) {
        set.insert([w[0], w[1], w[2]]);
    }
    set
}

#[cfg(test)]
mod tests {
    use super::{Store, SCHEMA};
    use rusqlite::{params, Connection};

    fn mem_store() -> Store {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        Store {
            conn,
            path: std::path::PathBuf::new(),
            sys_cache: std::cell::RefCell::new(None),
            place_cache: std::cell::RefCell::new(None),
        }
    }

    fn a_hole(system_id: i64, sig: &str) -> crate::wormholes::Wormhole {
        use crate::wormholes::{DestClass, Source, Wormhole};
        Wormhole {
            id: 0,
            system_id,
            signature: Some(sig.to_owned()),
            wh_type: None,
            dest: DestClass::Thera,
            dest_system_id: Some(31_000_005),
            dest_signature: None,
            dest_wh_type: None,
            size: None,
            is_drifter: false,
            reported_at: 1_700_000_000,
            explicit_expiry: None,
            source: Source::EveScout,
            updated_at: 1_700_000_000,
        }
    }

    #[test]
    fn a_dead_hole_stays_dead_through_a_resync() {
        let s = mem_store();
        let hole = a_hole(30_000_142, "ABC-123");
        let id = s.upsert_wormhole(&hole);
        assert_eq!(s.wormholes().len(), 1);

        s.kill_wormhole(id);
        assert!(s.wormholes().is_empty(), "a dead hole must not reach the map or the router");

        // EVE-Scout still lists it, so the next sync upserts the same hole. It must not come back.
        let mut again = a_hole(30_000_142, "ABC-123");
        again.updated_at = 1_700_009_999;
        assert_eq!(s.upsert_wormhole(&again), id, "the resync must land on the same row");
        assert!(s.wormholes().is_empty(), "a resync resurrected a hole the user killed");
    }

    #[test]
    fn battle_overrides_roundtrip() {
        let s = mem_store();

        assert_eq!(s.next_battle_tag(), 1);

        s.set_battle_tag(100, Some(7));
        s.set_battle_excluded(200, true);
        s.set_scrub(300, 42, true);

        let o = s.load_battle_overrides();
        assert_eq!(o.tag.get(&100), Some(&7));
        assert!(o.excluded.contains(&200));
        assert!(o.scrubs.contains(&(300, 42)));
        assert_eq!(s.list_scrubs(), vec![(300, 42)]);

        assert_eq!(s.next_battle_tag(), 8);

        s.set_battle_excluded(100, true);
        s.set_battle_tag(100, Some(9));
        let o = s.load_battle_overrides();
        assert_eq!(o.tag.get(&100), Some(&9));
        assert!(o.excluded.contains(&100));

        s.set_battle_excluded(200, false);
        s.set_scrub(300, 42, false);
        s.set_battle_tag(100, None);
        s.clear_battle_override(100);
        let o = s.load_battle_overrides();
        assert!(o.tag.is_empty());
        assert!(o.excluded.is_empty());
        assert!(o.scrubs.is_empty());
        assert!(s.list_scrubs().is_empty());
    }

    #[test]
    fn nearest_celestial_picks_closest() {
        let s = mem_store();

        assert!(s.nearest_celestial(30_000_142, [0.0, 0.0, 0.0]).is_none());

        let ins = |sys: i64, name: &str, x: f64, y: f64, z: f64| {
            s.conn
                .execute(
                    "INSERT INTO sde_celestials(system_id, name, x, y, z) VALUES(?1,?2,?3,?4,?5)",
                    params![sys, name, x, y, z],
                )
                .unwrap();
        };
        ins(30_000_142, "Jita IV", 100.0, 0.0, 0.0);
        ins(30_000_142, "Jita IV - Moon 4", 0.0, 10.0, 0.0);
        ins(30_000_142, "Perimeter gate", -50.0, 0.0, 0.0);
        ins(30_000_144, "Perimeter I", 1.0, 1.0, 1.0);

        let (name, dist) = s.nearest_celestial(30_000_142, [0.0, 12.0, 0.0]).unwrap();
        assert_eq!(name, "Jita IV - Moon 4");
        assert!((dist - 2.0).abs() < 1e-6, "distance was {dist}");

        let (name, _) = s.nearest_celestial(30_000_142, [95.0, 0.0, 0.0]).unwrap();
        assert_eq!(name, "Jita IV");

        assert!(s.nearest_celestial(31_000_000, [0.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn revival_roundtrip() {
        let s = mem_store();
        assert!(s.load_revivals().is_empty());

        s.set_revival("Bovine Worm", 1_000);
        s.set_revival("roamer", 2_000);
        let mut got = s.load_revivals();
        got.sort();
        assert_eq!(got, vec![("bovine worm".to_string(), 1_000), ("roamer".to_string(), 2_000)]);

        s.set_revival("bovine worm", 5_000);
        let got = s.load_revivals();
        assert_eq!(got.len(), 2);
        assert!(got.contains(&("bovine worm".to_string(), 5_000)));
    }

    fn add(conn: &Connection, name: &str, char_id: i64) {
        conn.execute(
            "INSERT INTO known_pilots(name_lc, name, char_id) VALUES(?1, ?2, ?3)
             ON CONFLICT(name_lc) DO UPDATE SET char_id=excluded.char_id, name=excluded.name
             WHERE excluded.char_id != 0",
            params![name.to_lowercase(), name, char_id],
        )
        .unwrap();
    }

    fn char_id(conn: &Connection, name_lc: &str) -> i64 {
        conn.query_row(
            "SELECT char_id FROM known_pilots WHERE name_lc = ?1",
            params![name_lc],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn known_pilot_negative_is_upgraded_but_never_downgraded() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE known_pilots (name_lc TEXT PRIMARY KEY, name TEXT, char_id INTEGER)",
            [],
        )
        .unwrap();
        add(&conn, "Comet Navy", 0);
        assert_eq!(char_id(&conn, "comet navy"), 0);
        add(&conn, "Comet Navy", 12345);
        assert_eq!(char_id(&conn, "comet navy"), 12345);
        add(&conn, "Comet Navy", 0);
        assert_eq!(char_id(&conn, "comet navy"), 12345);
    }
}
