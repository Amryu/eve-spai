# UI-028 &mdash; Rescue chat lines allocate 26px for 15px of ink

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `rescue_chat_line` (`fc-rescue` feature) |
| **Found by** | UI-027 |

## Symptom

Fourth instance of one root cause. `rescue_chat_line` (`app.rs:21425`) calls `render_message_body`
inside its own `ui.horizontal_wrapped`, which floors the row at `spacing().interact_size.y` (26.0)
while the text is 15px.

The first three were UI-013 (ping doctrine row), UI-018 (ping body) and UI-027 (chat body). Each was
fixed at its own container, because the floor comes from the parent, not the shared renderer.

## The established fix

```rust
let row = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
let size = egui::vec2(ui.available_size_before_wrap().x, 0.0);
ui.allocate_ui_with_layout(size, row, |ui| ..);
```

`render_message_body` already guards the empty-body case since UI-027, so that part is done.

## Why it is worth doing but not urgent

It is behind the `fc-rescue` feature, which published releases do not ship. It is also the delve911
capital-rescue window, where the chat tail is a supporting panel rather than the main surface.

## How to verify

**There is no harness scene for the rescue window.** GAP-009 records that both rescue windows are
uncovered, so this needs a scene before it can be verified visually, or it lands on measurement
alone. Note `cargo test --workspace` does not compile this code; use `--features fc-rescue`.

## Worth considering instead

Four instances of one bug suggests the shared renderer should own its row rather than every caller
remembering to. A `render_message_body` that opens its own zero-floored row, with callers passing a
plain container, would make a fifth instance impossible. That is a wider refactor than any single
ticket has needed, and it should be measured against the four call sites before being attempted.
