// Off-UI-thread battle-report computation. The render thread only writes `BrInputs` and reads
// `BrOutputs`; this worker does all filtering, roster building, and sorting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_core::battle::{Battle, Involvement, Participant, PartyKind};
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
    /// The open report's systems to show, all of them when empty.
    pub systems: Vec<i64>,
    /// Battles with fewer pilots are left out of the list.
    pub min_pilots: u32,
    /// Alliances, corporations or coalitions, lowercase: only battles one of them took part in.
    pub parties: Vec<String>,
}

/// One battle in the list: its newest kill, jumps from you, pilots, and the battle without kills.
pub type Card = (i64, Option<u32>, u32, Battle);

/// Everyone in a battle: victims and attackers, once each.
pub fn pilot_count(b: &Battle) -> u32 {
    let mut ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for e in &b.engagements {
        if e.victim_char != 0 {
            ids.insert(e.victim_char);
        }
        ids.extend(e.attackers.iter().map(|a| a.char_id).filter(|id| *id != 0));
    }
    ids.len() as u32
}

/// Whether one of `parties` (lowercase names) took part, by alliance, corporation or coalition.
pub fn has_party(b: &Battle, parties: &[String]) -> bool {
    parties.is_empty()
        || b.sides.iter().any(|s| {
            s.coalition.as_deref().is_some_and(|c| parties.iter().any(|p| c.eq_ignore_ascii_case(p)))
                || s.parties.iter().any(|x| parties.iter().any(|p| x.name.eq_ignore_ascii_case(p)))
        })
}

#[derive(Clone)]
pub struct CondensedRow {
    pub ship: i64,
    pub total: u32,
    pub lost: u32,
    pub ship_isk: f64,
    pub pod_isk: f64,
    /// Damage dealt by this side's pilots flying this hull, from the kills themselves.
    pub damage: i64,
}

#[derive(Clone)]
pub struct BattleDetail {
    pub kid: i64,
    pub battle: Battle,
    pub inv: Involvement,
    pub rosters: Vec<Vec<Participant>>,
    pub condensed: Vec<Vec<CondensedRow>>,
    pub ship_ids: Vec<i64>,
    pub tiles: SideTiles,
    /// The fight through the systems picked, which everything shown is worked out from; `None`
    /// when all of them are. `battle` stays whole for saving, sharing and editing.
    pub shown: Option<Battle>,
}

impl BattleDetail {
    /// The battle as it is shown.
    pub fn view(&self) -> &Battle {
        self.shown.as_ref().unwrap_or(&self.battle)
    }
}

/// One hull type a side flew, as the report's tile shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShipTile {
    pub ship: i64,
    pub lost: u32,
    pub total: u32,
}

/// Per side: its pilots, and its hulls with how many of each were lost.
#[derive(Clone, Debug, Default)]
pub struct SideTiles {
    pub pilots: Vec<usize>,
    pub tiles: Vec<Vec<ShipTile>>,
}

/// The tiles of each side, as the web report groups them: one per hull, capsules left out, the
/// heaviest-hit hulls first, then the most flown.
pub fn ship_tiles(rosters: &[Vec<Participant>]) -> SideTiles {
    let mut out = SideTiles::default();
    for roster in rosters {
        let mut by_ship: HashMap<i64, (u32, u32)> = HashMap::new();
        for p in roster.iter().filter(|p| !br_core::battle::POD_TYPES.contains(&p.ship)) {
            let e = by_ship.entry(p.ship).or_default();
            e.1 += 1;
            if p.lost.is_some() {
                e.0 += 1;
            }
        }
        let mut tiles: Vec<ShipTile> = by_ship.into_iter().map(|(ship, (lost, total))| ShipTile { ship, lost, total }).collect();
        tiles.sort_by(|a, b| b.lost.cmp(&a.lost).then(b.total.cmp(&a.total)).then(a.ship.cmp(&b.ship)));
        out.pilots.push(tiles.iter().map(|t| t.total as usize).sum());
        out.tiles.push(tiles);
    }
    out
}

#[derive(Default)]
pub struct BrOutputs {
    pub sig: u64,
    pub ready: bool,
    pub cards: Vec<Card>,
    pub total: usize,
    pub filtered: usize,
    /// Alliance and coalition names in the listed battles, most battles first, to pick from.
    pub party_names: Vec<String>,
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

/// How far a card's systems may be from you and still say so.
const FROM_YOU_JUMPS: u32 = 50;

/// The distances the card filter asks about, walked once a pass. A walk per battle (and one per
/// battle and intel system for the tracked area) took seconds in a busy day, on every selection.
struct Reach {
    /// Jumps from each system to you.
    to_me: HashMap<i64, u32>,
    /// Systems within `ANCHOR_JUMPS` of somewhere intel named.
    area: std::collections::HashSet<i64>,
}

impl Reach {
    fn new(systems: &Systems, intel_sys: &[i64], player_sys: Option<i64>, max_jumps: u32) -> Self {
        let to_me = player_sys.map(|p| systems.jumps_to(p, max_jumps)).unwrap_or_default();
        let mut named = intel_sys.to_vec();
        named.sort_unstable();
        named.dedup();
        let mut area = std::collections::HashSet::new();
        for s in named {
            area.extend(systems.jumps_to(s, ANCHOR_JUMPS).into_keys());
        }
        Reach { to_me, area }
    }

