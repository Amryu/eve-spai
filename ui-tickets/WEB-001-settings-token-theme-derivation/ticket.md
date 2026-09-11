# WEB-001 &mdash; No web settings, no pairing token, and the theme palette exists only inside `Theme::apply`

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `settings.rs`, `theme.rs`, `web/auth.rs` |
| **Reported by** | user, remote web view |

## Gap

The app has no way to be reached from another device. Before any of that can be built it needs three
foundations, none of which exist: a place to store the web settings, a pairing secret, and a palette
the browser can be handed.

`Theme::apply` (`app/src/theme.rs:95`) derives the whole UI palette from three colours and then
writes it straight into an `egui::Visuals`. Nothing else can read those derived values, so a web page
would have to re-implement the arithmetic and would drift the first time either side changed.

## Deliverable

1. `WebSettings` in `settings.rs`, reached as `Settings::web`, holding `enabled`, `port` (6767),
   `bind_lan`, `allow_writeback`, `default_layout`, `token`. One sub-struct, mirroring `AlertSettings`
   (`settings.rs:419`), so every future web setting grows inside it and never at the `Settings` top
   level.
2. `web::auth::new_token()` producing 32 bytes from `getrandom` as base64url, and `ct_eq` for
   comparing it. Generated lazily on first enable so an existing settings blob upgrades cleanly.
3. `theme::Derived` and `theme::derived(&Theme) -> Derived` carrying exactly the arithmetic now inline
   in `apply`: `luminance(bg) < 0.5`, `mix(bg, contrast, 0.05 / 0.10 / 0.16 / 0.03 / 0.18)`,
   `mix(fg, bg, 0.45)`. Then refactor `apply` to consume it, so there is one implementation.

## Notes

**The settings trap.** `Store::load_settings` (`app/src/store.rs:390`) fails the whole parse on one
bad field, stashes the blob under `settings.bad` and returns `None`. Changing the type of an existing
field therefore resets every setting the user has. Only add. `Settings` has no `derive(Default)`, so
each new field also needs an explicit line in `impl Default` (`settings.rs:772`); a missing one is a
compile error, which is the intended guard.

The token goes in the settings blob, not the keyring. The keyring holds ESI refresh tokens, which
grant access to a player's EVE account; this token grants a LAN page on a machine the attacker is
already on. Settings storage also means `copysettings` export carries it.

This ticket adds no server and no UI. It is the foundation WEB-002 and WEB-003 build on, split out so
neither of those has to touch `settings.rs` or `theme.rs`.

## How to verify

- `cargo test` with a new `derived_matches_apply` asserting every field `apply` sets from the derived
  values equals what `derived()` returns, for all five presets in `Theme::presets()`.
- A round-trip test parsing a settings JSON blob captured before this change, asserting it still loads
  and that `web` comes back as its default.
- `ct_eq` tested against a one-bit difference at every byte position, and against differing lengths.
- A fix would be WRONG if it re-implemented the mix arithmetic in a second place rather than making
  `apply` consume `derived()`, or if it moved an existing settings field into the new sub-struct.
