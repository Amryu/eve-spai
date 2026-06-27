# EVE Spai — Design Document

> Name: **EVE Spai** (EVE slang for an intelligence "spy"). Workspace: `eve-spai`.
> Status: draft. This is a ground-up design. Everything here describes intended
> behavior and our own structure.

---

## 1. Purpose & vision

A fast, lightweight desktop intel tool for EVE Online. It watches the game's
chat/combat logs and ESI data, parses hostile/neutral activity from intel
channels, and surfaces it on a map and in live feeds with timely alerts.

The guiding principle: **do less, cost less.** A tiny resource footprint and instant
responsiveness come first; the UI is clean and legible rather than heavily styled.

### Primary user
A null/low-sec EVE player running this continuously alongside the game (often on a
second monitor), who needs at-a-glance situational awareness and fast alerts.

---

## 2. Design principles

These are requirements, not aspirations. The tool must stay small and responsive when
left running for days alongside the game: a low idle footprint, byte-bounded caches,
few threads, and no wasteful rendering.

1. **One window.** A single primary window hosts everything. Configuration and
   occasional deep-dives use modal/secondary dialogs, not a constellation of
   persistent windows.
2. **Minimal chrome, minimal motion.** No splash screen, no parallax, no animated
   portraits, no blur/transparency theatrics. Static, legible UI. Animation is
   limited to functional feedback (e.g. a fading "new intel" highlight).
3. **Low-resolution imagery.** Character/corp/alliance icons render at the small
   size they're displayed at (target 32–64 px). We request the smallest sufficient
   image size and cache by **bytes, not count**, with a hard cap.
4. **Bounded everything.** Every cache has a byte ceiling. Background work runs on a
   small shared async runtime, not per-feature thread pools. We set explicit memory
   targets (see §3) and treat exceeding them as a bug.
5. **Lazy by default.** Subsystems and their data load on first use, not at startup.
   Startup is fast enough that a splash screen is pointless.
6. **Offline-tolerant.** Static game data (the SDE) ships/caches locally; the app is
   usable for log parsing even when ESI is unreachable.
7. **Cross-platform, single binary.** Windows, macOS, Linux from one codebase, with
   no separate runtime to install.

### 3. Resource budget (non-functional targets)

| Metric | Target (idle, warmed) | Hard ceiling |
|---|---|---|
| Resident memory (RSS) | ≤ 250 MB | 400 MB |
| Threads | ≤ 24 | 40 |
| Image cache | ≤ 32 MB | 48 MB |
| CPU at idle (no new intel) | < 1% | — |
| Cold start to interactive | < 1.5 s | 3 s |

These are tracked as part of CI/manual verification, not left to chance.

---

## 4. Technology choices

| Concern | Choice | Rationale |
|---|---|---|
| Language | **Rust** | Single static binary, no runtime, predictable memory, cross-platform. |
| GUI | **egui** (via `eframe`) | Immediate-mode → no retained widget tree, no animation engine, tiny overhead. Single-window + side panel + modal dialogs are native idioms. Theming is a small `Style`/color struct. Best fit for "lean, low-resource." |
| Async runtime | **tokio** (single shared multi-thread runtime, capped worker threads) | One runtime for all I/O (ESI, zKill, file watch), not per-feature pools. |
| HTTP | **reqwest** (rustls) | ESI/zKill/image fetches; connection pooling. |
| File watching | **notify** | EVE log directory watching. |
| Persistence | **SQLite** via `rusqlite` (or `sqlx`) | Local store for tokens, settings, cached entities, intel history. |
| Static data | **EVE SDE**, **downloaded on first run** + refreshed periodically, baked into a local DB | System/region/type names + map geometry. Not bundled — the SDE updates regularly, so we fetch + version it locally. |
| Serialization | **serde** / `serde_json` | ESI payloads, settings, themes. |
| Audio | **rodio** | Alert sounds. |
| Notifications | **notify-rust** (+ platform fallbacks) | Native desktop toasts. |
| Tray | **tray-icon** | Minimize-to-tray. |
| Auth | OAuth2 PKCE against EVE SSO; local loopback callback (`tiny_http` or hyper) | Standard EVE SSO flow. |

