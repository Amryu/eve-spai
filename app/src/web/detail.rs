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
    /// Effective HP per layer, with the app's own average-resist formula, so the two windows cannot
    /// disagree about a number the user reads off both.
    pub shield_ehp: f64,
    pub armor_ehp: f64,
    pub hull_ehp: f64,
    pub drone_cap: f64,
    pub drone_bw: f64,
    pub turrets: i64,
    pub launchers: i64,
    pub high_slots: i64,
    pub mid_slots: i64,
    pub low_slots: i64,
    pub max_velocity: f64,
    pub warp_speed: f64,
    /// Phosphor glyph and label per role badge.
    pub roles: Vec<(String, String)>,
    pub traits: Vec<TraitGroup>,
}

/// One skill's worth of hull bonuses, the way the app groups them.
#[derive(Serialize)]
pub struct TraitGroup {
    /// The skill's name, or "Role Bonuses" for the ones that need no skill.
    pub skill: String,
    /// The bonus figure and its text. Zero means the line carries no number of its own, which is
    /// most role bonuses.
    pub lines: Vec<(f64, String)>,
}

pub fn ship(
    id: i64,
    store: &crate::store::Store,
    names: &std::collections::HashMap<i64, String>,
) -> Option<ShipInfo> {
    let d = store.ship_details(id)?;
    let traits = store.ship_traits(id);
    // Grouped by skill in the order the SDE lists them, which is the order the app shows them in.
    // Role bonuses last, under their own heading, because they apply whatever you have trained.
    let mut skills: Vec<i64> = Vec::new();
    for (s, _, _) in &traits {
        if *s > 0 && !skills.contains(s) {
            skills.push(*s);
        }
    }
    let mut groups: Vec<TraitGroup> = skills
        .iter()
        .map(|skill| TraitGroup {
            skill: format!(
                "{} (per level)",
                names.get(skill).cloned().unwrap_or_else(|| format!("Skill {skill}"))
            ),
            lines: traits
                .iter()
                .filter(|(s, _, _)| s == skill)
                .map(|(_, b, t)| (*b, t.clone()))
                .collect(),
        })
        .collect();
    let role: Vec<(f64, String)> =
        traits.iter().filter(|(s, _, _)| *s == -1).map(|(_, b, t)| (*b, t.clone())).collect();
    if !role.is_empty() {
        groups.push(TraitGroup { skill: "Role Bonuses".to_owned(), lines: role });
    }
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
        shield_ehp: crate::app::layer_ehp(d.shield_hp, d.shield_resist),
        armor_ehp: crate::app::layer_ehp(d.armor_hp, d.armor_resist),
        hull_ehp: crate::app::layer_ehp(d.hull_hp, d.hull_resist),
        drone_cap: d.drone_cap,
        drone_bw: d.drone_bw,
        turrets: d.turret_hardpoints,
        launchers: d.launcher_hardpoints,
        high_slots: d.high_slots,
        mid_slots: d.mid_slots,
        low_slots: d.low_slots,
        max_velocity: d.max_velocity,
        warp_speed: d.warp_speed,
        // The glyph itself, not a name: the page is already serving the same Phosphor font the app
        // draws with, and a second mapping from the app's constants to icon names is a second thing
        // to keep in step.
        roles: crate::app::derive_roles(&traits)
            .into_iter()
            .map(|(g, l)| (g.to_owned(), l.to_owned()))
            .collect(),
        traits: groups,
    })
}

/// Skill ids a ship's traits name, so the caller can resolve them before building the dialog.
pub fn ship_skill_ids(id: i64, store: &crate::store::Store) -> Vec<i64> {
    let mut s: Vec<i64> = store.ship_traits(id).into_iter().map(|t| t.0).filter(|&s| s > 0).collect();
    s.sort_unstable();
    s.dedup();
    s
}
