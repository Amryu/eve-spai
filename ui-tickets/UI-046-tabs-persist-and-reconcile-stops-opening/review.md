# UI-046 review cycle

**Status:** Fixed and verified
**Branch:** `ui-046-tabs-persist-and-reconcile-stops-opening`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | one session, no fix agent dispatched |
| **Patches rejected on review** | 0 |
| **App code changed** | +37 / -13, plus 150 lines of test module |
| **Harness code changed** | 0 |
| **Suite** | 585 to 594 with `fc-rescue`, 557 without |
| **Tabs opened by a fresh profile** | every joined room + every DM with history, to 0 |
| **Follow-ups** | none |

## What changed

`jabber_main_tabs` and `jabber_main_active` join `jabber_popout_windows` in settings. The main
window was the only chat window whose tab set was thrown away on exit; pop-outs have persisted
theirs through `ChatWindowCfg` all along. `sync_popout_settings` mirrors both on the same terms,
and `SpaiApp::build` restores them.

`jabber_reconcile` no longer walks `f.rooms` and `f.dm_keys` adding a tab for each. That loop is
why the tab bar was rebuilt from scratch every start: `f.rooms` is every joined room and
`f.dm_keys` is every JID with stored history, so the bar was "everything the app knows about,
minus a closed-list". The closed-lists only ever held conversations closed since the feature
existed, which made every older room and DM permanently un-closable.

What replaces it is narrow. New traffic still surfaces a conversation, because otherwise a DM
from someone with no tab would be invisible outside the sidebar: an unread DM opens a tab, an
unread room opens one only on a mention, and neither reopens anything on a closed-list. That is
the UI-039 rule, now the only way a tab appears without a click.

## A behaviour change the ticket did not ask for

Two UI-039 tests asserted that a server force-join opens a tab. They now assert the opposite:
the room returns to `jabber_rooms` and to the Channels list, and the tab bar is left alone. A
force-join is the server changing what you are in, not a request to change what you are looking
at, and "no tabs the user did not open" reads as the stronger rule here. Recorded because it is
a real change in behaviour that fell out of the fix rather than being specified by it.

## Rejected

Keeping the bulk add but relying on the closed-lists. That is the status quo, and UI-045 showed
what it costs: one stale setting emptied the room list and every tab came back forever. State
what is open, not what is not.

Persisting the tab bar without touching reconcile. The restore would have been overwritten on
frame one by the bulk add, so the two halves only work together.

## Teeth

`before/tests-fail-without-the-fix.txt` is the suite with the bulk add restored and the
persistence removed. Four of nine fail: `a_joined_room_does_not_open_a_tab_by_itself`,
`only_the_previously_open_tabs_come_back`, `room_traffic_surfaces_a_tab_only_on_a_mention`, and
`the_tab_bar_is_mirrored_into_settings`.

The four that pass in both states are preservation tests, except one weakness the teeth run
exposed: `a_restored_tab_is_not_pruned_on_the_first_frame` and
`a_restored_tab_survives_the_room_being_gone` passed because the test helper hand-replayed the
restore, so `SpaiApp::build`'s own restore was never covered. The restore is now
`restored_main_tabs(&Settings)`, called by both `build` and the helper, with
`the_restore_drops_an_active_tab_that_is_not_in_the_bar` covering it directly. Testing it
through the real `build` would have meant pointing a test at a profile on disk via an
environment variable every parallel test in this binary reads, which UI-043 already established
is not worth it.

That function also fixed a bug nobody had reported: a saved active tab that is no longer in the
saved bar used to select a tab that does not exist. It now falls back to the Fleet pings feed.

## Screenshots

None. The change is which tabs exist, and a scene renders whatever tab set it is handed, so a
screenshot would show a decision the fixture made rather than one the code made. The tests are
the signal.

## Residual risk

`jabber_main_tabs` is written whenever the bar changes, which is a settings write per tab open,
close, drag and pop-out. `persist` already backs off on failure and the pop-out list has the
same shape, so this adds no new write pattern, but it does make the tab bar a persisted
document that can now be corrupted by a bad write where before it was ephemeral.

A user upgrading into this lands on an empty tab bar once, because they have no saved set yet.
That reads as "it forgot my tabs" exactly one time.
