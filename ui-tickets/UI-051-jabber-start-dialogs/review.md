# UI-051 review cycle

**Status:** Fixed and verified
**Branch:** `ui/ui-051-jabber-start-dialogs`, shared with WEB-027

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 690, unchanged |
| **Follow-ups** | none |

## What changed

The sidebar button is gone. `jabber_join_rooms` was already being set by whichever start row opened
the dialog, and it was the only caller that did not set it, so removing it makes the flag always
meaningful. The dialog now renders one half, titled and iconed for that half.

The recent lists are filtered by kind against the room JIDs in `channels`, which is the authority on
what a room is, rather than by guessing at the JID's domain. The ping feed is excluded too: it is a
chat key, not a person.

`jabber_recent_list` replaces the frameless buttons: full-width hit target, full-width hover fill,
the same treatment `jabber_convo_row` got when the rows in the list behind it had the same problem.
Capped at 100 inside a 220px scroll area, so the cap is a bound on the list rather than on what the
user can reach.

## Verified

`after/jabber_start_dm.png` and `after/jabber_join_room.png` are new scenes, one per half. The DM
dialog lists three people and no rooms; the room dialog lists five rooms and no people, which is the
defect and its absence in the same pair of images. `after/jabber_sidebar_convos.png` is the existing
scene, now without the button at the top.
