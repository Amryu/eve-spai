# UI-048 review cycle

**Status:** Fixed and verified
**Branch:** `web/jabber-convos`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 680, unchanged: the change is rendering, which the scene covers |
| **Follow-ups** | tray and taskbar unread badge |

## What changed

`JabberPane` is `Convos` and `Directory`. Convos is the default and the first tab.

Sorting is unread first, **then** recency. Recency alone would put a conversation that just went
unread with no prior history at the bottom of the list, which is the one place it must not be.

Recency comes from `chats[jid].last().time` rather than a timestamp maintained alongside. The last
message is exactly what "most recent conversation" means; a separate clock is a second thing that can
be wrong.

`unread_counts` is a `BTreeMap` beside the existing set, incremented where the set is inserted into
and cleared at all four sites that remove from it. Missing one would leave a badge on a conversation
with no dot, which is worse than no badge.

A mention is the accent-filled pill rather than a second badge beside the count: what matters is that
the row is different, and two badges compete for the same glance.

## Sticky rows

A conversation that goes unread is pinned into the list until the tab is left. Without it, reading a
DM removes the thing you are pointing at: the row was only in the list *because* it was unread, so
clearing that made it vanish under the cursor.

## MOTDs

Dropping Channels dropped the only place room MOTDs were shown. Room rows carry theirs as a hover
instead, so the information survives the list it used to live in.

## Verified

`after/jabber_sidebar_convos.png`: Convos first and selected, Channels gone, direct messages above
rooms, `Random Guy` carrying an accent pill for a mention next to `Wingmate Alpha`'s plain one, a
struck-through room that is history-only, and counts on the rows that have them.
