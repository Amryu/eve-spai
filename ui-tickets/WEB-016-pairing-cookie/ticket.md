# WEB-016 &mdash; Scanning the pairing QR lands on "this device is not paired"

| | |
|---|---|
| **Severity** | Critical |
| **Status** | Open |
| **Region** | `web/server.rs`, `web/routes.rs`, `assets/app.js` |
| **Reported by** | user, pairing a phone |

## Symptom

"Opening the link does not seem to be pairing properly. Scanned the QR code, it seemed to be the
correct url, but it then says it is missing that code."

The phone reaches the server, so the address and the token are right. It is served the not-paired
page anyway.

## Cause

Pairing answered `302` with `Set-Cookie: ...; SameSite=Strict` and `Location: /`.

`SameSite=Strict` withholds a cookie from any navigation that did not start on the same site. A QR
scan does not start in the browser at all, so the whole chain counts as cross-site: the browser
accepted the cookie, then declined to attach it to the redirect it had just been told to follow. The
device landed on `/` with no cookie, having successfully paired one request earlier.

`curl` does not reproduce it, because it has no same-site notion and attaches the cookie regardless.
That is why the round-trip test passed and the phone did not.

## Notes

`Strict` was chosen as the CSRF defence for the write endpoint. It is not carrying that weight:
`/api/action` checks `Origin` against the same IP-literal test `Host` uses, and that is the check
that actually holds. `Lax` still withholds the cookie from cross-site POSTs.

## How to verify

Scan the QR from a phone that has never paired. It must land on the feed. A fix would be WRONG if it
left the token in the address bar: the redirect existed to remove it, so whatever replaces the
redirect has to remove it too.
