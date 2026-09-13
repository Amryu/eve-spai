# UI tickets

Findings from the headless UI harness (`app/src/uitest/`). One folder per ticket:
`ticket.md`, `before/` screenshots, `after/` screenshots once fixed, and `review.md`
recording the fix and review cycle.

The process is defined in `CLAUDE.md` under "UI issue workflow", which is the authority.
Short version: reproduce into a ticket, fix in a worktree via one agent, review the patch
before applying it, verify by re-rendering the scene and looking at it, land on a branch
merged `--no-ff`, then write the resolution report.

Every closed ticket opens its `review.md` with a Resolution table: outcome, agent time and
tool calls, patches rejected on review, app and harness lines changed, suite before and
after, and any follow-up tickets it spawned.

## Defects

| Ticket | Severity | Region | Wave | Status |
|---|---|---|---|---|
| [UI-001 Nav rail separator strikes through the Jabber row](UI-001-nav-rail-separator-strikethrough/) | High | `nav.rs` | 1 | **Fixed** |
| [UI-002 Invisible 34x28 click target on every intel card](UI-002-invisible-jump-label/) | Medium | `intel_row` | 2 | **Fixed** |
| [UI-003 Fleet ping body dimmer than a routine reminder](UI-003-fleet-ping-body-contrast/) | Medium | `render_ping` | 1 | **Fixed** |
| [UI-004 Alerts toolbar reads "zKill intel within feed"](UI-004-drag-value-reads-within-feed/) | Medium | `alerts_view` | 2 | **Fixed** |
| [UI-005 Battles spinner has no exit path](UI-005-battles-spinner-has-no-exit/) | Medium | `battles_view` | 3 | **Fixed** |
| [UI-006 Settings truncates directory paths](UI-006-settings-paths-truncated/) | Medium | `settings_view` | 3 | **Fixed** |
| [UI-007 Alert title bar cannot be grabbed](UI-007-alert-titlebar-drag-blocked/) | Medium | `alert_cb` | 4 | **Fixed** |
| [UI-008 Sixteen .small() sites on content text](UI-008-small-font-on-content-text/) | Medium | cross-cutting | 8 | **Fixed** |
| [UI-009 Resolving-pilot chip shoves its row](UI-009-resolving-chip-width-jitter/) | Low | `intel_row` | 4 | **Fixed** |
| [UI-010 `uncertain` silently keyed by lowercase](UI-010-uncertain-set-lowercase-contract/) | Low | `intel_row` | 7 | **Fixed** |
| [UI-011 Reporter footer flows inline with badges](UI-011-reporter-footer-inline-with-badges/) | Low | `intel_row` | 5 | **Fixed** |
| [UI-012 Battles toolbar ends on a dangling separator](UI-012-dangling-toolbar-separator/) | Low | `battles_view` | 5 | **Fixed** |
| [UI-013 Doctrine row floats in extra air](UI-013-doctrine-row-floats/) | Low | `render_ping` | 6 | **Fixed** |
| [UI-014 Copy button undersized](UI-014-copy-button-undersized/) | Low | `render_ping` | 7 | **Fixed** |
| [UI-015 Near-duplicate celestial on two rows](UI-015-duplicate-celestial-rows/) | Low | `intel_row` | 6 | **Fixed** |
| [UI-017 Work-throttle ComboBox overflows at wide widths](UI-017-combobox-overflows-at-wide-widths/) | Low | `battles_view` | unscheduled | **Fixed** |
| [UI-018 Ping body lines allocate 26px for 15px of ink](UI-018-ping-body-lines-overallocate/) | Low | `render_ping_body` | unscheduled | **Fixed** |
| [UI-027 Chat message bodies allocate 26px for 15px of ink](UI-027-chat-body-lines-overallocate/) | Medium | `render_message_body` | UI-018 | **Fixed** |
| [UI-028 Rescue chat lines allocate 26px for 15px of ink](UI-028-rescue-chat-line-overallocates/) | Low | `rescue_chat_line` | UI-027 | **Fixed** |
| [UI-029 The alert overlay cannot show the bridge flag](UI-029-overlay-cannot-flag-bridges/) | Medium | `ipc::AlertMsg` | UI-026 | **Fixed** |
| [UI-019 Audit the remaining small_button call sites](UI-019-small-button-audit/) | Low | various | unscheduled | **Fixed** |
| [UI-025 Intel cards count jump bridges regardless of the setting](UI-025-intel-jump-range-ignores-bridge-setting/) | High | `jumps_from_you` | user | **Fixed** |
| [UI-026 Show when an intel jump range depends on a bridge](UI-026-flag-bridge-dependent-jump-range/) | Medium | `intel_row` | user | **Fixed** |
| [UI-023 Dragging a chat tab shows nothing at the cursor](UI-023-tab-drag-needs-cursor-indicator/) | Medium | `jabber_tab_bar_ui` | user | **Fixed** |
| [UI-024 Composer scrolls its own border instead of its contents](UI-024-composer-scrolls-its-border/) | Medium | composer | user | **Fixed** |
| [UI-022 Long chat histories lag, no virtualization plus a per-frame clone](UI-022-chat-history-not-virtualized/) | High | `jabber_conversation_ui` | user | **Fixed** |
| [UI-020 Always-on-top pin floats over popout content](UI-020-ontop-pin-overlaps-content/) | Medium | `ontop_pin` | user | **Fixed** |
| [UI-021 Composer should grow to 10 rows, drop Send](UI-021-composer-grows-and-drops-send/) | Medium | composer | user | **Fixed** |
| [UI-047 A server force-join into a new room surfaces nowhere](UI-047-force-join-opens-once/) | Medium | `jabber_reconcile` | user | **Fixed** |
| [UI-046 Every joined room and every remembered DM gets a tab on every start](UI-046-tabs-persist-and-reconcile-stops-opening/) | High | `jabber_reconcile`, `SpaiApp::build` | user | **Fixed** |
| [UI-045 Closing a room tab leaves the channel, and can silently kill Rescue Mode](UI-045-close-hides-and-rescue-room-pinned/) | High | `close_jabber_tab`, `jabber_forget` | user | **Fixed** |
| [UI-043 Chat timestamps are minute-resolution](UI-043-chat-timestamps-need-seconds/) | Medium | `eve_time_label` | user | **Fixed** |
| [UI-044 Chat timestamps render at 9.5px](UI-044-chat-timestamp-below-body-size/) | Low | chat timestamp labels | UI-043 | Open |
| [UI-041 No way to forget a remembered room or private chat](UI-041-forget-known-conversation/) | Medium | sidebar panes, `jabber_frame` | user | **Fixed** |
| [UI-042 The contacts star is a 9px hit target](UI-042-contacts-star-is-9px/) | Low | Directory pane rows | UI-041 | **Fixed** in UI-068 |
| [UI-040 Jump Plan mode has no command carrier](UI-040-jump-plan-command-carrier/) | Medium | `SHIP_CLASSES`, `jump_plan_content` | user | **Fixed** |
| [UI-039 A hidden chat tab comes back, and leaving a room does not stick](UI-039-jabber-hidden-and-left-rooms-not-persisted/) | High | `jabber_reconcile`, `close_jabber_tab` | user | **Fixed** |
| [UI-038 A full disk kills the app, with no warning before and no record after](UI-038-no-warning-or-survival-when-the-disk-fills/) | Critical | `store.rs` writes, panic hook | user | **Fixed** |
| [UI-037 Intel jump distance measured from the selected character, not the one the alert fired on](UI-037-intel-jumps-only-from-the-selected-character/) | High | `intel_row` | user | **Fixed** |
| [UI-036 Switching chat tabs opens the next conversation at the top of its history](UI-036-chat-scroll-shared-across-tabs/) | High | `jabber_conversation_ui` | user | **Fixed** |
| [UI-035 Rescue jump-off system ranked by map distance, not jumps](UI-035-rescue-jump-off-ranked-by-lightyears/) | High | `update_rescue_range` | user | **Fixed** |
| [UI-034 A number that means a timer, a range or a name is counted as hostiles](UI-034-number-double-counted-as-hostiles/) | High | `parse_count` | user | **Fixed** |
| [UI-033 The always-on-top pin overlays dialog content](UI-033-pin-overlays-dialog-content/) | Medium | `ontop_pin` | GAP-001 | **Fixed** |
| [UI-032 The intel toolbar is too cramped](UI-032-intel-toolbar-too-cramped/) | Medium | `intel_view` | user | **Fixed** |
| [UI-030 Alert rule names truncate in the default panel](UI-030-rule-names-truncate/) | Low | `alert_rules_editor` | UI-019 | **Fixed** |
| [UI-031 Regenerate button uses a bare U+21BB](UI-031-bare-glyph-may-be-tofu/) | Low | `rescue_window_body` | UI-019 | **Fixed** in UI-068 |
| [UI-016 Ping window has no chrome](UI-016-ping-window-has-no-chrome/) | Low | decision | n/a | Closed, not a defect |

