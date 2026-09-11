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

Screenshots for WEB-005 and everything after it are pending: this session had no browser available.
The Claude in Chrome extension is not connected, and the flatpak Firefox on this machine does not
complete a `--headless --screenshot` run. Taking them needs one command and a browser:

```
cargo test --bin eve-spai webdemo -- --ignored --nocapture
# then open http://127.0.0.1:6799/?t=demo
```

Until they exist, each WEB ticket's `after/` folder is empty and its `review.md` says so. That is the
honest state, not a passed check.

## How to close it

Either wire a headless browser into the repo so a render can be produced the way `uitest_screenshots`
produces PNGs, or accept that this surface is verified by eye and keep saying so in each review.
Automating it is only worth doing if the WEB series keeps growing after the first twelve tickets.