**Decision status (locked):** GUI = **egui** (D1 resolved). If we later want richer
custom map rendering, egui integrates raw `wgpu`/painter callbacks without changing
the shell. Alternatives ruled out: `iced` (retained/Elm — heavier for many small
dynamic panels), `tauri` (a web/browser engine's memory cost contradicts our budget).

---

## 5. Architecture

Layered, feature-driven crates in one Cargo workspace. This is our own decomposition,
organized around *data flow* (ingest → process → present), not around UI windows.

```
┌──────────────────────────────────────────────────────────────┐
│  app-shell (egui/eframe)                                       │
│   single window · nav rail · views · dialogs · theming         │
└───────────────▲───────────────────────────▲──────────────────┘
                │ view-models (state slices)  │ commands
┌───────────────┴───────────────────────────┴──────────────────┐
│  core (no UI)                                                  │
│   ┌─────────────┐ ┌──────────────┐ ┌──────────────────────┐   │
│   │ intel-engine│ │ map-model    │ │ alerts + notify       │  │
│   └─────▲───────┘ └──────▲───────┘ └──────────▲───────────┘   │
│         │                │                     │              │
│   ┌─────┴──────┐  ┌──────┴───────┐  ┌──────────┴───────────┐  │
│   │ log-watch  │  │ sde (static) │  │ esi-client + auth     │  │
│   │ (notify)   │  │ data         │  │ zkill · images        │  │
│   └────────────┘  └──────────────┘  └──────────────────────┘  │
│   ┌────────────────────────────────────────────────────────┐ │
│   │ store (SQLite) · config/themes · async runtime          ││
│   └────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### Crate / module sketch
- **`platform`** — async runtime, SQLite store, settings + theme persistence,
  bounded byte-caches, paths, OS integration (tray, notifications, audio).
- **`sde`** — preprocessed EVE static data: systems, regions, gates (graph),
  type/name lookups, map geometry. Loaded lazily, read-mostly.
- **`esi`** — typed ESI client, OAuth2/PKCE auth, token store + refresh,
  multi-character identity. Rate-limit aware. Also wraps zKillboard + image server.
- **`ingest`** — EVE log/chat file discovery + tailing (combat logs, chat logs,
  Local). Emits raw line events.
- **`intel`** — the parser + state engine: turns chat/zkill/dscan lines into typed
  intel entities, tracks decay/movement/clear, computes distances.
- **`mapmodel`** — projection + overlays state for the map view (security, intel,
  character location, routing).
- **`alerts`** — rule evaluation over intel/log/comms events → actions.
- **`app`** — egui shell: window, nav rail, views, dialogs, theme engine,
  character context, status bar. Holds view-models that subscribe to core state.

### Data flow (intel example)
`notify` sees a chat log change → `ingest` tails new lines → `intel` parser
classifies entities, resolves names via `sde`/`esi`, updates the intel state
(with decay timers) → `mapmodel` + feed view-models observe the change → `alerts`
evaluates rules → `platform` fires sound/desktop/push actions. The egui shell
repaints only the affected views.

### Concurrency model
- **One** tokio runtime with a small fixed worker count.
- Core state lives behind channels + an `Arc<RwLock<…>>` snapshot the UI reads each
  frame; the UI never blocks on I/O.
- No feature spins up its own thread pool. Periodic pollers (location, zkill) are
  async tasks with intervals, cancelled when their view/feature is disabled.

---

## 6. UI / UX design

### 6.1 Window layout
A single resizable window, three regions:

```
┌──┬───────────────────────────────────────────────┐
│N │  Top bar: active character ▾ · EVE-time clock · │
│A │           connection/ESI status · global search │
│V ├───────────────────────────────────────────────┤
│  │                                                 │
│R │                MAIN CONTENT                     │
│A │            (the selected view)                  │
│I │                                                 │
│L │                                                 │
│  ├───────────────────────────────────────────────┤
│  │ Status bar: intel count · last update · alerts ●│
└──┴───────────────────────────────────────────────┘
```

- **Nav rail (left):** the "Neocom" — a vertical list of icon buttons, one per view.
  - **Collapsed (default):** icons only, narrow (~48 px).
  - **Expanded:** icon + text label per item (~180 px). A pin/chevron toggles it;
    state persists. (Optional: auto-expand on hover, click-to-pin.)
  - The active view is indicated with the theme **accent** color (left bar + tint).
  - Rail order (essential views first): Overview/Dashboard, Map, Intel, Characters,
    Alerts, Settings. Advanced views append as built (Assets, Wallet, PI, Comms…).
  - Bottom of rail: secondary actions (Settings, About) pinned apart from primary
    views.
- **Top bar:** active-character selector (the character whose location/region drives
  "near me" filters and routing), EVE-time/local clock, a compact connectivity
  indicator, and a global entity search box.
- **Main content:** the selected view fills the area. Views are internally free to
  use split panes, tables, and an embedded map canvas.
- **Status bar:** live intel count, last-update time, and an alert/mute indicator.

### 6.2 Dialogs vs views
- **Views** (in the rail): things you monitor continuously.
- **Dialogs** (modal or detached): configuration, one-off detail drill-downs
  (e.g. a single colony's pin layout, a contact editor, a fit viewer). Dialogs are
  cheap to open/close and never persist as background windows.

### 6.3 Theming — exactly three colors
A theme is **three user-chosen colors**:

| Role | Meaning | EVE-default |
|---|---|---|
| `background` | window/base fill | very dark blue-black (≈ `#0B0F12`) |
| `foreground` | primary text & icons | light grey (≈ `#C8D2D8`) |
| `accent` | highlights, active nav, headers, borders, selection | EVE cyan (≈ `#3FA9C9`) |

Everything else is **derived** at runtime: surface/panel fills by lightening/darkening
`background`; muted text by blending `foreground` toward `background`; hover/pressed
states by alpha on `accent`. This keeps theming to "pick 3 colors" while staying
cohesive.

- Ship a few **presets** demonstrating the range (e.g. *Caldari* cyan-on-black default,
  *Amarr* gold-on-black, *Gallente* green-on-black, plus a high-contrast light theme).
- **Semantic standing colors** (hostile / neutral / friendly / corp / alliance /
  warning) are a **separate small fixed palette** with sensible defaults (red / grey /
  green / blue / purple / amber). These are *not* part of the 3-color theme because
  friend-vs-foe meaning can't be derived from chrome colors. Overriding them is an
  **advanced** setting.
- No transparency/blur effects in the essential build (they were a measured source of
  GPU/CPU waste). Window opacity may return as an *advanced*, off-by-default toggle.

### 6.4 Imagery policy
- Request character/corp/alliance images at the **display size** (32 or 64 px), never
  256/512/1024.
- Decode to a small texture; cache textures by **total bytes** with a hard cap (§3);
  evict LRU; re-fetch from disk cache when needed.
- Missing/loading images show a neutral placeholder immediately (no layout shift).

---

## 7. Feature roadmap

Features are split into **Essential (MVP)** and **Advanced (designed-for, built
later)**. The architecture in §5 must accommodate all Advanced items now (interfaces,
data model, nav-rail extensibility) even though they ship later.

### 7.1 ESSENTIAL — the MVP intel tool

**E1. App shell**
- Single window; collapsible Neocom nav rail (icon ↔ icon+label).
- Top bar (active character, clock, connectivity, search); status bar.
- 3-color theming + presets; light/dark; EVE-time vs local; ISK formatting.
- Tray icon + minimize-to-tray; single-instance. **No splash screen.**

**E2. Identity & static data**
- EVE SSO OAuth (PKCE), multi-character, encrypted token storage + auto-refresh.
- Minimal ESI scope set for MVP: location/online/ship, character affiliation,
  read contacts (for standings), write-waypoint + open-window. (Broader scopes
  deferred with their features.)
- SDE downloaded on first run (then refreshed when a newer version is published),
  baked into a local DB: system/region/type names, gate graph, map geometry.

**E3. Intel ingestion & parsing** — core
- Discover + tail intel chat channels (configurable; optional region binding) and
  Local for own-presence.
- Entity detection: named characters, character counts, ships (+counts/plurals),
  gates/Ansiblex, celestials w/ distance, wormholes, killmail links, spike, ESS,
  skyhook, gate camp, combat probes/scanners, bubbles, no-visual, movement.
- Status keywords (clear/clr, nv, etc.) incl. common language variants; question
  detection; count parsing (`5x`, `+3`, `=10`, words).
- State engine: decay/TTL, conversation merge window, movement tracking, clear
  handling, kill-removes-pilot, dedup, distance/jumps to active character.

**E4. Intel presentation**
- **Intel view**: live per-system feed + chronological report list in one view
  (tabs or split). Filters: channel, space type, distance (all / my regions /
  within N jumps), entity type. Sort by time/distance. Search. Compact density.
- New-intel highlight (brief fade) — the one allowed bit of motion.

**E5. Map (lean)**
- 2D region map + a simple cluster/region selector. Security coloring.
- Overlays: **intel** (markers + staleness), **own character location(s)**, gate
  connections. Hover/click system info popup.
- Basic routing: set destination / add waypoint via ESI; show route.
- (3D cluster, the ~24 color strategies, jump-bridge network, sov overlays,
  distance maps, map notes → Advanced.)

**E6. Killboard intel**
- Live zKillboard feed; inject kills as intel; show victim/attacker/ship/system/time.
- (Analytics/recent-activity dashboards → Advanced.)

**E7. Location tracking**
- Poll active/selected character location; Local-based system change detection;
  drive "near me" filters + map. (Fleet roster → Advanced.)

**E8. Basic standings**
- Classify entities friendly / neutral / hostile from your ESI contacts and/or a
  manual list (and config-pack data later). Drives intel + map coloring.
- (Full contact CRUD, labels, watch/block → Advanced.)

**E9. Alerts & notifications (core)**
- Triggers: intel matches (any/named character, any/specific ship, hostile in
  system/within N jumps), channel inactivity.
- Actions: in-app notification, native desktop toast, sound (built-in/custom),
  per-alert cooldown, enable/disable, grouping.
- (Game-action/PI/Jabber-ping triggers + push services → Advanced.)

**E10. Settings**
- Dialog-based: EVE log/settings paths (auto-detect + manual), intel channels,
  characters, theme, alerts, units/time. Robust settings load (backup on corruption).
- (Setup wizard, config packs, "what's new", debug panel → Advanced.)

### 7.2 ADVANCED — designed now, built later

> The shell, data model, and core interfaces must leave room for these. Each becomes
> a new nav-rail view or an extension of an existing view/dialog.

**A1. Full map suite** — 3D cluster + 2D cluster layouts; all
~24 system color strategies; configurable per-system indicators (≤6) + info box;
jump-bridge network (import/auto-discover/export/forget, opacity); sov-upgrade
overlay; distance/jump-band maps; map notes/markers; NPC kills, incursions, storms,
industry indices, etc.

**A2. Account-data views** — **Assets** (hierarchical, pricing, fits),
**Wallet** (balances, journal, charts, insights, loyalty points), **Clones/implants**,
**Contacts** (full CRUD, labels, watch/block, universe search), **Planetary Industry**
(colony monitoring, pin maps, simulation, expiry alerts, exports), **Opportunities**
(career/corp projects/freelance jobs). Each pulls the additional ESI scopes it needs.

**A3. Game-log combat awareness** — under-attack/attacking/scrambled/
decloaked/out-of-charges/clone-jump events; recently-targeted; as alert triggers and
a small live status.

**A4. Comms** — Jabber/XMPP (roster, MUC, DMs, presence), in-app chat
aggregation, fleet **Pings** (FC/formup/PAP/doctrine/comms parsing, open Mumble).

**A5. Sovereignty upgrades** — tracking, clipboard "hack" import, map
overlay + filters.

**A6. External log-stream ingestion** — TCP intake of EVE log relays; filter
view.

**A7. Extended alerts & push** — PI alerts, Jabber-ping
alerts, chat/regex alerts, **Pushover** + **ntfy** push, advanced notification
positioning.

**A8. Onboarding & packs** — first-run setup wizard; configuration
packs (preset channels/jump-bridges/sov/standings by coalition); settings-copy between
characters; what's-new; debug/diagnostics panel.

**A9. Clipboard integrations** — d-scan (dscan.info/adashboard),
jump-bridge, sov-hack paste parsing.

**A10. Niceties** — jukebox (likely **dropped** unless requested);
active-EVE-window detection + environment warnings (clock sync, fullscreen client,
language).

### 7.3 Out of scope
Deliberately not built — each works against the resource and simplicity goals:
- **Splash screen** — startup is fast; pointless.
- **Animated/parallax "dynamic portraits"** — static low-res icons instead (§6.4).
- **Transparency/blur by default** — measured GPU/CPU waste; advanced opt-in at most.
- **High-resolution portrait fetching/caching** — replaced by the §6.4 policy.
- **Telemetry/analytics** — not included.
- **Per-feature thread pools** — replaced by one shared async runtime.

---

## 8. Data model & storage (sketch)
- **SQLite** local DB: `characters` (+ encrypted tokens), `settings`, `themes`,
  `intel_history`, `entity_cache` (name/affiliation/standing), `jump_bridges` (adv),
  `alerts`, `map_notes` (adv).
- **SDE store**: read-only data (systems, regions, gates graph, type names, map
  coordinates) **downloaded on first run** and baked into a local DB. Versioned; a
  background check refreshes it when CCP publishes a newer SDE. App degrades
  gracefully (log parsing still works) if the SDE isn't downloaded yet.
- **Image disk cache**: small files keyed by `entity:size`; in addition to the
  in-memory byte-capped texture cache.
- All caches: byte ceilings + LRU. Intel history pruned by age/size.

---

## 9b. Deferred — built-in player lookup (zKill fit analysis)
TODO. A search to look up a pilot via zKillboard and infer the **fits they bring**
from their recent **losses**, shown prominently: weapon type + optimal/falloff
**range**, **speed**, **tank type** (active — with rep count — vs buffer), **EHP**,
and **resist profile**. Requires the ship-attribute (dogma) data + zKill character
losses + fit parsing (overlaps the deferred ship/character intelligence work).

## 9c. Map 2D layout — `position2D` (future)
EVE's in-game 2D star map uses a **precomputed `position2D`** per system (and per
constellation/region): the "schematic" layout that keeps regions clustered while
preserving the big inter-region distances. It is NOT in the Fuzzwork CSV dump
(only 3D `x/y/z`); it lives in the **modern JSONL SDE** as
`mapSolarSystems.jsonl` → `position2D.x` / `position2D.y` (X follows 3D X, Y
follows 3D Z). Until we ingest the JSONL SDE, the map approximates it: anchor each
region at its geographic centroid and de-overlap systems only *within* a region
(`map::spaced_layout`). TODO: add a JSONL-SDE fetch and bake `position2D` for an
accurate star-map layout. Refs: developers.eveonline.com map-data guide.