## Harness coverage gaps

Tool work rather than app defects. Ticketed so the backlog is complete; not scheduled yet.

| Ticket | Blocks | Effort |
|---|---|---|
| [GAP-001 28 dialogs unreachable](GAP-001-dialogs-unreachable/) | ~6,000 lines | **8 of 28 covered** |
| [GAP-002 Scratch store is never seeded](GAP-002-seed-the-scratch-store/) | 5 of 9 views | Medium |
| [GAP-003 Map is painter-only](GAP-003-map-painter-only/) | ~80% of Map pixels | Medium |
| [GAP-004 Jabber view uncovered](GAP-004-jabber-view-uncovered/) | ~1,200 lines | **Popout done**, in-app view open |
| [GAP-005 Wall clock frozen](GAP-005-wall-clock-frozen/) | 13 sites | Small-medium |
| [GAP-006 Alert auto-dismiss untested](GAP-006-alert-auto-dismiss-untested/) | overlay click passthrough | **Closed** |
| [GAP-007 Viewport commands dropped](GAP-007-viewport-commands-dropped/) | 12 sites, overlay process | Small / large |
| [GAP-008 Input kinds undriven](GAP-008-input-kinds-undriven/) | 4 menus, 2 DnD systems | Small / large |
| [GAP-010 Checker never compares a hit target against text](GAP-010-checker-misses-widget-over-text/) | UI-033's class of bug | Small |
| [GAP-009 i18n and platform branches](GAP-009-i18n-and-platform/) | CJK overflow, rescue windows | Small / large |

