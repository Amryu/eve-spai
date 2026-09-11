# WEB-003 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-003-http-server`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **App code changed** | 1255 added, 4 removed |
| **Suite** | 590 to 617 |
| **Follow-ups** | none |

## What changed

**`web::routes`, pure.** `classify`, `host_allowed`, `origin_allowed`, `authorize` and the query and
cookie parsers answer from their arguments alone, so the table of "which request gets which answer"
is tested without a socket. That is most of the security surface, and it is the part worth being able
to read as a table.

**`web::server`, the socket.** Four workers over `recv_timeout`, so `Drop` can stop them by clearing
a flag and calling `unblock()`. The bound address is read back from `server_addr()` rather than echoed
from the setting, which is what lets the tests bind port 0.

**`web::assets`.** An `include_str!` table, served with a version-keyed weak `ETag` and honouring
`If-None-Match`. No bundler: the repo has no JS toolchain, the battle-report server does the same, and
`<script type="module">` needs none.

**`web::css`.** `/api/theme.css` generated from `theme::derived` plus `theme::standing`,
`severity_color` and the eleven `security_color` stops. The page names no colour anywhere. A
per-device override arrives as `?bg=&fg=&accent=` and runs the *same Rust derivation*, so the
override cannot drift from the app the way a JavaScript reimplementation would.

**`web::icons`.** The name to codepoint map emitted from `egui_phosphor::regular`, and the font served
from `Variant::Regular.font_bytes()`, which is already linked into the binary. Same file, same
codepoints, no new asset, no size cost. Phosphor escapes are opaque four-hex sequences, so hand-copying
them is both easy to get wrong and impossible to review, and the failure mode is a tofu square.

**Lifecycle**, in `sync_web_server`, called from `drain_alerts` next to the other worker config push.
The settings are hashed and compared, so only a real change touches the socket, and toggling the
feature or the port needs no app restart. A bind failure logs, stores the message for the settings
pane and leaves the app running, the way `instance::start_control_listener` does.

The token is minted lazily on first enable. If `getrandom` fails the feature turns itself back off
rather than opening the socket with something weaker.

## What was rejected

**Rendering the shell with `maud`**, as `crates/server` does. It would have meant a new dependency in
the desktop binary to produce a page that is static: the page is a shell, and everything in it arrives
as JSON. `include_str!` was already enough.

**Sending the token on every request instead of exchanging it for a cookie.** A token in the query
string lives in the address bar, in history, in the referrer and in any screenshot of the page, and
the page is meant to be screenshotted and shared. The redirect costs one round trip, once per device.

## How the tests were proven to have teeth

| Reverted | Result |
|---|---|
| `host_allowed`'s `parse::<IpAddr>()` swapped for `starts_with("127.")` / `starts_with("192.168.")` | `host_accepts_loopback_and_literals_only`, `origin_is_tested_the_same_way` and `a_request_for_someone_elses_hostname_is_refused` all fail |
| `note_failure` call dropped from the denied branch | `repeated_bad_tokens_are_rate_limited` fails |

The first is the one that matters. `127.0.0.1.evil.com` is a hostname an attacker can actually
register and point at a LAN address, and a prefix check waves it straight through while passing every
other test in the file. It is in the table for both the pure function and the live server.

`a_request_for_someone_elses_hostname_is_refused` deliberately sends a **valid** token with the wrong
`Host`. Checking the host only for unpaired requests would leave the case this defends against wide
open, since by then the token is exactly what the attacker is trying to spend.

## Note on the `app.rs` budget

The series rule allows WEB-003 the `if !headless` start hook. What actually landed is four fields, a
`sync_web_server` method and its one call. It also made `severity_color` and `security_color`
`pub(crate)` so `web::css` can emit them rather than copying two colour tables into CSS by hand,
which would have defeated the point of generating the sheet. Recorded here rather than left to be
noticed.
