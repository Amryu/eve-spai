# UI-026 &mdash; Show when an intel jump range depends on a jump bridge

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `intel_row` jump chip |
| **Reported by** | user |
| **Depends on** | UI-025 |

## The request

When an intel report's jump range comes from a route that uses a jump bridge, colour it differently,
show a jump bridge icon, and explain it in a tooltip.

## Detection is cheap, no path reconstruction needed

`Systems` keeps two adjacency maps: `adjacency`, which has bridges folded in by `add_bridges`, and
`gate_adjacency`, which is gates only. So:

```
bridged = jumps(from, to)  <  jumps_gates_only(from, to)      // a bridge shortened the route
       || jumps_gates_only(from, to).is_none()                // gates alone cannot reach it
```

Two BFS runs instead of one, on a graph already in memory, bounded by the existing 50-jump cap. That
is the whole detection. `Systems::is_bridge(a, b)` exists too but is not needed for this.

Watch the second case: if gates alone cannot reach the target at all, the bridged number is not
"shorter", it is the only answer. The UI should say something different there, since "2j via bridge"
when the gate route is impossible is a materially different situation from "2j via bridge, 9j by
gate".

## The three parts

1. **Colour.** Distinct from the normal jump colour, which is `theme::standing::CORP`. Do not reuse
   the hostile or warning colours; this is informational, not a threat level.
2. **Icon.** Grep `egui-phosphor` and confirm the glyph exists before using it, or it renders as a
   tofu square. The map overlay already uses `ARROWS_LEFT_RIGHT` for jump bridges (`app.rs:10827`);
   matching it keeps the vocabulary consistent.
3. **Tooltip.** Say what the number means and why it is flagged. The alert rule tooltip is the model
   for tone: *"Off = gate-only (how far a hostile, who can't use your bridges, really is)."* A
   hostile cannot use your bridges, so a bridged range understates how far away they effectively are.

## Constraints

- The jump chip is `{jtxt:>4}` monospace and shares a column down a stack of cards (see UI-002).
  Adding an icon must not break that alignment or reintroduce the dead-space bug UI-002 fixed.
- The chip is drawn per card in a virtualized feed, so two BFS runs per card per frame is the cost.
  Measure it against a full feed before assuming it is free, and cache per report if it is not.

## How to verify

Add a fixture where the player and the target are connected by a bridge, and one where they are
connected only by gates, and assert the chip differs. `fixtures::systems()` builds a small graph
already, and `Systems::add_bridges` is public.
