# WEB-043 review cycle

**Status:** Delivered
**Branch:** `feat/web-043-route-warnings`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 700, unchanged |
| **Follow-ups** | none |

## The threshold

Danger and above, and nothing below it. A nullsec route passes through dozens of systems someone has
said something about; a warning on all of them is a warning on none. Kills come from ESI's hourly
figures, ships and pods counted separately, because a pod kill on a route is a different kind of news.

## Where it is attached

A pass over the finished routes rather than an argument to the three builders: what counts as
dangerous is a property of the moment, not of the path, and threading it through would put the same
lookup in three places.

Two ways in, one output. The browser's comes from the map pane's per-system intel and the status
pane's kill counts, both of which it already holds. The desktop's comes from the raw reports and the
severity rules, because the app has no published snapshot to read when the web view is switched off.
They meet at `danger_from_marks`, so the two clients agree on the threshold without either one owning
it.

The app's jump planner gets the same warnings as the route windows, from the same map.

## The grab

The drag handler released the pointer for a `button`, an `a` and an `input`. A drop-down is none of
those, so picking a hull dragged the window instead of opening the list. It now releases for anything
interactive.

## Mobile

The windows are a bottom sheet on a phone, so everything in them has to earn its height: the panel
scrolls rather than the lists inside it, the hull picker takes its own row, and a hop's costs and its
warning ride on the same line as the hop instead of on two lines under it. The ship window's rows,
resist bars and hull render all come down a size.