## 9d. Deferred — Jump planner (A1 extension)
A capital jump-route planner. We already have true 3D coordinates → light-year
distances and the jump-range bands (capital 5 / black-ops 8 / JF 10 ly), so range
maths is done. Open work:
- **Structure awareness.** Routing must know which structures can be jumped to and
  which can dock capitals. ESI exposes only *your* corp structures
  (`/corporations/{id}/structures`, auth) and limited public ones — coalition-wide
  cap-dockable/cyno structures are **not** fully public. So: import a structure
  list (paste/parse, like jump bridges) + use ESI for own structures.
- **Configurable skills** (Jump Drive Calibration etc.) → range/fuel.
- **Auto-pathfinding** (Dijkstra over jump-reachable structures within range) +
  **manual path** building by ship type/skills.
- **Alternative systems** (same range / same jumps / valid structure exists).
- **High-activity warnings** along the route (intel in the last 4 h + capsuleer
  kills from `/universe/system_kills`).
Estimate: its own milestone; structure import is the gating piece.

## 9e. Deferred — Jabber / XMPP comms (A4)
Embedded XMPP client (poppable out), one window combining the buddy list and chat:
active chats at the top, a divider, then visible contacts with director/commander
groups first (folders). Parses + notifies **Imperium pings**; normal MUC/DM chat;
online presence/roster. No conversation persistence for now.
- **Feasibility.** Needs an async XMPP stack (e.g. `tokio-xmpp`), which pulls in
  tokio + TLS — a real dependency-weight increase vs. the current lean build. The
  Imperium Jabber server + ping message format must be handled. This is a full
  subsystem and its own milestone, not an incremental add.

