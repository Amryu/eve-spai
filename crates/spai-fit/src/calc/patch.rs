//! What the static data does not spell out as modifiers, added by hand. Every entry is described
//! in docs/FITTING_MECHANICS.md with its reason.

use super::{Calc, ItemIdx, Source};
use crate::dogma::{attr, Op};
use crate::fit::State;

/// Propulsion modules carry no modifiers in the data. While active, one adds `massAddition` to the
/// ship's mass, blooms the signature by `signatureRadiusBonus` percent, and raises the top speed
/// by `speedFactor × speedBoostFactor / mass` percent (the mass with the addition; an overloaded
/// module's `speedFactor` already carries its overload bonus). The speed bonus is penalised like
/// any other velocity modifier.
const PROP_EFFECTS: [&str; 2] = ["moduleBonusAfterburner", "moduleBonusMicrowarpdrive"];

/// Weapon skills whose bonus effect is empty in the data: the skill's `damageMultiplierBonus`
/// (already multiplied by its level) applies to the damage of whatever needs the skill.
const SKILL_DAMAGE_EFFECTS: [(&str, &[crate::ids::AttrId]); 5] = [
    ("missileEMDmgBonus", &[attr::EM_DAMAGE]),
    ("missileExplosiveDmgBonus", &[attr::EXPLOSIVE_DAMAGE]),
    ("missileKineticDmgBonus2", &[attr::KINETIC_DAMAGE]),
    ("missileThermalDmgBonus", &[attr::THERMAL_DAMAGE]),
    ("droneDmgBonus", &[attr::DAMAGE_MULTIPLIER]),
];

/// The same for specialisation skills' rate of fire: `rofBonus` on the `speed` of the launchers
/// needing the skill.
const SKILL_ROF_EFFECT: &str = "selfRof";

/// Subsystems change the hull's slot and hardpoint counts through attributes the data names in
/// two empty effects.
const SUBSYSTEM_SLOT_EFFECTS: [&str; 2] = ["slotModifier", "hardPointModifierEffect"];
const SUBSYSTEM_SLOTS: [(crate::ids::AttrId, crate::ids::AttrId); 5] = [
    (attr::HI_SLOT_MODIFIER, attr::HI_SLOTS),
    (attr::MED_SLOT_MODIFIER, attr::MED_SLOTS),
    (attr::LOW_SLOT_MODIFIER, attr::LOW_SLOTS),
    (attr::TURRET_HARDPOINT_MODIFIER, attr::TURRET_SLOTS_LEFT),
    (attr::LAUNCHER_HARDPOINT_MODIFIER, attr::LAUNCHER_SLOTS_LEFT),
];

/// Ewar and remote assistance carry no modifiers in the data either. Each projected module or
/// drone switched on changes the ship's (or its weapons' and charges') attribute by its own
/// percentage, penalised among its kind: (effects, what it changes, by which of its attributes,
/// on what: the ship, or what needs a skill).
const REMOTE_EFFECTS: [(&[&str], &[(&str, &str)], Remote); 6] = [
    (&["remoteWebifierFalloff", "remoteWebifierEntity"], &[("maxVelocity", "speedFactor")], Remote::Ship),
    (&["remoteTargetPaintFalloff", "remoteTargetPaintEntity"], &[("signatureRadius", "signatureRadiusBonus")], Remote::Ship),
    (&["remoteSensorDampFalloff", "remoteSensorDampEntity", "remoteSensorBoostFalloff"], &[("maxTargetRange", "maxTargetRangeBonus"), ("scanResolution", "scanResolutionBonus")], Remote::Ship),
    (&["shipModuleTrackingDisruptor", "npcEntityWeaponDisruptor", "shipModuleRemoteTrackingComputer"], &[("maxRange", "maxRangeBonus"), ("falloff", "falloffBonus"), ("trackingSpeed", "trackingSpeedBonus")], Remote::Needing(GUNNERY)),
    (&["npcEntityWeaponDisruptor"], &[("maxVelocity", "missileVelocityBonus"), ("explosionDelay", "explosionDelayBonus"), ("aoeCloudSize", "aoeCloudSizeBonus"), ("aoeVelocity", "aoeVelocityBonus")], Remote::Needing(MISSILE_LAUNCHER_OPERATION)),
    (&["shipModuleGuidanceDisruptor"], &[("maxVelocity", "missileVelocityBonus"), ("explosionDelay", "explosionDelayBonus"), ("aoeCloudSize", "aoeCloudSizeBonus"), ("aoeVelocity", "aoeVelocityBonus")], Remote::Needing(MISSILE_LAUNCHER_OPERATION)),
];

