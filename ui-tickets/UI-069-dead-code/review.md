# UI-069 review cycle

**Status:** Done
**Branch:** `chore/ui-069-dead-code`

Zero warnings in both `cargo check --bin eve-spai` and `--features fc-rescue`, down from eleven.

## Deleted

`jabber_channels_list_ui` and `motd_preview` with its three tests, orphaned by the Convos rework.
`char_missing_scope`, whose warning is computed inline in `characters_view` anyway.
`recompute_jump_route` and the three fields it was the only writer of, `jump_route`, `jump_legs` and
`jump_alt`, along with the map drawing that read them: nothing filled them, so it drew nothing.
`jumproute::plan`, `flatten` and the `Leg` they returned, which went with it. `warn_line`, replaced by
`warn_button` when the warning became clickable. `TravelEnd`, whose two variants had come down to one
reachable case, folded into `travel_set_start`.

## Kept, with a reason in the code

Three things the compiler calls dead that are not leftovers:

`route_cost` and `RouteCost` are read only by the tests that pin the fatigue and fuel rules against
the published figures. Deleting them deletes the check, not the weight.

`Cmd::DiscoRooms` has no sender since the Channels pane went, but behind it is a working disco#items
browse with per-room access probes. I started removing it, got as far as the command and the handler,
and put it back: half a subsystem is worse than either end, and this one costs more to write again
than to leave.

`Store::stashed_settings` has no reader yet, and it is the recovery half of a safety net whose writing
half is live. `settings.bad` is the only copy of a config that failed to parse; deleting the reader
makes the stash write-only, which is the state it exists to end.

## Also

Two capabilities are gone for real, both with the UI the user asked to remove, and both worth knowing
rather than discovering later: the server room browse has no button, and Travel's "set as destination"
from the map is gone, though the panel's own field still sets it.
