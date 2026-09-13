# WEB-027 &mdash; Easing, AU units, hidden panes, mobile tabs, and the message behind a card

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/assets/*` |
| **Reported by** | user |

## Symptoms

1. "The easing is a bit too rough, give it a bit more time. Also can you use a more natural looking
   easing function?"
2. "Alerts in web view: The conversion to AU seems to be way off. Verify that it is done the same way
   as in the app."
3. "Clicking alert cards should reveal the original messages, just like in the app"
4. "Turning off views in the web does not remove them from the grid. If an uneven amount of views is
   selected, the map should claim 2 columns first (so it becomes full width)."
5. "Make the view buttons at the top on mobile equal width and take 100% of the viewport width"

## Cause

1. WEB-026 used exponential decay over 80ms: all of its speed in the first frame, then an asymptote.

2. `near_celestial` carries **metres**, and the page read the number as kilometres. Every reading was
   a thousand times too far, which is why the AU figures looked absurd. The app also stops showing the
   chip past 15,000 km; the page had no such ceiling.

3. Never implemented on the page. The app toggles the raw message on a click anywhere on the card
   that a badge did not consume.

4. Both grid and columns give `.pane` `display: flex`, and an author rule beats the UA's
   `[hidden] { display: none }`, so `el.hidden = true` did nothing visible. Same class of bug as the
   dialog that could not be closed.

5. The tab strip was `flex: 0 0 auto` with `overflow-x: auto`, so the last pane sat off the edge of
   the screen with nothing to say it was there.

## How to verify

2 is the one with an objective answer: the app's own code divides by 1000 and groups thousands, so
the same report must read the same in both. A fix would be WRONG if it only rescaled the AU threshold.
