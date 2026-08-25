# UI-006 &mdash; Settings truncates directory paths beside 900px of free space

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | `settings_view` |
| **Wave** | 3 |

## Symptom

The EVE chat-log and settings directory fields are each ~280px wide inside a panel that runs to x=1250, so they clip to `/home/smense/.steam/steam/steamapps/co...`. The user cannot read which directory is configured, and there is no Browse button beside either field.

## Cause

Both fields are fixed-width single-line `TextEdit`s in `settings_view`. Real EVE paths are longer than the fixtures shown here.

## Notes

`rfd` is already a dependency and is used elsewhere for file dialogs, so a Browse button is cheap if wanted.

## How to verify

`view_settings.png` must show both full paths, or make them readable some other way. Nothing may escape the panel horizontally.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/view_settings.png`

After: `after/view_settings.png`, `after/view_settings_narrow.png`
