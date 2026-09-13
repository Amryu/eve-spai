# WEB-025 &mdash; Map text and markers stay sidebar-sized on a full-screen map

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js` |
| **Reported by** | user |

## Symptom

"I am looking at the full screen map view (in tab mode). System names and icons just seem tiny there
in comparison. System names and icons should probably grow to a certain degree, without causing
overlaps."

## Cause

Names, markers and the ADM and range readouts were all fixed pixel sizes, chosen to work in a
column. The same map at several times the area labelled itself as though it were still a sidebar.

Growing them without more is not enough on its own: bigger names collide more, and system names were
drawn unconditionally with no collision test, unlike the region labels.

## How to verify

The map in tabs mode at a large window against the same map in a four-column layout: text and markers
should be visibly larger in the first and unchanged in the second. A fix would be WRONG if it scaled
text without handling the overlaps that causes.
