# UI-077 — an encrypted, account-bound fallback when there is no OS keychain

> "As for the fallback: Figure out a properly secured fallback, where we can store the key in an
> encrypted manner, tied to the users account. Just try to make this platform-independent. We still
> do not allow unsecured plaintext."

Follows UI-075, where a user reported the app unusable on a machine with no Secret Service provider:
`store_character` refused to persist anything it could not put in the keychain, so the login never
completed and every ESI feature stayed dark.

## Design

`app/src/sealed.rs`. The OS keychain is still tried first, always; this is only ever reached after it
has actually failed, and the token moves back on its own the moment a keychain appears.

| | |
|---|---|
| **Cipher** | ChaCha20-Poly1305 (AEAD), from `ring` |
| **Key** | PBKDF2-HMAC-SHA256, 200k rounds, over the account binding, with a per-install random salt |
| **Binding** | username, home directory, hostname, and `/etc/machine-id` where it exists |
| **AAD** | the character id, so a blob cannot be moved onto another character |
| **Nonce** | 96 bits, fresh per write |
| **File** | `<data dir>/tokens.sealed`, mode 0600, written to a temp file and renamed |

`ring` was chosen because it is **already compiled** as rustls's crypto provider, so taking it
directly costs no build time, and because nothing here should be hand-rolled. Platform-specific code
is limited to reading `/etc/machine-id` when present and setting a Unix file mode; everything else is
portable, and a platform missing a binding component simply gets a less specific one — the salt
carries per-install uniqueness regardless.

## What it protects, stated honestly

The file alone is worthless: copied to another machine, another user, a backup, a synced folder or a
disk image, it will not open. That is what "tied to the user's account" buys and it is real.

It does **not** protect against code running as that user. Any key the app can derive unattended, a
program running as the same account can derive too. That is the ceiling for any unattended secret,
and an unlocked OS keychain sits at exactly the same ceiling — it is not a weakness of this design
against that baseline. The settings line and its tooltip say this plainly rather than implying more.

A passphrase would raise that ceiling, at the cost of typing it at every start. Not built: it is not
what "tied to the user's account" means, and it is a UX decision rather than an implementation one.
