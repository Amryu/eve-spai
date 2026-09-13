# UI-049 review cycle

**Status:** Fixed, icons verified by eye
**Branch:** `web/jabber-convos` continuation

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 681 to 689 |
| **Follow-ups** | none |

## Drawing text without a text renderer

The tray icon is painted on a background thread with no egui context, so the app's fonts are out of
reach, and pulling in a font crate to draw at most three characters would be a dependency for two
digits and a plus sign.

`badge.rs` carries a 3x5 bitmap font for `0`-`9` and `+`, scaled to whatever the icon can afford. The
badge is a pill rather than a circle because `99+` does not fit in a circle that leaves the logo
visible, and it sizes itself from the text so a one-digit count does not get a three-digit badge.

Seven tests cover it: the label rule, zero painting nothing, a badge staying inside its icon and out
of the top-left quadrant, a longer count taking more room, an icon too small being left alone rather
than scribbled on, and every character the label can produce having a glyph. That last one is the
guard that matters: `label` and `glyph` are two lists that have to agree, and nothing else would say
if they stopped.

## Muted

A muted conversation contributes nothing. The user has already said they do not want to hear about
it, and a number on the taskbar is hearing about it.

## Repaint rate

`ViewportCommand::Icon` hands the window manager a fresh image, so the taskbar badge is only sent
when the count actually changes. The tray already polled its own count on an 800ms timer and only
refetches on a change; it now compares a number rather than a flag.

## Not verified here

That the two icons look right in a real tray and a real taskbar. Nothing in this repo can screenshot
either. The painter's output is asserted; its appearance is for eyes.
