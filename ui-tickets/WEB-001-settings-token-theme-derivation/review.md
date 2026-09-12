# WEB-001 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-001-settings-token-theme`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **App code changed** | 277 added, 17 removed |
| **Harness code changed** | 0, this ticket predates any web render surface |
| **Suite** | 574 to 583 |
| **Follow-ups** | WEB-014 (QR pairing) |

## What changed

**`theme::Derived` and `theme::derived()`.** The palette arithmetic that was inline in
`Theme::apply` is now a function returning a struct, and `apply` destructures that struct instead of
recomputing. Nothing about the rendered app changes; the point is that a second consumer can now read
the same values. The alternative, letting the web view re-derive `mix` and `luminance` in JavaScript,
was rejected: the two would agree on the day they were written and drift on the first theme change,
and the drift would be invisible until someone compared a phone against a monitor.

**`Settings.web: WebSettings`.** One sub-struct holding `enabled`, `port`, `bind_lan`,
`allow_writeback`, `default_layout` and `token`, plus a `WebLayout` enum. Defaults to disabled on
6767, LAN binding, write-back allowed, `Auto` layout, empty token.

Putting all six inside one sub-struct rather than at the `Settings` top level is the load-bearing
decision. `Store::load_settings` (`store.rs:390`) fails the whole parse on one bad field, stashes the
blob under `settings.bad` and returns `None`, so a key added or retyped at the top level is exactly
how a user loses every setting they have. A sub-struct keeps every future web key away from that.

**`web::auth::new_token` and `ct_eq`.** 32 bytes of `getrandom` as unpadded URL-safe base64, 43
characters. `new_token` returns `Option` rather than falling back to anything weaker when the OS rng
fails; a caller that gets `None` must leave the server off.

The token lives in the settings blob, not the OS keyring. The keyring holds ESI refresh tokens, which
reach a player's EVE account. This one reaches a LAN page on a machine an attacker is already on, and
keeping it in settings means the existing export and backup paths carry it without new work.

`app/src/web/` carries `#[allow(dead_code)]` until WEB-003 calls into it, so this ticket lands without
warnings and WEB-003 owns only `web/server.rs` plus one hook in `app.rs`.

## How the tests were proven to have teeth

Each was checked by reverting the behaviour under test, not by reverting a file.

| Reverted | Test that failed |
|---|---|
| `faint: mix(bg, contrast, 0.03)` to `0.04` | `derived_pins_the_default_palette`, `left: #15191B, right: #121619` |
| `v.faint_bg_color = faint` to `= surface` | `apply_puts_the_derived_palette_into_visuals`, "Caldari faint_bg_color" |
| `ct_eq`'s body to `a.starts_with(b)` | `ct_eq_matches_only_identical_strings` |

The two theme tests are deliberately different in kind. `apply_puts_the_derived_palette_into_visuals`
only checks that `apply` routes each derived value to the right `Visuals` field, which stays true if
the arithmetic itself changes; `derived_pins_the_default_palette` is what pins the arithmetic. Either
alone passes a change the other catches.

`ct_eq_rejects_a_single_flipped_bit_anywhere` did **not** fail against `starts_with`, since a flipped
bit at any position also breaks the prefix. The length cases in
`ct_eq_matches_only_identical_strings` are what carry that one.

`a_settings_blob_written_before_the_web_feature_still_loads` parses a three-key blob with no `web`
object and asserts the other keys survive. It is the regression test for the failure mode described
above, and it is cheap to keep forever.

## Deferred, and now filed

The ticket said a QR of the pairing link "lands here only if that new dependency is acceptable,
otherwise it becomes its own follow-up". It did not land and the follow-up was not filed at the time,
which is how a deferral turns into a thing nobody remembers. It is **WEB-014** now.
