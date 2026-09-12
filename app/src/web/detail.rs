//! What a dialog needs, built on request.
//!
//! The dialogs render in the browser rather than opening a window on the desktop, which is what the
//! user asked for: a tap on a phone must not raise a viewport on a machine in another room. So these
//! are plain reads, not `IntelClick`, which exists precisely to open a viewport.

use serde::Serialize;

#[derive(Serialize, Default)]
pub struct SystemInfo {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub constellation: String,
    pub region: String,
    pub faction: String,
    pub jove: bool,
    pub wormhole: bool,
    /// Gate neighbours, named, so the dialog can offer somewhere to go next.
    pub gates: Vec<(i64, String)>,
    pub sov: Option<String>,
    pub adm: Option<f64>,
    pub incursion: bool,
    pub ship_kills: u32,
    pub pod_kills: u32,
    pub npc_kills: u32,
    pub jumps_from_you: Option<u32>,
}

pub fn system(
    id: i64,
    graph: &crate::geo::Systems,
    status: Option<&crate::systemstatus::SysFlags>,
    player_sys: Option<i64>,
    count_bridges: bool,
) -> Option<SystemInfo> {
    let info = graph.info_of(id)?;
    let mut gates: Vec<(i64, String)> = graph
        .neighbors_gates_only(id)
        .iter()
        .filter_map(|n| graph.info_of(*n).map(|i| (i.id, i.name.clone())))
        .collect();
    gates.sort_by(|a, b| a.1.cmp(&b.1));
    Some(SystemInfo {
        id,
        name: info.name.clone(),
        security: info.security,
        constellation: info.constellation.clone(),
        region: info.region.clone(),
        faction: info.faction.clone(),
        jove: crate::jove::has(id),
        wormhole: crate::geo::is_wormhole_system(id),
        gates,
        sov: status.and_then(|s| s.sov.clone()),
        adm: status.and_then(|s| s.adm),
        incursion: status.is_some_and(|s| s.incursion),
        ship_kills: status.map_or(0, |s| s.ship_kills),
        pod_kills: status.map_or(0, |s| s.pod_kills),
        npc_kills: status.map_or(0, |s| s.npc_kills),
        // Same walk `jumps_from_you` does, against the borrow this function already holds.
        jumps_from_you: player_sys.and_then(|from| {
            if count_bridges {
                graph.jumps(id, from, crate::app::JUMP_SCAN_CAP)
            } else {
                graph.jumps_gates_only(id, from, crate::app::JUMP_SCAN_CAP)
            }
        }),
    })
}


#[derive(Serialize)]
pub struct ShipInfo {
    pub id: i64,
    pub name: String,
    pub group: String,
    pub shield_hp: f64,
    pub armor_hp: f64,
    pub hull_hp: f64,
    pub shield_resist: [u32; 4],
    pub armor_resist: [u32; 4],
    pub hull_resist: [u32; 4],
    pub drone_cap: f64,
    pub drone_bw: f64,
    pub turrets: i64,
    pub launchers: i64,
    pub traits: Vec<String>,
}

pub fn ship(id: i64, store: &crate::store::Store) -> Option<ShipInfo> {
    let d = store.ship_details(id)?;
    Some(ShipInfo {
        id,
        name: d.name,
        group: d.group,
        shield_hp: d.shield_hp,
        armor_hp: d.armor_hp,
        hull_hp: d.hull_hp,
        shield_resist: d.shield_resist,
        armor_resist: d.armor_resist,
        hull_resist: d.hull_resist,
        drone_cap: d.drone_cap,
        drone_bw: d.drone_bw,
        turrets: d.turret_hardpoints,
        launchers: d.launcher_hardpoints,
        traits: store.ship_traits(id).into_iter().map(|(_, _, t)| t).collect(),
    })
}
