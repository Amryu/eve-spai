# WEB-012 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-012-settings`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 655 to 657, scenes 49 to 51 |
| **Follow-ups** | WEB-014 (the QR) |

## What changed

A "Remote web view" section in `settings_view`: enable, port, a LAN toggle, a write-back toggle,
copy link, open in browser, regenerate, and a live count of connected devices. A failed bind shows
its message here in `standing::WARNING` rather than only in a terminal nobody opened.

With the feature off the section collapses to one checkbox and the line that matters: LAN only, not
encrypted, do not forward the port.

**The link is not shown by default.** It carries the token, and a settings pane is exactly the kind
of screen people share or stream. Revealing it is a `collapsing`, so it takes a deliberate click.

**`lan_address()` makes the link usable.** The bound address is `0.0.0.0`, which is every interface
and not something anyone can type into a phone. A UDP socket "connected" to TEST-NET-1 sends no
packets; the kernel picks the interface it would route out of and the local address falls out. That
beats enumerating interfaces and guessing which one the phone can reach. It returns `None` rather
than loopback or `0.0.0.0`, and the pane then says so instead of offering a link that cannot work.

## A leak the scene caught

The first render of the new scene showed this machine's **real EVE install paths**, home directory
and all, because path detection runs and finds them. Renders get committed to ticket folders and
pushed to a public repo.

`web_settings_scene` sets both directories to fixture values. Worth knowing that the existing
`view_settings` scenes have the same property: they are only a problem when someone commits one.

The scene is 2600px tall because the web section sits below Alerts. The first version was 800px and
showed the top of Settings and none of the subject, which is the exact failure this repo already
records: a scene that crops its own subject reads as coverage without being any.

## Verified

`after/web_settings_section.png`, cropped from the `web_settings` scene, and
`after/web_settings_narrow.png` for the 720px column. The harness checks for overlapping click
targets and widgets escaping their row run over the new controls as part of `cargo test uitest`.
