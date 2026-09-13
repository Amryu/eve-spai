# WEB-037 &mdash; Jabber messages are flat text

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/panes-jabber.*`, `web/jabber.rs`, `app.rs` |
| **Reported by** | user |

## Ask

"Jabber: Make sure that any links are clickable. Mentions should be highlighted. Make sure that
'x requests the attention of: ...' messages are summarized just as in the app to shorten them. If the
same person keeps texting up to 5min after the first message in this block, do not repeat the name
and time header. (Only if no one else spoke inbetween)"

## Gap

The body was escaped and printed. A fleet ping's URL was text to retype, being named in a busy room
looked like every other line, the ping bot's roll-call buried whole conversations under a
multi-kilobyte list of recipients, and one person saying four things in a row repeated their name and
the same minute four times.

## Acceptance

All four, and the escaping has to survive all four: a message body is EVE chat, which is to say
attacker-controlled text.