    fn in_area(&self, b: &Battle) -> bool {
        b.systems.iter().any(|(id, _, _)| self.area.contains(id))
    }

    fn from_me(&self, b: &Battle, max_jumps: u32) -> Option<u32> {
        b.systems.iter().filter_map(|(id, _, _)| self.to_me.get(id).copied()).filter(|&d| d <= max_jumps).min()
    }
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

fn match_data(
    b: &Battle,
    max_jumps: Option<u32>,
    systems: &Systems,
    type_names: &HashMap<i64, String>,
    ship_sizes: &HashMap<i64, ShipSize>,
    reach: &Reach,
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
                    if let Some(c) = br_core::packs::coalition_of(p.id) {
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
    d.in_intel_area = reach.in_area(b);
    if let Some(maxj) = max_jumps {
        d.min_jumps_from_me = reach.from_me(b, maxj);
    }
    d
}

fn shown(
    b: &Battle,
    rules: &BattleFilter,
    systems: &Systems,
    type_names: &HashMap<i64, String>,
    ship_sizes: &HashMap<i64, ShipSize>,
    reach: &Reach,
) -> bool {
    if rules.is_default_only() {
        return reach.in_area(b);
    }
    let data = match_data(b, rules.max_jumps_condition(), systems, type_names, ship_sizes, reach);
    match battle_decision(&rules.rules, &data) {
        Some(RuleAction::Include) => true,
        Some(RuleAction::Exclude) => false,
        None => reach.in_area(b),
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
/// Fills each condensed row's damage: every attacker in that hull on that side, summed once per kill.
pub fn condensed_damage(b: &Battle, cond: &mut [Vec<CondensedRow>]) {
    let mut by: HashMap<(usize, i64), i64> = HashMap::new();
    for e in &b.engagements {
        for a in &e.attackers {
            if let Some(i) = b.side_of(&a.party) {
                *by.entry((i, a.ship)).or_default() += a.damage;
            }
        }
    }
    for (i, rows) in cond.iter_mut().enumerate() {
        for r in rows.iter_mut() {
            r.damage = by.get(&(i, r.ship)).copied().unwrap_or(0);
        }
    }
}

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
                    CondensedRow { ship, total, lost, ship_isk, pod_isk, damage: 0 }
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
    inp.systems.hash(&mut h);
    inp.min_pilots.hash(&mut h);
    inp.parties.hash(&mut h);
    h.finish()
}

/// How long after the battles view last drew the worker keeps recomputing.
const DEMAND_GRACE_MS: u64 = 2_000;

pub fn now_ms() -> u64 {
    crate::clock::system()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Called by the battles view every frame it draws: the worker runs while someone is looking.
pub fn want(demand: &AtomicU64) {
    demand.store(now_ms(), Ordering::Relaxed);
}

fn signature(deps: &Deps, inp: &BrInputs) -> u64 {
    ui_signature(&deps.battles, &deps.history, &deps.filter_gen, &deps.overrides_gen, &deps.intel, inp)
}

fn compute(deps: &Deps, inp: &BrInputs, sig: u64) -> BrOutputs {
    let mut out = BrOutputs { sig, ready: true, ..Default::default() };
    let Some(systems) = deps.systems.clone() else { return out };
    let player = (inp.player_sys != 0).then_some(inp.player_sys);
    let source = if inp.show_history { &deps.history } else { &deps.battles };
    // Snapshot under a short lock: the UI thread locks the same battles list every frame in
    // `ui_signature`, so holding it across the cards loop freezes the UI.
    let battles: Vec<Battle> = source.lock().unwrap().clone();

    let intel_sys = intel_systems(&deps.intel);
    let query = inp.query.trim().to_lowercase();
    // A copy, not the lock: the UI thread reads the same names every frame to draw ship names, and
    // waited out the whole filter and roster build below when it was held throughout.
    let type_names = deps.type_names.lock().unwrap().clone();

    {
        let rules = deps.filter.lock().unwrap();
        let reach = Reach::new(&systems, &intel_sys, player, FROM_YOU_JUMPS.max(rules.max_jumps_condition().unwrap_or(0)));
        let mut cands: Vec<(i64, Option<u32>, f64, u32, Battle)> = Vec::new();
        let mut seen: HashMap<String, u32> = HashMap::new();
        for b in battles.iter() {
            let vis = inp.show_history
                || shown(b, &rules, &systems, &type_names, &deps.ship_sizes, &reach);
            if b.kills >= 2 && b.matches(&query) && vis {
                let from_you =
                    reach.from_me(b, FROM_YOU_JUMPS);
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
                for s in &light.sides {
                    let mut names: Vec<&str> = s.parties.iter().filter(|p| p.kind == br_core::battle::PartyKind::Alliance).map(|p| p.name.as_str()).collect();
                    names.extend(s.coalition.as_deref());
                    for n in names {
                        *seen.entry(n.to_owned()).or_default() += 1;
                    }
                }
                cands.push((kid, from_you, b.isk, pilot_count(b), light));
                if cands.len() >= MAX_CANDIDATES {
                    break;
                }
            }
        }
        let mut names: Vec<(String, u32)> = seen.into_iter().collect();
        names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out.party_names = names.into_iter().map(|(n, _)| n).collect();
        let keep = |c: &(i64, Option<u32>, f64, u32, Battle)| c.2 >= inp.min_isk && c.3 >= inp.min_pilots && has_party(&c.4, &inp.parties);
        out.total = cands.iter().filter(|c| keep(c)).count();
        out.filtered = cands.len() - out.total;
        out.cards = cands
            .into_iter()
            .filter(|c| keep(c))
            .take(MAX_CARDS)
            .map(|(kid, from_you, _, pilots, b)| (kid, from_you, pilots, b))
            .collect();
    }

    if let Some(kid) = inp.selected_kid {
        let b = battles
            .iter()
            .find(|b| b.engagements.iter().any(|e| e.kill_id == kid))
            .cloned();
        if let Some(b) = b {
            // One entry per hull type, not per attacker: tens of thousands in a big fight.
            let mut ship_ids: Vec<i64> = b
                .engagements
                .iter()
                .flat_map(|e| std::iter::once(e.victim_ship).chain(e.attackers.iter().map(|a| a.ship)))
                .filter(|&id| id != 0)
                .collect();
            ship_ids.sort_unstable();
            ship_ids.dedup();
            let shown = (!inp.systems.is_empty() && b.systems.iter().any(|s| !inp.systems.contains(&s.0)))
                .then(|| b.in_systems(&inp.systems));
            let v = shown.as_ref().unwrap_or(&b);
            let inv = v.involvement();
            let rosters: Vec<Vec<Participant>> = (0..v.sides.len()).map(|i| v.roster(i)).collect();
            let tiles = ship_tiles(&rosters);
            let (rosters, mut condensed) =
                sorted_detail(&rosters, inp.sort, &deps.ship_sizes, &type_names);
            condensed_damage(v, &mut condensed);
            out.detail =
                Some(Arc::new(BattleDetail { kid, battle: b, inv, rosters, condensed, ship_ids, tiles, shown }));
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
    demand: Arc<AtomicU64>,
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
    let _ = std::thread::Builder::new().name("br-view".into()).spawn(move || {
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
            // Only while the battles view is on screen. Otherwise every kill anywhere in New Eden
            // rebuilt the whole card list in the background, for nobody.
            let idle = now_ms().saturating_sub(demand.load(Ordering::Relaxed)) > DEMAND_GRACE_MS;
            if idle {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The real fight, half of it moved to a second system, open in the worker.
    fn two_system_fight() -> (Deps, i64, i64, usize) {
        let (b, _) = crate::uitest::fixtures::real_battle();
        let mut engs = b.engagements.clone();
        for e in engs.iter_mut().filter(|e| e.kill_id % 2 == 0) {
            e.system_id = 30_004_759;
            e.system_name = "1DQ1-A".into();
        }
        let in_second = engs.iter().filter(|e| e.system_id == 30_004_759).count();
        let b = br_core::battle::preview_battle(engs, br_core::battle::BATTLE_BREAK_SECS);
        let kid = b.engagements.iter().map(|e| e.kill_id).max().unwrap();
        let deps = Deps {
            systems: Some(crate::uitest::fixtures::systems()),
            intel: Arc::new(Mutex::new(IntelState::default())),
            battles: Arc::new(Mutex::new(vec![b])),
            history: Arc::new(Mutex::new(Vec::new())),
            filter: Arc::new(Mutex::new(Default::default())),
            ship_sizes: Arc::new(HashMap::new()),
            type_names: Arc::new(Mutex::new(HashMap::new())),
            overrides_gen: Arc::new(AtomicU64::new(0)),
            filter_gen: Arc::new(AtomicU64::new(0)),
        };
        (deps, kid, 30_004_759, in_second)
    }

    /// Picking a system shows that system's killmails only, and leaves the battle itself whole.
    #[test]
    fn an_open_report_narrows_to_the_systems_picked() {
        let (deps, kid, second, in_second) = two_system_fight();
        let all = compute(&deps, &BrInputs { selected_kid: Some(kid), ..Default::default() }, 1).detail.unwrap();
        assert!(all.shown.is_none());
        assert_eq!(all.view().systems.len(), 2);
        let one = compute(&deps, &BrInputs { selected_kid: Some(kid), systems: vec![second], ..Default::default() }, 2).detail.unwrap();
        assert_eq!(one.view().kills, in_second);
        assert_eq!(one.view().systems.iter().map(|s| s.0).collect::<Vec<_>>(), vec![second]);
        assert_eq!(one.battle.kills, all.battle.kills, "the battle saved or shared stays whole");
        let pilots: usize = one.tiles.pilots.iter().sum();
        assert!(pilots > 0 && pilots < all.tiles.pilots.iter().sum::<usize>(), "the tiles count the systems picked");
    }
}
