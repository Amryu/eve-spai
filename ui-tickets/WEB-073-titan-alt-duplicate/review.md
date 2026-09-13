# Review

Tests: 703 passed, 0 failed (floor: 702, +1 new).

`a_titan_leg_is_marked_so_its_options_are_not_offered_twice` builds its own seven-system line and two
titans rather than using `uitest::fixtures::systems()`. The shipped fixture is three systems wide and
cannot produce two titan options at all, so a test written against it would have passed without
exercising anything — the same trap that made `picking_a_leg_alternative_changes_the_route` vacuous
and got it deleted. The test asserts `out.len() == 2` first, so if the graph ever stops producing two
options it fails rather than going quiet.

No screenshot: the demo server's fixture graph cannot produce a titan route with two options, for the
reason above. Verified by the test and by reading the two call sites that render leg switchers
(`dialogs.js` `paintRoute`, `app.rs` route panel), both of which now skip a `whole_route` leg.
