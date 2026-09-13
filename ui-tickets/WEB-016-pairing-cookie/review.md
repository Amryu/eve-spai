# WEB-016 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-016-pairing`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 664 to 667 |
| **Follow-ups** | none |

## What changed

Two changes, either of which would have fixed it; both are worth having.

**`SameSite=Lax`.** `Strict` is withheld on any navigation that did not start on this site, and a QR
scan starts outside the browser entirely. `Lax` attaches the cookie to top-level GET navigations,
which is exactly the pairing case, and still withholds it from cross-site POSTs.

**No redirect.** A valid `?t=` now serves the page directly with the cookie attached, so the first
load does not depend on a cookie surviving a hop. The token still leaves the address bar, now via
`history.replaceState` in the page, which is what the redirect was for.

Also: a token that does not match now says so. A regenerated link and a device that never paired are
different problems with different fixes, and telling them apart is the difference between "scan the
new code" and "where do I find the link".

## Why the tests did not catch it

`the_pairing_round_trip` passed throughout. `reqwest` has no same-site notion, so it attached the
cookie to the redirect exactly as `Strict` says it should not. The test asserted the mechanism I had
built rather than the outcome the user needed, and the difference only exists in a real browser.

`pairing_serves_the_page_itself_rather_than_a_redirect` now asserts the outcome: a single 200 with
the page in it, and a cookie that is not `Strict`. That is checkable without a browser because it no
longer depends on browser policy at all, which is the point of removing the redirect.
