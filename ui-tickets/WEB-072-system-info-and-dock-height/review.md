# Review

Tests: 702 passed, 0 failed (floor: 702).

Evidence in `after/`:

- `route-fills-dock.png` — 1400×1000, right-hand dock. "Save route" now sits at y≈967 against a dock
  bottom of 985. Before the fix it sat at 774, with 200px of dock empty below it.
- `system-desktop.png` — the system dialog docked, showing sov, ADM, the alliance logo, the traffic
  card coloured against the Delve average, the Blood Raiders rat profile, a scanned hole, a sov
  upgrade and the neighbour chips.
- `system-phone.png` — 420×900. The stat grid drops to two columns, the header wraps without
  overlapping and the sov chip moves onto its own line.

Checked by hand: the bookmark star renders `#4db6ac` when set (sampled from the PNG, not eyeballed),
and grey when not.

Not done, and deliberately: the dock keeps a fixed 45%/30% share rather than a draggable edge. A
resize handle was built and reverted — the complaint was that the list did not fill the box it had,
which it now does, and a new control is not what was asked for.
