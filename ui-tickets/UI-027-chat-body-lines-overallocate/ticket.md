# UI-027 &mdash; Chat message bodies allocate 26px for 15px of ink

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `render_message_body`, `jabber_conversation_ui` |
| **Found by** | UI-018 |

## Symptom

Every chat message's body row is floored at `spacing().interact_size.y` (26.0) while its text is 15px
tall. On a long conversation that is 11px of dead air per message, which is both loose to read and
wasted vertical space in a popout.

## Cause

Third instance of the same root cause as UI-013 and UI-018. `render_message_body` (`app.rs:18687`)
is called from `jabber_conversation_ui` at `app.rs:4021` inside that function's own
`ui.horizontal_wrapped`, which floors row height on the assumption the row holds something
interactive.

UI-018's ticket wrongly claimed `render_ping_body` was shared with chat. It is not: chat has its own
renderer, and it was not fixed. Measured before and after UI-018, chat geometry is byte-identical.

## The established fix

```rust
let row = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
let size = egui::vec2(ui.available_size_before_wrap().x, 0.0);
ui.allocate_ui_with_layout(size, row, |ui| ..);
```

Two things already learned, do not rediscover them:

- Setting `ui.spacing_mut().interact_size.y = 0.0` inside the closure does **nothing**. egui reads
  `interact_size` off the parent before creating the child.
- Blank lines need an explicit `add_space`. UI-018 found that a zero-floored row gives an empty line
  literally 0.0px, so the author's paragraph breaks vanish.

## What makes this one different, and riskier than UI-018

`jabber_conversation_ui` was virtualized by UI-022, with a per-row height cache keyed by content and
width. **Changing row height changes every cached entry.** The cache is computed from the live
layout so it should stay consistent, but this must be verified rather than assumed:

- `uitest_jabber_long_history_builds_only_what_is_near_the_viewport`
- `uitest_jabber_long_history_keeps_its_tail_divider_and_grouping`
- `uitest_jabber_long_history_survives_a_scroll_round_trip`

All three must stay green, and the scroll round-trip is the one that would catch a spacer that no
longer matches its row.

Message grouping and the `— new —` divider also depend on row layout; both are covered by the tests
above.

## How to verify

`jabber_popout*` scenes cover this surface. Measure body row heights and message-to-message pitch
before and after, and expect the popout to fit more messages. Multi-row wrapped bodies are the case
to watch: UI-018 measured chat's wrapped body at 41.0 (26 + 15), so the second row is already
correct and only the first is floored.
