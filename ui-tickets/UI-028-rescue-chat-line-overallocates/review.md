# UI-028 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-028-rescue-chat-row`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed, option (a) |
| **Agent time** | 7.7 min across 1 round, 33 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 6/4 lines (added/removed), excluding the harness |
| **Harness code changed** | 48/0 lines |
| **Suite, `--features fc-rescue`** | 490 to 491 passing |
| **Suite, default features** | 464, unchanged (the code is feature-gated) |
| **Follow-ups** | one, a shared `message_line` helper |

## The real result: option (b) is wrong, and it was disproved rather than declined

The ticket asked whether `render_message_body` should own its own row, so a fifth instance becomes
impossible. The agent probed that shape at 360px and measured it:

| shape | nick rect | body block | row height |
|---|---|---|---|
| flat, one zero-floored row (today) | h 15.0, top 0.0 | starts x 0.0 | **30.0** |
| body owns a nested row inside a zero-floored row | h 15.0, top 56.0 | x 37.9, top 63.5 | **37.5** |
| body owns a nested row inside a plain `horizontal_wrapped` | **h 26.0, still floored** | x 37.9 | ~43 |

Three separate failures:

1. The nested block measures `available_size_before_wrap().x` **after** the nick, so every wrapped
   line indents 38px under the nick instead of returning to the left margin.
2. The outer `Align::Center` centres a two-row child against the nick, so nick and first body line
   stop sharing a top (56.0 against 63.5), and the row grows 7.5px past the flat shape.
3. **The decisive one.** With the body owning its row, a caller who still writes
   `horizontal_wrapped` gets a 26px floored **nick**. So (b) does not close the class, it relocates
   the bug from the body onto the nick.

That last point is the insight. **The floor belongs to whoever creates the row**, and both real
callers deliberately put nick and body in one row. Fixing it "once" in the renderer cannot work while
the caller owns the row.

## The change

`rescue_chat_line`'s `horizontal_wrapped` becomes the established zero-floored layout. A pointless
save/restore of `item_spacing.x` went with it: it was restoring a child `Ui` about to be dropped.

`render_message_body` is **byte-identical**, so no shared code moved. That is why no screenshots
were re-rendered and no benchmark re-run: the brief tied both to option (b), and (b) was not taken.
The jabber path, including UI-022's height cache, sees no code change at all.

## Measured, and not screenshot-verified

GAP-009 leaves the rescue window without a scene, so this is measurement only and the review says so
rather than implying otherwise. A probe drove `rescue_chat_line` directly at 360px through the real
app fonts, reading pitch off `ui.cursor().top()`:

| line | before | after |
|---|---|---|
| one-line body | 34.0 | **23.0** |
| one-line, grouped, no nick | 34.0 | **23.0** |
| wrapped 2-row body | 49.0 | **38.0** |
| blank body | 34.0 | **23.0**, still held open by UI-027's guard |

11.0px per line, exactly the 26.0 to 15.0 delta, and the pitch stays a multiple of 15.0 as rows are
added.

## Teeth

`uitest_rescue_chat_lines_are_one_line_tall`, `fc-rescue`-gated, building an ad-hoc `Scene` over
`rescue_chat_feed` rather than a window scene. **I re-ran the check myself**: reverting only the
layout line fails with `a rescue chat body is 26.0px tall, still floored at interact_size 26.0`.

I also confirmed the default-feature suite is unchanged at 464, which is the evidence the change is
genuinely feature-gated.

## Correction to my brief

I quoted the `--features fc-rescue` floor as 487. It was **490** at `c673f07`; I had carried the
number forward from before UI-026 and UI-027 landed. The agent caught it. Quoted floors go stale
every time a ticket lands, so they should be measured at brief-writing time, not copied.

## Follow-up worth its own ticket

The shape that *would* close the class is a shared `message_line(ui, prefix, body)` owning the row,
with the nick passed as a closure, since both real callers are nick-then-body. That unifies 25 lines
of jabber nick logic with rescue's one line and does touch the virtualized path, so it wants its own
ticket and its own before/after rather than being smuggled into this fix.
