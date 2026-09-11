# WEB-005 &mdash; Web work has no verification surface, and real intel must never reach a screenshot

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `uitest/webdemo.rs`, `web/demo.rs`, `assets/index.html` pane slots |
| **Reported by** | user, remote web view |

## Gap

`app/src/uitest/` renders egui surfaces headlessly and is the regression gate for every UI ticket in
this repo. It cannot render HTML. Every web ticket from here on needs a browser screenshot in
`after/`, and those screenshots are pushed to a public repo.

The rule that governs this is already written down: `harness::build` and `harness::shot` call
`assert_no_live_profile`, because room names, contact JIDs, fleet pings and intel are operational
information and must not leave the machine in a PNG. A web screenshot taken against the running app
would carry exactly that.

## Deliverable

An ignored test, matching the existing `uitest_screenshots -- --ignored` convention:

```
cargo test --bin eve-spai webdemo -- --ignored --nocapture
```

It calls `assert_no_live_profile`, builds a `web::Snapshot` from `app/src/uitest/fixtures.rs`, serves
it with a fixed token on loopback, prints the URL, mutates the fixture on a timer so SSE, badge
updates and sound are demonstrable rather than static, and blocks.

Also lands `index.html`'s four empty `<section data-pane>` slots, so WEB-006 and WEB-007 add panes
without both editing the shell.

## Notes

`mod uitest` is `#[cfg(test)]` (`app/src/main.rs:64`), so none of this reaches a release binary. Add a
second guard specific to this path: the demo server refuses to bind anything but loopback, since the
fixed token exists to make the URL easy to open.

Fixtures already cover the hard cases: `intel_torture`, `intel_across_the_bridge`, `intel_clear`,
`intel_resolving`, `ping_fleet`, `ping_plain_multiline`, `card_chars_bridged`, `uncertain`. Add to
`fixtures.rs` rather than pasting real chat if a render needs data they do not have.

## How to verify

- The test runs, prints a URL, and the page loads in a browser.
- It fails when `EVE_SPAI_DATA_DIR` is not the scratch profile.
- The first committed screenshot lands in `after/`.
- A fix would be WRONG if it made the demo reachable off loopback, or if it read the live profile to
  "make the screenshot look real".
