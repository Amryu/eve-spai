# WEB-005 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-005-demo`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified, except the screenshot |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 629 to 631 |
| **Follow-ups** | GAP-011 |

## What changed

`web::demo` builds a `Snapshot` from `uitest::fixtures` and nothing else, growing by one report and
one ping per tick so the push channel, the age clock and later the sound have something to
demonstrate. `uitest::webdemo` is an `#[ignore]`d test that seeds it, starts the server on
`127.0.0.1:6799` with the token `demo`, prints the URL and blocks.

Two guards, both asserted by ordinary tests that do run: the demo refuses to bind off loopback, and
it refuses to start without the scratch profile. A third test reads `web/demo.rs` and fails if it
ever reaches for a store or an `IntelState`, which is the thing that would quietly turn a committed
screenshot into published intel.

`index.html` also gained its four `<section data-pane>` slots, so WEB-006, WEB-007 and WEB-011 add
panes without three tickets editing the same file.

## Verified

The demo was run and exercised over HTTP:

| | |
|---|---|
| `/healthz` | 200 |
| `/?t=demo` | 302 with the pairing cookie |
| `/api/snapshot` | seq 18, 6 intel cards, 3 pings, 3 systems with intel |
| first card | `319-3D clr`, 1 jump from the fixture player |

The sequence reaching 18 on its own is the publish loop and the rev comparison working end to end:
the panes are re-hashed every tick and only the ones that actually changed move it.

## What was NOT verified at the time, and what that cost

This ticket originally landed with no screenshot, on the belief that no browser was available. The
page had been proven to serve correct data and had not been proven to look right.

It did not look right. The first render was blank, and the second drew every pane inside the header.
Both are written up in **WEB-013**, along with the three ways a headless Firefox silently produces no
file, which is what the "no browser available" conclusion actually was.

`after/` now holds shots from `app/src/uitest/webshot.sh`. The lesson is in the demo's favour: the
demo server was the right thing to land before the panes, and the mistake was closing a ticket on
green tests while the only surface that could show the defect went unlooked at.
