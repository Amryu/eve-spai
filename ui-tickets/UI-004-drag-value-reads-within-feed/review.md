# UI-004 review cycle

**Status:** Fixed and verified
**Wave:** 2 (paired with UI-001 on `nav.rs`, no region overlap)
**Worktree:** `wt/ui-004`

## The change

Both sites now call one new `SpaiApp::kill_intel_range`, placed above `alerts_view`. The alerts
toolbar loses its standalone `ui.label("within")`; the intel toolbar loses its inline duplicate.

```rust
fn kill_intel_range(ui: &mut egui::Ui, jumps: &mut u32) -> egui::Response {
    ui.add(
        egui::DragValue::new(jumps)
            .range(0..=20)
            .prefix(format!("{}  ", egui_phosphor::regular::CARET_UP_DOWN))
            .custom_formatter(|n, _| match n as u32 {
                0 => "within the intel feed's range".to_owned(),
                1 => "within 1 jump".to_owned(),
                n => format!("within {n} jumps"),
            })
            // The formatter writes words, which the default numeric parser cannot read back.
            .custom_parser(|s| {
                s.chars().filter(char::is_ascii_digit).collect::<String>().parse().ok()
            }),
    )
    .on_hover_text("How far from you a kill counts as intel. The lowest setting follows the intel feed's own jumps filter.")
}
```

## Review

The two rows were not the same shape, which the ticket did not say. Alerts had a preceding `within`
label; the intel toolbar had no label at all, so it read `[x] zKill intel [feed]`. Adding a label to
the intel row would have fixed the grammar and left the duplication. Moving the preposition into the
control fixes both rows and makes them structurally identical: checkbox, then one control carrying
the whole phrase. That is the better call and it removes a copy-pasted DragValue.

The zero copy is accurate to behaviour, not invented. `app.rs:17144` reads `(0, feed) => feed`, so
zero falls back to the intel view's own jumps filter. "within the intel feed's range" says that.
Rejected alternatives "any" and "unlimited" would have been false, and "feed range" is jargon.

Affordance: egui 0.34's `DragValue` has no built-in stepper arrows, so a `CARET_UP_DOWN` prefix
stands in. The icon was grepped and confirmed present in `egui-phosphor` before use, per the project
rule, and it renders as a real glyph in the screenshot rather than tofu.

The `custom_parser` is required, not decoration: once the formatter emits words, the default numeric
parser cannot read its own output back when the user clicks into edit mode. Keeping ASCII digits
makes "5", "5 jumps" and a lightly edited string all round-trip. A string with no digits fails to
parse and egui keeps the previous value, which is the right failure.

## Non-zero verification

The harness renders defaults, so the ticket asked how the non-zero case was checked. The agent
rendered it rather than reasoning about it: temporary scenes at 0, 1 and 5 for both toolbars, then
reverted them. `1` reads "within 1 jump", `5` reads "within 5 jumps". `uitest_layout` was green with
those scenes present, so the wider control does not escape or overlap in the crowded intel row.

Rendering the intel toolbar required `chat_dir` to be set, otherwise `intel_view` early-returns on
its "EVE chat logs not found" branch and the toolbar never draws, plus `settings` and `chat_dir`
opened to `pub(crate)`. Those changes were reverted; `git status` in the worktree showed
`M app/src/app.rs` alone, which I confirmed before applying.

## Coverage gap this exposes

The intel toolbar site is now **unverified by any permanent scene**. `view_intel.png` is still the
"EVE chat logs not found" placeholder, so the second of the two call sites is only covered by
throwaway scenes that no longer exist. This is exactly GAP-002: until the scratch store is seeded
and `chat_dir` can be set, half of this fix is confirmed by an agent's transient render and not by
the suite. Noted on GAP-002 as a concrete consumer.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 410 passed, 2 ignored (+32) | 410 passed, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 11 passed | 11 passed |
| Layout assertions | clean | clean |

`cargo check --workspace --all-targets --all-features`: only the pre-existing `unused_mut` at
`app/src/intel.rs:5605`.

## Screenshots

- `before/view_alerts.png`: the row reads `[x] zKill intel  within  [feed]`. "feed" sits in a plain
  bordered box that looks exactly like the "Alert rules (1 on)" button below it, and the sentence
  dead-ends on a noun that does not follow "within".
- `after/view_alerts.png`: the row reads `[x] zKill intel  [caret within the intel feed's range]`,
  one control with a small up/down caret at its left edge. `after/view_alerts.debug.png` labels that
  widget `click+drag` against the neighbouring checkbox's `click`, confirming the drag target.

## Rejected

- Keeping the static `within` label and swapping only the zero word: grammatical, but leaves the
  sentence split between a label and a control, and leaves the two rows inconsistent.
- Zero as "any" or "unlimited": both false, zero clamps to the feed filter.
- Adding the caret prefix to the neighbouring `<= jumps` and `outdated after` DragValues for
  row-internal consistency: they display digits and already read as values. Worth its own ticket if
  the row should be uniform.
- A combo or segmented picker for the "feed range vs N jumps" choice: over-engineering for a 0..=20
  setting.
