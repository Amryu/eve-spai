# UI-043 review cycle

**Status:** Fixed and verified
**Branch:** `ui-043-chat-timestamps-need-seconds`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | one session, no fix agent dispatched (one function, two call sites) |
| **Patches rejected on review** | 0 |
| **App code changed** | +3 / -2, plus 47 lines of test module |
| **Harness code changed** | +88 (the opsec guard, two scenes, one test) |
| **Suite** | 571 to 577 |
| **Timestamp resolution** | 60s to 1s |
| **Follow-ups** | UI-044 |

## What changed

`eve_time_label` gained `%S` in both of its branches. Both windows read that one helper, so
the Jabber history and the Rescue chat cannot drift apart, and the older-than-today form keeps
its date.

That is the whole fix. The rest of this ticket is the harness work needed to look at it.

## The screenshot rule this ticket added

The user, mid-ticket: renders must never use real chat data, because some of what passes
through these windows is operational.

The harness already pointed at a scratch profile, and `SpaiApp::build` already refused to open
a store headlessly without `EVE_SPAI_DATA_DIR`. Both are redirects, not guards. They rely on
every scene calling `scratch_profile()`, and the redirect was `Once`-gated, so nothing would
have caught the variable being set back to a live path afterwards.

`harness::assert_no_live_profile` now fails the test unless the profile resolves to
`target/uitest-profile`, checked through `store::data_dir()` rather than the raw variable so it
proves the choke point honours the override. It runs in `harness::build`, and again in
`harness::shot`, which is the last moment before a PNG exists on disk. `scratch_profile` lost
its `Once` and re-asserts the redirect on every call.

The rule is written into CLAUDE.md next to the harness notes, and the incident into the
ui-tickets skill's `references/lessons.md`, per the skill's own instruction to land the rule
with the work that taught it.

## Rejected

Testing the guard by setting `EVE_SPAI_DATA_DIR` to a live path and catching the panic. Written,
then thrown away: `cargo test` runs this binary's tests in parallel threads in one process, so
that test writes a variable every other scene reads. It would have flaked, and it could have
made an unrelated scene fail or pass for the wrong reason. The logic is now a pure
`profile_objection(got, want)` and the test calls that with no environment mutation at all.

Giving every message its own timestamp rather than one per group. Consecutive messages from one
sender inside five minutes share a header, so second-accuracy currently lands on group heads.
Changing that is a visible density change nobody asked for, and it is called out in the ticket
Notes rather than assumed.

Fixing the 9.5px label size while in the file. Filed as UI-044 instead, to keep this patch to
the format change.

## Teeth

`before/tests-fail-without-the-fix.txt` is the suite with `%S` removed from both branches,
tests left in place. Three of five fail: `same_day_carries_seconds`,
`an_older_message_keeps_its_date_and_gains_seconds`, and
`the_jabber_and_rescue_windows_share_one_format`.

That run also caught a bad test of my own. `two_messages_in_one_minute_are_distinguishable`
passed *without* the fix, because 12:02:35 plus 30 seconds is 12:03 and the two labels differed
under the old format too. It asserted nothing. It now steps 10 seconds and asserts up front that
both stamps really are inside 12:02, so the inequality is load-bearing. This is the second time
a test has passed vacuously in this repo; the reverting run is what exposed it, which is the
argument for doing the teeth check rather than assuming.

`uitest_a_live_profile_is_refused` covers the guard: an unset override and a live path are both
refused, the scratch path is accepted.

## Screenshots

`before/` and `after/` for `jabber_popout_stamps` and `rescue_chat_stamps`, the same two scenes
rendered either side of the change. Before: `EVE 13:48`, `EVE 13:49`, `EVE 13:50`, three
messages that could be anywhere in a three-minute window. After: `EVE 13:48:35`, `EVE 13:49:36`,
`EVE 13:50:03`, which is enough to reconstruct what happened when.

`rescue_chat_stamps` is new. GAP-009 left the Rescue window without a scene, so its chat had
never been photographed; this renders the feed directly, the way the UI-028 probe drives it,
rather than the window around it. Both scenes use fixture traffic written for the purpose.

## Residual risk

Grouped messages still show one timestamp per group, so a burst inside five minutes is stamped
only at its head. That is the pre-existing grouping rule, unchanged here, and it limits how much
of the new resolution is actually visible in a busy channel.

The guard protects renders from this harness. It does nothing about a screenshot taken by hand
from the running app, which is the other way a real room name reaches a ticket folder.
