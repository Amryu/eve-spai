# UI-019 &mdash; Audit the remaining `small_button` call sites

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | various |
| **Wave** | unscheduled |
| **Found by** | UI-014 |

## What this is

UI-014 fixed two `small_button` calls in `render_ping` that produced a 17px hit target against the
app's 27-28px norm. Sixteen other call sites remain:

`app/src/app.rs` lines 5197, 5321, 8046, 8058, 8375, 8380, 11886, 11895, 11906, 15331, 15348,
15529, 15537, 21807
`app/src/copysettings.rs` lines 366, 411, 416

## This is an audit, not an assertion

Most of these are icon-only buttons in dense toolbars, where a smaller control is a defensible
choice rather than a defect. The Copy button was different: it carried a text label, sat beside a
full-size button in the same row, and was the smallest hit target in its scenes.

The question per site is whether the control reads as tappable and matches its neighbours, not
whether it uses `small_button`.

## How to verify

`cargo test --bin eve-spai uitest_census -- --ignored --nocapture` prints the smallest hit target
per scene. Any site that is genuinely too small will show up there once its view has a scene, which
several currently do not (see GAP-002).
