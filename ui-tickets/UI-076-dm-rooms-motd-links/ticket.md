# UI-076 — rooms listed as direct messages, and MOTD links that are not links

> "'Direct messages' in jabber shows uninteractable rooms instead (the duplicates on the rooms work
> just fine)"
> "MOTD: make sure links in the MOTD are clickable."

## Two bugs, one symptom

The frame's `convos` is built from every conversation that has history, and a room has history, so
rooms are in it. The Direct messages filter read:

```rust
sticky.contains(jid) || (!closed.contains(jid) && (dm_keys.contains(jid) || contacts.contains(jid)))
```

`jabber_sticky` was filled from *any* unread conversation, rooms included, and it sat on the left of
the `||`, so it bypassed the DM test entirely. Every room that ever went unread was listed under
Direct messages, permanently, alongside its own entry under Rooms.

The duplicate was dead for a second reason. `jabber_convo_row` interacts with `ui.id().with(jid)`,
so two rows for one jid ask egui to hit-test one id twice and only one of them wins — which is why
the copy under Rooms worked and the copy under Direct messages did not.

Both are fixed: being a DM is now the gate and stickiness only overrides having been *closed*, which
is all it was ever for; rooms are no longer stuck at all; and each list gets its own `push_id` scope
so a jid appearing twice can never silently kill a row again.

## MOTD links

The dialog escaped the MOTD flat. A MOTD is where the doctrine link, the forum thread and the comms
details live, and those are the part people want out of it. Both clients now linkify it: the app
renders line by line through the same `render_linked_text` the chat uses (which knows nothing about
newlines, hence line by line, and a MOTD's own line breaks are half of what makes it readable), and
the page factors `linkify` out of `bodyHtml` so chat lines and the MOTD share one implementation of
where a URL ends.
