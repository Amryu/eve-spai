# UI-065 review cycle

**Status:** Delivered
**Branch:** `feat/ui-065-rescue-always-on`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 705 plain, 744 with the feature |
| **Follow-ups** | none |

## One decision, one switch

The feature being on is the mode. The enter/leave button is gone, and so is the branch that made the
map wear a preset while it was on: `rescue_preset` is deleted, not merely unused, because a dead
preset is one somebody re-wires later.

## A tab, not a window

The rescue view is a row in the rail under Jabber and a view in the central panel, and the always-on-
top viewport is deleted. A window that had to be opened and closed was a second place to look and a
second thing to lose behind the game client.

The row exists only where the feature does: `rail` takes the rows to show rather than reading
`View::primary()` itself, because on every other install rescue is not a row that is disabled, it is a
row that does not exist.

## What is left of `active`

It still gates the pollers and the ping selection, which is genuinely per-session state rather than a
mode. Nothing in the UI switches it any more.
