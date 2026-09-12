# WEB-013 &mdash; The first browser render of the web view showed a blank page, then rendered every pane inside the header

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `web/assets/app.js`, `web/assets.rs`, `web/server.rs` |
| **Reported by** | user, "figure out how to properly screenshot and verify" |

## Symptom

WEB-001 through WEB-005 landed with every Rust test green and no screenshot: no browser was reachable
in the session that wrote them. The first time the page was actually rendered it was broken twice
over.

`before/01-blank-behind-connecting-1440.png`: the page is empty. A header, the brand, the word
"connecting", and nothing else. No tabs, no panes.

`before/02-panes-rendered-into-the-tabs-1440.png`: with that fixed, every pane's content renders
inside the header, in four rounded boxes, centred. The boxes are the tab buttons. The four pane
sections below them are empty and the page body is blank.

## Measured

| | |
|---|---|
| Rust tests passing when the page was blank | 631 |
| Tests that failed because of either defect | 0 |

Both defects are in JavaScript and in the DOM, which nothing in the suite looks at.

## Cause

**Blank page.** `main()` was `async` and `await`ed `/api/icons.json` before its first `render()`. The
page painted nothing until that request resolved, and would have painted nothing at all if it never
did. Icons are decoration; the page was blocking its entire first paint on them.

**Panes in the header.** `renderTabs()` emitted `<button class="tab" data-pane="...">` and `render()`
looked panes up with `document.querySelector('[data-pane="..."]')`. The tab buttons come first in the
document, so every lookup matched a button and every pane rendered into the header. The centred
rounded boxes in the before shot are buttons, which is why the layout looked nothing like the CSS.

## Notes

Two further defects were found while fixing these, both in the same verification pass:

- **Assets never revalidated.** `etag()` was keyed on `CARGO_PKG_VERSION` plus the path, with a
  comment claiming it stopped a phone running yesterday's JavaScript. It does not: the version only
  moves at release, so an edited file kept its tag and browsers served the cached copy. This cost a
  debugging round where a fixed page kept rendering the bug, with identical PNG byte counts as the
  only clue.
- **A restart left streams hanging.** Changing the port, token or theme drops the `Handle`, which
  stops the listener but not the SSE threads it already handed out. `EventSource` only reconnects
  when a stream ends, so every connected phone would sit on stale data forever. `Frame::Bye` existed
  for this and was never constructed, which the compiler said once the module-wide
  `#![allow(dead_code)]` came off.

The page also wraps its brand to two lines at 390px, fixed here since it is shell chrome rather than
layout, which WEB-010 owns.

## How to verify

```
app/src/uitest/webshot.sh
```

Shoots the fixture demo at 1440 and 390. The page must show the brand, four tab chips with counts,
and four pane sections **below** the header, on both. A fix would be WRONG if it made the panes
appear by removing the tabs, or if it left the first paint waiting on any request: the page carries
its first snapshot inline for exactly that reason.
