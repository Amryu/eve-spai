# UI-068 review cycle

**Status:** Fixed
**Branch:** `fix/ui-068-small-targets`

## Restored

**Marking a hole dead** is a button on each row of the Wormholes view, which is where the holes are
listed. In the first column, not the last: that grid is eight columns and scrolls sideways, so the far
end is off screen exactly when the window is small enough to need it. The harness caught that on the
first attempt, in the narrow scene, before it shipped.

**Dock permits** are on the route row menu, beside the ring they draw. A route's own rows are where
you decide where you can sit down.

## Removed

**JumpPlan** comes off the mode picker. The route panel replaced it and nothing fills the overlay it
drew, so selecting it gave an empty mode. Taking it off the list is the honest fix; leaving the
variant costs nothing and avoids a cascade through the saved-route types.

## Also in this ticket

Two older open tickets, cleared while the file was open:

- **UI-042**: the contacts star was `.small()`, about 9px, the hardest thing in the list to hit and
  the easiest to hit by accident, and it adds or drops a contact. Now 15px.
- **UI-031**: the rescue template's regenerate button was a bare `↻`, which is a tofu square on a
  machine without that glyph. It is the Phosphor icon the rest of the app uses.

**UI-044 stays open.** Raising the chat timestamp from 9.5px made the message row taller, and at the
minimum pop-out size the room title then overlapped the first message:

```
jabber_popout_min:
  [overlapping text] Label "delve.imperium" at [[16.0 48.5] - [125.1 63.5]]
                 <-> Label "primary is the Loki..." at [[20.0 60.0] - [290.0 90.0]]
```

That is a pre-existing tightness in the pop-out header, not something the timestamp caused, and fixing
the legibility means fixing the header first. Reverted rather than shipped, and the evidence is here
for whoever takes it.
