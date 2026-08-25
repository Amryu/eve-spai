# UI-006 review cycle

**Status:** Fixed and verified
**Wave:** 3 (paired with UI-005 on `battles_view`, no region overlap)
**Worktree:** `wt/ui-006`
**Branch:** `fix/ui-006-settings-dir-picker`

## The change

New free helper `dir_picker_row(ui, hint, value) -> bool` beside `color_row`, used by both
directory fields in `settings_view`.

```rust
ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
    if ui.button(format!("{}  Browse…", egui_phosphor::regular::FOLDER_OPEN)).clicked() { ... }
    let width = ui.available_width();
    changed |= ui.add(egui::TextEdit::singleline(value).desired_width(width).hint_text(hint)).changed();
});
```

Right-to-left so the button reserves its width first and the field claims whatever is left. That is
width-driven rather than a hardcoded number, so it holds at 1280 and at the 720px minimum without a
magic constant, which is why the comment records the ordering rather than the arithmetic.

Browse uses `rfd::FileDialog::new().pick_folder()`, matching the existing `pick_file()` callers at
`app.rs:7042`, `7894` and `21636`. It seeds `set_directory` from the current value, or from the
auto-detect hint when the field is empty, guarded by `is_dir()` so a stale or bogus path does not
confuse the dialog. `FOLDER_OPEN` was confirmed present in egui-phosphor 0.12 before use, per the
project rule.

Both fields get identical treatment, and the settings-dir side effects (`eve_settings_path` slot,
`copy_settings.invalidate()`) are preserved at the call site.

## Review

The ticket offered three options and the right answer was two of them, not one. Widening alone
fixes 1280px but leaves hand-typing as the only way to set the value. A Browse button alone leaves
the configured value unreadable, which is the actual complaint. Widening makes the value readable;
the button makes the setting usable. Taking either on its own would have been half a fix.

Middle-elision was rejected on a real constraint rather than taste: egui's `TextEdit` cannot
middle-elide, so it would mean swapping in a painted label when unfocused and losing in-place
editing. A multiline field was rejected because a path is one line and `multiline` eats Enter.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 410 passed, 2 ignored (+32) | 410 passed, 2 ignored (+32) |
| Screenshot scenes | 52 | 53 |
| Layout assertions | clean | clean, including the new 720px scene |

`cargo check --workspace --all-targets --all-features`: only the pre-existing `unused_mut` at
`app/src/intel.rs:5605`.

## Screenshots

- `before/view_settings.png`: both fields are ~280px wide in a panel running to x=1250, clipping to
  `/home/smense/.steam/steam/steamapps/co…`. Roughly 900px sits unused to their right, and there is
  no way to set the value except typing it.
- `after/view_settings.png` (1280px): both fields span x=72 to x=1160 with Browse at x=1168 to
  x=1265, inside the panel. Both paths read end to end, including
  `…/compatdata/8500/pfx/drive_c/users/steamuser/Documents/EVE/logs`. The Alerts section below is
  unchanged.
- `after/view_settings_narrow.png` (720px, new): the field runs to x=604 with Browse at x=612 to
  x=702, both inside the panel, no overflow and no wrap.

## Harness change

`view_settings_narrow` at 720x800 is kept permanently, matching the precedent set by
`intel_row_torture_narrow` and the `nav_rail_*_short` scenes. It is the only guard that a future
width change does not push the Browse button off the panel at the minimum window size, and because
`uitest_layout` covers it, a regression fails the suite rather than only looking wrong in a PNG.

## Residual limitation

At 720px the paths still clip at the tail, to `…/users/steamus…`. That is unavoidable with 640px of
panel, and it is not what the ticket was about: the value is readable at any normal window size, and
the picker means nobody has to type it. Recorded here so a future report of "still clipped" is
recognised as this known floor rather than a regression.
