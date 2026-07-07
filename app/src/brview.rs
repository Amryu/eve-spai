// Off-UI-thread battle-report computation. The render thread only writes `BrInputs` and reads
// `BrOutputs`; this worker does all filtering, roster building, and sorting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::battle::{Battle, Involvement, Participant, PartyKind};
use crate::geo::Systems;
use crate::intel::IntelState;
use crate::settings::{battle_decision, BattleFilter, MatchData, RuleAction, ShipSize};
use crate::zkill::{SharedBattleFilter, SharedBattles, ShipSizes, ANCHOR_JUMPS};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RosterSort {
    #[default]
    Value,
    Hull,
}

#[derive(Clone, Default)]
pub struct BrInputs {
    pub query: String,
    pub min_isk: f64,
    pub show_history: bool,
    pub break_secs: i64,
    pub player_sys: i64,
    pub selected_kid: Option<i64>,
    pub sort: RosterSort,
    pub condensed: bool,
}

#[derive(Clone)]
pub struct CondensedRow {
    pub ship: i64,
    pub total: u32,
    pub lost: u32,
    pub ship_isk: f64,
    pub pod_isk: f64,
}

#[derive(Clone)]
pub struct BattleDetail {
    pub kid: i64,
    pub battle: Battle,
    pub inv: Involvement,
    pub rosters: Vec<Vec<Participant>>,
    pub condensed: Vec<Vec<CondensedRow>>,
    pub ship_ids: Vec<i64>,
}

#[derive(Default)]
pub struct BrOutputs {
    pub sig: u64,
    pub ready: bool,
    pub cards: Vec<(i64, Option<u32>, Battle)>,
    pub total: usize,
    pub filtered: usize,
    pub detail: Option<Arc<BattleDetail>>,
}

pub type SharedInputs = Arc<Mutex<BrInputs>>;
pub type SharedOutputs = Arc<Mutex<BrOutputs>>;
/// Lets the UI wake the worker the instant inputs change (e.g. a new selection) instead of waiting
/// for its next poll, so opening a battle's detail doesn't lag a frame or two.
pub type Wake = Arc<(Mutex<bool>, std::sync::Condvar)>;

pub fn poke(wake: &Wake) {
    let (lock, cv) = &**wake;
    *lock.lock().unwrap() = true;
    cv.notify_one();
}

fn jumps_to(systems: &Systems, player_sys: Option<i64>, target: i64) -> Option<u32> {
    systems.jumps(target, player_sys?, 50)
}

fn intel_systems(intel: &Arc<Mutex<IntelState>>) -> Vec<i64> {
    intel
        .lock()
        .unwrap()
        .reports
        .iter()
        .flat_map(|r| r.systems.iter().map(|s| s.id))
        .collect()
}

fn in_tracked_area(b: &Battle, systems: &Systems, intel_sys: &[i64]) -> bool {
    b.systems
        .iter()
        .any(|(id, _, _)| intel_sys.iter().any(|&s| systems.jumps(*id, s, ANCHOR_JUMPS).is_some()))
}

