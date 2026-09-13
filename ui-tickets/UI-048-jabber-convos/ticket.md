# UI-048 &mdash; A direct message is easy to miss behind the sidebar's tabs

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `jabber_ui` sidebar, `JabberPane`, `jabber_frame` |
| **Reported by** | user |

## Symptom

"It is easy to miss DMs as it is now."

The sidebar had three tabs: Directory, Contacts and Channels. A direct message could arrive into a
list the user was not looking at, and nothing about the other tabs said so. Contacts showed only
starred people, so a message from anyone else did not appear there at all.

## Cause

The sidebar was organised by **what a conversation is** rather than by **whether it wants you**.
Rooms lived in one tab, starred people in another, everyone else in a third. Unread was a single
boolean per conversation, rendered as a dot, with no count and no ordering attached to it.

## Deliverable

One `Convos` list, first, replacing Contacts. `Channels` goes entirely.

- Direct messages at the top, rooms below.
- Both sorted unread first, then by recency.
- A DM that goes unread joins the list and **stays** there while the tab is open, so reading it does
  not make it disappear mid-click.
- Each row carries an unread count, and a mention is marked differently from ordinary traffic.

## Notes

Unread was a `BTreeSet` of JIDs. A count needs a number per conversation, incremented where the set
is inserted into and cleared everywhere it is removed from, of which there are four sites.

Recency should come from the message history rather than a separate clock: the last message is
exactly what "most recent" means and cannot drift from it.

## How to verify

The `jabber_sidebar_convos` scene. A room with a MOTD must keep it reachable, since the Channels list
was where MOTDs were shown. A fix would be WRONG if reading a DM removed its row while the list was
open, or if a mention looked the same as four ordinary messages.
