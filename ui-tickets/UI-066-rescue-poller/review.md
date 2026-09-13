# UI-066 review cycle

**Status:** Fixed
**Branch:** `fix/ui-066-rescue-always`

`active` now follows the feature: the frame that runs the rescue housekeeping sets it once and selects
the newest call. It still exists because it genuinely gates the pollers and is per-session, but nothing
in the UI switches it, which is what "no more switching in between" means.

Removing the switch without moving what it switched is the mistake UI-065 made, and it is worth naming:
a flag with one writer looks inert when the writer goes, and everything reading it fails quietly rather
than loudly. The fleet poller did not error, it simply never polled.

The exit bar is gone, and so are `enter_rescue_mode` and `rescue_window_open`: dead code around a
deleted concept is how the concept comes back. The toolbar shortcut and the settings button both open
the tab now, which is the only thing either of them can usefully do.
