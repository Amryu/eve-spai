# UI-050 &mdash; Convos lists closed chats, will not reopen them, and the rows barely respond

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `jabber_convos_list_ui`, `jabber_convo_row`, `jabber_join_dialog` |
| **Reported by** | user, using UI-048 |

## Symptom

- "Convos is showing closed direct chats. Clicking them does not properly seem to open the DM
  again."
- "The status dot for the DMs should be filled in, not empty. Also the status dot only properly
  shows for directorbot."
- "A hover highlight is missing as well for each entry. The current highlight only highlights the
  name, but should highlight the row."
- "The last entry in Direct message should be a 'Start a DM' button, that shows recently talked to
  contacts in a dialog, with a search (sorted by recency). A convo may also be opened if providing
  the exact name instead. Same thing with the rooms."

## Cause

**Closed chats.** The list was built from `dm_keys`, which is everything with history. Closing a DM
records it in `settings.jabber_closed_dms`, which nothing here consulted.

**Reopening.** The row cleared `jabber_forgotten` and opened a tab, but not `jabber_closed_dms`, so
the conversation was reopened into a state that still said it was closed.

**The dot.** `CIRCLE` is an outline glyph. At nine pixels an outline in the offline grey is very
nearly nothing, which is why the only dot that read was a contact who happened to be online.

**The row.** The click target and both highlights were the `Label`, so everything past the end of
the name did nothing and showed nothing.

## How to verify

The `jabber_sidebar_convos` scene, and a closed DM staying closed until it goes unread. A fix would
be WRONG if closing a DM could lose mail: closing is curation, and an unread message has to bring it
back.