fn match_data(
    b: &Battle,
    max_jumps: Option<u32>,
    systems: &Systems,
    type_names: &HashMap<i64, String>,
    ship_sizes: &HashMap<i64, ShipSize>,
    intel_sys: &[i64],
    player_sys: Option<i64>,
) -> MatchData {
    let mut d = MatchData { total_isk: Some(b.isk), ..Default::default() };
    for (id, name, _) in &b.systems {
        d.systems.insert(name.to_lowercase());
        if let Some(info) = systems.info_of(*id) {
            if !info.region.is_empty() {
                d.regions.insert(info.region.to_lowercase());
            }
            if !info.constellation.is_empty() {
                d.constellations.insert(info.constellation.to_lowercase());
            }
        }
    }
    for side in &b.sides {
        if let Some(c) = &side.coalition {
            d.coalitions.insert(c.to_lowercase());
        }
        for p in &side.parties {
            match p.kind {
                PartyKind::Alliance => {
                    d.alliances.insert(p.name.to_lowercase());
                    if let Some(c) = crate::packs::coalition_of(p.id) {
                        d.coalitions.insert(c.to_lowercase());
                    }
                }
                PartyKind::Corporation => {
                    d.corporations.insert(p.name.to_lowercase());
                }
                _ => {}
            }
        }
    }
    let mut max = ShipSize::Other;
    let mut note_ship = |id: i64, d: &mut MatchData| {
        if let Some(&sz) = ship_sizes.get(&id) {
            if sz > max {
                max = sz;
            }
        }
        if let Some(n) = type_names.get(&id) {
            d.ship_names.insert(n.to_lowercase());
        }
    };
    for e in &b.engagements {
        note_ship(e.victim_ship, &mut d);
        for a in &e.attackers {
            note_ship(a.ship, &mut d);
        }
        d.pilots.insert(e.victim_pilot.to_lowercase());
        for a in &e.attackers {
            d.pilots.insert(a.pilot.to_lowercase());
        }
    }
    d.max_size = max;
    d.in_intel_area = in_tracked_area(b, systems, intel_sys);
    if let (Some(maxj), Some(me)) = (max_jumps, player_sys) {
        d.min_jumps_from_me =
            b.systems.iter().filter_map(|(id, _, _)| systems.jumps(*id, me, maxj)).min();
    }
    d
}

fn shown(
    b: &Battle,
    rules: &BattleFilter,
    systems: &Systems,
    type_names: &HashMap<i64, String>,
    ship_sizes: &HashMap<i64, ShipSize>,
    intel_sys: &[i64],
    player_sys: Option<i64>,
) -> bool {
    if rules.is_default_only() {
        return in_tracked_area(b, systems, intel_sys);
    }
    let data =
        match_data(b, rules.max_jumps_condition(), systems, type_names, ship_sizes, intel_sys, player_sys);
    match battle_decision(&rules.rules, &data) {
        Some(RuleAction::Include) => true,
        Some(RuleAction::Exclude) => false,
        None => in_tracked_area(b, systems, intel_sys),
    }
}

fn name_of(id: i64, type_names: &HashMap<i64, String>) -> String {
    if id == 0 {
        return "?".to_owned();
    }
    crate::intel::structure_name_by_type(id)
        .map(|s| s.to_owned())
        .or_else(|| type_names.get(&id).cloned())
        .unwrap_or_else(|| format!("Type {id}"))
}

/// Produce the per-side render data for the current sort/condensed: participant rows sorted for the
/// normal view, and hull-aggregated rows for the condensed view. Callers render these verbatim.
pub fn sorted_detail(
    rosters: &[Vec<Participant>],
    sort: RosterSort,
    ship_sizes: &HashMap<i64, ShipSize>,
    type_names: &HashMap<i64, String>,
) -> (Vec<Vec<Participant>>, Vec<Vec<CondensedRow>>) {
    let mut rows_out: Vec<Vec<Participant>> = Vec::with_capacity(rosters.len());
    let mut cond_out: Vec<Vec<CondensedRow>> = Vec::with_capacity(rosters.len());
    for roster in rosters {
        // Normal rows: roster() is already value-sorted; only Hull needs a resort.
        let mut rows = roster.clone();
        if matches!(sort, RosterSort::Hull) {
            let val = |p: &Participant| p.lost.as_ref().map_or(0.0, |l| l.value + l.pod_value);
            rows.sort_by(|a, b| {
                let sa = ship_sizes.get(&a.ship).copied().unwrap_or(ShipSize::Other);
                let sb = ship_sizes.get(&b.ship).copied().unwrap_or(ShipSize::Other);
                sb.cmp(&sa)
                    .then(a.ship.cmp(&b.ship))
                    .then_with(|| val(b).total_cmp(&val(a)))
                    .then(a.pilot.cmp(&b.pilot))
            });
        }
        rows_out.push(rows);

        // Condensed: aggregate by hull then sort.
        let mut order: Vec<i64> = Vec::new();
        let mut agg: HashMap<i64, (u32, u32, f64, f64)> = HashMap::new();
        for p in roster.iter() {
            let e = agg.entry(p.ship).or_insert_with(|| {
                order.push(p.ship);
                (0, 0, 0.0, 0.0)
            });
            e.0 += 1;
            if let Some(l) = &p.lost {
                e.1 += 1;
                e.2 += l.value;
                e.3 += l.pod_value;
            }
        }
        order.sort_by(|a, b| {
            let (ta, tb) = (agg[a], agg[b]);
            let (va, vb) = (ta.2 + ta.3, tb.2 + tb.3);
            match sort {
                RosterSort::Value => vb.total_cmp(&va).then(tb.1.cmp(&ta.1)).then(tb.0.cmp(&ta.0)),
                RosterSort::Hull => {
                    let sa = ship_sizes.get(a).copied().unwrap_or(ShipSize::Other);
                    let sb = ship_sizes.get(b).copied().unwrap_or(ShipSize::Other);
                    sb.cmp(&sa).then_with(|| vb.total_cmp(&va))
                }
            }
            .then_with(|| name_of(*a, type_names).cmp(&name_of(*b, type_names)))
        });
        cond_out.push(
            order
                .into_iter()
                .map(|ship| {
                    let (total, lost, ship_isk, pod_isk) = agg[&ship];
                    CondensedRow { ship, total, lost, ship_isk, pod_isk }
                })
                .collect(),
        );
    }
    (rows_out, cond_out)
}

