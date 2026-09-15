//! Battle history: engagements, per-battle overrides and scrubs, and cached kill details.

use super::*;

impl Store {
    pub fn save_engagement(&self, e: &br_core::battle::Engagement) {
        if let Ok(json) = serde_json::to_string(e) {
            self.exec_historic(
                "INSERT OR REPLACE INTO engagements(kill_id, time, system_id, json)
                 VALUES(?1, ?2, ?3, ?4)",
                params![e.kill_id, e.time, e.system_id, json],
            );
        }
    }

    pub fn load_engagements(&self, since: i64) -> Vec<br_core::battle::Engagement> {
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

    /// Deletes in batches: the first pass after an upgrade clears six figures of rows, and one
    /// statement that long holds the write lock past every other connection's busy timeout.
    pub fn prune_engagements(&self, before: i64) -> usize {
        const BATCH: i64 = 20_000;
        let mut total = 0;
        loop {
            let n = self
                .conn
                .execute(
                    "DELETE FROM engagements WHERE kill_id IN
                     (SELECT kill_id FROM engagements WHERE time < ?1 LIMIT ?2)",
                    params![before, BATCH],
                )
                .inspect_err(crate::disk::note_sqlite_error)
                .unwrap_or(0);
            total += n;
            if (n as i64) < BATCH {
                return total;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    pub fn set_battle_tag(&self, kill_id: i64, tag: Option<i64>) {
        let _ = self.conn.execute(
            "INSERT INTO battle_overrides(kill_id, group_tag, excluded) VALUES(?1, ?2, 0)
             ON CONFLICT(kill_id) DO UPDATE SET group_tag=?2",
            params![kill_id, tag],
        );
    }

    pub fn set_battle_excluded(&self, kill_id: i64, excluded: bool) {
        let _ = self.conn.execute(
            "INSERT INTO battle_overrides(kill_id, group_tag, excluded) VALUES(?1, NULL, ?2)
             ON CONFLICT(kill_id) DO UPDATE SET excluded=?2",
            params![kill_id, excluded as i64],
        );
    }

    #[cfg(test)]
    pub fn clear_battle_override(&self, kill_id: i64) {
        let _ = self.conn.execute("DELETE FROM battle_overrides WHERE kill_id=?1", params![kill_id]);
    }

    pub fn next_battle_tag(&self) -> i64 {
        self.conn
            .query_row("SELECT COALESCE(MAX(group_tag),0)+1 FROM battle_overrides", [], |r| r.get(0))
            .unwrap_or(1)
    }

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

    pub fn load_battle_overrides(&self) -> br_core::battle::Overrides {
        let mut o = br_core::battle::Overrides::default();
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

    pub fn list_excluded_engagements(&self) -> Vec<br_core::battle::Engagement> {
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
        self.exec_historic(
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
}
