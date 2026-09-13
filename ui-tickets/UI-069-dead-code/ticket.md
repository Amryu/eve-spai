# UI-069 &mdash; Dead code left by this round of reworks

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `app.rs`, `jumproute.rs`, `jabber.rs`, `store.rs`, `web/*` |
| **Reported by** | user |

## Ask

"Remove the dead code, leave the other stuff alone."

## What was dead

Everything the compiler reported as never used or never read across both feature configurations, after
UI-068 gave back the three features that had lost their entry points.
