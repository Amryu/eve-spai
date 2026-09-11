# WEB-008 &mdash; Badges are inert, and nothing done on the phone reaches the app

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `web/routes_detail.rs`, `assets/dialogs.{js,css}`, `ipc.rs`, the `OverlayToMain` drain |
| **Reported by** | user, remote web view |

## Gap

In the app a badge opens a window: system, ship, pilot, or the verdict prompt for an uncertain name.
On the page they do nothing, and a `?` pilot cannot be classified from the phone.

## Deliverable

Dialogs render **in the browser**, not on the desktop. Read-only endpoints build the DTOs from the
same sources the egui dialogs read:

| Route | Source |
|---|---|
| `GET /api/system/{id}` | `Systems::info_of`, `system_status` `SysFlags`, the `jove` table, as `system_window` (`app.rs:14434`) |
| `GET /api/ship/{id}` | `store.ship_details(id)`, `store.ship_traits(id)`, as `ship_window` (`app.rs:14696`) |
| `GET /api/pilot/{name}` | `lookup::spawn_lookup` into a web-owned cache, surfacing `Idle / Loading / Done / Failed` as such, as `pilot_window` (`app.rs:8833`) |

Write-back: `POST /api/action` carrying `OverlayToMain`, queued in
`web::Inbox = Arc<Mutex<Vec<OverlayToMain>>>` and drained in the **same block** that already drains
the overlay inbox (`app.rs:18327`), hitting the same arms. `Verdict { name, hidden }` reaches
`apply_pilot_verdict` (`app.rs:6216`). `CompactToggle` reaches settings. One new variant
`AlertAck { id }` clears a single report from `alert_feed`, `rule_feeds` and `SharedAlertWindow.feed`.

The page applies the change optimistically and reconciles on the next snapshot.

## Notes

Badge taps are deliberately **not** `IntelClick`. That enum exists to open a desktop viewport
(`act_on_intel_click`, `app.rs:9459`), which is the behaviour the user ruled out: a tap on the phone
must not raise a window on the desktop.

Adding a variant to `OverlayToMain` is safe because it only flows child to parent, and the parent
always spawns the child from its own `current_exe`, so the parent is never the older binary. `ipc.rs`
has compatibility tests; extend them rather than working around them.

`store::ShipDetails` (`store.rs:244`) has no `Serialize` derive. Add one. It is never persisted, so
there is no compatibility risk.

Every POST sits behind `web.allow_writeback` as well as the token, so the page can run read-only.

The verdict dialog in the app is two modals: the explainer, then the prompt (`verdict_dialog`,
`app.rs:6240`). Carry the same wording; it is what stops a user hiding real pilots by accident, which
UI-010 and the demotion work were both about.

## How to verify

- An `ipc` round-trip test for `AlertAck`, and the existing forward-compatibility tests still passing.
- Route tests for each detail endpoint against a scratch store.
- A test that a posted verdict lands in the inbox and that draining it persists through
  `apply_pilot_verdict` into the store.
- Demo screenshots of each dialog at phone width.
- A fix would be WRONG if it routed a badge tap through `IntelClick`, or if it took a second queue
  rather than the existing drain.
