# UI-031 &mdash; Regenerate button uses a bare U+21BB, not a phosphor icon

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `rescue_window_body` (`fc-rescue`) |
| **Found by** | UI-019 |

## Symptom, unconfirmed

`app.rs:5422` renders the ping regenerate button as a bare `"↻"` (U+21BB) rather than an
`egui_phosphor` glyph. It therefore depends on egui's default emoji fallback font rather than the
icon font the rest of the app uses.

The project rule is explicit: confirm a glyph exists or it renders as a tofu square. This one has
never been confirmed, because the rescue window has **no harness scene** (GAP-009) and it is behind
`fc-rescue`.

## What to do

Check whether U+21BB is actually in egui's bundled fallback. If it is not, or if it renders
inconsistently across platforms, swap it for a phosphor icon: `ARROW_CLOCKWISE` or
`ARROWS_CLOCKWISE`, grepped first.

Either way this is cheap, and the outcome should be recorded so nobody has to wonder again.

## How to verify

Needs a rescue window scene, which GAP-009 tracks. Failing that, render the glyph in an ad-hoc scene
through `harness::prepare` (the same font setup the app uses) and check it is not a tofu box, which
is how UI-028 measured a surface with no scene.
