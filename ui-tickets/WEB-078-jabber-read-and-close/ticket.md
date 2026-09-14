# WEB-078 — reading on the web marks nothing read, and nothing can be closed

> "Watching a jabber message on the web is not marking it as read. Also, there is no way to close
> convos."

## Read

The unread marker lives in the app's `JabberState`, and `/api/jabber/chat` is a plain read that
never touches it. Nothing anywhere told the app the page was looking at a conversation, so a message
read on a phone stayed bold on the desktop and kept its badge on both — including the taskbar and
tray counts, which are built from the same `unread_counts`.

`OverlayToMain::JabberRead { jid }` joins the existing write-back path and reaches the app's own
`jabber_mark_read`, so one rule clears it for every client.

The page sends it under three conditions, because "the page has this conversation selected" is not
the same as "somebody is reading it":

- the jabber pane is the one actually on screen (`hidden`/`offsetParent`, the same test the map dock
  uses),
- the browser tab is in the foreground,
- and there is something unread to clear — which is also what stops it posting on every snapshot for
  the rest of the session.

Switching to the pane and coming back to the tab both count as reading, and neither goes through
`register`, which only fires on a snapshot. `apply()` now emits `spai:panes` for the first, and
`visibilitychange` covers the second.

## Close

Each row gets an X, and `OverlayToMain::JabberClose { jid }` reaches `close_jabber_tab` — the same
thing the app's own tab X does. Hiding, not leaving: it comes back on the next unread message, and
the rescue rooms' guard against being closed applies here too because the decision stays in the app.

The button is a **sibling** of the row, not nested: a button inside a button is invalid HTML and the
browser hoists it out, which would have stopped the row being clickable at all. It fades in on hover
on a pointer device and is always there, thumb-sized, where there is no hover.
