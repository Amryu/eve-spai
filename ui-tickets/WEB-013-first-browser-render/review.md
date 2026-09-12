# WEB-013 review cycle

**Status:** Fixed and verified
**Branch:** `web/fixes-and-screenshots`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed, and found two further defects while fixing |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 631 to 636 |
| **Follow-ups** | none |

## Getting a screenshot at all

The previous session reported no browser. There is one; three separate things were making it look
otherwise, and each fails by producing nothing rather than by erroring.

1. **The flatpak Firefox can only write under `xdg-download`.** `--screenshot /tmp/x.png` exits 0 and
   writes no file. `--profile` outside that path says "Could not find profile folder". Both the
   scratch profile and the PNGs now live under `~/Downloads` and get copied out.
2. **Without `--no-remote` and its own profile**, a running Firefox swallows the URL and exits 0.
3. **`--screenshot` fires on the `load` event**, so anything fetched afterwards is not in the shot.

`app/src/uitest/webshot.sh` encodes all three, starts the demo if nothing is on 6799, and shoots at
1440 and 390. The traps are in `CLAUDE.md` so the next person does not spend the same hour.

Point 3 is why the page now carries its first snapshot as an inlined JSON island, the same trick
`crates/server/src/views.rs` uses. That was going to be needed anyway: without it the page costs two
round trips before it shows anything, which on a phone is the difference between instant and visibly
slow. It also makes a load-time screenshot show real data, so Firefox is sufficient and no headless
Chromium is needed.

The island carries EVE chat verbatim, so it is escaped on the way in exactly as `js_safe_json` does:
`<`, `>` and `&` to `\uXXXX`. Anyone in an intel channel can type `</script>`, and
`chat_cannot_close_the_script_element_it_travels_in` is the test that says so, asserting both that
the payload is neutralised and that a JSON parser still reads back the original string.

## The four defects

**Blank page.** `main()` awaited `/api/icons.json` before its first `render()`. Icons are decoration
and the page was blocking its entire first paint on them. Now it paints, connects, and folds the
icons in when they arrive. `the_page_paints_before_it_fetches_anything` asserts no awaited fetch in
`app.js`, which is a crude rule and the right one for a shell.

**Panes rendered into the header.** Tab buttons and pane sections both carried `data-pane`, and the
buttons come first in the document, so `querySelector` matched a button every time. Tabs are now
`data-tab` and the pane lookup is scoped to `#panes`. Both halves are asserted, because fixing only
one of them leaves the next pane ticket free to reintroduce it.

**Assets never revalidated.** `etag()` was keyed on `CARGO_PKG_VERSION` plus the path, under a
comment claiming it stopped a phone running yesterday's JavaScript. It did the opposite: the version
only moves at release, so an edited file kept its tag and the browser kept its copy. This cost a
debugging round where a fixed page kept rendering the old bug, with identical PNG byte counts as the
only clue that nothing had changed. The tag is now a hash of the content.

**A restart left streams hanging.** Dropping the `Handle` stops the listener but not the SSE threads
it already handed out, and `EventSource` only reconnects when a stream ends, so every connected phone
would hold a socket that would never carry another byte. `Frame::Bye` had been written for this and
never constructed; taking the module-wide `#![allow(dead_code)]` off is what surfaced it.

## What the compiler said once the blanket allow came off

`web/mod.rs` carried `#![allow(dead_code)]` while the module was half-wired. Removing it surfaced the
`Bye` variant above, and left three genuinely-not-yet-used items, each now allowed individually with
the ticket that will use it named: `origin_allowed` (WEB-008), `Handle::clients` and `Handle::addr`
(WEB-012). A blanket allow would have hidden the `Bye` bug indefinitely.

Also removed: an `AtomicU32` client counter left behind when `clients()` moved to reading the hub. It
was written on every request and read by nothing.

## One thing I broke and backed out

Fixing an `unused variable: systems` warning, I replaced the binding at every site instead of the one
that warned. Two of the three are used under `fc-rescue`, so `cargo check --all-features`, which is
what `cross-check.yml` runs, went from clean to seven errors. Backed out and applied to the single
line that actually warns. A default-feature build never sees any of this, which is exactly why the
`--all-features` check exists.

## Verification

`app/src/uitest/webshot.sh`, at 1440 and 390, in `after/`.

The 390 shot is the better evidence: it shows **"live" in green** and counts that have advanced past
the boot snapshot, which means a real browser opened the `EventSource`, the server accepted it, and a
later publish reached the page. The Rust tests prove the wire; that shot proves the browser.
