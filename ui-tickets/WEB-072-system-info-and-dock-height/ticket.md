# WEB-072 — the web system dialog, and the docked list's height

> "The web map: The system info is missing a lot of stuff the in-app map has. Copy that extra info
> to the web as well (but make it look a bit nicer there). Also, the jump/gate list is STILL not
> using the entire available height. Figure this out already."

## The height

`.mpanel` carries `max-height: min(80vh, 44rem)`, which is right for a floating window and wrong for
a docked one. `.rdock .mpanel` overrode flex, overflow and padding but never that cap, so however
tall the dock was the panel stopped at 704px and the hop list scrolled inside whatever was left. On
a 1000px window that left roughly 200px of empty dock below the "Save route" row.

Two earlier passes missed it because every screenshot was taken against a demo server left running
from an earlier session: `webshot.sh` only started one when the port was free, so a CSS or JS change
screenshotted as if it had never been made. The script now kills whatever holds 6799 and starts its
own, and wipes the Firefox profile between runs so the ETag-cached assets go with it.

## The system dialog

What the app's system window had and the page did not:

| | |
|---|---|
| Last-hour **jumps**, and all four counters coloured against the **region average** | twenty kills is a quiet hour in Delve and a siege in Aridia |
| **Rat profile** — faction, damage dealt, damage they are weak to, EWAR | |
| **Gate camp** banner, with level, kills, span and age | |
| **Scanned wormholes** at either end of this system | |
| **Sov upgrades** | |
| **Neighbours** with security colour and a tint when the gate leaves the constellation or region | the page had a flat list of gate names |
| **Bookmark** toggle, **sov alliance logo**, **ADM**, **FW** | |

Laid out as cards rather than the app's single column of labels: the page has the width, and traffic,
rats and wormholes are three separate questions that were reading as one wall of text. Neighbour
chips carry an intel-severity dot from the snapshot the page already holds, which is the same
question the app's "(3)" report count answers, answered better.

`Bookmark { id, on }` joins `OverlayToMain` so the star writes back to the same list the app's does.
