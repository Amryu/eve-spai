# WEB-052 review cycle

**Status:** Delivered
**Branch:** `feat/web-052-logo`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 701 to 702, one new |
| **Follow-ups** | none |

## The logo

Served straight out of the binary, the same bytes the window and the tray use, so the page cannot
disagree with the app about what the app looks like. It is the favicon as well.

## Advanced

Two settings, both behind a collapsed header and behind a warning that has to be accepted once. The
warning says what is actually at stake, which is not "your settings" but the intel feed, fleet pings,
your jabber conversations and any opsec channel you are in, over a connection with no TLS and, with
pairing off, no password.

Unpaired serving skips the token and nothing else: the DNS-rebinding host check, the origin check on
writes and the rate limiter all still apply. Those defend against a different attacker than the token
does, and turning off one is not a reason to turn off the others.

## Never the default

`exposure_is_never_the_default` is the check you asked for, and it checks the upgrade path as well as
`Default`: a settings blob written before these fields existed parses to off for all three and keeps
its token. Worth a test rather than trusting the struct, because "it defaults to safe" stays true
until someone reorders something.

All three are `serde(default)` additions to `WebSettings`, never a retype of an existing field: a
changed field type fails the whole parse and resets every setting there is.
