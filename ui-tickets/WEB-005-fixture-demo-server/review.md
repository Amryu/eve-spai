# WEB-005 review cycle

**Status:** Delivered, browser evidence pending
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

## What is NOT verified

**No screenshot.** This session had no browser: the Claude in Chrome extension is not connected, and
the flatpak Firefox on this machine does not complete a `--headless --screenshot` run. So the page has
been proven to serve correct data and has not been proven to look right.

`after/` is therefore empty, and that is recorded rather than papered over. GAP-011 carries the full
picture of what the egui harness cannot reach here and what stands in for it. Anyone with a browser
closes this in about a minute:

```
cargo test --bin eve-spai webdemo -- --ignored --nocapture
# open http://127.0.0.1:6799/?t=demo
```
