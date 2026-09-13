# Review

Tests: 708 passed, 0 failed. Two new here (`only_a_refusal_from_sso_counts_as_a_lost_login`,
`a_rejection_carries_what_sso_said`) and three in UI-074, which shares this branch.

The classification test is the one that matters: it pins 400/401/403 as a lost login and
500/502/503/504 as transient. Getting that backwards either cries wolf on every SSO hiccup or stays
silent when a login is gone, and the silence is what this ticket exists to fix.

`after/auth_banner_expired.png` and `after/auth_banner_keychain_narrow.png` come from two new harness
scenes. The narrow one is deliberate: the keychain remedy is three sentences, and it has to wrap
rather than clip or push the nav rail down. The harness checks both scenes for overlap and for
controls outside the window, and passes.

Not verified end to end: neither failure can be reproduced on this machine without breaking a real
login or the real keyring, which `safe-verification` rules out. What is verified is the decision that
turns a failure into a warning, plus the rendering of the warning itself. The path from
`refresh_access_token` to `note_problem` is four lines and was read, not exercised.

Left open, for the user to decide: `store_character` still aborts rather than falling back when the
keychain is unavailable, so on a machine with no Secret Service provider the app remains unusable for
ESI features — legibly so now, rather than mysteriously. A fallback would mean writing an EVE account
credential somewhere less safe than the keychain.
