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

## 9. Stats (`stats`)

Pure functions of the resolved tree. `stats::tests::*` pin each on a synthetic dataset;
`the_real_blob_computes_a_known_rifter` (ignored, `SPAI_DOGMA_BLOB`) checks a classic fit lands
in the band fitting tools agree on.

| Stat | Rule |
|---|---|
| CPU, power | Sum of `cpu` (50) / `power` (30) of every module that is at least online, against the ship's `cpuOutput` (48) / `powerOutput` (11). |
| Calibration | Sum of rigs' `upgradeCost` (1153) against `upgradeCapacity` (1132). |
| Slots, hardpoints | Fitted per rack against `hiSlots`/`medSlots`/`lowSlots`/`rigSlots`/`maxSubSystems`/`serviceSlots`; modules with the `turretFitted` / `launcherFitted` effect against `turretSlotsLeft` (102) / `launcherSlotsLeft` (101). |
| EHP | Per layer `hp / Σ_t w_t · resonance_t` with the damage profile's weights normalised to 1 (pyfa `damagePattern.py`); shield 263 with 271–274, armour 265 with 267–270, hull 9 with 113/110/109/111. |
| Shield regen | Peak `2.5 × shieldCapacity / (shieldRechargeRate / 1000)` per second, at 25% (same curve as the capacitor). |
| Local reps | Active repairers only: `armorDamageAmount` (84) / `shieldBonus` (68) / `structureDamageAmount` (83) per `duration` (73) seconds, overload already in the attributes. An ancillary armour repairer with Nanite Repair Paste (group 916) repairs `chargedArmorDamageMultiplier` (1886) times as much for its load (cycles = `floor(capacity / paste volume / chargeRate)`), then reloads `reloadTime` (1795): the sustained figure counts the reload, the burst figure does not. An ancillary shield booster with charges runs its load, then reloads; without, it runs on capacitor. |
| Turret volley | `damageMultiplier` (64) of the turret × the charge's four damage attributes (114, 118, 117, 116). No charge, no damage. Effects: `projectileFired`, `targetAttack`, `ChainLightning`, `targetDisintegratorAttack`. |
| Disintegrator spool | Volley × `(1 + damageMultiplierBonusMax (2734) × spool)`, spool 0..1 from the fit (`Module::spool`, fully spooled by default). |
| Missile volley | The charge's damage × the pilot's `missileDamageMultiplier` (212, which Ballistic Control Systems raise); launcher effect `useMissiles`, charge effect `missileLaunching` (or the FoF / breacher variants). |
| Drone volley | `damageMultiplier` × the drone's damage × drones in space; effect `targetAttack`. |
| Smartbomb | The module's own damage per `duration`, range `empFieldRange` (99). |
| Cycle | `speed` (51) for weapons, plus `moduleReactivationDelay` (669). |
| Load | `floor(floor(capacity / chargeVolume) / chargeRate)` cycles; crystals that wear (`crystalsGetDamaged` 786): `floor(hp / (crystalVolatilityDamage × crystalVolatilityChance))` per crystal carried; scripts and unworn crystals: unlimited. |
| DPS | Burst `volley / cycle`; sustained `volley × shots / (shots × cycle + reloadTime)` (pyfa `module.py` cycle sequence). |
| Weapon groups | Same type, same charge, same cycle: one group (so split ammo is two groups). |
| Missile range | `maxVelocity (37) × explosionDelay (281) / 1000`, the charge's values after modifiers. (pyfa adds the launching ship's radius and the missile's acceleration; this build does not yet.) |
| Capacitor | Peak `2.5 × capacitorCapacity (482) / (rechargeRate (55) / 1000)`; usage = every active effect's `dischargeAttributeID` per its `durationAttributeID`, reloads counted; capacitor boosters (`powerBooster`) give the charge's `capacitorBonus` (67); an ancillary shield booster with charges draws nothing. |
| Align | `ln 2 × agility (70) × mass (4) / 500 000` seconds (pyfa, EVEShipFit). |
| Warp | `warpSpeedMultiplier (600) × baseWarpSpeed (1281)` AU/s. |
| Lock time | `min(40 000 / (scanResolution × asinh(signature)²), 1800)` seconds (pyfa `calc.py`, EVE Uni). |
| Sensors | Strength and kind: the largest of 208–211; locked targets: the lower of the ship's and the pilot's `maxLockedTargets` (192). |
| Drones | Control range: the pilot's `droneControlDistance` (458); bandwidth: Σ `droneBandwidthUsed` (1272) of drones in space against the ship's 1271; bay: Σ volume against `droneCapacity` (283); in space at most the pilot's `maxActiveDrones` (352). |

