//! Kills and losses of tracked fleets' pilots, recorded off the zKill feed.

use super::*;

/// One killmail a tracked fleet took part in: `loss` when the victim was one of its pilots, a
/// kill when some of them were on it. `members` are the fleet's pilots involved. A pod names the
/// ship loss it followed in `pod_of`, 0 otherwise.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FleetKill {
    pub fleet_id: String,
    pub kill_id: i64,
    pub at: i64,
    pub system_id: i64,
    pub loss: bool,
    pub victim_char: i64,
    pub victim_name: String,
    pub ship_type_id: i64,
    pub value: f64,
    pub members: Vec<i64>,
    pub pod_of: i64,
}

impl Store {
    pub fn add_fleet_kill(&self, k: &FleetKill) {
        let members = k.members.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(",");
        self.exec_historic(
            "INSERT OR IGNORE INTO fleet_kills (fleet_id, kill_id, at, system_id, loss, victim_char,
                 victim_name, ship_type_id, value, members, pod_of)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                k.fleet_id,
                k.kill_id,
                k.at,
                k.system_id,
                k.loss,
                k.victim_char,
                k.victim_name,
                k.ship_type_id,
                k.value,
                members,
                k.pod_of
            ],
        );
    }

    /// The ship kill a pod of `victim_char` followed: same fleet and side, within ten minutes before.
    pub fn fleet_kill_ship_before(&self, fleet_id: &str, victim_char: i64, loss: bool, at: i64, pods: &[i64]) -> Option<i64> {
        let mut st = self
            .conn
            .prepare(
                "SELECT kill_id, ship_type_id FROM fleet_kills
                 WHERE fleet_id = ?1 AND victim_char = ?2 AND loss = ?3 AND at BETWEEN ?4 AND ?5
                 ORDER BY at DESC",
            )
            .ok()?;
        let found = st
            .query_map(params![fleet_id, victim_char, loss, at - 600, at], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .ok()?
            .flatten()
            .find(|(_, ship)| !pods.contains(ship))
            .map(|(id, _)| id);
        found
    }

    #[cfg(feature = "fleet")]
    pub fn fleet_kills(&self, fleet_id: &str) -> Vec<FleetKill> {
        let Ok(mut st) = self.conn.prepare(
            "SELECT kill_id, at, system_id, loss, victim_char, victim_name, ship_type_id, value, members, pod_of
             FROM fleet_kills WHERE fleet_id = ?1 ORDER BY at",
        ) else {
            return Vec::new();
        };
        st.query_map(params![fleet_id], |r| {
            let members: String = r.get(8)?;
            Ok(FleetKill {
                fleet_id: fleet_id.to_owned(),
                kill_id: r.get(0)?,
                at: r.get(1)?,
                system_id: r.get(2)?,
                loss: r.get(3)?,
                victim_char: r.get(4)?,
                victim_name: r.get(5)?,
                ship_type_id: r.get(6)?,
                value: r.get(7)?,
                members: members.split(',').filter_map(|m| m.parse().ok()).collect(),
                pod_of: r.get(9)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    #[cfg(feature = "fleet")]
    pub fn set_fleet_br(&self, fleet_id: &str, url: &str, edit_key: &str, at: i64) {
        let _ = self.exec_essential(
            "INSERT INTO fleet_brs (fleet_id, url, edit_key, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(fleet_id) DO UPDATE SET url = ?2, edit_key = ?3, created_at = ?4",
            params![fleet_id, url, edit_key, at],
        );
    }

    #[cfg(feature = "fleet")]
    pub fn fleet_br(&self, fleet_id: &str) -> Option<String> {
        self.conn
            .query_row("SELECT url FROM fleet_brs WHERE fleet_id = ?1", params![fleet_id], |r| r.get(0))
            .ok()
    }
}
