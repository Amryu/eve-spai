# UI-074 — the jabber room topic is unreachable

> "Ouch 😛 Whatever changed made it so it's now impossible to see jabber channel TOPIC?"
> — reported in chat

> "The jabber channel topic should show in the title bar when the room tab is open (as one line).
> Add a button to that bar to show the entire MOTD in a dialog."
> "Also, right clicking a room on the convos side bar should also have an option 'Show MOTD'. A
> tooltip will also show up to 6 lines of the MOTD."

## What happened

The MOTD was never lost: `Event::RoomSubject` still stores it in `JabberState::room_subjects`, and
it still survives a restart in `Settings::jabber_room_subjects`. What went was every way of reading
it. The only surviving display was a hover tooltip on the sidebar row, and `jabber_motd_expanded` —
the state behind the expander that used to show it — was written and cleared but **never read**,
orphaned by an earlier cleanup the same way `kill_wormhole` and `toggle_dock_permit` were in UI-068.

## What it does now

- The room's title bar carries the topic on one line, next to the room name, in both clients.
- A button on that bar opens the whole MOTD in a dialog: selectable, with its own line breaks kept,
  because that is where the ping format, the comms details and the forum link live.
- Right-clicking a room in the Convos list offers **Show MOTD**, opening the same dialog.
- The row tooltip is capped at six lines with a `…` marker. It was showing the whole notice board,
  which is taller than the sidebar it hangs off.

One line means one line: blank lines and a rule of dashes are dropped from the collapsed form, since
a separator separates nothing on one line and the rule alone took a third of the bar. The dialog
shows the MOTD exactly as written.

`WebConvo` gains `motd` so the page has the same text; `?jabber=<jid>` opens a conversation, which
is how the harness can screenshot a chat header at all.
