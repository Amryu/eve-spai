# WEB-009 review cycle

**Status:** Fixed and verified, except on a real phone
**Branch:** `web/web-009-sound`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 649 to 655 |
| **Follow-ups** | none |

## A bug found on the way in

`sound::PRESETS` listed `siren` unconditionally, but its arm in `preset()` is behind `fc-rescue`. So a
stock build offered a sound in the picker that resolved to `None` and played as **silence**, with
nothing anywhere saying why. Found because the first version of
`every_preset_renders_a_real_wav` walked `PRESETS` and panicked on `siren has no tone`.

The list is gated now, and `every_offered_preset_resolves_in_this_build` is the regression test: every
name a build offers has to resolve to a tone in that build. It runs in both feature sets.

## What changed

`sound::preset_wav(name)` renders in memory rather than through `ensure_tone`'s temp-dir cache: a
request should not depend on a writable temp dir. **Volume is not baked in.** The desktop bakes it
only because Windows `PlaySoundW` has no per-sound gain; a browser has a real `GainNode`.

The page uses one `AudioContext` with decoded buffers behind a single `GainNode`, not a pool of
`<audio>` elements: one gain mutes everything at once, latency is lower, and iOS behaves. Mute and
volume are per device.

`sound::gate_allows`'s two-second cooldown with severity breakthrough is ported to JS, so a burst of
intel does not machine-gun. A first load deliberately sounds nothing: the alert feed already holds
everything that fired before the page opened, and replaying it as a volley would be worse than
silence.

## The path that is not a path

`/assets/sound/{name}-v{rev}.wav` parses `name` as lowercase ASCII and nothing else. This socket can
be on the LAN, so a sound name that could become a path is a file-read primitive.
`a_sound_name_cannot_escape_into_the_filesystem` covers `../`, percent-encoded `..`, an embedded
slash, uppercase, and an empty name, and the live server returns 404 for the traversal attempt.

## Verified

| | |
|---|---|
| `/assets/sound/warning-v6.wav` | 200, 24740 bytes, `audio/wav` |
| `/assets/sound/critical-v6.wav` | 200, 44142 bytes |
| `/assets/sound/nope-v6.wav` | 404 |
| `/assets/sound/../../etc/passwd-v6.wav` | 404 |

`after/web-900.png` shows the arm button in the header, amber, before any gesture.

**Not verified: that a phone actually makes a noise.** Nothing here can click, and the audio unlock
is specifically the thing that only a real gesture exercises. The wire, the gate and the routing are
covered; the last step needs a person with a phone. That is the honest state of this ticket.
