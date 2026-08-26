# UI-022 &mdash; Long chat histories lag: no virtualization, plus a full clone per frame

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `jabber_conversation_ui`, `jabber_msgs` |
| **Reported by** | user |
| **Effort** | Large, this one is genuinely tricky |

## Symptom

Chats with a long history lag.

## Three separate costs, all per frame, all scaling with history length

**1. The whole history is cloned every frame.**
`jabber_msgs` (`app.rs:2836`) is
`self.jabber.lock().unwrap().chats.get(jid).cloned().unwrap_or_default()`, called at
`app.rs:3789` on every pass. That deep-clones every `ChatMsg`, each carrying owned `String`s for
body and sender. History is capped at 1000 per conversation (`jabber.rs:431` drains to 1000), so the
worst case is 1000 struct clones with several heap allocations each, per frame, per open window.
This is likely the dominant cost and it is not a layout problem at all.

**2. No virtualization.**
`for (mi, m) in sel_msgs.iter().enumerate()` (`app.rs:3882`) builds a full `message_row` for every
message every frame, including ones scrolled far out of view. `message_row` is not cheap: it does
`ui.interact` plus painter work and lays out an action strip per row.

The intel feed already solved this in this codebase. `app.rs:5774` uses
`ScrollArea::vertical().show_viewport(..)` with a per-card height cache and a `CARD_CAP` of 250.
CLAUDE.md records why: **egui has no built-in variable-height virtualization**, so it has to be done
by hand. Chat rows are variable height for the same reasons intel cards are: wrapped bodies,
grouping headers, the day separator.

**3. Per-message work inside the loop.**
`mention_hit(&m.body, &names)` runs for every message every frame, and `eve_time_label` formats a
timestamp for every ungrouped message.

## Why this is tricky

- Rows are variable height, so a fixed row-height virtualizer will not work. The intel feed's
  height-cache approach is the precedent to follow, not `show_rows`.
- Message grouping depends on the *previous* message (`prev_sender`, `prev_time`, the 300s window),
  so a virtualized window cannot start rendering at an arbitrary index without knowing the state of
  the message before it.
- The `— new —` unread divider depends on a scan from the start of history.
- Scroll-to-bottom on a new message, and scroll position stability when history loads above, both
  interact badly with a virtualized viewport.
- Multiple popout windows can show the same conversation, each paying the cost independently.

## Suggested order of attack

Do 1 before 2. Removing the clone is small, low risk, and may be most of the win on its own, which
would let 2 be judged on its remaining merit rather than assumed. Borrow under the lock, or cache a
snapshot keyed on a cheap change signal, rather than cloning per frame.

## Verification is not a screenshot

The harness catches layout, not frame time. This ticket needs a different measure: a bench or a test
asserting the number of `message_row` builds for a long history stays bounded, plus a manual check
against a real long conversation. Note the harness cannot currently render the jabber view at all
until GAP-004 lands.
