# UI-075 — an expired EVE login says nothing, and a missing keychain says the wrong thing

> "Hey, is it known issue thay currently Eve Spai map does not show players position? It did work
> just fine like few days ago, but not anymore"
> "Ahh, seems like it has just quietly expired the token in the background... Might be good idea to
> warn user about it"
>
> "Login failed: writing refresh token to keychain: Could not access platform storage. SS error:
> result not returned fron SS API: EE error: result not returned fron SS API"
> "so, currently unusable at this state"
> "Above could be resolved by installing for example KeepassXC and using it to manage secrets."
> — both reported in chat

## The silence

`esi::current_access_token` ended in `auth::refresh_access_token(...).ok()?`. Every failure became
`None`, and `None` is also the ordinary answer for "no token right now", so a refresh token EVE SSO
had permanently rejected was indistinguishable from a network blip. Nothing logged, nothing warned;
the symptom was the map no longer showing where you are.

The settings list did show "token valid"/"token expired", but off `c.expires_at`, which is the
**access** token's twenty-minute expiry. That is routinely in the past between refreshes, so it read
"expired" for perfectly healthy characters and said nothing about the one that was actually dead.

Now: `RefreshError` splits SSO's answer into `Rejected` (4xx — a logout that has already happened,
only a new login fixes it) and `Transient` (5xx, timeouts — retry in silence). A rejection records
an `AuthProblem` against the character, and a banner above every view names it and offers
**Log in again**. No dismiss: it does not clear itself. The settings row now reads the real state.

## The keychain

`Error::NoResult` out of the Secret Service crate means the D-Bus service answered but has **no
collection at all** — not that it is locked. The message surfaced as `result not returned fron SS
API`, nested twice, which reads like a bug in this app, and the reporter had to work out on his own
that installing a provider fixed it.

`tokens::try_load_refresh` now keeps "no token saved for this character" apart from "the keychain
could not be reached", and both the login error and the banner say what is actually wrong and what
to do: start GNOME Keyring or KWallet's Secret Service module, or install a provider such as
KeePassXC, and make sure a keyring exists and is unlocked.

**No fallback was added.** `store_character` still refuses to persist anything if the keychain write
fails, by design — the comment there says it does not silently fall back to plaintext, and the
refresh token grants access to the player's EVE account. Making the failure legible is a fix;
quietly writing the token to disk instead would be a different decision, and the user's to make.