const MAX_CARDS: usize = 150;
const MAX_CANDIDATES: usize = 1000;

struct Deps {
    systems: Option<Arc<Systems>>,
    intel: Arc<Mutex<IntelState>>,
    battles: SharedBattles,
    history: SharedBattles,
    filter: SharedBattleFilter,
    ship_sizes: ShipSizes,
    type_names: Arc<Mutex<HashMap<i64, String>>>,
    overrides_gen: Arc<AtomicU64>,
    filter_gen: Arc<AtomicU64>,
}

/// The signature both the worker and the UI compute from the same inputs, so the UI can tell
/// whether the published outputs are current (render) or stale (spinner).
pub fn ui_signature(
    battles: &SharedBattles,
    history: &SharedBattles,
    filter_gen: &AtomicU64,
    overrides_gen: &AtomicU64,
    intel: &Arc<Mutex<IntelState>>,
    inp: &BrInputs,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let source = if inp.show_history { history } else { battles };
    {
        let b = source.lock().unwrap();
        b.len().hash(&mut h);
        if let Some(f) = b.first() {
            f.end.hash(&mut h);
            f.kills.hash(&mut h);
        }
        if let Some(l) = b.last() {
            l.end.hash(&mut h);
            l.kills.hash(&mut h);
        }
    }
    inp.query.hash(&mut h);
    inp.show_history.hash(&mut h);
    inp.player_sys.hash(&mut h);
    filter_gen.load(Ordering::Relaxed).hash(&mut h);
    overrides_gen.load(Ordering::Relaxed).hash(&mut h);
    inp.break_secs.hash(&mut h);
    intel.lock().unwrap().reports.len().hash(&mut h);
    inp.min_isk.to_bits().hash(&mut h);
    inp.selected_kid.hash(&mut h);
    inp.sort.hash(&mut h);
    inp.condensed.hash(&mut h);
    h.finish()
}

fn signature(deps: &Deps, inp: &BrInputs) -> u64 {
    ui_signature(&deps.battles, &deps.history, &deps.filter_gen, &deps.overrides_gen, &deps.intel, inp)
}

