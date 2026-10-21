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

pub(super) fn apply(c: &mut Calc) {
    remote_effects(c);
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
