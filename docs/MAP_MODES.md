# Map Expansion: Travel / Hunting / Safety modes

Status: **design / planning** (not yet implemented). This expands the map from a static
intel view into four selectable *modes* that share one routing + data layer.

## 1. The mode model

A single `MapMode` selector (top of the map controls) switches behaviour. The map data and
overlays are shared; each mode adds its own control panel + map decorations.

- **Standard** — today's map (intel sightings, sov/ADM/activity/upgrade overlays). Default.
- **Travel** — plan a constrained route between two systems for a chosen ship.
- **Hunting** — a live, location-sorted target/intel board for a roaming hunter.
- **Safety** — AFK watch: alarms + screen flash when hostiles/kills come within range.

```rust
enum MapMode { Standard, Travel, Hunting, Safety }
```

Modes are mutually exclusive in the UI but Hunting/Safety keep showing Standard's intel
decorations underneath ("the current state of the map is Standard mode").

## 2. Shared infrastructure (build first — every mode needs it)

### 2.1 Routing engine (the core technical piece)

`geo::Systems` already has gate adjacency, jump bridges, `path()`, `jumps()`,
`jumps_gates_only()`. We extend it into a **constrained, multi-edge router**:

- **Node mask**: a predicate `allowed(system_id) -> bool` built from the active constraints
  (sec band, sov owner, structures, kill thresholds). Disallowed systems are not traversable
  (except possibly the start/end).
- **Edge kinds**: `Gate`, `JumpBridge` (Ansiblex), `ShipJump(range_ly)`. The available kinds
  depend on the chosen ship + the allow/disallow toggles.
- **Regional vs intra-region gates**: a `Gate` edge `(a,b)` is a *regional gate* when
  `info_of(a).region != info_of(b).region` (cheap region compare per edge). "Disallow regional
  gates" drops exactly those edges, so the router must leave a region by jump bridge or ship
  jump — the point is to avoid the camped region-boundary stargates. Intra-region gates stay.
- **Ship-jump edges**: generated on demand — for a jump-capable ship, connect cyno-able
  systems within the ship's range (real light-year coords, already used for the jump-range
  overlay). Fatigue is out of scope for v1 (note it on the route instead).
- **Algorithm**: Dijkstra over the masked graph with per-edge cost (1 per gate, configurable
  weight per jump). Returns the system list + per-leg edge kind. Wrap the existing BFS; add a
  weighted variant. Cache per (ship, constraints, start) like `map_draw_cache`.

This single router serves Travel (explicit A→B), Safety (time-to-safe = jumps to nearest
allowed hi/low system), and Hunting (distance ranking).

### 2.2 Data sources & gaps

| Need | Have? | Source |
|------|-------|--------|
| Gate graph, jump bridges | ✅ `geo::Systems` | SDE + user-pasted bridges |
| Sec status, region/const | ✅ `SystemInfo` | SDE |
| NPC/ship/pod kills, jumps (hourly) | ✅ `systemstatus` (5-min poll) | ESI `/universe/system_kills`,`/system_jumps` |
| Sov upgrades per system | ✅ `settings.sov_upgrades` | pasted I-Hub |
| Coalition→alliance map | ✅ `settings.coalitions` | bundled snapshot, user-editable |
| **Sov *owner* per system** | ❌ | **add ESI `/sovereignty/map`** (system→alliance_id), refreshed hourly |
| **Friendly Keepstar/Fortizar per system** | ❌ → **paste** | new `settings.friendly_structures: Vec<{ system, kind }>` with a paste parser modelled on `parse_sov_upgrades` (kind = Keepstar/Fortizar/…) |
| zKill live kills | partial (`kills.rs` lookups) | **add a feed**: zKillboard RedisQ (poll). ⚠ killmails can lag the real event by **minutes** — always key freshness/age/ETA off the **killmail timestamp**, never receipt time |
| **Per-stargate positions** (gate→gate AU) | ❌ | add to the SDE ingest (mapDenormalize/stargates) — for the §5.1 warp model |
| **Ship warp speed + role bonus + agility** | partial (SDE) | hull attributes/traits — for §5.1 |

