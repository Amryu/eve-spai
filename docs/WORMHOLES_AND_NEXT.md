# Next phase — Wormholes, wizard, d-scan, fits, integrations

Working plan for the features accepted after the release-infra step. Tick items as
they land. Companion to `DESIGN.md` (this is the active worklist; DESIGN.md is the
reference). Account-data views (A2), A7 push extras, 9c/9d are **out of scope**.

## Resolved
- **A5 — Sovereignty upgrades.** Already built (`settings::SovUpgrade` paste import +
  map overlay/filter). No work needed.
- **A6 — LogLite (TCP intel relay).** Skipped for now (keep it simple).

---

## W — Wormholes view (primary)

A dedicated nav view listing known wormholes, seeded from EVE-Scout on a timer and
auto-populated from intel channels.

**Data per wormhole**
- system it's in (required)
- signature id (optional, e.g. `ABC-123`)
- type (optional, e.g. `K162`, `J377`)
- destination (optional): class `J / NS / LS / HS / Thera / Turnur`, or a specific
  system if scouted/tested
- size (optional): small / medium / large / xlarge
- lifetime left (optional) — if absent, show "reported Nh ago" instead
- is-drifter (bool)
- source: EVE-Scout / intel / manual

**Lifetime rules**
- Normal WH: max **2 days** from report. Drifter WH: max **1 day**.
- Effective expiry = explicit lifetime if given, else `reported_at + max_life`.
- Prune (hide/drop) once expired.

**Destination = J-space is allowed** (disconnected systems). Those are **not drawn on
the map**, but the system-info window must open for them.

### Steps
- [x] **W1. Model + store.** `Wormhole`/`DestClass`/`ShipSize`/`Source`; lifetime
      (2 days, 1 drifter), dedup + merge (unit-tested); `wormholes` table + upsert/
      list/prune.
- [x] **W2. EVE-Scout seeding.** Polls `api.eve-scout.com/v2/public/signatures` every
      5 min → records (verified live: 24 Thera/Turnur connections).
- [x] **W3. Intel extraction.** Static catalogue of all 96 WH codes
      (`wormholes::WH_TYPES`); parser recognises a code → `wh_type`, plus text detail
      (destination class, EOL, drifter, signature). Watcher creates intel records.
      **Facts win:** the type's class/size/drifter and EVE-Scout data are never
      overridden by an intel guess (source-ranked merge); intel only fills genuine
      gaps (e.g. K162's destination). **Connections are paired** — each holds both
      endpoints + both signatures, and a signature matching either endpoint pairs with
      the existing connection instead of creating a second one. (Scouted-specific-
      system from free text is still TODO.)
- [x] **W4. Wormholes view.** Nav item + table: system [sig], type (+ drifter),
      destination with the **target system's constellation + region**, size, life or
      "reported N ago", source. Clickable system + scouted-dest breadcrumbs. Filters:
      destination class, source, expiring <4h.
- [x] **W5. Map integration.** Direct k-space↔k-space holes drawn as teal lines;
      chains linking two k-space systems through J-space drawn as purple dashed lines
      labelled with the J-space hop count (BFS through J-space only, skipping public
      hubs by degree so Thera spokes aren't bogus chains); a spiral marker on systems
      with a hole into J-space (J-space itself stays off the map but opens in the
      system-info window).

---

## A8 — Setup wizard
- [ ] First-run wizard (log/settings paths, intel channels, characters, theme),
      **dismissable**, re-runnable later from Settings.

## A9 — D-scan clipboard upload
- [ ] Watch the clipboard; when it holds valid d-scan data, prompt to upload it to a
      sharer (dscan.info / adashboard) and return a shareable link.

## 9b — zKill fit-from-losses lookup
- [ ] Pilot search → recent zKill losses → inferred fit (weapon + range, speed, tank
      type/EHP, resist profile). Needs dogma ship attributes + fit parsing.

## Integrations to wire
- [ ] **EVE-Scout** — via W2 (signatures) + storms endpoint → condition chips.
- [ ] **dscan.info / adashboard** — via A9.
- [ ] **EveWho** — corp/alliance membership lookups.
- [ ] **NTP** — clock-sync drift warning (EVE-time accuracy).

*(ntfy / A7 push extras: skipped. position2D map / jump planner: skipped.)*
