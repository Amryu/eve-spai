# WEB-009 &mdash; The page is silent, so it has to be watched

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `sound.rs`, `web/routes_sound.rs`, `assets/sound.js` |
| **Reported by** | user, remote web view |

## Gap

A second screen you have to keep looking at is not much of a second screen. The app plays a
per-severity, per-rule sound; the page plays nothing, and the WAVs are unreachable: `preset()` and
`wav()` are private and `ensure_tone` writes to a temp dir.

## Deliverable

`pub fn sound::preset_wav(name) -> Option<Vec<u8>>` exposing what `play` already synthesizes, rendered
in memory. Served at `/assets/sound/{name}-v{rev}.wav`, immutable, for every name in `sound::PRESETS`.
A rule whose `sound` is a file path is served **by rule index**, and only when that index currently
resolves to an enabled rule whose sound is a file path.

Alert events carry the rule's `sound` and `volume`. The page holds an `AudioContext` with decoded
buffers behind one `GainNode`. A mute button in the fixed header, reachable in one tap in every
layout, never inside a pane. Mute and volume persist per device.

## Notes

**Do not bake volume into the WAV.** The desktop does that only because Windows `PlaySoundW` has no
per-sound gain (`sound.rs:138` comment). The browser has a real `GainNode`; serve at authored
amplitude.

**Never take a file path from the query string.** Serving `AlertRule.sound` by index, validated
against the current enabled rules, is what keeps this from being an arbitrary-file-read hole on a
socket bound to the LAN.

**Autoplay.** A mobile browser refuses audio before a user gesture. Open with a "tap to enable sound"
bar that resumes the context and plays a one-sample silent buffer, the standard iOS unlock, then
decodes the presets. The gesture is needed once per page load.

Port `sound::gate_allows`'s 2s cooldown with severity breakthrough into JS so a burst of intel does
not machine-gun. The Rust one is already unit-tested; keep the constants coming from the server.

Document that a backgrounded iOS tab gets neither audio nor SSE. This is a second screen, not a pager;
Pushover remains the thing that wakes a phone.

## How to verify

- A test that `preset_wav(n)` is a valid RIFF header for every name in `PRESETS` and byte-equal to the
  existing private path for the same tone.
- A test that the rule-sound route rejects a path supplied in the query and rejects an index that does
  not resolve to an enabled file-path rule.
- Manual: an alert sounds on a real iOS device and a real Android device, and mute silences it. Name
  both in `review.md`.
- A fix would be WRONG if it accepted a path parameter, or if it played through `<audio>` elements,
  which cannot be muted as one and are worse on iOS.
