# UI-018 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-018-ping-body-rows`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 16.0 min across 1 round, 93 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 10/1 lines (added/removed), excluding the harness |
| **Harness code changed** | 120/1 lines |
| **Suite** | 452 to 455 passing |
| **Follow-ups** | UI-027, the same bug in the chat body renderer |

## My ticket was wrong about the third caller

It said `render_ping_body` is "shared with the jabber body renderer at `app.rs:23707`", and that
this was why UI-013 could not fix it. **There is no jabber caller.** `render_ping_body` has exactly
three call sites: the Fleet arm, the Plain arm, and a unit test. `git log -S` confirms it never had
one at any commit, which I verified.

Chat bodies go through `render_message_body`, a sibling that shares `render_linked_text`, called
from `jabber_conversation_ui` at `app.rs:4021` inside that function's own `horizontal_wrapped`.
Same root cause, different call site, inside the region this ticket was told not to touch.

So the fix was safer than the ticket implied, and the chat surface is **not** fixed by it. Filed as
UI-027 rather than quietly widened.

## The change

`render_ping_body` swaps `horizontal_wrapped` for the same zero-floored wrapping layout UI-013 used
one row above. It did not repeat UI-013's recorded dead end: setting `interact_size.y` inside the
closure does nothing, because egui reads it off the parent.

**One thing UI-013 did not have to handle:** `body.lines()` yields empty strings for the author's
paragraph breaks, and a `horizontal_wrapped` gave those an accidental 26px row. A zero-floored row
gives them literally nothing, measured at 0.0px, so blank lines would have vanished. An explicit
`add_space` of one body-line height restores the break at a sane size. That is a bug the fix would
have introduced, caught before it shipped, and it has its own test.

## Measurements

`ping_plain_multiline`, 520x320, three-line body with a link on line 2:

| line | before | after |
|---|---|---|
| "Sov timer in 68FT-6 at 19:40." | h 26.0, top 48 | h 15.0, top 48 |
| "Fits and doctrine: " | h 26.0, top 80 | h 15.0, top 69 |
| the link | h 26.0, top 80 | h 15.0, top 69 |
| "Bring a mobile depot..." | h 26.0, top 112 | h 15.0, top 90 |

Top-to-top pitch 32.0 to 21.0, gaps unchanged at 6.0 (that is `item_spacing.y`). The body block runs
48..138 before and 48..105 after, 33px saved on three lines. `ping_fleet` card 9px shorter,
`ping_plain` 11px shorter.

**Chat is byte-identical in geometry**, before and after, which is the evidence that
`render_message_body` is a genuinely separate path.

## The hyperlink case

A link IS interactive, which is what the row floor existed for, so this was the case most likely to
break. `uitest_ping_body_link_stays_on_its_line` asserts the link shares its line's top and height,
sits right of the text before it, and does not overrun the line below.

Worth recording: egui declares `Role::Link`, but the selectable-label pass overwrites it, so a
hyperlink reports `Role::Label` in the tree. The test filters on the node answering
`Action::Click` instead. UI-013's chip test hit the same shape.

## Teeth, confirmed independently

**My first teeth-check patched the wrong line.** UI-013's doctrine row has a byte-identical
`available_size_before_wrap().x, 0.0`, and my substitution hit that one, so the test passed and
briefly looked toothless. Targeting the line inside `render_ping_body` fails with
`body line "Sov timer in 68FT-6" is 26.0px tall, still floored at interact_size 26.0`.

A reminder that a teeth-check has to revert the specific behaviour, not a matching string.

The blank-line test fails with `a blank line opened 0.0px, not the 15.0px of the line it stands
for`. The link test passes before and after by design, guarding against a future fix splitting the
link onto its own row.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 452 passed, 4 ignored (+32) | **455 passed**, 4 ignored (+32) |
| with `--features fc-rescue` | 478 passed | 481 passed |
| `cargo test --bin eve-spai uitest` | 40 passed | 43 passed |

## Screenshots

- `after/ping_plain_multiline.png` (new scene): three lines set as a paragraph instead of three
  loosely spaced rows, with the link inline on line 2 on the same baseline as the text before it.
- `after/ping_fleet.png`: description one line tighter under Doctrine, card 9px shorter. Doctrine to
  description now reads at the same leading as FC to Formup.
- `after/ping_window_mixed.png`: Fleet card 9px shorter, Plain card moves up with it, both inside
  the overlay.
- `jabber_popout.png` and `jabber_popout_long.png`: layout pixel-identical, only clock labels
  differ. Confirms chat does not run through this function, and UI-022's height cache is untouched.

Note on method: most of `target/uishots` differs run to run because fixtures are wall-clock
relative, so the agent rendered the fixed code twice and counted only files stable across both runs
as real changes.
