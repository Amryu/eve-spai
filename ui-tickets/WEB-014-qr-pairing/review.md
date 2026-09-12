# WEB-014 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-014-qr`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 657 to 660 |
| **Follow-ups** | none |

## The dependency

`qrcode 0.14.1`, default features **off**. Its `image` feature would pull an encoder this never
needs: the code is rasterised straight into an `egui::ColorImage`. Pure Rust, no system libraries, so
it costs nothing on the cross-builds. That was the decision the ticket was deferred for, and it comes
out clearly in favour: typing 43 characters of mixed-case base64 into a phone is where people give up.

## Two details that are not decoration

**The quiet zone.** Four modules of light border are part of the spec, not padding: without it a
reader cannot find the code against whatever is behind it. Asserted on all four edges.

**The scale.** At one pixel per module the code is postage-stamp sized on a modern display and
nothing can read it. It renders at four, and `scale_multiplies_the_raster` pins the relationship.

## Secrets

The token is a password, and a QR of it is that password in a form a camera across the room can read.
So neither the link nor the code is on screen by default, and revealing them is a button that does
not persist: a revealed secret should not survive a restart into the next time someone shares this
pane or streams it.

The texture is keyed on the URL, so regenerating the token or changing the port replaces it rather
than leaving a code that pairs nothing.

## A leak caught while taking the screenshot

The first render showed **this machine's real LAN address**, because `lan_address()` had done its job.
These renders get committed and pushed.

The probe is now skipped headlessly, which is right on its own terms: a harness build performs no
side effects, and opening a UDP socket is one. The render reads `<this machine's address>:6767`.

## Verified

`after/pairing_qr.png`: the reveal toggle, the fixture pairing URL, and a scannable code with its
quiet zone. Not verified: that a phone camera resolves it to the right URL. The encoding is the
crate's job and the geometry is asserted, but the last step needs a phone.
