//! What the app saw, kept for as long as the user chose: intel reports, every kill in EVE, the
//! hourly system statistics, sov changes, their own characters' moves and their local scans.
//! Each kind is pruned on its own retention by [`super::run_maintenance`].

use rusqlite::params;
use std::sync::RwLock;

use super::Store;
use crate::settings::Retention;

/// The retention in force, set by the app from its settings so the writers on other threads can
/// tell whether to record at all.
static RETENTION: RwLock<Option<Retention>> = RwLock::new(None);

pub fn set_retention(r: &Retention) {
    *RETENTION.write().unwrap_or_else(|e| e.into_inner()) = Some(r.clone());
}

pub fn retention() -> Retention {
    RETENTION.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS intel_log (
    received INTEGER NOT NULL,
    channel  TEXT NOT NULL,
    reporter TEXT NOT NULL,
    text     TEXT NOT NULL,
    -- Space-padded system ids and newline-padded lower-case pilots, for LIKE filters.
    systems  TEXT NOT NULL,
    pilots   TEXT NOT NULL,
    json     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_intel_log_received ON intel_log(received);
CREATE TABLE IF NOT EXISTS kill_log (
    kill_id            INTEGER PRIMARY KEY,
    time               INTEGER NOT NULL,
    system_id          INTEGER NOT NULL,
    ship_type_id       INTEGER NOT NULL,
    value              REAL NOT NULL,
    victim_char        INTEGER,
    victim_corp        INTEGER,
    victim_alliance    INTEGER,
    attackers          INTEGER NOT NULL,
    -- Space-padded alliance ids of the attackers, and their hull ids.
    attacker_alliances TEXT NOT NULL,
    attacker_ships     TEXT NOT NULL,
    on_gate            INTEGER NOT NULL,
    camp_gear          INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_kill_log_time ON kill_log(time);
CREATE INDEX IF NOT EXISTS idx_kill_log_sys ON kill_log(system_id, time);
-- One row per hour: ESI's kills and jumps for every system with any, packed (see pack_stats).
CREATE TABLE IF NOT EXISTS system_stats (hour INTEGER PRIMARY KEY, data BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS sov_log (
    time        INTEGER NOT NULL,
    system_id   INTEGER NOT NULL,
    alliance_id INTEGER,
    PRIMARY KEY (system_id, time)
);
CREATE TABLE IF NOT EXISTS char_moves (
    time      INTEGER NOT NULL,
    character TEXT NOT NULL,
    from_id   INTEGER NOT NULL,
    to_id     INTEGER NOT NULL,
    ship      INTEGER,
    docked    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_char_moves_time ON char_moves(time);
CREATE TABLE IF NOT EXISTS local_scans (
    time      INTEGER NOT NULL,
    system_id INTEGER,
    json      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_local_scans_time ON local_scans(time);
";

/// One kill as the log keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KillRow {
    pub kill_id: i64,
    pub time: i64,
    pub system_id: i64,
    pub ship_type_id: i64,
    pub value: f64,
    pub victim_char: Option<i64>,
    pub victim_corp: Option<i64>,
    pub victim_alliance: Option<i64>,
    pub attackers: u32,
    pub attacker_alliances: Vec<i64>,
    pub attacker_ships: Vec<i64>,
    pub on_gate: bool,
    pub camp_gear: bool,
}

/// One system's hour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HourStats {
    pub system_id: i64,
    pub ship_kills: u16,
    pub pod_kills: u16,
    pub npc_kills: u16,
    pub jumps: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveRow {
    pub time: i64,
    pub character: String,
    pub from: i64,
    pub to: i64,
    pub ship: Option<i64>,
    pub docked: bool,
}

/// Ids padded with spaces on both ends, so `LIKE '% 42 %'` matches one id and never a part of one.
fn padded(ids: impl IntoIterator<Item = i64>) -> String {
    let mut s = String::from(" ");
    for id in ids {
        s.push_str(&id.to_string());
        s.push(' ');
    }
    s
}

fn unpad(s: &str) -> Vec<i64> {
    s.split_whitespace().filter_map(|x| x.parse().ok()).collect()
}

/// 12 bytes a system: id as u32, then four u16 counts, little-endian.
pub fn pack_stats(rows: &[HourStats]) -> Vec<u8> {
    let mut b = Vec::with_capacity(rows.len() * 12);
    for r in rows {
        b.extend_from_slice(&(r.system_id as u32).to_le_bytes());
        for v in [r.ship_kills, r.pod_kills, r.npc_kills, r.jumps] {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b
}

pub fn unpack_stats(b: &[u8]) -> Vec<HourStats> {
    b.chunks_exact(12)
        .map(|c| {
            let u = |i: usize| u16::from_le_bytes([c[i], c[i + 1]]);
            HourStats { system_id: u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as i64, ship_kills: u(4), pod_kills: u(6), npc_kills: u(8), jumps: u(10) }
        })
        .collect()
}

impl Store {
    /// Replaces the intel kept from `since` on with `reports`, the live window as it stands, so
    /// reports merged or corrected since the last write are not kept twice.
    pub fn replace_intel_window(&self, since: i64, reports: &[crate::intel::IntelReport]) {
        if retention().intel == 0 {
            return;
        }
        let tx = self.conn.unchecked_transaction();
        let Ok(tx) = tx else { return };
        let _ = tx.execute("DELETE FROM intel_log WHERE received >= ?1", params![since]);
        for r in reports.iter().filter(|r| r.received >= since) {
            let Ok(json) = serde_json::to_string(r) else { continue };
            let pilots: String = r.pilots.iter().map(|p| format!("\n{}", p.to_lowercase())).collect::<String>() + "\n";
            let _ = tx.execute(
                "INSERT INTO intel_log (received, channel, reporter, text, systems, pilots, json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![r.received, r.channel, r.reporter, r.text, padded(r.systems.iter().map(|s| s.id)), pilots, json],
            );
        }
        let _ = tx.commit();
    }

    /// Intel from `since` to `until`, newest first. Narrowed to any of `systems`, a pilot, and
    /// reports holding every word of `text` (in their text, pilots, ships or alliances), each when given.
    pub fn intel_history(&self, since: i64, until: i64, systems: &[i64], pilot: Option<&str>, text: Option<&str>, limit: usize) -> Vec<crate::intel::IntelReport> {
        let mut sql = String::from("SELECT json FROM intel_log WHERE received >= ?1 AND received <= ?2");
        let mut args: Vec<rusqlite::types::Value> = vec![since.into(), until.into()];
        if !systems.is_empty() {
            let ors: Vec<String> = systems
                .iter()
                .map(|id| {
                    args.push(format!("% {id} %").into());
                    format!("systems LIKE ?{}", args.len())
                })
                .collect();
            sql.push_str(&format!(" AND ({})", ors.join(" OR ")));
        }
        if let Some(p) = pilot.filter(|p| !p.trim().is_empty()) {
            args.push(format!("%\n{}%", p.trim().to_lowercase()).into());
            sql.push_str(&format!(" AND pilots LIKE ?{}", args.len()));
        }
        for w in text.unwrap_or_default().split_whitespace() {
            args.push(format!("%{w}%").into());
            sql.push_str(&format!(" AND json LIKE ?{}", args.len()));
        }
        args.push((limit as i64).into());
        sql.push_str(&format!(" ORDER BY received DESC LIMIT ?{}", args.len()));
        let Ok(mut stmt) = self.conn.prepare(&sql) else { return Vec::new() };
        let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(0));
        rows.map(|it| it.flatten().filter_map(|j| serde_json::from_str(&j).ok()).collect()).unwrap_or_default()
    }

    pub fn log_kill(&self, k: &KillRow) {
        if retention().kills_all == 0 {
            return;
        }
        let _ = self.conn.execute(
            "INSERT OR IGNORE INTO kill_log (kill_id, time, system_id, ship_type_id, value, victim_char, victim_corp, victim_alliance, attackers, attacker_alliances, attacker_ships, on_gate, camp_gear) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                k.kill_id, k.time, k.system_id, k.ship_type_id, k.value, k.victim_char, k.victim_corp, k.victim_alliance, k.attackers,
                padded(k.attacker_alliances.iter().copied()), padded(k.attacker_ships.iter().copied()), k.on_gate, k.camp_gear
            ],
        );
    }

    /// Logged kills from `since`, newest first, in any of `systems` and involving `alliance` (as
    /// victim or attacker), each when given.
    pub fn kill_history(&self, since: i64, systems: &[i64], alliance: Option<i64>, limit: usize) -> Vec<KillRow> {
        let mut sql = String::from(
            "SELECT kill_id, time, system_id, ship_type_id, value, victim_char, victim_corp, victim_alliance, attackers, attacker_alliances, attacker_ships, on_gate, camp_gear FROM kill_log WHERE time >= ?1",
        );
        let mut args: Vec<rusqlite::types::Value> = vec![since.into()];
        if !systems.is_empty() {
            sql.push_str(&format!(" AND system_id IN ({})", systems.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",")));
        }
        if let Some(a) = alliance {
            args.push(a.into());
            args.push(format!("% {a} %").into());
            sql.push_str(&format!(" AND (victim_alliance = ?{} OR attacker_alliances LIKE ?{})", args.len() - 1, args.len()));
        }
        args.push((limit as i64).into());
        sql.push_str(&format!(" ORDER BY time DESC LIMIT ?{}", args.len()));
        let Ok(mut stmt) = self.conn.prepare(&sql) else { return Vec::new() };
        let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
            Ok(KillRow {
                kill_id: r.get(0)?,
                time: r.get(1)?,
                system_id: r.get(2)?,
                ship_type_id: r.get(3)?,
                value: r.get(4)?,
                victim_char: r.get(5)?,
                victim_corp: r.get(6)?,
                victim_alliance: r.get(7)?,
                attackers: r.get(8)?,
                attacker_alliances: unpad(&r.get::<_, String>(9)?),
                attacker_ships: unpad(&r.get::<_, String>(10)?),
                on_gate: r.get(11)?,
                camp_gear: r.get(12)?,
            })
        });
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    /// Stores the hour's statistics, replacing an earlier snapshot of the same hour.
    pub fn log_system_stats(&self, hour: i64, rows: &[HourStats]) {
        if retention().system_stats == 0 || rows.is_empty() {
            return;
        }
        let _ = self.conn.execute("INSERT OR REPLACE INTO system_stats (hour, data) VALUES (?1, ?2)", params![hour, pack_stats(rows)]);
    }

    /// One system's hours from `since_hour`, oldest first.
    pub fn system_stats_history(&self, system_id: i64, since_hour: i64) -> Vec<(i64, HourStats)> {
        let Ok(mut stmt) = self.conn.prepare("SELECT hour, data FROM system_stats WHERE hour >= ?1 ORDER BY hour") else { return Vec::new() };
        let rows = stmt.query_map(params![since_hour], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)));
        rows.map(|it| {
            it.flatten()
                .map(|(h, b)| (h, unpack_stats(&b).into_iter().find(|s| s.system_id == system_id).unwrap_or(HourStats { system_id, ..Default::default() })))
                .collect()
        })
        .unwrap_or_default()
    }

    /// The holder each system last had, as logged.
    pub fn sov_latest(&self) -> std::collections::HashMap<i64, Option<i64>> {
        let Ok(mut stmt) = self.conn.prepare("SELECT system_id, alliance_id FROM sov_log s WHERE time = (SELECT MAX(time) FROM sov_log t WHERE t.system_id = s.system_id)") else {
            return Default::default();
        };
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)));
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    pub fn log_sov(&self, time: i64, system_id: i64, alliance: Option<i64>) {
        if retention().sov == 0 {
            return;
        }
        let _ = self.conn.execute("INSERT OR REPLACE INTO sov_log (time, system_id, alliance_id) VALUES (?1, ?2, ?3)", params![time, system_id, alliance]);
    }

    /// A system's holders over time, oldest first.
    pub fn sov_history(&self, system_id: i64) -> Vec<(i64, Option<i64>)> {
        let Ok(mut stmt) = self.conn.prepare("SELECT time, alliance_id FROM sov_log WHERE system_id = ?1 ORDER BY time") else { return Vec::new() };
        let rows = stmt.query_map(params![system_id], |r| Ok((r.get(0)?, r.get(1)?)));
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    pub fn log_move(&self, m: &MoveRow) {
        if retention().moves == 0 {
            return;
        }
        let _ = self.conn.execute(
            "INSERT INTO char_moves (time, character, from_id, to_id, ship, docked) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![m.time, m.character, m.from, m.to, m.ship, m.docked],
        );
    }

    pub fn move_history(&self, since: i64, character: Option<&str>, limit: usize) -> Vec<MoveRow> {
        let Ok(mut stmt) = self.conn.prepare(
            "SELECT time, character, from_id, to_id, ship, docked FROM char_moves WHERE time >= ?1 AND (?2 IS NULL OR character = ?2) ORDER BY time DESC LIMIT ?3",
        ) else {
            return Vec::new();
        };
        let rows = stmt.query_map(params![since, character, limit as i64], |r| {
            Ok(MoveRow { time: r.get(0)?, character: r.get(1)?, from: r.get(2)?, to: r.get(3)?, ship: r.get(4)?, docked: r.get(5)? })
        });
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    pub fn log_local_scan(&self, time: i64, system_id: Option<i64>, json: &str) {
        if retention().local_scans == 0 {
            return;
        }
        let _ = self.conn.execute("INSERT INTO local_scans (time, system_id, json) VALUES (?1, ?2, ?3)", params![time, system_id, json]);
    }

    /// Saved local scans from `since`, newest first, as (time, system, the saved lookup's JSON).
    pub fn local_scan_history(&self, since: i64, limit: usize) -> Vec<(i64, Option<i64>, String)> {
        let Ok(mut stmt) = self.conn.prepare("SELECT time, system_id, json FROM local_scans WHERE time >= ?1 ORDER BY time DESC LIMIT ?2") else {
            return Vec::new();
        };
        let rows = stmt.query_map(params![since, limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)));
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    /// Drops what is older than each kind's retention. Returns the rows deleted.
    pub fn prune_history(&self, r: &Retention, now: i64) -> usize {
        let cut = |days: u32| if days >= Retention::FOREVER { None } else { Some(now - days as i64 * 86_400) };
        let mut n = 0;
        let mut run = |sql: &str, days: u32| {
            if let Some(before) = cut(days) {
                n += self.conn.execute(sql, params![before]).unwrap_or(0);
            }
        };
        run("DELETE FROM intel_log WHERE received < ?1", r.intel);
        run("DELETE FROM kill_log WHERE time < ?1", r.kills_all);
        run("DELETE FROM sov_log WHERE time < ?1", r.sov);
        run("DELETE FROM char_moves WHERE time < ?1", r.moves);
        run("DELETE FROM local_scans WHERE time < ?1", r.local_scans);
        if let Some(before) = cut(r.system_stats) {
            n += self.conn.execute("DELETE FROM system_stats WHERE hour < ?1", params![before / 3600]).unwrap_or(0);
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::mem()
    }

    #[test]
    fn stats_pack_and_unpack() {
        let rows = vec![HourStats { system_id: 30_004_759, ship_kills: 3, pod_kills: 1, npc_kills: 400, jumps: 77 }, HourStats { system_id: 31_000_005, ..Default::default() }];
        assert_eq!(unpack_stats(&pack_stats(&rows)), rows);
    }

    #[test]
    fn the_intel_window_is_replaced_not_doubled_and_old_rows_stay() {
        let s = store();
        let rep = |received: i64, text: &str, sys: i64, pilot: &str| crate::intel::IntelReport {
            received,
            channel: "Delve.Imperium".into(),
            reporter: "Scout".into(),
            text: text.into(),
            systems: vec![crate::intel::DetectedSystem { id: sys, name: String::new(), security: 0.0 }],
            pilots: vec![pilot.into()],
            ..Default::default()
        };
        s.replace_intel_window(0, &[rep(100, "old", 1, "Old Hostile")]);
        s.replace_intel_window(1000, &[rep(1500, "frat +5", 2, "Some Hostile")]);
        s.replace_intel_window(1000, &[rep(1500, "frat +5 merged", 2, "Some Hostile"), rep(1600, "clr", 3, "x")]);
        let all = s.intel_history(0, 9999, &[], None, None, 10);
        assert_eq!(all.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), vec!["clr", "frat +5 merged", "old"]);
        assert_eq!(s.intel_history(0, 9999, &[2], None, None, 10).len(), 1);
        assert_eq!(s.intel_history(0, 9999, &[], Some("some hostile"), None, 10).len(), 1);
        assert_eq!(s.intel_history(0, 9999, &[], None, Some("frat"), 10).len(), 1);
        assert!(s.intel_history(0, 9999, &[22], None, None, 10).is_empty(), "system 2 is not system 22");
    }

    #[test]
    fn kills_moves_and_sov_are_kept_then_pruned_by_their_own_retention() {
        let s = store();
        let now = 100 * 86_400;
        s.log_kill(&KillRow { kill_id: 1, time: now - 40 * 86_400, system_id: 7, attacker_alliances: vec![99], ..Default::default() });
        s.log_kill(&KillRow { kill_id: 2, time: now - 3600, system_id: 7, victim_alliance: Some(5), ..Default::default() });
        s.log_kill(&KillRow { kill_id: 2, time: now - 3600, system_id: 7, ..Default::default() });
        assert_eq!(s.kill_history(0, &[7], None, 10).len(), 2, "a kill is logged once");
        assert_eq!(s.kill_history(0, &[], Some(99), 10)[0].kill_id, 1);
        assert_eq!(s.kill_history(0, &[], Some(5), 10)[0].kill_id, 2);
        s.log_sov(now - 400 * 86_400, 7, Some(1));
        s.log_sov(now - 10, 7, Some(2));
        assert_eq!(s.sov_latest().get(&7), Some(&Some(2)));
        s.log_move(&MoveRow { time: now - 5, character: "Me".into(), from: 1, to: 2, ship: Some(12015), docked: false });
        s.log_system_stats(now / 3600 - 24 * 40, &[HourStats { system_id: 7, jumps: 9, ..Default::default() }]);
        s.log_system_stats(now / 3600, &[HourStats { system_id: 7, jumps: 3, ..Default::default() }]);
        let r = Retention { kills_all: 30, sov: 365, system_stats: 30, ..Retention::default() };
        let gone = s.prune_history(&r, now);
        assert_eq!(gone, 3, "the old kill, the old sov holder and the old hour");
        assert_eq!(s.kill_history(0, &[], None, 10).len(), 1);
        assert_eq!(s.system_stats_history(7, 0).iter().map(|(_, h)| h.jumps).collect::<Vec<_>>(), vec![3]);
        assert_eq!(s.move_history(0, Some("Me"), 10).len(), 1);
        let forever = Retention { kills_all: Retention::FOREVER, ..r };
        s.log_kill(&KillRow { kill_id: 3, time: 1, ..Default::default() });
        s.prune_history(&forever, now);
        assert_eq!(s.kill_history(0, &[], None, 10).len(), 2);
    }
}
