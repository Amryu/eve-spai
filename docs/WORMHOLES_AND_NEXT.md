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
- [ ] **W1. Model + store.** `Wormhole` struct, `DestClass`, `ShipSize`, `Source`;
      lifetime/expiry + dedup logic (unit-tested); `wormholes` table + upsert/list/prune.
- [ ] **W2. EVE-Scout seeding.** Poll `api.eve-scout.com/v2/public/signatures` (~5 min);
      map fields → records (`in_system`→system, `out_system`→Thera/Turnur dest,
      `wh_type`, `max_ship_size`→size, `remaining_hours`→expiry, reporter→source).
- [ ] **W3. Intel extraction.** Upgrade the parser's `wormhole: bool` to a structured
      detection: existence, type (`K162`/J-codes), destination class or system, size,
      lifetime, drifter. Feed records (source=intel).
- [ ] **W4. Wormholes view.** `View::Wormholes` nav item + list/table: system, type,
      destination, size, life-left or "reported Nh ago", drifter, source. Filters
      (destination class, source, expiring soon); clickable system breadcrumbs.
- [ ] **W5. Map integration.** Connected-destination WHs shown on the map; J-space
      destinations excluded from the map but clickable → system-info window. Verify
      J-space systems exist in the SDE for the info window.

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
