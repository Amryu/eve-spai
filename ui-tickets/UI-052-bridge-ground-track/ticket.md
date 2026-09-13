# UI-052 &mdash; The in-app map draws a dashed line under every jump bridge arc

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `app.rs` map |
| **Reported by** | user |

## Symptom

"The in-app map shows both dashed lines AND a jump bridge arc. only the arc should be there"

## Cause

`arch_ground`, added with the arcs, drew a faint dashed track under each one on the theory that it
would read as height. On a map that already uses dashed for a regional gate and dotted for an
inter-constellation one, it reads as a third kind of connection instead.
