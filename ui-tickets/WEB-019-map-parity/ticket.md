# WEB-019 &mdash; The web map is missing most of the app's read-only layers, and the snapshot is 2.9 MB

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `web/{snapshot,publish,facts,state}.rs`, `assets/map.{js,css}`, `app.rs` overlay sources |
| **Reported by** | user |

## Symptom

"The map is also lacking most of the extra information visible inside the app. (Dont need the
routing features here, just displaying bridges, upgrades, wormholes, etc.)" and, separately, "I want
it to have parity in terms of read-only features with the in-app one."

## Measured

The live snapshot, fetched from the running instance:

| Pane | Bytes | Note |
|---|---|---|
| intel | 1,207,929 | of which `lookups.status` is 1,043,628 |
| alerts | 1,073,089 | of which `msg.status` is 1,043,628, the **same data again** |
| pings | 737,987 | 1163 pings |
| map | 133,450 | |
| meta | 608 | |
| **total** | **2,861,054** | |

`SysFlags` serializes at ~190 bytes a system across 5382 systems, and both panes carried a copy, so
it was re-sent whenever either changed.

What the app draws and the web map did not: sovereignty by alliance or by coalition, activity (ship
kills, pod kills, NPC kills, jumps), ADM, cyno generators, sov upgrades with their kind and level,
and the region view.

## How to verify

The layer panel must offer the same read-only layers the app's does, and the snapshot must come down
under a megabyte. A fix would be WRONG if it reached parity by sending more: the status data is
shared reference data and belongs in one place, sent once.
