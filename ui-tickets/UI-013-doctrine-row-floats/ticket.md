# UI-013 &mdash; Doctrine row floats in extra air

| | |
|---|---|
| **Severity** | Low |
| **Status** | Fixed, see `review.md` |
| **Region** | `render_ping` |
| **Wave** | 6 |

## Symptom

The Doctrine line sits in a 27px row containing 9px of ink. Ping card baseline gaps run 12, 11, 7, 15, 22px, so the rhythm visibly breaks at that line.

## Cause

`app.rs:21358` wraps the row in a `horizontal_wrapped`, which inflates it to `interact_size.y` even when no link chip is present.

## Notes

The Comms row is legitimately 27px because it hosts the Join Mumble button. Doctrine is not.

## How to verify

`ping_fleet.png` must show even vertical rhythm across the metadata rows.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/ping_fleet.png`, `before/ping_window_fleet.png`

After: `after/ping_fleet.png`, `after/ping_fleet_doctrine_link.png`, `after/ping_fleet_no_doctrine.png`, `after/ping_window_mixed.png`