#[derive(Clone, Copy)]
enum Remote {
    Ship,
    /// The ship's items needing the skill: turrets for Gunnery, missiles for Missile Launcher
    /// Operation.
    Needing(crate::ids::TypeId),
}

const GUNNERY: crate::ids::TypeId = 3300;
const MISSILE_LAUNCHER_OPERATION: crate::ids::TypeId = 3319;

/// The Rorqual's PANIC core: its effect is empty and its buffs carry no values, the server applies
/// them. While it runs the shield's resonances are multiplied by this, worked back from one in-game
/// reading (a fitted Rorqual at 4,905,113,104 EHP); armor and hull are left alone.
pub const PANIC_SHIELD_RESONANCE: f64 = 0.000_137_055;
const PANIC_EFFECT: &str = "moduleBonusIndustrialInvulnerability";

/// The reactive armor hardener: its effect is empty in the data. Its four armor resonances (0.85
/// each, 60% resistance in all) multiply the ship's, unpenalised. Running and not kept fixed, it
/// adapts to the incoming damage: each cycle the two types hurting the ship least give up to
/// `resistanceShiftAmount` percent each, shared between the two hurting it most (with a single
/// type coming in, the other three give to it); the split shown is the average over the cycle it
/// settles into.
const RAH_EFFECT: &str = "adaptiveArmorHardener";
const ARMOR_RESONANCES: [crate::ids::AttrId; 4] = [attr::ARMOR_EM_RESONANCE, attr::ARMOR_THERMAL_RESONANCE, attr::ARMOR_KINETIC_RESONANCE, attr::ARMOR_EXPLOSIVE_RESONANCE];

pub(super) fn apply(c: &mut Calc) {
    remote_effects(c);
    panic_core(c);
    reactive_hardener(c);
    prop_mods(c);
    skill_bonuses(c);
    subsystem_slots(c);
}

fn has_effect(c: &Calc, item: ItemIdx, names: &[&str]) -> bool {
    c.effect_names(item).any(|n| names.contains(&n))
}

fn prop_mods(c: &mut Calc) {
    let ship = c.ship_idx();
    let prop: Vec<ItemIdx> = c.modules().filter(|(_, it)| it.state >= State::Active).map(|(i, _)| i).filter(|&i| has_effect(c, i, &PROP_EFFECTS)).collect();
    for i in prop {
        c.add_source_mod(ship, attr::MASS, Op::ModAdd, Source::Attr(i, attr::MASS_ADDITION), false);
        if c.has(i, attr::SIGNATURE_RADIUS_BONUS) {
            c.add_source_mod(ship, attr::SIGNATURE_RADIUS, Op::PostPercent, Source::Attr(i, attr::SIGNATURE_RADIUS_BONUS), true);
        }
        c.add_source_mod(ship, attr::MAX_VELOCITY, Op::PostPercent, Source::PropBonus(i), true);
    }
}

fn skill_bonuses(c: &mut Calc) {
    let skills: Vec<(ItemIdx, crate::ids::TypeId)> = c.skills().map(|(i, it)| (i, it.type_id)).collect();
    for (skill, type_id) in skills {
        let targets = c.requiring(type_id);
        if targets.is_empty() {
            continue;
        }
        for (name, attrs) in SKILL_DAMAGE_EFFECTS {
            if has_effect(c, skill, &[name]) {
                for &t in &targets {
                    for &a in attrs {
                        c.add_source_mod(t, a, Op::PostPercent, Source::Attr(skill, attr::DAMAGE_MULTIPLIER_BONUS), false);
                    }
                }
            }
        }
        if has_effect(c, skill, &[SKILL_ROF_EFFECT]) {
            for &t in &targets {
                c.add_source_mod(t, attr::SPEED, Op::PostPercent, Source::Attr(skill, attr::ROF_BONUS), false);
            }
        }
    }
}

fn subsystem_slots(c: &mut Calc) {
    let ship = c.ship_idx();
    let subs: Vec<ItemIdx> = c.subsystems().map(|(i, _)| i).filter(|&i| has_effect(c, i, &SUBSYSTEM_SLOT_EFFECTS)).collect();
    for i in subs {
        for (from, to) in SUBSYSTEM_SLOTS {
            if c.has(i, from) {
                c.add_source_mod(ship, to, Op::ModAdd, Source::Attr(i, from), false);
            }
        }
    }
}

