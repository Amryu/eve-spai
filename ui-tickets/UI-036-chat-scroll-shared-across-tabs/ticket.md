# UI-036 &mdash; Switching chat tabs drops you at the top of the next conversation's history

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `jabber_conversation_ui` (the history `ScrollArea`) |
| **Reported by** | user |

## Symptom

Leaving a private chat for another, longer conversation opens that conversation scrolled to its
oldest message instead of its newest. The newest messages are a full history away, off the bottom,
and the view does not snap back to them when new messages arrive. Scrolling to the bottom by hand
is the only way out, and the next tab switch does it again.

Going the other way, into a conversation short enough to fit the window, hides the fault: there is
nothing to be scrolled away from.

## Measured

Pop-out at 520x480, tabs `delve.imperium` (1000 messages, the cap `jabber.rs` drains to),
`corp.chat`, `wingmate` (2 messages). Opened on the DM, room tab clicked.

| | Top message on arrival | Bottom message | Offset |
|---|---|---|---|
| Before | `gate is clear #0` | `warp to me #11` | 0.0px |
| Expected | `... #990` | `warp to me #999` | end of content |

989 messages out of 1000, timestamped 13:39 against the newest at 15:20 in the same fixture.

## Cause

`app/src/app.rs:3963`. The history `ScrollArea` was salted `"msgs"`, a constant. The enclosing
`push_id(("jwin", win))` in `jabber_window_body` scopes that id per window, not per conversation,
so every tab in a window shared one `scroll_area::State`: one offset and one `scroll_stuck_to_end`
flag.

Two egui behaviours turn that sharing into the observed jump, both in
`egui-0.34.3/src/containers/scroll_area.rs`:

- Sticking is only re-entered from a body that fits (`available_offset < 0`) while `stick_to_end`
  is asked for that frame (line 1510). `jabber_conversation_ui` drops `stick_to_bottom` whenever
  the pointer is down (`selecting`, added so an incoming message does not wipe a drag-selection),
  and the press that switches tabs is exactly such a frame. So the short DM leaves the shared state
  with `scroll_stuck_to_end = false` and `offset = 0`.
- On the release frame the room is drawn against that state. `stick_to_end` is asked for again but
  line 1245 also requires `scroll_stuck_to_end`, which is now false, so nothing pulls the offset to
  the end. The offset stays 0, and the flag is then recomputed as `0 == available_offset`, false
  again, which is why it never recovers.

## Notes

- Not the virtualization. `show_viewport` plus `jabber_msg_heights` (UI-022) measures the content
  correctly here; the offset is wrong before any culling decision is made.
- Pop-outs already differ by window through `push_id(("jwin", win))`, so this is per window, not
  global. It reproduces in the main window's tab strip the same way.
- Adjacent: UI-022 owns the same loop, UI-027 the message rows inside it. Neither touches the id.
- A latent second defect sits behind the same `selecting` gate and is left open here: any pointer
  press while a conversation shorter than the window is showing clears that conversation's sticky
  flag, so once it grows past a screenful it stops following new messages. Per-conversation state
  makes it persistent rather than reset by the next tab switch.

## How to verify

`cargo test --bin eve-spai uitest_jabber_tab_switch_opens_at_the_newest_message`, and
`cargo test --bin eve-spai uitest_screenshots_tab_switch -- --ignored` against `before/`.

The fix is wrong if it reaches the newest message by forcing every tab to the bottom on arrival:
a conversation the user deliberately scrolled back in has to still be scrolled back when they
return to it. It is also wrong if it drops the `selecting` guard, which exists so a drag-selection
survives an incoming message.