Sov-owner is the only remaining ❌; the structure data is a paste (same UX as sov upgrades),
and everything else can ship without it.

### 2.3 Alert + notification engine

Generalise today's alert rules into a small trigger bus so all modes reuse it:

- triggers: `HostileWithin(jumps)`, `KillsInArea(scope, since)`, `SystemIntel(system)`,
  `CynoLit(system)`, `MapDataChanged(system, metric)`.
- actions: sound (existing), desktop notification (existing), **screen flash** (new: a
  full-window coloured overlay pulse), push (existing).
- Each mode wires a preset set of triggers; the user tunes thresholds.

### 2.4 Overlay visibility & mode-aware filtering

`MapOverlays` already toggles sov/ADM/activity/upgrades/bridges/jump-range.
- **Auto-adapt per mode**: switching mode applies a preset that surfaces only what that mode
  needs and hides the clutter — Travel: ship-kills (danger) + bridges (routing); Hunting &
  Safety: ship-kills + intel; Standard: the user's saved layers. The user can still toggle
  within a mode (the toggles are remembered per mode).
- **Master hide toggle**: a one-click "hide all map icons" + per-layer eye toggles in the
  overlay menu, persisted per mode.

## 3. Travel Mode

**Panel (left of the map):**
1. **Ship** picker (search the ship index) → sets jump capability + range, gate access.
2. **Midpoint constraints** (each a checkbox/▾):
   - Security: High / Low / Null (multi-select).
   - Coalition / Alliance space (pick which; uses `coalitions` + `/sovereignty/map`).
   - Friendly Keepstar / Fortizar only (from the pasted `friendly_structures`; can filter by
     kind, e.g. "Keepstar only").
   - Max NPC / ship / pod kills in the last hour (per-metric `DragValue`; uses `systemstatus`).
   - Allow / disallow **regional gates** (region-crossing stargates; see §2.1).
   - Allow / disallow **jump bridges** (Ansiblex).
3. **Start / End** system pickers (reuse the fuzzy search).
4. **Plan** → runs §2.1 router → an **editable route plan**: ordered list with per-leg type
   (gate/bridge/jump), kills/sec per hop, and add-waypoint / avoid-system / drag-reorder.

**Map:** highlight the route (reuse the dashed-flow line), shade disallowed systems, badge
each hop. Recompute live as constraints change. "No route" state explains which constraint
blocked it (helpful for tuning).

## 4. Hunting Mode

**Setup:** jump-capable? (changes distance metric to range vs gates) + which feeds to watch.

**Live board (the heart of this mode):** the app tracks the hunter's location (existing
`player.locations`) and shows a **distance-sorted list** of:
- intel sightings (pilots/ships/fleets) within range,
- watched zKill activity (system / constellation / region the user picked),
- **active cynos** (from intel cyno detection) — alert on new ones,
- map-data changes the user opted into (e.g. NPC kills rising = someone ratting).

The list re-sorts as their location changes (hook the location-update path already feeding
the map "follow" feature). Each row: distance (jumps or LY), age, one-click "set destination".

**Notifications:** opt-in per feed — "notify on new kill in watched scope", "notify on new
cyno", "notify when NPC kills in <system> jump up".

## 5. Ratting / Safety Mode (AFK watch)

**Setup:**
- Hostile proximity: alert if hostile intel is within **N jumps**.
- zKill proximity: alert on zKill feed kills within **N systems**.
- Specific system watch: alert if listed system(s) get hostile intel.
- Intel "hot" duration: how long a sighting stays active if the entity isn't re-reported.
- AFK alarm config: **loud looping sound + full-screen flashing** (the screen-flash action).

**Behaviour:** while AFK, any matching trigger fires the alarm. For each tracked hostile the
map shows an **ETA-to-you** — an estimate of how long that entity would take to reach *your*
system if it burned straight for you. (Not how long you take to flee; it's how much warning
you have.)

