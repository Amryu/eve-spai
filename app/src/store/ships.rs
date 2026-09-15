//! Ship types from the static data: names, sizes, traits and details.

use super::*;

impl Store {
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
}
