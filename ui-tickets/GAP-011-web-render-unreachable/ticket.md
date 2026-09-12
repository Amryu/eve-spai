# GAP-011 &mdash; The harness cannot render the web view

| | |
|---|---|
| **Severity** | Gap |
| **Status** | Open |
| **Region** | `app/src/uitest/`, the WEB series |
| **Reported by** | spun off WEB-005 |

## What the harness cannot reach

`app/src/uitest/` drives egui through `egui_kittest` and inspects the AccessKit tree. The web view is
HTML in a browser: no egui surface, no AccessKit tree, nothing for `checks.rs` to assert against.
Every visual claim in the WEB series therefore rests on a person looking at a page.

## What stands in for it

`cargo test --bin eve-spai webdemo -- --ignored --nocapture` serves the fixture snapshot on
`127.0.0.1:6799`, cycling it every three seconds. That is the surface a screenshot is taken against,
and it is why the demo landed before any pane: fixtures only, loopback only, no live profile, so the
PNGs are safe to commit to a public repo.

Underneath it, the parts that can be tested without a browser are:

| Covered by a Rust test | Not covered |
|---|---|
| routing, pairing, host and origin checks, rate limiting | whether the page looks right |
| SSE framing, first-event latency, replay ring, client cap | swipe feel, momentum, pinch zoom |
| the emitted palette matching the app's | whether a badge matches its egui counterpart |
| the page subscribing rather than polling | iOS audio unlock |
| every asset being referenced by the page | anything about a real phone |

## Notes

**Resolved, in part.** `app/src/uitest/webshot.sh` now shoots the demo at 1440 and 390 with the
flatpak Firefox. The first report that no browser was available was wrong: Firefox works, but fails
silently in three ways, each of which looks like "headless is broken here".

- It can only write under `xdg-download`, so `--screenshot /tmp/x.png` exits 0 and writes nothing,
  and a `--profile` outside that path reports "Could not find profile folder".
- Without `--no-remote` and its own profile, a running Firefox swallows the URL and exits 0.
- `--screenshot` fires on `load`, so anything fetched after that is not in the shot. The page now
  inlines its first snapshot, which makes a load-time shot show real data.

What that leaves: a shot is a single frame at load. Interaction, swipe, pinch and audio unlock are
still verified by hand on a real device, and each `review.md` names the device.

## How to close it

The static-render half is closed. What is left is driving the page rather than photographing it:
clicks, swipes, pinch, and the audio unlock. That needs a browser that can be scripted, which the
flatpak Firefox cannot be without a WebDriver setup. Worth doing only if the WEB series keeps growing
past the first twelve tickets; until then each review names what was checked by hand and on what.