### 5.1 Threat-ETA model

`ETA = Σ over the jumps on the hostile→you gate path of:`
`  align/enter-warp + in-system warp (gate→gate) + gate session-change overhead`

- **Path**: `geo` gate path from the hostile's last-known system to yours (gates only, unless
  the hostile is flagged jump-capable).
- **In-system warp time**: warp the real entry→exit stargate distance using EVE's full
  accelerate→cruise→decelerate warp curve (closed-form). Requires **per-stargate positions**
  added to the SDE ingest (gate→gate AU per system).
- **Ship**: warp speed = SDE base × the hull's warp-speed **role bonus** (interceptors,
  blockade runners, some T3s warp far faster) + agility for align — hull from the intel ship or
  the zKill victim/attacker. Unknown hull → a sensible default (e.g. a cruiser).
- **Overhead**: one fixed session-change + gate-tunnel constant per jump.
- **zKill latency** (important): a killmail surfaces minutes after the event, and the hostile
  could have left the instant it died — there is **no grace window**. Model the threat as a
  **reachable set that grows from the killmail timestamp**: systems within travel-time
  `≤ (now − kill_time)` of the kill system (the kill system is just the last-known centre). The
  warning is `effective_eta = model_eta(kill_system → you) − (now − kill_time)`, floored at 0;
  when it reaches 0 the reachable set has touched your system ("could already be here"). Drop
  the threat once the set covers your whole watch range. The "hot" TTL counts from this stamp.
- Cache per `(hull, system→system)`; never re-solve per frame.

If the hostile's hull is jump-capable and lit a cyno path, the ETA can collapse to near-zero —
surface that as a distinct "can hotdrop you" warning rather than a misleadingly long gate ETA.

## 6. Decisions

**Resolved**
1. **Regional gates** = region-crossing stargates (edge model in §2.1). ✅
2. **Friendly structures** = pasted list (`friendly_structures`), same UX as sov upgrades. ✅
3. **zKill feed** = RedisQ polling (~1 s, no auth). ✅ (recommended default)
4. **Jump fatigue** = warn-only for v1, no routing penalty. ✅ (recommended default)

5. **Sov owner** = auto-pull ESI `/sovereignty/map` (system→alliance) hourly + a maintained
   alliance→coalition list (keep the snapshot only as the coalition grouping). ✅
6. **"Time-to-safe"** = the hostile's **ETA to you**, not your escape — the warp/gate model in
   §5.1, discounted by zKill latency. ✅
7. **Active cyno** = intel cyno keyword **+ zKill inference** (cyno hull dying / covert fit),
   accepting some false positives. ✅

8. **zKill staleness** = no grace window; the threat is a reachable set that grows from the
   killmail timestamp (§5.1), dropped once it covers the whole watch range. ✅
9. **Warp fidelity** = full accel/cruise/decel curve over real stargate distances, warp speed
   from the SDE incl. hull role bonuses (§5.1); needs per-stargate positions in the ingest. ✅

All design decisions are settled. Remaining is implementation: the SDE ingest extension
(stargate positions + warp/agility/role-bonus attributes) and the Phase 1 work in §7.

## 7. Suggested phasing

1. **Routing engine** (§2.1) + the `MapMode` selector + overlay-hide toggle. No new data.
2. **Travel Mode** with the constraints we already have (sec, kills, jump bridges, coalition
   via existing `coalitions`). Editable route plan.
3. **Sov-owner ingestion** (`/sovereignty/map`) → unlocks accurate coalition/alliance-space
   constraints; **structure data** decision → friendly-Keepstar constraint.
4. **zKill RedisQ feed** + the trigger bus + screen-flash action.
5. **Safety Mode** (smallest behavioural surface on top of the bus + router).
6. **Hunting Mode** (the live board; reuses the feed, router, and location tracking).

Each phase is independently shippable and testable.
