# UI-001 &mdash; Nav rail separator strikes through the Jabber row

| | |
|---|---|
| **Severity** | High |
| **Status** | Fixed, see `review.md` (one round rejected) |
| **Region** | `nav.rs` |
| **Wave** | 1 |

## Symptom

A 1px separator is drawn at y=487, dead centre of the Jabber row (y=465..503). It cuts the chat icon, the word "Jabber", and the orange unread dot, so the item reads as struck out and disabled. Jabber and Settings click rects also end up abutting at y=503 with no gap, against a uniform 9px gap everywhere else in the rail.

## Cause

`nav.rs:130` opens a `Layout::bottom_up` block containing `add_space(10) / Settings / add_space(8) / separator()`, anchored to the panel bottom. It needs roughly 62px below the last primary item but has only ~53px at a 560px rail height, so it overflows upward into the Jabber row.

## Notes

Height-dependent. Invisible at 800px, which is why only the 560px harness scene caught it. Do not fix by shrinking the rail scene.

## How to verify

`nav_rail_expanded.png` and `nav_rail_collapsed.png` must show no line crossing any nav item, and the Jabber/Settings gap must match the 9px used between other items.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/nav_rail_expanded.png`, `before/nav_rail_collapsed.png`, `before/nav_rail_expanded.debug.png`

After: `after/nav_rail_expanded.png`, `after/nav_rail_collapsed.png`, `after/nav_rail_expanded_short.png`, `after/nav_rail_collapsed_short.png`, `after/nav_rail_expanded_tall.png`
