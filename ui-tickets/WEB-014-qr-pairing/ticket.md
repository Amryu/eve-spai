# WEB-014 &mdash; Pairing a phone means typing a 43-character token by hand

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `settings_view`, `web/auth.rs` |
| **Reported by** | spun off WEB-001 |

## Gap

The pairing link is `http://192.168.1.5:6767/?t=<43 chars of base64url>`. On the machine running the
app that is a copy button. On the phone the link is meant for, it is 43 characters of mixed-case
base64 typed into a browser address bar, which is where people give up.

WEB-001 named this and deferred it: a QR needs one new dependency, and that is a decision worth
taking on its own rather than inside a ticket about settings plumbing.

## Deliverable

A QR of the paired URL in the web settings section, next to the link and the copy button.

`qrcode` (pure Rust, no C, no system libs) renders to a bitmap that becomes an `egui::ColorImage`,
which is roughly a dozen lines at the call site. It is the only new dependency, and the release
profile is tuned for size (`opt-level = "z"`, lto), so the cost belongs in the review.

## Notes

The token is a secret. A QR of it is a secret in visual form, so it does not belong on a screen
someone is sharing. Whatever this looks like, it should be behind a deliberate "show" rather than
painted into the settings pane for anyone walking past or watching a stream.

Rotating the token must invalidate the code, which falls out of rendering it from the live setting
rather than caching it.

## How to verify

A `uitest` scene of the settings section with the code shown, through the normal `ui-tickets` flow,
and one phone that actually scans it and lands paired. A fix would be WRONG if it rendered the code
without a deliberate reveal, or if it cached the image past a token rotation.
