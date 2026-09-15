//! What a dialog needs, built on request.
//!
//! Plain reads rather than `IntelClick`, because a tap on a phone must not open a desktop viewport.

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
    pub gates: Vec<(i64, String)>,
    pub sov: Option<String>,
    pub adm: Option<f64>,
    pub incursion: bool,
    pub ship_kills: u32,
    pub pod_kills: u32,
    pub npc_kills: u32,
    pub jumps_from_you: Option<u32>,
    /// (from, light-years), as in the app's system tooltip.
    pub ly_from_staging: Option<(String, f64)>,
    pub ly_from_you: Option<(String, f64)>,
    /// Last-hour counters with their region averages. The app colours a counter against its region,
    /// since twenty kills is a quiet hour in Delve and a siege in Aridia.
    pub jumps: u32,
    pub avg_jumps: f64,
    pub avg_ship_kills: f64,
    pub avg_npc_kills: f64,
    pub bookmarked: bool,
    pub fw: Option<String>,
    /// Alliance holding sov, for its logo.
    pub sov_alliance: Option<i64>,
    pub camp: Option<CampInfo>,
    pub rats: Option<RatInfo>,
    pub holes: Vec<HoleInfo>,
    pub upgrades: Vec<String>,
    /// Every graph neighbour, as the app's neighbour buttons show them.
    pub neighbours: Vec<Neighbour>,
}

#[derive(Serialize)]
pub struct CampInfo {
    /// "likely", "possible" or "flag", matching the app's three levels.
    pub level: &'static str,
    pub kills: usize,
    pub span_min: i64,
    pub age_min: i64,
}