## 9. External integrations
EVE **SSO** (OAuth2 PKCE) · EVE **ESI** (REST, rate-limit aware) · EVE **image
server** (small sizes only) · **zKillboard** (+ redisq/stream for live kills) ·
**EveWho** *(adv)* · **eve-scout** *(adv, wormholes/storms)* · **dscan.info /
adashboard** *(adv)* · **Pushover / ntfy** *(adv)* · **NTP** *(adv, clock-sync
warning)*. Every integration is lazy and failure-tolerant; none block the UI.

> **eve-scout** (`api.eve-scout.com`) — TODO/deferred. Covers metaliminal **storms**
> and Thera/Turnur wormholes, which ESI does NOT expose. When added, feed the same
> per-system condition chips as incursions/FW/sovereignty.

---

## 10. Milestones
- **M0 — Skeleton:** workspace, egui shell, nav rail, 3-color theming, settings dialog,
  SQLite store, tray. (No EVE data yet.)
- **M1 — Identity + SDE:** SSO login, multi-char, token refresh; first-run SDE
  download + bake + version check; name/system lookups; active-character context.
- **M2 — Intel core:** log discovery/tailing, parser, state engine, Intel view (E3/E4).
- **M3 — Map (lean) + location:** E5/E7; intel on the map.
- **M4 — Killboard + alerts:** E6/E9; sound + desktop notifications.
- **M5 — Hardening:** meet §3 budgets; polish; packaging for Win/mac/Linux.
- **Post-MVP:** Advanced items A1–A10 as prioritized.

