# UI-050 review cycle

**Status:** Fixed and verified
**Branch:** `ui-050-convos-polish`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 690, unchanged: rendering, which the scene covers |
| **Follow-ups** | none |

## Closed, and staying closed

A closed DM is filtered out, **unless it is sticky**, which is what an unread message makes it. So
closing one is curation and cannot lose mail: the next message brings it straight back. That is the
same rule the frame already applied to forgotten conversations, now applied where the list is built.

Reopening now undoes every reason a conversation was hidden rather than one of them:
`jabber_closed_dms`, `jabber_closed_rooms`, forgotten and left. A row that is listed but stays closed
looks exactly like a click that did nothing, which is what was reported.

## The dot

`CIRCLE` is an outline glyph, and at nine pixels an outline in the offline grey is very nearly
nothing. It is painted rather than lettered now, which makes it filled by construction and the same
size whatever the font does. That also explains why only one contact's dot read: it was the only one
online, in a colour bright enough for an outline to survive.

## The row

The whole row is the hit target and carries both highlights. The background is reserved with a
`Shape::Noop` before the content and filled in afterwards, once the row's height is known: painting
it after the content would paint over it.

A label-sized target in a full-width list means most of the row does nothing when clicked and nothing
when pointed at, which reads as the list being dead.

## Starting a conversation

Each section ends with the way to add to it: `Start a DM` and `Join a room`. Both open the existing
join dialog, which already resolved an exact name or a bare handle, and which now also lists the six
most recent of that kind, filtered by whatever is in the field.

The recent list is the field's search results, not a separate control: one box that both filters the
list and accepts an exact name is fewer things to explain than two that do one each.

## Verified

`after/jabber_sidebar_convos.png`: filled dots in grey and green, `Start a DM` closing the direct
messages, `Join a room` closing the rooms, and the badges from UI-048 still reading correctly.