**Per module** (`stats::module_stats`): each fitted module's CPU and powergrid (zero while
offline), its capacitor draw per second (the same users the simulation runs, section 10, by
module) and its share of its weapon group's DPS (the group's DPS over the modules in it). The
fit table shows these beside the module.

## 10. Capacitor simulation (`stats::cap_sim`)

Event-driven, as pyfa's `capSim.py`:

- Between activations the capacitor recharges along
  `c(t + dt) = C × (1 + (√(c / C) − 1) × e^(−5 dt / τ))²`, τ = `rechargeRate` in seconds. The rate at
  fraction `p` is `10 C / τ × (√p − p)`, peaking at `p = 0.25` with `2.5 C / τ`.
- Each user activates every `cycle`, drawing `need` and giving `give`; after `shots` activations it
  reloads for `reloadTime`. Identical users start staggered over one cycle.
- A capacitor booster is held back while its charge would overfill the capacitor (pyfa's
  injectors); when a module finds less capacitor than it needs, the ready booster that wastes the
  least fires first, and only when none can is the capacitor dry.
- A capacitor booster waits while `cap + give > C` (pyfa holds injectors back from overfilling),
  looking again a second later.
- The run stops when an activation finds less capacitor than it needs: `lasts_s`. Otherwise the
  capacitor is "stable at" the lowest level of the run's last fifth (horizon one hour).
- The closed-form estimate, where a steady drain `D` meets the curve:
  `p = ¼ (1 + √(1 − 0.4 D τ / C))²`, none when `D` exceeds the peak. (The factor is 0.4, not the 2
  sometimes quoted: 2 belongs with τ expressed as the time to the peak.)

Tests: `stats::cap_sim::tests::*` (curve, peak, settle vs estimate, running dry, boosters,
stagger), `capacitor_usage_comes_from_the_effects_discharge_attributes`.

## 11. Formats (`formats`)

- **EFT**: header `[Ship, Name]`; racks low, mid, high, rig, subsystem, service with one blank line
  between; `Name, Charge`, ` /OFFLINE`, `[Empty X slot]`, ` [N]` mutation references (stripped),
  `Name xN` for drones, fighters and cargo; implants and boosters by name. Import places every
  line by its type (`formats::place_of`: category, then the `hiPower`/`medPower`/`loPower`/
  `rigSlot`/`subSystem`/`serviceSlot` effect), so rack order does not matter; unknown names are
  returned, not fatal. A module arrives active when it has a capacitor need or an active effect,
  else online.
- **ESI fitting JSON**: flags low 11–18, mid 19–26, high 27–34, rig 92–99, subsystem 125–132,
  service 164–171, drone bay 87, fighter bay 158, cargo 5; loaded charges are exported into cargo
  as the game does.
- **DNA**: `ship:type;count:...::`, subsystems first, then modules, drones, fighters, charges.

Tests: `formats::tests::*`, `eft::tests::lines_parse_every_decoration`.

## 12. Warnings (`warnings`)

Errors: CPU, power, calibration, slots, hardpoints, drone bandwidth, bay or count over; unknown
types. Warnings: no propulsion module, offline modules, an ancillary armour repairer without
paste, a capacitor or ancillary shield booster without charges, a weapon without ammo, capacitor
running dry, missing skills. Info: free slots; ammo coverage: for each turret or launcher group,
the charges it can load (`chargeGroup1..5` 604/605/606/609/610 and `chargeSize` 128) are compared
with what is loaded and carried: a damage type nothing carried deals though some compatible charge
does, and a short or long end of the reach (`weaponRangeMultiplier` 120 for turret ammo, velocity ×
flight time for missiles) nothing carried covers.

Tests: `warnings::tests::*`.

## 13. History (`history`)

Snapshot undo: the fit before each edit, up to 200 steps; slider and target edits of the same kind
within 600 ms fold into one step; a new edit drops the redo branch. Test: `history::tests::*`.

## 14. Applied damage (`stats::application`)