fn remote_effects(c: &mut Calc) {
    let ship = c.ship_idx();
    let sources: Vec<(ItemIdx, u32)> = c.projected_items().filter(|(_, it)| it.state >= State::Active).map(|(i, it)| (i, it.projected_count())).collect();
    for (src, times) in sources {
        for (effects, changes, on) in REMOTE_EFFECTS {
            // Only what the data leaves empty: an effect with modifiers of its own already applies.
            let empty = c.item_effects(src).any(|e| effects.contains(&e.name.as_str()) && e.modifiers.is_empty());
            if !empty {
                continue;
            }
            let targets = match on {
                Remote::Ship => vec![ship],
                Remote::Needing(skill) => c.needing_on_ship(skill),
            };
            for (changed, by) in changes {
                let (Some(changed), Some(by)) = (c.dogma.attr_named(changed).map(|a| a.id), c.dogma.attr_named(by).map(|a| a.id)) else { continue };
                if !c.has(src, by) {
                    continue;
                }
                for &t in &targets {
                    for _ in 0..times {
                        c.add_source_mod(t, changed, Op::PostPercent, Source::Attr(src, by), true);
                    }
                }
            }
        }
    }
}

fn panic_core(c: &mut Calc) {
    let ship = c.ship_idx();
    let running = c.modules().any(|(i, it)| it.state >= State::Active && c.effect_names(i).any(|n| n == PANIC_EFFECT));
    if !running {
        return;
    }
    for a in [attr::SHIELD_EM_RESONANCE, attr::SHIELD_THERMAL_RESONANCE, attr::SHIELD_KINETIC_RESONANCE, attr::SHIELD_EXPLOSIVE_RESONANCE] {
        c.add_source_mod(ship, a, Op::PostMul, Source::Const(PANIC_SHIELD_RESONANCE), false);
    }
}

fn reactive_hardener(c: &mut Calc) {
    let ship = c.ship_idx();
    let rahs: Vec<ItemIdx> = c.modules().filter(|(i, it)| it.state >= State::Active && c.effect_names(*i).any(|n| n == RAH_EFFECT)).map(|(i, _)| i).collect();
    if rahs.is_empty() {
        return;
    }
    // The ship's armor before any reactive hardener: what it adapts against.
    let before: [f64; 4] = ARMOR_RESONANCES.map(|a| c.value(ship, a));
    for &rah in &rahs {
        if !c.fixed.contains(&rah) {
            let shift = c.dogma.attr_named("resistanceShiftAmount").map(|a| a.id).filter(|a| c.has(rah, *a)).map_or(6.0, |a| c.value(rah, a)) / 100.0;
            let start = ARMOR_RESONANCES.map(|a| 1.0 - c.value(rah, a));
            let adapted = adapt(start, before, c.incoming, shift);
            for (k, a) in ARMOR_RESONANCES.into_iter().enumerate() {
                c.set_override(rah, a, 1.0 - adapted[k]);
            }
        }
        for a in ARMOR_RESONANCES {
            c.add_source_mod(ship, a, Op::PostMul, Source::Attr(rah, a), false);
        }
    }
    c.forget_values();
}

/// The hardener's resistances after it settles: cycles of shifting from the two types hurting the
/// ship least to the two hurting it most, averaged over the loop they fall into.
pub(crate) fn adapt(start: [f64; 4], ship: [f64; 4], incoming: [f64; 4], shift: f64) -> [f64; 4] {
    let mut res = start;
    let mut seen: Vec<[f64; 4]> = Vec::new();
    for _ in 0..200 {
        let key = res.map(|r| (r * 1e6).round() / 1e6);
        if let Some(at) = seen.iter().position(|s| *s == key) {
            let lap = &seen[at..];
            let mut avg = [0.0; 4];
            for s in lap {
                for k in 0..4 {
                    avg[k] += s[k] / lap.len() as f64;
                }
            }
            return avg;
        }
        seen.push(key);
        let taken: [f64; 4] = std::array::from_fn(|k| incoming[k] * ship[k] * (1.0 - res[k]));
        let mut order: Vec<usize> = (0..4).collect();
        order.sort_by(|a, b| taken[*a].total_cmp(&taken[*b]).then(a.cmp(b)));
        let hit = taken.iter().filter(|t| **t > 0.0).count();
        // Nothing taken, or every type hurting alike: nothing to shift towards.
        if hit == 0 || taken.iter().all(|t| (t - taken[0]).abs() < 1e-12) {
            return res;
        }
        let (donors, takers): (Vec<usize>, Vec<usize>) = if hit == 1 { (order[..3].to_vec(), order[3..].to_vec()) } else { (order[..2].to_vec(), order[2..].to_vec()) };
        let mut pool = 0.0;
        for d in donors {
            let give = shift.min(res[d]);
            res[d] -= give;
            pool += give;
        }
        for t in &takers {
            res[*t] += pool / takers.len() as f64;
        }
    }
    res
}
