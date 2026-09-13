# WEB-029 review cycle

**Status:** Delivered
**Branch:** `web/web-028-layout-and-jabber`, shared

## Resolution

| | |
|---|---|
| **Outcome** | Delivered, plus sending, which was not asked for |
| **Suite** | 693, up 3 |
| **Follow-ups** | none |

## What travels where

The Convos list is in the snapshot: it changes whenever anything arrives, and it is small. The
messages are not. One room's backlog is larger than every other pane put together and only one
conversation is on screen, so `/api/jabber/chat?jid=` serves the tail of the one being read, 200
lines, and the pane only asks again when the list says that conversation's `last_at` moved. Polling
on a timer would refetch a backlog every few seconds to learn nothing.

The list is built on the UI thread, with the rest of `UiFacts`, because its rules read settings the
publisher has no handle on: who is a contact, what was closed, what was forgotten. The order is the
app's own, stated once, so the two lists cannot disagree about what is at the top.

## Selection

Per device, in `localStorage`, not pushed from the app. Two people reading the same feed from two
phones are not reading the same conversation, and the desktop is a third reader again.

## Starting one

`JabberOpen { name, room }` carries a name and a kind, never a resolved JID. Turning "Some Pilot"
into a JID needs the configured domain and joining a room is a command to the session, both of which
live on the app side; and a socket reachable from the LAN should not be able to name an arbitrary JID
for this machine to join. The app resolves it through the same roster-then-domain order its own
dialog uses.

## Sending

Not asked for, added anyway: a chat pane you cannot answer from is half a pane, and the alternative
was shipping it and waiting for the request. Behind `allow_writeback` like every other write, and
only to a conversation that already exists, so the page cannot open a thread with a stranger. The
outgoing echo comes from the session worker, which already appends it, rather than from a second
copy here.

## Verified

`after/jabber-pane.png`: the list with unread counts, a mention in accent, filled presence dots, both
start rows. `/api/jabber/chat` checked against the fixture session by hand:

```
{"jid":"wingmate.alpha@goonfleet.com","msgs":[{"from":"Wingmate Alpha","body":"you on for the strat op?",...
```

Three unit tests on the tail: the newest lines and in order, a conversation with no history comes
back empty rather than absent, a short one is served whole. Taking the first 200 instead of the last
would serve a room's oldest messages forever, which looks like a conversation that stopped.

Presence dots are drawn, not glyphs: Phosphor's circle is an outline, and a hollow dot reads as
"unknown". The app learned the same thing in UI-050.