| Weapon | Share of listed damage that lands |
|---|---|
| Turret | `hit = 0.5^((angular × optimalSigRadius / (trackingSpeed × sig))²) × 0.5^((max(0, d − optimal) / falloff)²)`, `angular = transversal / d` in rad/s, and 0 past `optimal + 3 × falloff` (pyfa). Then the average over normal and wrecking shots: `0.5 × min(hit² + 0.98 hit + 0.0501, 6 hit)` (EVE Uni "Turret mechanics"). `trackingSpeed` is the post-2016 figure (33.792 for a 425mm AutoCannon II), paired with the 40 000 m `optimalSigRadius`. |
| Missile | `min(1, sig / aoeCloudSize, ((sig × aoeVelocity) / (aoeCloudSize × targetVelocity))^aoeDamageReductionFactor)` (EVE Uni "Missile mechanics"); nothing beyond the flight distance. |
| Drone | Nothing past the pilot's control range. A drone as fast as the target keeps up and lands its listed damage; a slower one is a turret whose transversal is the speed it lacks, around its own orbit (an approximation: pyfa models drone speed capping the attacker's). |
| Smartbomb | All within `empFieldRange`, nothing beyond. |

Tests: `stats::application::tests::*` (every shot hits in optimal, half at one falloff, cut-off at
three; the 425mm AutoCannon II figure against a 400 m target at 0.1 rad/s; wrecking shots below
a 1% chance; missiles against big slow and small fast targets).

## 15. Graphs (`graphs`)

- Damage vs distance, target speed (moving fully transversally at the chosen distance) and target
  signature: one series per weapon group with its applied DPS, plus the total.
- Capacitor over time: the simulation's trace (section 10), the recharge curve between activations,
  zero once dry.
- Shield over time: the same curve from empty, no users.
- Speed over time: `v(t) = vmax × (1 − e^(−t / τ))`, `τ = agility × mass / 10⁶` (pyfa `fitMobility`).
- Warp time vs distance (pyfa `fitWarpTime`): `k_a` = warp speed in AU/s, `k_d = min(k_a / 3, 2)`,
  `V = k_a × AU`, drop-out speed `min(sub-warp speed / 2, 100)`, `D_min = AU + V / k_d`; a warp
  shorter than `D_min` peaks at `d × k_a × k_d / (k_a + k_d)` and never cruises;
  `t = ln(V / k_a) / k_a + cruise + ln(V / v_drop) / k_d`. A 5 AU/s frigate warps 10 AU in about
  20 s.
- Lock time vs signature: section 9.

Tests: `graphs::tests::*`, `stats::cap_sim::tests::a_trace_follows_the_run_and_hits_zero_when_dry`.

## 16. Variations (`variations`)

A fit carries named variations, each a list of operations: put a type in a position or empty it,
set a position's charge or state, replace or re-charge every module matching a rule (rack, group,
effect name or type), or set the implants, boosters, drones or cargo whole. The switched-on
variations apply in the order they were switched on; the numbers, the racks and the warnings show
the fit with them applied, and the positions they decide are read-only in the racks. Two active
variations that touch the same position or section are a warning (the later one wins).

A variation is recorded by editing: every variation is switched off, the fit is edited, and the
difference between the snapshot and the edit (`variations::diff`: one operation per changed
position or section) becomes the variation, which is switched on while the fit goes back to the
snapshot.

Variations are partial on purpose (a doctrine's afterburner refit must touch only the propulsion
module of each hull), and to keep what they combine into checkable, **one of each kind is on at a
time**: switching an ammo load on switches the other ammo load off, likewise refits, implant sets,
booster sets and drone loads (`variations::activate`). The numbers and warnings always describe
the fit with every active variation applied, so a combination over CPU or powergrid shows as an
error at once.

A **refit key** such as `prop:ab` links one variation across every fit of a doctrine. The part
before the colon is its family: switching `prop:ab` on switches `prop:mwd` off whatever their
kinds, so a fleet swaps as one (`variations::apply_refit_key`).

Tests: `variations::tests::*`, `warnings::tests::clashing_variations_are_pointed_out`,
`spai-fit-ui` `recording_a_variation_keeps_the_difference_and_switches_it_on`.

## 17. Remote modules and fighters (`stats::remote`, `stats::fighter_weapons`)

**Remote modules** (`Computed.remote.modules`): every active module whose effect names it as
acting on another ship, with the amount it gives or takes, its optimal (`maxRange`, warp
scramble range for tackle) and its falloff (`falloffEffectiveness`, else `falloff`):

