# UI-018 &mdash; Ping body lines allocate 26px for 15px of ink

| | |
|---|---|
| **Severity** | Low |
| **Status** | Fixed, see `review.md` |
| **Region** | `render_ping_body`, shared with the jabber body renderer |
| **Wave** | unscheduled |
| **Found by** | UI-013 |

## Symptom

Every line of a ping body sits in a 26px row containing 15px of ink, so a multi-line body is loosely
leaded against the metadata rows above it. Visible as the remaining 15px gap between the Doctrine
row and the description in `ping_fleet.png` after UI-013.

## Cause

`render_ping_body` wraps each line in its own `horizontal_wrapped`, which floors row height at
`spacing().interact_size.y` on the assumption the row holds something interactive. Body lines hold
text and the occasional link.

Same root cause as UI-013. The fix there was
`allocate_ui_with_layout(vec2(w, 0.0), Layout::left_to_right(..).with_main_wrap(true), ..)`.

## Why it was not fixed with UI-013

`render_ping_body` is called by the Fleet arm, the Plain arm, and a unit test. (WRONG: there is no
jabber caller, see `review.md`. Chat uses `render_message_body`, filed as UI-027.) Changing it reaches well outside UI-013's region and needs its own verification
across all three callers.

## Notes

Setting `ui.spacing_mut().interact_size.y` inside the closure does NOT work: egui reads
`interact_size` off the parent before creating the child ui. Measured during UI-013.

## How to verify

Measure body line heights in `ping_fleet.png` and `ping_plain.png`, and check the jabber body
renderer's callers still look right. Note the jabber view has no harness coverage yet (GAP-004), so
that third caller cannot be screenshot-verified today.
