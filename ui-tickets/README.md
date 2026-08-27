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
| [UI-034 A number that means a timer, a range or a name is counted as hostiles](UI-034-number-double-counted-as-hostiles/) | High | `parse_count` | user | **Fixed** |
| [UI-033 The always-on-top pin overlays dialog content](UI-033-pin-overlays-dialog-content/) | Medium | `ontop_pin` | GAP-001 | **Fixed** |
| [UI-032 The intel toolbar is too cramped](UI-032-intel-toolbar-too-cramped/) | Medium | `intel_view` | user | **Fixed** |
| [UI-030 Alert rule names truncate in the default panel](UI-030-rule-names-truncate/) | Low | `alert_rules_editor` | UI-019 | **Fixed** |
| [UI-031 Regenerate button uses a bare U+21BB](UI-031-bare-glyph-may-be-tofu/) | Low | `rescue_window_body` | UI-019 | Open |
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