| Kind | Effect | Amount |
|---|---|---|
| armour, shield, hull repair | `shipModuleRemote{Armor,Shield,Hull}…`, the ancillary and mutadaptive ones | repair per second (amount / cycle) |
| capacitor transfer, nosferatu | `shipModuleRemoteCapacitorTransmitter`, `energyNosferatuFalloff` | GJ per second |
| neutraliser | `energyNeutralizerFalloff` | GJ drained per second |
| web, painter, damp, tracking and guidance disruptor | `remoteWebifierFalloff`, `remoteTargetPaintFalloff`, `remoteSensorDampFalloff`, `shipModule{Tracking,Guidance}Disruptor` | the strongest of its bonuses, as a magnitude in percent (a −60 % web reads 60) |
| ECM | `remoteECMFalloff` | the strongest sensor-type jam strength |
| scrambler, disruptor | `warpScramble…`, `warpDisrupt` | warp points |

The totals `armor_rep`, `shield_rep`, `hull_rep`, `cap_transfer`, `neut` and `rep_range`
(the longest optimal plus falloff of the repairers) feed the fleet numbers (section 18).

`stats::range_factor(d, optimal, falloff)`: 1 inside the optimal, `0.5^((d − optimal) / falloff)²`
beyond it, 0 past three falloffs or with no falloff. The graphs "Remote repair vs distance"
(repairs by type, cap transfers apart, with a total of the repairs) and "Ewar vs distance"
(each ewar module's strength) are `amount × range_factor`.

**Fighters** (`fighter_weapons`): an active squadron is a weapon group of kind `Fighter` per
ability it has, the primary attack (`fighterAbilityAttackM`) and heavy missiles
(`fighterAbilityMissiles`): volley = the ability's four damages × its damage multiplier × the
squadron size, cycle = the ability's duration, no reload. The ability attributes are found by
name (`fighterAbilityAttackMissileDamageEM` and so on) rather than by baked id. Application is
the missile formula (section 14) with the ability's explosion radius, velocity and reduction
factor, times `range_factor` of the ability's optimal and falloff (missiles: range, no falloff).
The reduction sensitivity, the squadron's own tank, speed and abilities beyond damage are not
modelled.

Tests: `stats::tests::remote_modules_are_listed_with_their_reach_and_fighters_shoot_like_missiles`,
`graphs::tests::remote_and_ewar_curves_follow_the_modules_reach`.

## 18. Fleets (`fleet`)

A fleet is a list of saved fits with a count rule each: a fixed number, a share of the fleet size
in percent, or "fill" (what the others leave, within bounds: the line ship). Fixed and percent
counts are taken first, fill slots share the rest, a typed number overrides its rule
(`fleet::resolve_counts`).

**Bursts.** One booster fit per burst kind; every active burst module with a charge names up to
four buffs (`warfareBuffNID`, `warfareBuffNValue`, the module's values after the charge's
`chargeBonusWarfareCharge` modifiers and the booster's own skills), the strongest of each buff id
is kept (section 6) and every ship of the fleet is computed with them (`fleet::env_of`).

**Fleet numbers** (`fleet::stats`): each slot computed with the fleet's buffs and its own skills
profile, scaled by its count: DPS, volley, logistics (remote armour and shield repairs per second,
section 17), neuts, the slowest and fastest hull.

**Against** (`fleet::against`), fleet A on fleet B's main hull at A's engagement range:

- A's ewar on the target: unless typed by hand (`ewar_auto` off), what A's ships project,
  every web, painter and tracking disruptor fitted times the ships carrying it
  (`FleetStats.projected`); each web −60% speed, each painter +30% signature, penalised like
  modules; B's tracking disruptors cut A's turret optimal, falloff and tracking by 17.19%
  each, penalised. The target moves fully transversally at its speed.
- Applied DPS and volley: each weapon group's listed figure times its applied share (section 14),
  summed by count.
- Critical mass: `ceil(EHP of B's main hull / (applied volley of one A main ship × efficiency))`,
  the ships of A's main slot that must volley together to kill one B main hull; the efficiency
  (85% by default) stands for lacking skills and mechanical failures.
- Kills per volley: `applied volley × efficiency / EHP`.
- Break point: B's logistics plus B's main hull's own repairs per second; the margin is A's
  applied DPS × efficiency above it, negative when B out-repairs A.

**From a doctrine** (`fleet::from_doctrine`): every preset of a doctrine becomes a slot with a
rule by its hull's group: logistics 15 %, command ships and command destroyers 2, interdictors
and heavy interdictors 3, recons and electronic attack ships 2, everything else fills; the first
filling slot is the main; a fit with an active command burst becomes the booster of that kind.

Tests: `fleet::tests::*`, `spai-fit-ui` `fleets_ask_for_fit_documents_and_compute_once_they_arrive`.
