# Review

Tests: 711 passed, 0 failed (floor: 708, +3).

The DM/room rule was extracted into `shows_in_dm_list` rather than tested through the widget, because
the bug was the shape of one boolean and that is what the tests now pin:

- a room stays out of the DM list **even while it is sticky**, which is the exact state the bug left
  behind;
- a closed-but-unread DM still comes back, which is what stickiness is for and what the narrowed
  rule must not break;
- a contact with no history is still a DM, since `dm_keys` only covers conversations that exist.

The egui harness passes all five scenes. `after/sidebar.png` is the Convos list.

Not covered by a test: the `push_id` scoping. It is insurance against a duplicate rather than a fix
for one — with the filter corrected, the duplicate cannot occur — and asserting on egui id
allocation would test egui rather than this.
