# GAP-010 &mdash; The checker never compares a hit target against text

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Open |
| **Found by** | GAP-001 |
| **Effort** | Small, but sequence it after UI-033 |

## The hole

`checks.rs::inspect` runs two overlap passes:

- click targets against click targets
- `Label` against `Label`

**Nothing compares a click target against text.** A button drawn over a paragraph is invisible to the
suite, which is exactly the UI-033 defect: the always-on-top pin sits on dialog header text, and on a
hyperlink, with `uitest_layout` green.

A second, related hole: an egui hyperlink reports `Role::Label`, not `Role::Link`, because the
selectable-label pass overwrites the role. UI-018 hit this too. So a widget covering a link is not
caught by either pass.

## The fix

A third pass comparing each interactive rect against each painted `Label` rect, with the same
ancestry exclusion the other passes use, since a button legitimately contains its own label.

Expect false positives to need tuning: a chip's own text, a checkbox's caption, and any widget that
deliberately paints text inside itself will all overlap by construction. The ancestry check should
handle most; measure before adding more special cases.

## Sequence this after UI-033

Adding the pass now would immediately fail the eight GAP-001 dialog scenes on UI-033's bug. Land
UI-033 first, then add the pass, then confirm it stays green.

## How to verify

Self-tests in the style of the existing ones: a deliberate button-over-paragraph must fail, and the
existing scenes must stay green. The three dialogs in UI-033 are the natural real-world fixture.
