# UI-032 &mdash; The intel toolbar is too cramped

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | the intel toolbar in `intel_view` |
| **Reported by** | user |

## Symptom

The intel toolbar is cramped. The user asked for it to be more compact, suggesting a dropdown in
place of the filter buttons and shorter labels.

## Measured, at 1280px

The toolbar runs x=72 to about x=1164, roughly **1092px of controls**, leaving about 100px for the
search field. Widths from the AccessKit tree:

| control | width |
|---|---|
| `within the intel feed's range` (zKill range) | **201.2** |
| `count jump bridges` | **129.8** |
| `outdated after` label | 85.4 |
| `Severity…` | 79.3 |
| `zKill intel` | 70.3 |
| `Hostile` | 60.7 |
| `Threat` | 58.2 |
| `Clear` | 49.8 |
| `300s` | 47.6 |
| `≤ jumps` label | 45.1 |
| `any` | 40.2 |
| `Kill` | 37.5 |
| `All` | 35.0 |

The five type buttons plus their gaps come to roughly 260px.

## Two of the worst offenders are recent, and mine

- **UI-004** replaced a terse `feed` / `5j` DragValue with `within the intel feed's range`, 201px,
  to fix genuinely broken English ("zKill intel within feed").
- **UI-025** added the `count jump bridges` checkbox, 130px, because the card silently ignored the
  bridge setting.

Together that is **331px, about 30% of the toolbar**, added in the last two days. Both fixed real
problems, and both traded width for clarity without anyone checking the width budget. That is the
actual cause of the report.

**Do not simply revert either.** The grammar fix and the bridge control both need to survive; they
need to survive *compactly*.

## Options, with the width each would recover

| change | saves | cost |
|---|---|---|
| zKill range: drop the prose, keep the meaning in a tooltip | ~130px | the zero case has to still read as something, not "feed" |
| `count jump bridges`: fold into the jumps control as an icon toggle | ~100px | discoverability, which was the whole point of UI-025 |
| Five type buttons to a dropdown | ~160px | **loses one-click switching on the most-used control in the view** |
| `outdated after` to `stale` | ~50px | slightly less obvious |
| `Severity…` to an icon button | ~46px | needs a tooltip to stay discoverable |

The user suggested the dropdown, so it is sanctioned, but weigh it: a filter you flip constantly is
a poor candidate for hiding behind a click. A segmented control, or icon-only chips with tooltips,
may get most of the width back while keeping one click.

## Constraints

- **The app's minimum window width is 720px** (`main.rs`). Whatever the toolbar becomes, it must
  wrap sanely there, and `toolbar_combo` exists because a `ComboBox` in a wrapping row does not
  reserve its true width (UI-017).
- `view_intel` and `view_intel_feed` are covered scenes, so this is measurable and screenshot-verifiable.
- Do not reintroduce the UI-004 grammar bug, and do not hide the bridge setting so far that UI-025's
  point is lost.

## How to verify

Re-measure the control widths and report the new total. **Add a permanent width-budget assertion**,
so the toolbar cannot silently creep back: something that fails when the toolbar's content exceeds a
share of a 1280px window would have caught both UI-004 and UI-025 at the time.

Also render at 720px and confirm the wrap is sane.
