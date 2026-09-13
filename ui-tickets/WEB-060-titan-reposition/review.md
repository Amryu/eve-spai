# WEB-060 review cycle

**Status:** Fixed
**Branch:** `fix/web-060-titan-reposition`

## The objective

The fleet's gate count, which is the thing anyone is trying to reduce. Every system the titan can reach
that the fleet can also gate to is scored as "gates out to meet it" plus "gates in after the bridge",
and the best score wins. A titan that hops one system over has moved and helped nobody.

Two distance balls carry it: gates from the start, and gates from the destination, which on an
undirected graph is also gates *to* the destination. After that a landing costs two lookups and one
scan for the system the bridge should land on, rather than a search per candidate.

Strictly better than not repositioning, which is what "saves at least one jump" means. A reposition
that ties is a titan cycling its drive for nothing, so those are dropped rather than offered.

The reach for the fleet went from six gates to eight, because the search can now afford to look and
the old cap was there to keep a bad search small.

## The line

Its own colour, and running the right way. A positive `lineDashOffset` runs dashes backwards, so it
read as the titan jumping to where it already was; sharing the capital-jump colour said the two were
the same move, when one is a different ship going the other way.
