# UI-008 review cycle

**Status:** Fixed and verified
**Wave:** 8, run alone because it touches 16 sites across the file
**Branch:** `fix/ui-008-small-fonts`

## Verdict per site: 13 changed, 3 kept

Changed, all content the user reads to decide something: the jabber sound-preset reference list,
the `— new —` unread divider, the wormholes empty-state subtitle, the 8 wormhole column headers, the
`⚠ drifter` hazard tag, the store error string and its remediation paragraph, and six wizard lines
(step counter, three error messages, the resolved-path confirmation).

Kept, all icon-only frameless buttons carrying no text: the contact star (`app.rs:3259`), the jabber
tab close (`3642`), the lookup tab close (`6115`). Small is a defensible affordance there and the
census confirms their scenes' smallest hit target is unchanged at 18px.

That split is the reason this was framed as a judgement ticket rather than a find-and-replace, and
the agent justified each keep individually.

## My ticket was wrong about one site

It named "the alert window countdown" as an offender. It is not: that label carries `.weak()` only
and already renders at body size. I verified this in source after the agent flagged it. The
`before/alert_window_typical.png` attached to the ticket is also stale against current main, since
chip wrapping has changed under it.

## Pre-existing bug found, and fixed at the cause

The new 720px wormhole scene failed `uitest_layout` with `content is 809px wide in a 720px window`.

**I checked whether this ticket caused it.** Reverting both the header `.small()` removal *and* the
scroll change, back to the exact pre-ticket state, still fails at **809px in a 720px window**. So the
8-column grid was already 89px wider than the app's own minimum window width, and `.small()` was
only masking part of it. This change adds 24px to a pre-existing overflow rather than creating one.

Fixed at the cause: `ScrollArea::vertical()` becomes `ScrollArea::both()` on the grid, so the
columns are reachable by horizontal scroll instead of clipped off the edge. That is scope expansion,
and justified: leaving it would have meant shipping a knowingly clipped table to keep a diff small.

## Second near-miss, caught before it shipped

The sound-preset hint is ~93 characters in a grid cell inside a 540px dialog viewport, and `.small()`
was the only thing keeping it on one line. Removing it alone would have pushed the row past the
dialog edge, since a grid cell imposes no wrap width. Changed to `Label::new(..).wrap()`.

No scene covers that dialog, because it is a separate viewport (GAP-001). So this one is reasoned
rather than screenshotted, and the review records that distinction rather than implying it was
verified visually.

## Tests

- `uitest_wormhole_table_text_is_body_size`: headers and the drifter tag are no shorter than a body
  cell. Verified to fail with `.small()` restored.
- `uitest_ping_footer_is_body_size`: verified to fail with `.small()` restored, reporting
  `10.0px against a 15.0px metadata row`.

Both measure AccessKit label box height, which is the only font-size signal the tree carries.

New scenes `view_wormholes_rows` and `view_wormholes_rows_narrow`, both feeding `uitest_layout`.
Before this, `view_wormholes` rendered the empty state only under headless, so **the wormhole column
headers had never appeared in a single screenshot.** Seeding them needed `SpaiApp::systems` and
`wh_cache` opened to `pub(crate)`, matching `view` and `battle_detail_cache`.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 433 passed, 2 ignored (+32) | **435 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 24 passed | 26 passed |
| `.small()` in `app.rs` | 16 | 3 |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Screenshots

- `after/view_wormholes.png`: the empty-state subtitle goes from ~10px to body size.
- `after/view_wormholes_rows.png` (new): the table with three seeded holes, headers and the amber
  `⚠ drifter` tag at body size, all 8 columns fitting 1280 with ~460px to spare.
- `after/view_wormholes_rows_narrow.png` (new, 720px): the Source column sits off the right edge and
  is reachable by horizontal scroll, which the census confirms via a `ScrollBar` node.
- `after/ping_fleet.png`: the footer reads at body size, card ~5px taller.
- Alert window and intel card renders are byte-identical to an unpatched baseline, since `intel_row`
  has no `.small()`.

## Left alone, reported

No `.small()` anywhere else in `app/src/`, `crates/` or `site/`. Sub-body sizes do exist as explicit
`.size()` calls: `.size(8.0)` unread dots, `.size(9.5)` chat and rescue timestamps, `.size(11.0)` at
`app.rs:6407`. The two `.size(9.5)` timestamps are the closest remaining candidates but sit outside
this ticket's list.