## Branch layout

Each fix is a branch merged back with `--no-ff`, so a single merge commit carries the code, its
`review.md` and its `after/` screenshots. To back one out: `git revert -m 1 <merge-commit>`.

## Wave schedule

Two agents at most, never two in the same region. Most of the UI is in one file, so the
pairing is what keeps them from clobbering each other.

| Wave | Tickets | Regions |
|---|---|---|
| 1 | UI-001, UI-003 | `nav.rs` + `render_ping` |
| 2 | UI-002, UI-004 | `intel_row` + `alerts_view` |
| 3 | UI-005, UI-006 | `battles_view` + `settings_view` |
| 4 | UI-007, UI-009 | `alert_cb` + `intel_row` |
| 5 | UI-011, UI-012 | `intel_row` + `battles_view` |
| 6 | UI-013, UI-015 | `render_ping` + `intel_row` |
| 7 | UI-014, UI-010 | `render_ping` + `intel_row` |
| 8 | UI-008 | cross-cutting, runs alone |

## Cost so far

Fifteen defects, one false positive, one harness gap closed. Measured from the agent runs, so
this is fix time and excludes ticket writing and review.

| | |
|---|---|
| Agent time | about 3h 10m across 18 tickets |
| Agent tool calls | 966 |
| Patches rejected on review | 1 (UI-001, a reachability regression it introduced) |
| App code changed | 792 lines added, 269 removed |
| Harness code changed | 1401 lines added, 17 removed |
| Suite | 409 to 441 passing |
| Scenes | 21 to 49 |
| Tickets spawned by fixing others | 4 (UI-017, UI-018, UI-019, UI-024) |

The harness-to-app line ratio is the number worth watching: **1.8 lines of test for every line of
app code changed**, and on the small tickets far more. UI-014 changed 2 lines of app code and added
25 lines of test. That is the cost of the "a fix is not done until a screenshot shows it fixed"
rule, and it is deliberate.

