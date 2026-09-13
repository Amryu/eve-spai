# Review

Tests: 708 passed, 0 failed (floor: 703, +3 for the MOTD helpers, +2 for UI-075 in the same branch).

The egui harness passes all five screenshot scenes, which is what checks the new title-bar row for
overlapping text and controls pushed outside the window. `after/app-title-bar.png` is the pop-out at
its narrow default: the topic truncates and the mute bell stays on the bar, which was the risk of
putting a paragraph next to the room name.

`after/web-title-bar.png` and `after/web-phone.png` are the same bar in the browser at 1400 and 420.

Three unit tests pin the collapsing rules: the rule-of-dashes and blank lines are dropped from the
one-line form, the preview caps at N lines and marks that it did, and an empty MOTD collapses to
nothing rather than to a bare separator with a dead button beside it.

Two silent-failure traps in `webshot.sh` were fixed on the way, both of which had already wasted
time: it copied out the previous run's staged PNG when a shot failed (so a screenshot could show the
wrong page with no error), and it reused a demo server left running from an earlier session (so an
asset change screenshotted as if it had never been made — see WEB-072).