fn compute(deps: &Deps, inp: &BrInputs, sig: u64) -> BrOutputs {
    let mut out = BrOutputs { sig, ready: true, ..Default::default() };
    let Some(systems) = deps.systems.clone() else { return out };
    let player = (inp.player_sys != 0).then_some(inp.player_sys);
    let source = if inp.show_history { &deps.history } else { &deps.battles };
    // Snapshot under a short lock, then do the heavy filtering + jump-distance work lock-free.
    // Holding `source` across the whole cards loop stalls the UI thread, which locks the same
    // battles list every frame in `ui_signature` (felt as a freeze when opening a battle).
    let battles: Vec<Battle> = source.lock().unwrap().clone();

    let intel_sys = intel_systems(&deps.intel);
    let query = inp.query.trim().to_lowercase();

    // Cards.
    {
        let type_names = deps.type_names.lock().unwrap();
        let rules = deps.filter.lock().unwrap();
        let mut cands: Vec<(i64, Option<u32>, f64, Battle)> = Vec::new();
        for b in battles.iter() {
            let vis = inp.show_history
                || shown(b, &rules, &systems, &type_names, &deps.ship_sizes, &intel_sys, player);
            if b.kills >= 2 && b.matches(&query) && vis {
                let from_you =
                    b.systems.iter().filter_map(|(id, _, _)| jumps_to(&systems, player, *id)).min();
                let kid = b.engagements.iter().map(|e| e.kill_id).max().unwrap_or(0);
                let light = Battle {
                    engagements: Vec::new(),
                    start: b.start,
                    end: b.end,
                    systems: b.systems.clone(),
                    sides: b.sides.clone(),
                    kills: b.kills,
                    isk: b.isk,
                    ambiguous: b.ambiguous,
                    suggested_splits: b.suggested_splits.clone(),
                };
                cands.push((kid, from_you, b.isk, light));
                if cands.len() >= MAX_CANDIDATES {
                    break;
                }
            }
        }
        out.total = cands.iter().filter(|c| c.2 >= inp.min_isk).count();
        out.filtered = cands.len() - out.total;
        out.cards = cands
            .into_iter()
            .filter(|c| c.2 >= inp.min_isk)
            .take(MAX_CARDS)
            .map(|(kid, from_you, _, b)| (kid, from_you, b))
            .collect();
    }

    // Detail for the selected battle.
    if let Some(kid) = inp.selected_kid {
        let b = battles
            .iter()
            .find(|b| b.engagements.iter().any(|e| e.kill_id == kid))
            .cloned();
        if let Some(b) = b {
            let ship_ids: Vec<i64> = b
                .engagements
                .iter()
                .flat_map(|e| {
                    let mut v = vec![e.victim_ship];
                    v.extend(e.attackers.iter().map(|a| a.ship));
                    v
                })
                .filter(|&id| id != 0)
                .collect();
            let inv = b.involvement();
            let rosters: Vec<Vec<Participant>> = (0..b.sides.len()).map(|i| b.roster(i)).collect();
            let type_names = deps.type_names.lock().unwrap();
            let (rosters, condensed) =
                sorted_detail(&rosters, inp.sort, &deps.ship_sizes, &type_names);
            out.detail =
                Some(Arc::new(BattleDetail { kid, battle: b, inv, rosters, condensed, ship_ids }));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    systems: Option<Arc<Systems>>,
    intel: Arc<Mutex<IntelState>>,
    battles: SharedBattles,
    history: SharedBattles,
    filter: SharedBattleFilter,
    ship_sizes: ShipSizes,
    type_names: Arc<Mutex<HashMap<i64, String>>>,
    overrides_gen: Arc<AtomicU64>,
    filter_gen: Arc<AtomicU64>,
    inputs: SharedInputs,
    outputs: SharedOutputs,
    wake: Wake,
    battles_enabled: Arc<std::sync::atomic::AtomicBool>,
    ctx: egui::Context,
) {
    let worker = Deps {
        systems,
        intel,
        battles,
        history,
        filter,
        ship_sizes,
        type_names,
        overrides_gen,
        filter_gen,
    };
    std::thread::spawn(move || {
        let mut last = 1u64;
        loop {
            {
                let (lock, cv) = &*wake;
                let g = lock.lock().unwrap();
                let (mut g, _) = cv.wait_timeout(g, Duration::from_millis(33)).unwrap();
                *g = false;
            }
            if !battles_enabled.load(Ordering::Relaxed) {
                continue;
            }
            let inp = inputs.lock().unwrap().clone();
            let sig = signature(&worker, &inp);
            if sig == last {
                continue;
            }
            last = sig;
            let out = compute(&worker, &inp, sig);
            *outputs.lock().unwrap() = out;
            ctx.request_repaint();
        }
    });
}
