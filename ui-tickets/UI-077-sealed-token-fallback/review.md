# Review

Tests: 719 passed, 0 failed (floor: 711, +8). Run twice to check the new tests do not flake.

The eight tests assert the properties that make this a fallback rather than obfuscation, since
"encrypted" is easy to claim:

- a round trip returns the token, and a character with nothing sealed is `None` rather than an error
  — that is how the caller tells "never logged in" from "would not open";
- **the file contains neither the token nor base64 of it**, which is the difference between
  encryption and an encoding pretending to be one;
- **a vault from another account does not open**, the property the whole design rests on;
- a sealed blob moved onto another character id inside the file is refused (the AAD);
- a flipped bit is refused rather than yielding rubbish;
- deleting the last token removes the file instead of leaving an empty vault reporting itself in use;
- two seals of the same value differ in nonce and ciphertext — a repeated nonce is the classic way an
  AEAD stops protecting anything;
- the file is owner-only on platforms with file modes.

The tests override the vault directory and the account name **per thread**, not through
`EVE_SPAI_DATA_DIR` and `USER`. Both are process-wide and read concurrently by the rest of this
binary, so setting them would have moved another test's profile out from under it. `in_use()`'s cache
is bypassed under `cfg(test)` for the same reason: a process-wide cache cannot represent a per-thread
vault.

Not verified end to end: the glue in `tokens.rs` that reaches the fallback only after the keychain
fails. Breaking the real keyring or writing a fake character into it are both out of bounds under
`safe-verification`, and there is no seam to inject a failing keychain without restructuring
`keyring::Entry` behind a trait. The decision path is about fifteen lines and was read, not exercised.
What is exercised is every property of the store it falls back to.

Also fixed while here: `in_use()` was a filesystem read per frame from the settings view, on the
common machine that has a working keychain and no vault at all.
