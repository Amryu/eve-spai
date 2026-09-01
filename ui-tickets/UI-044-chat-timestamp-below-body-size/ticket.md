# UI-044 Chat timestamps render at 9.5px

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `app.rs:4170` (Jabber history), `app.rs:22260` (Rescue chat) |
| **Reported by** | spun off UI-043 |

## Symptom

The timestamp above each chat message group is drawn at 9.5px, well under the app's body
size, in both the Jabber and Rescue windows.

```rust
ui.label(egui::RichText::new(eve_time_label(m.time, now)).weak().size(9.5));
```

UI-008 removed sixteen `.small()` sites on content text for exactly this reason. These two
were not in that sweep, and a timestamp on an intel report is content: it is the thing you
read the log back for.

UI-043 made the string three characters longer by adding seconds, which is what prompted a
second look rather than a change in the rendering.

## Notes

- Both call sites must move together, they are the same control in two windows.
- `.weak()` is doing the real work of keeping the stamp quiet. Dropping `.size(9.5)` and
  keeping `.weak()` is the obvious first attempt.
- Row height feeds the Jabber history's virtualization height cache, so this is not purely
  cosmetic: check the feed still scrolls correctly after the change.

## How to verify

Re-render `jabber_popout_stamps` and `rescue_chat_stamps` and compare against UI-043's
`after/`. The fix is WRONG if the timestamp starts competing with the sender name, or if
the history's scroll position jumps when messages re-flow at the new row height.
