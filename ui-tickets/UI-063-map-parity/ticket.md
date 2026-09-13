# UI-063 &mdash; The in-app map is behind the web one, and the reactivation timer looks stuck

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `app.rs` map and route panel, `jumproute.rs`, `assets/map.js` |
| **Reported by** | user |

## Asks

1. "The jump route dialog in-app is missing features from the web version. Make sure the in-app map
   has the same route features as the web version. Make a full assessment, write it down, then see
   where the two maps differ and fix it."
2. "Also: verify if the calculation for the Jump reactivation timer is actually correct. It seems to
   always be 7min, 7min, 30min, 30min,..."
3. "Do not use an alert prompt window to get a name for the route to save."
4. "Pochven system can't be used in jump planning"
5. "Many console errors being thrown that seem related to painting on the map. Seems like a null
   pointer"

## The assessment

Feature by feature, web against app, before this ticket.

| | web | app |
|---|---|---|
| Drag from a system, snap, distance readout | yes | yes |
| Radial menu on drop | yes | yes |
| Context menu: start/restart, destination, waypoint, remove, clear | yes | yes |
| Context menu: avoid once / always, titan set | yes | yes |
| Kind switcher in the panel | no, it is the wheel | yes |
| Hull, JDC, JFC | yes | yes |
| Titan at start / reposition | yes | yes |
| Route via wormholes | yes | yes |
| Avoid list, named, removable | yes | yes |
| Per-leg alternatives switcher | yes | yes |
| Hop rows with costs and warnings | yes | yes |
| Warning opens the intel | yes | yes |
| Row actions menu | yes | yes |
| &nbsp;&nbsp;— "Other systems between…" | yes | **no** |
| &nbsp;&nbsp;— "Show info" | yes | **no** |
| Detour note ("2 jumps longer, avoiding") | yes | **no** |
| Titan jump line in the panel | yes | **no** |
| Save / load routes | yes | **no** |
| Map: waypoint rings | yes | **no** |
| Map: avoided systems crossed | yes | **no** |
| Map: pending-start ring | yes | **no** |
| Map: animated dashed route | yes | **no**, solid |
| Map: titan markers, titan jump line | yes | yes |

Eleven gaps, all on the app side, all in this ticket.
