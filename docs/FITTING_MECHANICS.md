# Fitting mechanics

How the fitting simulator (`crates/spai-fit`) computes what it shows, with the source of each rule.
Every rule here is pinned by a test named next to it. Change the code and this file together.

Sources, by what they may be used for:

- **EVE's static data** (`eve-online-static-data-latest-jsonl.zip` from developers.eveonline.com):
  the attributes, effects and modifiers themselves.
- **EVEShipFit dogma-engine** (MIT, github.com/EVEShipFit/dogma-engine): the reference for how
  the data is interpreted (operator order, normalisation, penalty exemptions, state gating). Read
  and adapted, not copied.
- **pyfa** (GPL-3, github.com/pyfa-org/Pyfa): formulas only (its numbers are the known answers
  the tests compare against); no code.
- **EVE University wiki**: the published game formulas (hit chance, missile application, capacitor,
  align, warp, lock time, heat).

## 1. Dataset (`dogma`, `bake`)

Baked from the JSONL zip by `spai_fit::bake::from_zip` into one blob
(`dogma-v<format>.bin.gz`: a `SPAIFIT` header with format and SDE build, then gzip of postcard).

| Member | Taken |
|---|---|
| `types.jsonl` | Published types of categories 6 ship, 7 module, 8 charge, 18 drone, 20 implant/booster, 32 subsystem, 87 fighter; every 16 skill published or not; type 1373 `CharacterType` (the pilot's base attributes). English name, group, market group, meta group/level, tech level, `variationParentTypeID`; `mass`, `capacity`, `volume`, `radius` are injected as attributes 4, 38, 161, 162 when the dogma lacks them. |
| `typeDogma.jsonl` | Attribute values and effects of the kept types. `requiredSkillN` / `requiredSkillNLevel` (182/277, 183/278, 184/279, 1285/1286, 1289/1287, 1290/1288) are also folded into `required_skills`. |
| `dogmaAttributes.jsonl` | All: `stackable`, `highIsGood`, default, unit, min/max attribute, display name. |
| `dogmaEffects.jsonl` | Effects used by kept types: category, offensive/assistance flags, the duration/range/falloff/tracking/discharge/usage-chance/resistance attribute ids, and `modifierInfo`. A modifier is dropped when its `func` is `EffectStopper`, its `operation` is 9 (skill level from skill points) or missing, or its domain is unknown. `skillTypeID = -1` becomes `SkillRef::Own`. |
| `dbuffCollections.jsonl` | All with a known operation: aggregate mode, operation, and the four modifier lists. |
| `groups`, `categories`, `marketGroups`, `metaGroups`, `dogmaUnits` | Groups of kept types plus groups named by modifiers; market groups of kept types plus their ancestors. |
| `typeBonus.jsonl` | Ships' trait text (role, misc, per skill), tags stripped, ordered by importance. |

Structures (65, 66) are not baked: structure fitting is out of scope.

Tests: `bake::tests::*`, `dogma::blob::tests::*`, `the_real_zip_bakes_small_and_consistent`
(ignored, `SPAI_SDE_ZIP`), `the_real_blob_agrees_with_the_named_attributes` (ignored,
`SPAI_DOGMA_BLOB`; pins every id in `dogma::attr::NAMED`).

## 2. Items (`calc`)

A fit becomes a tree of items, each with a type, a state, a *location* and an *owner*:

| Item | State | Location | Owner |
|---|---|---|---|
| Pilot (type 1373) | online | – | – |
| Ship | online | – | pilot |
| Every skill in the data, `skillLevel` (280) overridden with the pilot's level | online | pilot | pilot |
| Module (any rack, rigs and subsystems included) | as fitted | ship | pilot |
| Charge | its module's | ship | pilot |
| Drone, fighter | active in space, else offline | ship | pilot |
| Implant, booster | online | pilot | pilot |
| Projected module (another ship's) and its charge | as set | – | – |

Unknown type ids are listed in `Calc::unknown_types` and left out. Mutated modules carry their
mutated values as overrides.

## 3. Effects and states

An effect on an item runs when the item's state reaches the state its category needs
(dogma-engine `required_state`, with pyfa's rule that an offline module contributes nothing):

| Effect category | Needs |
|---|---|
| 0 passive, 4 online | Online |
| 1 active, 2 target, 3 area | Active |
| 5 overload | Overload |
| 6 dungeon, 7 system | never |

Also:

- A booster effect with a `fittingUsageChanceAttributeID` (a side effect) runs only when the
  pilot opted into it (`Booster::side_effects`).
- `skillEffect` (a skill's own level bookkeeping) is skipped.
- A projected item's modifiers run only in the `target` domain; a fitted item's never do.

Tests: `resistance_modules_stack_as_in_eve` (offline and online-only states),
`implants_are_exempt_from_the_penalty_and_booster_side_effects_are_opt_in`,
`skill_levels_scale_ship_bonuses_that_reach_the_right_modules` (a System effect never runs).

## 4. Modifiers

Each `modifierInfo` entry names a domain, a function, the modified and modifying attributes and an
operation (dogma-engine `pass_2`):

| Domain | Root item |
|---|---|
| `shipID`, `structureID` | the ship |
| `charID` | the pilot |
| `itemID` | the item carrying the effect |
| `otherID` | the module's charge, or the charge's module (nothing without a charge) |
| `targetID`, `target` | the ship, for projected items |

| Function | Targets |
|---|---|
| `ItemModifier` | the root |
| `LocationModifier` | everything located in the root |
| `LocationGroupModifier` | …of the given group |
| `LocationRequiredSkillModifier` | …requiring the given skill (`-1`: the effect's own type) |
| `OwnerRequiredSkillModifier` | everything the root owns that requires the skill |

The modifying value is the *source item's* attribute, resolved with its own modifiers (so a ship
bonus multiplied by a skill level flows through), lazily and memoised; a self-referencing cycle
reads the base value.

Tests: `a_charge_reaches_its_module_through_other_id`,
`owner_required_skill_modifiers_reach_drones_and_location_group_modifiers_their_group`,
`a_projected_module_reaches_the_ship_and_nothing_else`,
`clamps_follow_the_attribute_definitions_and_cycles_fall_back_to_base`.

## 5. Operators, aggregation and the stacking penalty (`calc::value`)

Operators apply in this order (dogma-engine `item.rs`; the data's numbers in brackets):
PreAssign (−1), PreMul (0), PreDiv (1), ModAdd (2), ModSub (3), PostMul (4), PostDiv (5),
PostPercent (6), PostAssign (7).

| Operator | Normalised `x` | Aggregation |
|---|---|---|
| Assign | `v` | the max of the values when the attribute is `highIsGood`, else the min; never penalised |
| ModAdd / ModSub | `v` / `−v` | summed; never penalised |
| Mul / Div / Percent | `v − 1` / `1/v − 1` / `v / 100` | `Π (1 + x)`; penalised ones see below |

Then the value is clamped by the attribute's `minAttributeID` / `maxAttributeID`.

**Stacking penalty.** A Mul/Div/Percent modifier is penalised when the modified attribute is not
`stackable` and the source item's category is not one of ship (6), charge (8), skill (16), implant
(20), subsystem (32), structure (65). Fleet buffs are always penalised. Within **one operator**,
the penalised bonuses (`x > 0`) and maluses (`x < 0`) are sorted apart by `|x|` descending and the
k-th (from 0) keeps

    1 + x · 0.8691199808003974^(k²)        (= exp(−(k / 2.67)²): 100%, 86.9%, 57.1%, 28.3%, 10.6%, 3.0%)

Grouping by operator is the rule in the data and in dogma-engine; pyfa groups by hand per effect.
It is why a Damage Control, whose resistance modifiers are `PreMul`, never competes with hardeners'
`PostPercent` modifiers: it sits in a penalty group of its own.

Tests: `calc::value::tests::*` (operator order, aggregation, the published curve, four 20%
hardeners = +65.8%, bonuses and maluses apart, strongest first, separate operator groups),
`resistance_modules_stack_as_in_eve`, `fleet_buffs_apply_penalised_and_the_strongest_wins`.

## 6. Fleet buffs (`Env`)

A command burst's charge sets `warfareBuffNID` and `warfareBuffNValue` on the burst module
(through `otherID` modifiers); the module's skills scale the value. The fleet keeps one value per
buff id, the strongest by the collection's aggregate mode (`Env::offer`). Each buff then modifies
the ship (`itemModifiers`), everything in it (`locationModifiers`), a group
(`locationGroupModifiers`) or what requires a skill (`locationRequiredSkillModifiers`) with the
collection's operation, penalised like a module.

Test: `fleet_buffs_apply_penalised_and_the_strongest_wins`.

## 7. The patch table (`calc::patch`)

What the data leaves out of `modifierInfo`, written by hand. Each entry exists because the effect
is empty in the data (`the_real_zip_bakes_small_and_consistent` enumerates the empty effects on
fitted types; everything there that changes an attribute must be listed here).

| Effect(s) | Rule | Source |
|---|---|---|
| `moduleBonusAfterburner`, `moduleBonusMicrowarpdrive` | While active: ship `mass` += module `massAddition`; ship `signatureRadius` PostPercent `signatureRadiusBonus` (penalised); ship `maxVelocity` PostPercent `speedFactor × speedBoostFactor / mass` (penalised; the mass after the addition; an overloaded module's `speedFactor` already carries `overloadSpeedFactorBonus` through the data). | EVE Uni "Propulsion equipment"; EVEShipFit `velocityBoost` patch |
| `missileEMDmgBonus`, `missileExplosiveDmgBonus`, `missileKineticDmgBonus2`, `missileThermalDmgBonus` on a skill | Items requiring the skill: that damage attribute PostPercent the skill's `damageMultiplierBonus` (292, already × level by `skillBoostDamageMultiplierBonus`). Unpenalised (skill source). | The skills' descriptions (5% per level); pyfa's effect code |
| `droneDmgBonus` on a skill | Drones requiring the skill: `damageMultiplier` PostPercent the skill's `damageMultiplierBonus`. | same |
| `selfRof` on a skill | Launchers requiring the skill: `speed` PostPercent the skill's `rofBonus` (293, × level by `rapidFiringSkillBoostRofBonus`). | same |
| `slotModifier`, `hardPointModifierEffect` on a subsystem | Ship `hiSlots`/`medSlots`/`lowSlots` += `hiSlotModifier`/`medSlotModifier`/`lowSlotModifier`; `turretSlotsLeft`/`launcherSlotsLeft` += `turretHardPointModifier`/`launcherHardPointModifier`. | The subsystems' descriptions |

Empty effects that are *not* attribute modifiers (what a module does rather than what it changes:
`projectileFired`, `targetAttack`, `missileLaunching`, `armorRepair`, `shieldBoosting`,
`fueledArmorRepair`, `powerBooster`, `remoteWebifierFalloff`, the `shipModuleRemote*`,
`energyNeutralizerFalloff`, `moduleBonusWarfareLink*`, `empWave`, `online`, the slot markers
`hiPower`/`medPower`/`loPower`/`rigSlot`/`subSystem`/`turretFitted`/`launcherFitted`) are read by
name in the stats and projection layers (section 8 onwards).

Tests: `a_propulsion_module_adds_mass_signature_and_speed_while_active`,
`skills_whose_bonus_effects_are_empty_in_the_data_still_apply`,
`subsystems_add_their_slots_and_hardpoints_to_the_hull`,
`the_real_blob_agrees_with_the_named_attributes` (Cruise Missiles V + Warhead Upgrades V = ×1.375
missile damage; Missile Launcher Operation, Rapid Launch and the specialisation at V = ×0.6885 cycle).

## 8. Skills (`skills`)

`SkillSet` holds levels with a fallback for unlisted skills (5 for "all V", 0 for a pilot from
ESI). `skills::missing` walks the required skills of the hull, modules, charges, drones, fighters,
implants and boosters and of those skills in turn, and reports each `(skill, needed, have)` with
the item that asks for it.

Tests: `skills::tests::*`.
