# UI-056 review cycle

**Status:** Delivered
**Branch:** `feat/ui-056-titan-systems`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 702 to 703, one new |
| **Follow-ups** | saving and loading routes is still open |

## Where the titans are

A list, per route, not a setting. Which ships are where is a fact about the operation being planned,
not about the installation, so it lives and dies with the route and the client carries it the way it
carries the avoid list.

Named titans beat both guesses. "Titan is in the starting system" is only ever a guess about where one
might be; once the user has said where they actually are, that is the answer. Each titan that helps
becomes an option, best first, and one that does not help is left out rather than listed as a worse
choice. When none of them help the answer is the gate route, saying so, which is the honest reply to
"is a titan any use here".

They are marked on the map whether or not the route goes near them, because that is the question being
asked and an unused titan is still an answer.

## Repositioning

With the titan at the start, it may jump first and have the fleet gate out to meet it, then bridge
from there. Two searches deep: where it can usefully land, and from each landing where it can throw
the fleet. Bounded by keeping only landings within six gates of the start, since a titan that lands
somewhere the fleet cannot reach quickly has helped nobody, and by trying the twelve nearest, since
each costs a second search.

The titan's jump is carried beside the route rather than in it: the fleet does not fly that leg, so it
is not a hop. Both maps draw it as a long dash running the other way, so it reads as a second ship
moving.

## One button per row

An actions menu instead of one button per action. A row is a system and a distance; three buttons
beside that is more chrome than content. What it offers depends on what the row is: intel only where
there is intel, avoidance only off the anchors, alternatives only on a jump route's middle hops,
titan only on a titan route.

## Alternatives

`jumproute::alternatives` already knew which systems are in range of both neighbours. Picking one
inserts it as a waypoint rather than replacing anything, which is what makes it a steer rather than a
different route.

## Zarzakh

It is null sec, so the security test let it through, and nothing jumps into or out of it. `jumpable`
replaces the bare security check everywhere, and `zarzakh_is_never_jumped_through` pins it: an eight
light year gap whose only midpoint is Zarzakh has no route at all rather than an unflyable one.

## Ansiblex

"bridge" next to a titan bridge was going to be read wrong exactly once, at the worst moment.
