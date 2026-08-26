# UI-020 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-020-ontop-pin-in-tab-bar`
**Depended on:** GAP-004, which made this surface renderable at all


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 10.1 min across 1 round, 53 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 49/16 lines (added/removed), excluding the harness |
| **Harness code changed** | 57/0 lines |
| **Suite** | 437 to 438 passing |
| **Follow-ups** | none |

## The change

`ontop_pin` split into three:

- `ontop_pin(ctx, id)` keeps the floating `Area` and calls the widget inside it. Unchanged for every
  caller that still uses it.
- `ontop_pin_ui(ui, id)` is the pin itself, laid out wherever it is called. Toggle state and the
  once-only `WindowLevel` send stay keyed on the viewport id, exactly as before.
- `ontop_pin_w(ui)` reports the width so a row can reserve it.

`jabber_tab_bar_ui` passes `Some("jabberwin_{id}")` for `ChatWinKey::Popout` and `None` for `Main`,
reserves `PIN_GAP + ontop_pin_w(ui)`, and draws the pin after the overflow caret. The popout viewport
callback no longer calls `ontop_pin`.

## Why the tab bar, and why not the header row

The tab bar was the ticket's suggestion, and I checked the obvious cheaper alternative before
accepting it. The conversation header, second row, has a large empty span next to the bell and would
cost no tab width at all.

It is wrong anyway: that header lives inside `jabber_conversation_ui`, and `app.rs:3333` shows the
window renders "No conversations in this window." with **no header row** when nothing is selected.
A pin there would vanish exactly when the window is otherwise empty. The tab bar always exists.

Rejected for the same kind of reason: a dedicated `Panel::top` per viewport reserves a whole row,
failing the user's second requirement, and on the 300x118 dscan window that row is a large fraction
of the window. Keeping the `Area` and insetting the central panel leaves the pin outside layout, so
the checker stays blind and the next widget added at the top right collides again.

## The cost, stated plainly

The pin takes about 39px off the tab strip. Visible in the shots: at 520x480 the third tab
ellipsizes from "wingmate" to "w…", and at 360x260 the overflow badge goes from 1 to 2.

That is the minimum a non-overlapping pin can cost, since anything in the layout takes space
somewhere, and horizontal tab width is cheaper than a whole reserved row. Flagged for the user: if
tab width matters more than a visible pin, the alternative is moving it into the overflow dropdown,
which costs nothing but hides it behind a click.

`PIN_GAP = 6.0` exists because the tab row runs at `item_spacing.x = 0`, so without it the pin butts
flush against the caret.

## Every `ontop_pin` caller

| Caller | Fate |
|---|---|
| `app.rs:2336` jabber popout | **Moved.** The tab bar draws it now. This was the bug. |
| `app.rs:13361` map viewport | Unchanged `Area`. Its central panel is the map canvas, no top-right widgets. |
| `app.rs:14623` `dscan_popup` | Unchanged. 300x118, a reserved row would be a large fraction of it. |
| `app.rs:19298` `dialog_viewport_ext` | Unchanged, and it is the shared body of ~18 dialogs. None has a tab row to host a pin. |

One moved, three keep byte-identical behaviour. `ChatWinKey::Main` gets `pin = None` so `pin_w` is
0.0 and the in-app Jabber page's tab arithmetic is untouched.

## Before and after

Reproduced first, with the pin added to the scene as an `Area`. `uitest_layout` failed in all four
popout scenes:

```
jabber_popout:     Button "PUSH_PIN" [[461.8 8.0]-[512.0 35.0]] <-> Button "CARET" [[481.0 6.0]-[514.0 33.0]]
jabber_popout_min: Button "PUSH_PIN" [[301.8 8.0]-[352.0 35.0]] <-> Button "CARET" [[321.0 6.0]-[354.0 33.0]]
```

Green after, with the pin present in the permanent scenes **through production code**, not a
test-only call, since the tab bar draws it for any `Popout`. `ontop_pin` stayed private.

`uitest_jabber_popout_pin_is_in_the_tab_bar` asserts at both sizes that the pin exists, stays inside
the window, sits right of the caret, shares its row, and intersects no other button. The agent
confirmed it fires by reverting to the `Area`: it fails with `the pin is not clear of the overflow
caret`.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 437 passed, 2 ignored (+32) | **438 passed**, 2 ignored (+32) |
| with `--features fc-rescue` | 463 passed | **464 passed** |
| `cargo test --bin eve-spai uitest` | 28 passed | 29 passed |
| census, `jabber_popout` | 4 hit targets | 5, the pin is now inspected |

## Screenshots

- `after/jabber_popout.png`: tab strip, overflow caret, a 6px gap, then the pin at the right edge in
  its selected state, rendering as a real push-pin glyph. Nothing under it, the bell sits on the row
  below, the third tab is ellipsized by the reserved width.
- `after/jabber_popout_min.png` (360x260): the pin is not pushed out and does not touch the caret.
  The badge reads 2 because one more tab overflowed.

## Process note

The agent initially read stale PNGs from the main tree and nearly reported a false negative.
Screenshots land in the worktree's own `target/uishots`, not in the shared `CARGO_TARGET_DIR`,
because the harness derives that path from `CARGO_MANIFEST_DIR`. Worth remembering.
