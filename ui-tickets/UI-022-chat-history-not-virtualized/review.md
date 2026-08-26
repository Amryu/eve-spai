# UI-022 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-022-chat-virtualization`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 30.0 min across 1 round, 95 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 79/14 lines (added/removed), excluding the harness |
| **Harness code changed** | 235/1 lines |
| **Suite** | 446 to 449 passing |
| **Frame time, 1000-message popout** | **4.31 ms to 0.22 ms** |
| **`message_row` builds per pass** | **1000 to 18** |
| **Follow-ups** | none |

## My ticket's headline claim was wrong

I wrote that the per-frame clone was "likely the dominant cost" and told the agent to fix it first
because it "may be most of the win". It measured, and it is not.

| | ms/frame, release |
|---|---|
| baseline | 4.31, 4.47, 4.38, 4.30 |
| after removing the clone | 4.18, 4.20, 4.21, 4.29 |
| after virtualization | 0.22, 0.22, 0.22, 0.22 |

A bare `Vec<ChatMsg>` clone of that conversation is **57 to 66 us** against a 4.4 ms frame, about
1.3%. The cost was always the 1000 `message_row` builds.

**I reproduced the after-number and the clone cost myself**: 0.22 ms/frame over 120 frames, clone at
57 us. The clone figure alone settles it regardless of the baseline, since 57 us cannot dominate a
4.4 ms frame.

Being wrong here cost nothing, because the instruction was to measure between the two steps rather
than to trust the hypothesis. That is the only reason we know which change earned the win.

## What changed

- `jabber_msgs` deleted. `jabber_conversation_ui` clones the `Arc`, takes the guard just before the
  `ScrollArea`, borrows `&[ChatMsg]`, and drops the guard after. The agent checked `mention_names`,
  `mention_hit`, `message_row`, `render_message_body` and `condense_attention_list` for
  re-entrancy on `self.jabber`, and no caller holds the lock.
- `prev_sender` became `Option<&str>`, removing a `String` allocation per message per frame.
- `.show` became `.show_viewport`. The loop still visits every message, but a row whose cached
  height puts it more than `MSG_OVERDRAW` (400.0, the intel feed's margin) outside the viewport
  becomes `ui.add_space(h)` and `continue`. `mention_hit` and `eve_time_label` moved behind that
  skip.
- `jabber_msg_heights`, keyed by a hash of from, body, time, outgoing, grouped and width. **Index is
  unusable as a key** because the 1000-cap drains from the front and shifts every index. Width is in
  the key because two windows can show one conversation at different widths.

## The three tricky parts, and why each was cheap

The ticket flagged grouping, the divider and scroll-to-bottom as what made this hard. All three fell
out of one decision: **keep visiting every message, skip only the build.**

- **Grouping** needed nothing. `prev_sender`, `prev_time` and the 300s window are still computed in
  order for every message.
- **The `— new —` divider** likewise: the scan from index 0 is unchanged.
- **Scroll-to-bottom** works because the spacer reproduces the exact cursor advance, so content
  height and `stick_to_bottom` are unaffected.

That is a better answer than the intel feed's, which caps at 250 cards. A chat cannot drop old
history, and this does not have to.

## Tests, all teeth-checked

- `..._builds_only_what_is_near_the_viewport`: 18 of 1000. **I verified this myself** by setting
  `MSG_OVERDRAW` to infinity: it fails with `1000 of 1000 rows built into a 480px window, the
  history is not virtualized`.
- `..._keeps_its_tail_divider_and_grouping`
- `..._survives_a_scroll_round_trip`, which drives the wheel and fails when the spacer is dropped.

Counting uses a `#[cfg(test)]` recorder modelled on `record_toolbar_sep`.

Screenshots were compared by reverse-applying the diff and rendering both: `jabber_popout.png`,
`_dm` and `_min` differ in exactly one 11x7 patch each, a timestamp digit that ticked over between
runs. Same messages, grouping, divider position and scroll position.

## Residual risk, all disclosed by the agent

- **The mutex is now held for the render**, roughly 0.2 ms per frame per window, rather than for the
  length of a clone. The XMPP worker blocks for that window. `jabber_frame` already took the same
  lock every frame, so this is a longer hold, not a new one.
- **Frame time is printed, not asserted.** The bench is `#[ignore]`d with no threshold, so a
  regression to 4 ms would pass. A threshold was judged too machine-dependent to be worth the
  flakiness. Reasonable, and worth knowing.
- **Drag-selecting text across rows more than 400px offscreen no longer works**, since those rows do
  not exist. The intel feed already made the same trade.
- The round-trip test cannot catch a spacer wrong by a constant factor: the skipped set is a pure
  function of the scroll offset, so a uniform error stays self-consistent. The agent confirmed that
  by inflating spacers 2% and watching it pass. It does catch a missing or mismatched spacer.
- Only the popout path is screenshot-verified; `View::Jabber` still cannot render (GAP-004). Both
  paths call the same `jabber_conversation_ui`.

## Harness change

Two lines in `harness.rs` are now gated on `debug_assertions`, because egui only carries
`Style::debug` in debug builds and the harness would not compile in release at all. That is what
made a release-mode benchmark possible; the debug harness is 8x slower and would have been
misleading.