---

## 11. Decisions
- **D1 — GUI framework. ✅ RESOLVED: egui.**
- **D2 — Map rendering.** Start with egui's painter (lines/circles/text) for the 2D
  region map; revisit a `wgpu` canvas only if the full 3D cluster (A1) needs it.
- **D3 — SDE pipeline. ✅ RESOLVED: download on first run + version-checked refresh**
  (not bundled), baked into a local DB. Open sub-question: which SDE source/format
  (CCP fuzzwork CSV, the official SDE archive, or a community JSON mirror).
- **D4 — DB layer.** `rusqlite` (sync, simplest) vs. `sqlx` (async). Leaning rusqlite
  on a blocking task.
- **D5 — Token encryption at rest.** OS keychain vs. app-encrypted DB field.
- **D6 — App name. ✅ RESOLVED: EVE Spai** (workspace `eve-spai`).

---

## 12. Traceability
Inventory section → roadmap item:
`1→E1` · `2→E2/A2` · `3→E3` · `4→E4` · `5→E5/A1` · `6→E6` · `7→E7/A4` · `8→A3` ·
`9→A6` · `10→A5` · `11→E9/A7` · `12→E9/A7` · `13→A4` · `14→E8/A2` · `15→cut` ·
`16→E10/A8/A9/A10` · `17→E2/E6/A-various`.
