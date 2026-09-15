//! Static data queries: systems, constellations, regions and their names and neighbours.

use super::*;

impl Store {
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

    pub(crate) fn ensure_sys_cache(&self) {
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

    pub(crate) fn ensure_place_cache(&self) {
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

    pub(crate) fn name_of(&self, sql: &str, id: i64) -> Option<String> {
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

    pub(crate) fn id_name_list(&self, sql: &str, id: i64) -> Vec<(i64, String)> {
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

    pub(crate) fn map_systems(&self, filter: &str, p: impl rusqlite::Params) -> Vec<MapSystem> {
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
}
