# UI-066 &mdash; Fleet detection stopped working, and an exit button for a mode that no longer exists

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `app.rs` |
| **Reported by** | user |

## Symptoms

"The 'Exit rescue mode button' is not needed anymore at the bottom. Also the fleet detection does not
seem to work anymore."

## Cause

UI-065 removed the enter/leave button, which was the only thing that ever set `RescueState::active`.
The fleet poller's first line is `if !rescue.lock().unwrap().active { continue }`, so it spun on a flag
that nothing set again and no fleet was ever read.

The exit button in the rescue view survived the same ticket because it lives in the view's own bottom
bar rather than in the settings panel where the other one was.

## How to verify

Start with the feature on and a fleet joined. The composition has to appear without anything being
clicked, which is the whole point of removing the mode.
