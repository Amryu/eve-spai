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
| **Friendly Keepstar/Fortizar per system** | ❌ | **open question** — ESI has no public all-structures list; options: user paste, manual pin, or derive from sov + a "friendly structure" list |
| zKill live kills | partial (`kills.rs` lookups) | **add a feed**: zKillboard RedisQ (poll) or websocket |

The two ❌ rows gate the richer Travel/Hunting constraints; everything else can ship without
them.

### 2.3 Alert + notification engine

Generalise today's alert rules into a small trigger bus so all modes reuse it:

- triggers: `HostileWithin(jumps)`, `KillsInArea(scope, since)`, `SystemIntel(system)`,
  `CynoLit(system)`, `MapDataChanged(system, metric)`.
- actions: sound (existing), desktop notification (existing), **screen flash** (new: a
  full-window coloured overlay pulse), push (existing).
- Each mode wires a preset set of triggers; the user tunes thresholds.

### 2.4 Overlay visibility ("hide icon overlays")

`MapOverlays` already toggles sov/ADM/activity/upgrades/bridges/jump-range. Add a master
"hide all map icons" toggle + per-layer eye toggles in the overlay menu, persisted per mode.

## 3. Travel Mode

**Panel (left of the map):**
1. **Ship** picker (search the ship index) → sets jump capability + range, gate access.
2. **Midpoint constraints** (each a checkbox/▾):
   - Security: High / Low / Null (multi-select).
   - Coalition / Alliance space (pick which; uses `coalitions` + `/sovereignty/map`).
   - Friendly Keepstar / Fortizar only (needs the structure data — gated).
   - Max NPC / ship / pod kills in the last hour (per-metric `DragValue`; uses `systemstatus`).
   - Allow / disallow **regional gates** *(decision needed — see §6)*.
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

**Behaviour:** while AFK, any matching trigger fires the alarm. The map shows tracked
intel with an **estimated time-to-safe**: jumps from the hunter to the nearest allowed
high/low-sec (or configured safe) system via §2.1, shown per hostile so the user knows if a
threat heading their way out-paces their escape.

## 6. Decisions needed before building

1. **"Regional gates"** — does this mean the Ansiblex jump-gate network, or stargates that
   cross region boundaries, or both? Changes the edge model.
2. **Friendly structures** — acceptable source? (paste like sov upgrades / manual pins on the
   map / skip for v1 and add later.)
3. **zKill feed** — RedisQ polling (simple, ~1s latency, no auth) vs websocket. Recommend RedisQ.
4. **Jump fatigue** — model it in routing, or just warn? (Recommend warn for v1.)
5. **Coalition data freshness** — keep the bundled snapshot user-editable, or auto-pull
   `/sovereignty/map` + a maintained alliance→coalition list?

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