| [UI-048 A direct message is easy to miss behind the sidebar's tabs](UI-048-jabber-convos/) | High | `jabber_ui` | unscheduled | **Fixed** |
| [UI-049 Tray and taskbar say only "something is unread"](UI-049-unread-badge/) | Medium | `tray.rs`, `badge.rs` | unscheduled | **Fixed** |
| [UI-050 Convos lists closed chats and the rows barely respond](UI-050-convos-rows/) | High | `jabber_convos_list_ui` | unscheduled | **Fixed** |

## Web companion (WEB-NNN)

A feature series, not defects: an opt-in local web server in the app serving a responsive page that
mirrors the intel feed, intel alerts, fleet pings and the map to a phone on the same network. Same
folder shape and the same cycle as a UI ticket, one branch per ticket merged `--no-ff`, at most two
agents and never in the same region. Design notes and the reasoning behind the transport and auth
choices live in each ticket.

For a feature ticket, `before/` states the gap and the acceptance evidence; there is no defect to
photograph. Browser screenshots come from WEB-005's fixture demo server, never from the live profile.

| Ticket | Region | Wave | Status |
|---|---|---|---|
| [WEB-001 Settings, pairing token, theme derivation](WEB-001-settings-token-theme-derivation/) | `settings.rs`, `theme.rs` | 1 | **Fixed** |
| [WEB-002 Snapshot and publisher thread](WEB-002-snapshot-publisher/) | `web/snapshot.rs` | 2 | **Fixed** |
| [WEB-003 HTTP server, auth, assets](WEB-003-http-server-auth-assets/) | `web/server.rs` | 3 | **Fixed** |
| [WEB-004 SSE transport](WEB-004-sse-transport/) | `web/sse.rs` | 4 | **Fixed** |
| [WEB-005 Fixture demo server](WEB-005-fixture-demo-server/) | `uitest/webdemo.rs` | 5 | **Fixed** |
| [WEB-006 Intel pane](WEB-006-intel-pane/) | `panes-intel.js` | 6 | **Fixed** |
| [WEB-007 Alerts and pings panes](WEB-007-alerts-and-pings-panes/) | `panes-alerts.js`, `panes-pings.js` | 6 | **Fixed** |
| [WEB-008 Browser dialogs and write-back](WEB-008-browser-dialogs-and-writeback/) | `web/detail.rs`, `dialogs.js` | 7 | **Fixed** |
| [WEB-009 Sound and mute](WEB-009-sound-and-mute/) | `sound.rs`, `sound.js` | 7 | **Fixed** |
| [WEB-010 Layout engine](WEB-010-layout-engine/) | `layout.js` | 8 | **Fixed** |
| [WEB-011 Map pane](WEB-011-map-pane/) | `web/map.rs`, `map.js` | 8 | **Fixed** |
| [WEB-012 Desktop settings UI](WEB-012-desktop-settings-ui/) | `settings_view` | 9 | **Fixed** |
| [WEB-013 First browser render was blank, then drew every pane in the header](WEB-013-first-browser-render/) | `assets/app.js`, `web/assets.rs` | after 5 | **Fixed** |
| [WEB-014 Pairing means typing 43 characters by hand](WEB-014-qr-pairing/) | `settings_view` | with 12 | **Fixed** |
| [WEB-015 Map drew wormhole space, rebuilt on every push, dialogs would not close](WEB-015-map-perf-and-panes/) | `web/map.rs`, `map.js`, `layout.*` | after 14 | **Fixed** |
| [WEB-016 Scanning the pairing QR lands on "not paired"](WEB-016-pairing-cookie/) | `web/server.rs` | after 15 | **Fixed** |
| [WEB-017 Every pane republished every tick; map mirrored, laggy and missing layers](WEB-017-map-layers-and-churn/) | `web/map.rs`, `map.js`, `publish.rs` | after 16 | **Fixed** |
| [WEB-018 Fixed pages kept rendering the old bug; clock stopped; swipe cut off](WEB-018-stale-assets-and-clock/) | `web/server.rs`, `pilot.rs` | after 17 | **Fixed** |
| [WEB-019 Map missing the app's read-only layers; snapshot 2.9 MB](WEB-019-map-parity/) | `web/snapshot.rs`, `map.js` | after 18 | **Fixed** |
| [WEB-020 Map link styling, labels, hover, and the Mumble link](WEB-020-map-links-and-labels/) | `app.rs` map, `map.js` | after 19 | **Fixed** |
| [WEB-021 Map controls: layer panel, upgrade marks, jump range, system window](WEB-021-map-controls/) | `map.js`, `dialogs.js` | after 20 | **Fixed** |
| [WEB-023 Map markers are squares, not the app's icons](WEB-023-map-marker-icons/) | `map.js`, `web/icons.rs` | after 22 | **Fixed** |
| [WEB-024 Markers tiny and stacked; system window over the filters](WEB-024-markers-and-float/) | `map.js`, `dialogs.js` | after 23 | **Fixed** |
| [WEB-025 Map text stays sidebar-sized on a full-screen map](WEB-025-map-scale/) | `map.js` | after 24 | **Fixed** |
| [WEB-026 Names sized off the pane; markers at every zoom; system window in the corner](WEB-026-zoom-scale-and-float/) | `map.js`, `dialogs.js` | after 25 | **Fixed** |
| [WEB-027 Easing, AU units, hidden panes, mobile tabs, raw messages](WEB-027-easing-units-grid/) | `web/assets/*` | after 26 | **Fixed** |
| [UI-051 One join dialog doing two jobs, with the wrong recents for both](UI-051-jabber-start-dialogs/) | `app.rs` jabber | after 50 | **Fixed** |
| [WEB-028 Panes cannot be rearranged, resized or bounded](WEB-028-pane-layout-controls/) | `layout.js`, `layout.css` | after 27 | **Fixed** |
| [WEB-029 No jabber in the web view](WEB-029-jabber-pane/) | `web/jabber.rs`, `panes-jabber.*` | after 28 | **Fixed** |
| [WEB-030 Eased zoom fights the cursor; jump-range rings wrong in 2D](WEB-030-map-followups/) | `map.js` | after 29 | **Fixed** |
| [UI-052 Dashed ground track under every bridge arc](UI-052-bridge-ground-track/) | `app.rs` map | after 51 | **Fixed** |
| [WEB-031 A swap moves the pane and leaves the size behind](WEB-031-swap-carries-the-span/) | `layout.js` | after 30 | **Fixed** |
| [WEB-032 Re-adding a pane disables another; one cycling span button; map broken when added late](WEB-032-span-toggles-and-late-map/) | `layout.js`, `map.js` | after 31 | **Fixed** |
| [WEB-033 A different zoom to the app's; pinch only scales; span would not clear](WEB-033-app-zoom-and-pinch/) | `map.js`, `layout.js` | after 32 | **Fixed** |
| [WEB-034 The wheel zooms the wrong way](WEB-034-zoom-direction/) | `map.js` | after 33 | **Fixed** |
| [WEB-035 Ship dialog has no data and steals the map's window; no per-pane close](WEB-035-own-windows-and-ship-data/) | `dialogs.js`, `layout.js`, `app.rs` | after 34 | **Fixed** |
| [WEB-036 Ship window a fraction of the app's; chips without affiliation; jabber reads badly](WEB-036-detail-parity/) | `detail.rs`, `dialogs.*`, `panes-*` | after 35 | **Fixed** |
| [WEB-037 Jabber messages are flat text](WEB-037-jabber-messages/) | `panes-jabber.*`, `web/jabber.rs` | after 36 | **Fixed** |
| [WEB-038 A theme change takes the server down; range tint drops on pointer move](WEB-038-selection-and-theme-restart/) | `server.rs`, `state.rs`, `map.js` | after 37 | **Fixed** |
| [WEB-039 Drag from a system to route to another, on both maps](WEB-039-map-routing/) | `web/route.rs`, `map.js`, `app.rs` | after 38 | **Delivered** |
| [WEB-040 A second route drag starts over; the start dialog shrinks](WEB-040-route-waypoints/) | `web/route.rs`, `map.js`, `app.rs` | after 39 | **Fixed** |
| [WEB-041 The radial menu asks again on every waypoint](WEB-041-menu-on-the-first-leg/) | `map.js`, `app.rs` | after 40 | **Fixed** |
| [WEB-042 Jump planner picks any shortest path; no hull or skills in the route window](WEB-042-jump-planner/) | `jumproute.rs`, `web/route.rs`, `app.rs` | after 41 | **Fixed** |
| [WEB-043 A route says nothing about what is waiting on it](WEB-043-route-warnings/) | `web/route.rs`, `dialogs.*`, `app.rs` | after 42 | **Fixed** |
| [WEB-044 The web map has no context menu](WEB-044-map-context-menu/) | `map.js`, `route.js` | after 43 | **Delivered** |
| [WEB-045 Which end the titan is at; bridges drawn straight; unreadable warning](WEB-045-titan-end-and-intel-dialog/) | `web/route.rs`, `dialogs.*`, `app.rs` | after 44 | **Delivered** |
| [WEB-046 No avoidance, no alternatives, no waypoint highlight](WEB-046-avoid-and-alternatives/) | `web/route.rs`, `map.js`, `dialogs.*` | after 45 | **Delivered** |
| [WEB-047 Eleven layer buttons; floating route window; half-done avoidance](WEB-047-layer-groups-and-dock/) | `map.*`, `dialogs.*` | after 46 | **Delivered** |
| [WEB-048 Only the route window docks](WEB-048-dock-tabs/) | `dialogs.*` | after 47 | **Delivered** |
| [WEB-049 Waypoints unmarked; a linked route not drawn at all](WEB-049-waypoint-rings/) | `map.js`, `dialogs.js` | after 48 | **Fixed** |
| [WEB-050 Long press opens the menu and the tap underneath it](WEB-050-longpress-tidy/) | `map.js`, `route.js` | after 49 | **Fixed** |
| [WEB-051 A route drag eats the pinch and every pan off a system](WEB-051-mobile-gestures/) | `map.js` | after 50 | **Fixed** |
| [WEB-052 No logo; long pane name; no bind address or unpaired mode](WEB-052-logo-and-advanced/) | `settings.rs`, `server.rs`, `app.rs` | after 51 | **Delivered** |
| [UI-053 The in-app map menu still has its old entries](UI-053-map-context-menu/) | `app.rs` map | after 52 | **Fixed** |
| [WEB-054 Avoid list cannot be inspected; controls too small to hit](WEB-054-avoid-list-and-touch/) | `web/route.rs`, `dialogs.*` | after 53 | **Fixed** |
| [UI-055 The in-app planner is a jump-only form with the route at the bottom](UI-055-route-sidebar/) | `app.rs`, `web/route.rs` | after 54 | **Delivered** |
| [UI-056 Titan systems, row actions, alternatives, Zarzakh](UI-056-titan-systems/) | `web/route.rs`, `jumproute.rs`, `app.rs` | after 55 | **Delivered** |
| [WEB-057 A dialog opened with the map pane off renders into nothing](WEB-057-dock-visibility/) | `dialogs.js` | after 56 | **Fixed** |
| [WEB-058 A waypoint can be added from the row menu but not taken back](WEB-058-remove-waypoint/) | `dialogs.js`, `app.rs` | after 57 | **Fixed** |
| [WEB-059 Layer popups clipped; warnings wrap; list stops short](WEB-059-popups-and-list/) | `map.js`, `dialogs.css` | after 58 | **Fixed** |
| [WEB-060 Titan repositions for its own convenience; jump runs backwards](WEB-060-titan-reposition/) | `web/route.rs`, `map.js`, `app.rs` | after 59 | **Fixed** |
| [WEB-061 A titan route does not say whether it is worth taking](WEB-061-titan-saving/) | `web/route.rs`, `dialogs.*`, `app.rs` | after 60 | **Fixed** |
| [WEB-062 No saving or loading routes in the web; hop list scrolls early](WEB-062-save-routes/) | `settings.rs`, `ipc.rs`, `dialogs.*` | after 61 | **Delivered** |
| [UI-063 The in-app map is behind the web one; reactivation timer verified](UI-063-map-parity/) | `app.rs`, `jumproute.rs`, `map.js` | after 62 | **Delivered** |
| [WEB-064 No rescue pane; switching a pane on switched another off](WEB-064-rescue-pane/) | `web/rescue.rs`, `panes-rescue.*`, `layout.js` | after 63 | **Delivered** |
| [UI-065 Rescue is a second switch, a second window, and a self-changing map](UI-065-rescue-is-a-tab/) | `app.rs`, `nav.rs` | after 64 | **Delivered** |
| [UI-066 Fleet detection stopped; exit button for a mode that is gone](UI-066-rescue-poller/) | `app.rs` | after 65 | **Fixed** |
| [UI-067 Route panel fell out of the dock; arcs do not move](UI-067-route-dock/) | `app.rs` | after 66 | **Fixed** |
| [UI-068 Three features lost their only entry point in the menu cleanup](UI-068-orphaned-by-the-menu-cleanup/) | `app.rs` | after 67 | **Fixed** |

### Browser evidence

The egui harness cannot render HTML, so the WEB series is shot with `app/src/uitest/webshot.sh`,
which renders the WEB-005 fixture demo at 1440 and 390 into `target/webshots`. `GAP-011` records what
that still leaves uncovered. `CLAUDE.md` carries the three ways a headless Firefox silently produces
no file at all.

WEB-013 is why this matters: five tickets landed with 631 green tests and a page that rendered
nothing.

### The `app.rs` rule

`app/src/app.rs` is 27k lines and is the path of least resistance for all of this. The whole series
is allowed exactly four edits to it, in four different tickets, so no two agents collide there:

| Ticket | Permitted edit |
|---|---|
| WEB-002 | the `UiFacts` field and one `publish_ui_facts()` call |
| WEB-003 | the `if !headless` server start hook, beside `instance::start_control_listener` |
| WEB-008 | one arm in the existing `OverlayToMain` drain |
| WEB-012 | the settings pane section |

Everything else lives under `app/src/web/`.
