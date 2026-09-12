# WEB-008 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-008-dialogs`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 645 to 649 |
| **Follow-ups** | none |

## What changed

**Dialogs render in the browser.** A badge tap is a plain `GET` to `/api/system/{id}`,
`/api/ship/{id}`, or a lookup in the snapshot for a pilot. Deliberately not `IntelClick`: that enum
exists to open a desktop viewport, and a tap on a phone must not raise a window on a machine in
another room.

**Write-back reuses the overlay's path entirely.** `POST /api/action` deserialises an
`ipc::OverlayToMain` into a queue drained in the same block the overlay's inbox is drained in. The
match itself is now one method, `apply_overlay_message`, called by both, so there is a single answer
to what a verdict does rather than two that can drift.

One new variant, `AlertAck { id }`. Safe to add because this enum only flows child to parent and the
parent always spawns the child from its own `current_exe`. `ack_alert` clears the report from
`alert_feed`, every `rule_feeds` entry **and** `alert_shared`: clearing one would leave the same alert
acknowledged in the Alerts tab and still outstanding in the overlay window.

**Deep links** (`#system/30004759`, `#ship/587`, `#pilot/Name`) were added for two reasons. A dialog
is a thing you want to send someone, and it is the only way a load-time screenshot can capture one,
since the harness cannot click.

## Access

Every `POST` is checked twice: `allow_writeback` must be on, and `Origin` must pass the same IP
literal test `Host` does. The cookie is already `SameSite=Strict`, so a cross-site post should not
carry it at all; the origin check is a second lock on the same door and costs nothing.
`a_post_from_somewhere_else_is_refused` covers `evil.com`, the `127.0.0.1.evil.com` bypass, and a
missing header, and asserts the inbox stayed empty.

## Verified

`after/dlg-system.png`: 1DQ1-A at -0.4 in the security colour, Delve, O-Elmg, "you are here", and its
gate to 319-3D as a chip that opens that system's dialog in turn.

`after/dlg-ship.png`: **"Not in the static data."** The fixture demo has a graph but no SDE store, so
the ship dialog has nothing to show and says so instead of rendering an empty shell. That is the
honest state of this screenshot: the ship dialog's happy path is covered by
`the_dialog_endpoints_answer_or_404` and by running the real app, not by the demo. Seeding a fake SDE
into the demo would make the picture prettier and the evidence worse.