#[derive(Serialize)]
pub struct RatInfo {
    pub faction: &'static str,
    pub deal: [&'static str; 2],
    pub weak: [&'static str; 2],
    pub ewar: Option<&'static str>,
}

#[derive(Serialize)]
pub struct HoleInfo {
    pub sig: String,
    pub to: String,
    pub to_id: Option<i64>,
    pub kind: Option<String>,
    pub size: Option<String>,
    /// Hours left, or `None` once it is into its final, unpredictable stretch.
    pub hours: Option<i64>,
}

#[derive(Serialize)]
pub struct Neighbour {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub constellation: String,
    pub region: String,
    pub cross_const: bool,
    pub cross_region: bool,
}

pub fn system(id: i64, d: &super::DetailState) -> Option<SystemInfo> {
    let graph = d.graph.as_deref()?;
    let info = graph.info_of(id)?;
    let status = d.status.get(&id);
    let now = chrono::Utc::now().timestamp();

    let mut gates: Vec<(i64, String)> = graph
        .neighbors_gates_only(id)
        .iter()
        .filter_map(|n| graph.info_of(*n).map(|i| (i.id, i.name.clone())))
        .collect();
    gates.sort_by(|a, b| a.1.cmp(&b.1));

    // All graph neighbours, not only gates, so a scanned hole shows up as a destination.
    let mut neighbours: Vec<Neighbour> = graph
        .neighbors(id)
        .iter()
        .filter_map(|n| graph.info_of(*n))
        .map(|ni| Neighbour {
            id: ni.id,
            name: ni.name.clone(),
            security: ni.security,
            constellation: ni.constellation.clone(),
            region: ni.region.clone(),
            cross_const: ni.constellation != info.constellation,
            cross_region: ni.region != info.region && !ni.region.is_empty(),
        })
        .collect();
    neighbours.sort_by(|a, b| a.name.cmp(&b.name));

    // Averaged over every system in the region, as the app does.
    let region_ids: Vec<i64> = d
        .store
        .as_ref()
        .and_then(|s| s.region_of_system(id).map(|r| s.region_systems(r)))
        .map(|v| v.into_iter().map(|m| m.id).collect())
        .unwrap_or_default();
    let avg = |sel: &dyn Fn(&crate::systemstatus::SysFlags) -> u32| -> f64 {
        if region_ids.is_empty() {
            return 0.0;
        }
        let sum: u64 = region_ids.iter().filter_map(|s| d.status.get(s)).map(|f| sel(f) as u64).sum();
        sum as f64 / region_ids.len() as f64
    };

    let camp = d.camps.as_ref().and_then(|c| {
        c.lock().unwrap_or_else(|e| e.into_inner()).camp(id, now).map(|c| CampInfo {
            level: match c.level {
                crate::camp::CampLevel::Likely => "likely",
                crate::camp::CampLevel::Possible => "possible",
                crate::camp::CampLevel::Flag => "flag",
            },
            kills: c.kills,
            span_min: (c.span / 60).max(0),
            age_min: (c.age / 60).max(0),
        })
    });

    let rats = crate::rats::rat_profile(&info.region).map(|rp| RatInfo {
        faction: rp.faction,
        deal: rp.deal,
        weak: rp.weak,
        ewar: (rp.ewar != "None").then_some(rp.ewar),
    });

    // A hole is listed from either end, so the "other side" is whichever end is not this system.
    let holes: Vec<HoleInfo> = d
        .wh_cache
        .iter()
        .filter(|w| w.system_id == id || w.dest_system_id == Some(id))
        .map(|w| {
            let here_is_near = w.system_id == id;
            let other_id = if here_is_near { w.dest_system_id } else { Some(w.system_id) };
            HoleInfo {
                sig: if here_is_near { w.signature.clone() } else { w.dest_signature.clone() }
                    .unwrap_or_else(|| "?".to_owned()),
                to: other_id
                    .and_then(|sid| graph.info_of(sid).map(|i| i.name.clone()))
                    .unwrap_or_else(|| w.dest.label().to_owned()),
                to_id: other_id.filter(|sid| graph.info_of(*sid).is_some()),
                kind: w.wh_type.clone(),
                size: w.effective_size().map(|s| s.label().to_owned()),
                hours: w.hours_left(now),
            }
        })
        .collect();

    let upgrades: Vec<String> = d
        .sov_upgrades
        .iter()
        .filter(|u| u.system.eq_ignore_ascii_case(&info.name))
        .flat_map(|u| crate::app::split_upgrade_label(&u.upgrade))
        .map(|u| u.to_owned())
        .collect();

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
        // Same walk as `jumps_from_you`, using the borrow already held.
        jumps_from_you: d.player_sys.and_then(|from| {
            if d.count_bridges {
                graph.jumps(id, from, crate::app::JUMP_SCAN_CAP)
            } else {
                graph.jumps_gates_only(id, from, crate::app::JUMP_SCAN_CAP)
            }
        }),
        ly_from_staging: d
            .staging
            .as_deref()
            .and_then(|n| graph.lookup(n.trim()))
            .and_then(|st| Some((st.name.clone(), graph.ly_between(st.id, id)?))),
        ly_from_you: d
            .player_sys
            .filter(|_| !d.active_character.is_empty() && d.active_character != "No character")
            .and_then(|from| Some((d.active_character.clone(), graph.ly_between(from, id)?))),
        jumps: status.map_or(0, |s| s.jumps),
        avg_jumps: avg(&|f| f.jumps),
        avg_ship_kills: avg(&|f| f.ship_kills),
        avg_npc_kills: avg(&|f| f.npc_kills),
        bookmarked: d.bookmarks.contains(&id),
        fw: status.and_then(|s| s.fw.clone()),
        sov_alliance: status.and_then(|s| s.sov_alliance),
        camp,
        rats,
        holes,
        upgrades,
        neighbours,
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
    /// Effective HP per layer, with the app's own average-resist formula.
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
    /// The bonus figure and its text. Zero means the line has no number, as with most role bonuses.
    pub lines: Vec<(f64, String)>,
}

pub fn ship(
    id: i64,
    store: &crate::store::Store,
    names: &std::collections::HashMap<i64, String>,
) -> Option<ShipInfo> {
    let d = store.ship_details(id)?;
    let traits = store.ship_traits(id);
    // SDE order, as the app shows them. Role bonuses last, since they need no skill.
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
        // The glyph itself, since the page serves the same Phosphor font and a name mapping would
        // be one more thing to keep in step.
        roles: crate::app::derive_roles(&traits)
            .into_iter()
            .map(|(g, l)| (g.to_owned(), l.to_owned()))
            .collect(),
        traits: groups,
    })
}

/// So the caller can resolve skill names before building the dialog.
pub fn ship_skill_ids(id: i64, store: &crate::store::Store) -> Vec<i64> {
    let mut s: Vec<i64> = store.ship_traits(id).into_iter().map(|t| t.0).filter(|&s| s > 0).collect();
    s.sort_unstable();
    s.dedup();
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_carries_light_years_from_staging_and_you() {
        let d = super::super::DetailState {
            graph: Some(crate::uitest::fixtures::systems()),
            player_sys: Some(30_003_704),
            active_character: "Amryu".into(),
            staging: Some("319-3D".into()),
            ..Default::default()
        };
        let s = super::system(30_004_759, &d).unwrap();
        let (from, ly) = s.ly_from_staging.unwrap();
        assert_eq!((from.as_str(), (ly * 100.0).round()), ("319-3D", 210.0));
        let (from, ly) = s.ly_from_you.unwrap();
        assert_eq!((from.as_str(), (ly * 100.0).round()), ("Amryu", 533.0));

        let d = super::super::DetailState { graph: d.graph.clone(), ..Default::default() };
        let s = super::system(30_004_759, &d).unwrap();
        assert!(s.ly_from_staging.is_none() && s.ly_from_you.is_none());
    }
}
