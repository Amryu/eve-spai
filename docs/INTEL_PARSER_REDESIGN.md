# Intel parser redesign — name resolution

Design note for review. No code changes yet. Captures the target architecture implied by
these principles:

1. **Plain text only.** Logs carry no `<url=>` markup (done — that machinery is removed).
2. **Don't go by capitalization.** Pilot name words can be lower case (`mixa kolodenko`,
   `bigfoott`). Case must never decide whether a token is a name.
3. **ESI is the source of truth.** The parser proposes candidates; ESI `/universe/ids`
   confirms which are real characters. Stop words exist only to discard candidates that are
   *entirely* common words — not to break names apart.
4. **Each token is parsed once.** A token that belongs to a confirmed pilot name is never
   also counted as a keyword, ship, system, or another pilot.
5. **Permutation claim.** A blob `A B C` is resolved by trying `A B C`, then `A B`+`C`, then
   `A`+`B C`, then `A`+`B`+`C`; claim the first/longest confirmed blob, then continue from the
   remainder + the next token.
6. **Safety bias on collisions.** *Every* token can be part of a name (there are pilots named
   `Rorqual`, `Tackled`, `Cyno`), but when a token also reads as a **ship class** or a **threat
   keyword** (tackled / point / scram / cyno / bubble / dread / hot-drop …), the entity/keyword
   interpretation **wins** over the pilot interpretation. Rationale: a pilot literally named
   `Rorqual Tackled` is vanishingly rare, whereas missing a real "rorqual tackled" sighting is a
   dangerous false-negative — better to misfire as the keyword group than to swallow it into a
   name. This is the one explicit exception to principle 4.
7. **Double-space is a hard separator.** An in-game paste separates distinct entities with a
   double space and never puts one *inside* a name. So `Rorqual  Tackled` (double space) is
   unambiguously two entities — a Rorqual hull + a Tackled keyword — never a two-word pilot.
   Double-spacing overrides blob-gluing and resolves most collisions cleanly when present.

## The core problem

Principles 2 and 4 can't both be satisfied at parse time, because **which tokens belong to a
pilot name is only known after ESI answers**, and ESI is asynchronous. Example:

- `Jita Trader` — if ESI confirms `Jita Trader` is a character, `Jita` is a *name* token.
- `Bob in Jita` — `Jita` is a *system*.

The two are indistinguishable by the parser alone (no capitalization tiebreaker allowed). The
only authority is ESI. So token *assignment* and everything that depends on it (system
detection, keyword suppression, the count) must happen **after** resolution, not during the
initial parse.

This is the shift: today the parser tries to produce final, clean pilots in one pass (leaning
on capitalization). The redesign splits it into three phases with ESI in the middle.

## Target architecture

### Phase A — Segment (synchronous, no ESI, no capitalization)

Tokenize the message body. A **double space is a hard boundary** (principle 7): no blob, name,
or entity ever spans it. Within a segment, classify each token as exactly one of:

- **Hard entity** — a hull in the ship index (incl. multi-word hulls and plural/typo forms), a
  structure, a wormhole code, a time token, a bare count, **or a threat keyword** (tackled /
  point / scram / cyno / bubble / dread …). By the safety bias (principle 6) these *win* a
  collision: their tokens are recorded as the entity/keyword and end a name run. They may still
  be enqueued to ESI as part of an adjacent span, but the keyword/ship flag fires regardless.
- **Ambiguous** — a system name or null-sec code that is **flanked by a name word** (`Bob
  Uitra`, `jita trader`). It is held as name-material and resolved by ESI; it becomes a system
  only if ESI rejects the name containing it (no positional rule — see Q2). A system/code token
  *not* adjacent to a name word (`hostiles in Jita`, `N3-JBX Uitra`) is a plain system and ends
  a name run.
- **Name material** — everything else, *including stop words and lower-case words*.

A **candidate blob** is a maximal run of {name-material ∪ ambiguous} tokens between hard
entities and double-space boundaries. A blob is dropped only if it is entirely lower-case stop
words (`gate is camped`). A whole name needs ≥3 letters total (not per word — `Bo Li`,
`Wolf E Kristjansson` are fine).

Output of Phase A: candidate blobs (with token offsets), hard entities, candidate systems.

> **Collision example.** `Rorqual  Tackled` → the double space splits it: `Rorqual` (hull) +
> `Tackled` (keyword), two entities, no pilot. `Rorqual Tackled` (single space) → both are
> threat/ship keywords, so the safety bias still resolves them to hull + keyword, not a pilot.
> `Cyno Toon online` → `Cyno` is a threat keyword (cyno flag fires) but `Cyno Toon` is also
> enqueued; if ESI confirms `Cyno Toon` as a character it is shown *in addition* — the keyword
> never gets swallowed.

### Phase B — Resolve (asynchronous, ESI)

For every blob, enqueue its 1–3 word spans (each whole span ≥3 chars). The background resolver
batches them through `/universe/ids` and caches verdicts:

- **character** → cache id (persisted, as today).
- **not a character** → cache a negative **with a 4 h TTL**, then re-check. (Fixes stale
  negatives that currently make a real name like `River Pixies` vanish permanently.)

`cover` performs the permutation claim (principle 5): at each position take the longest
*confirmed* span, claim it, advance past it; skip a span that resolved as a non-name; wait if a
longer span is still pending. No "refuse all-singles" guard — `A`+`B`+`C` is a valid result.

### Phase C — Assign & secondary parse (in the reconcile, after verdicts arrive)

1. Run `cover` on each blob → the confirmed pilot names. **Reserve** their token offsets.
2. Over the **unreserved** tokens only, parse:
   - **systems / gates** — a candidate system whose tokens weren't reserved by a name is a real
     system (`Bob in Jita` → Jita; `Jita Trader` → Jita reserved, no system).
   - **status keywords** — two classes:
     - *Soft* (clear / nv / status / eyes …): fire only from **unreserved** tokens, so
       `The Bubble Boy`-style names don't spoof them and a noise blob can't silence a real one.
     - *Threat* (tackled / point / scram / cyno / bubble / dread / hot-drop …): fire from the
       raw text **regardless of reservation** (principle 6 safety bias) — a confirmed pilot
       named `Cyno Toon` is shown *and* the cyno flag stands. This is the deliberate
       double-parse exception.
     No capitalization, no `strong-name-word` heuristic in either class.
   - **ships** not already matched as hard entities.
   - **count** — named pilots + leftover numbers.

This guarantees principle 4: every token is a name token **xor** a system/keyword/ship token,
decided by ESI, not by case.

## What each current pass becomes

| today | redesign |
| --- | --- |
| `extract_pilots` (Title-case runs) | **deleted** — capitalization-based; Phase A blobs replace it |
| `loose_pilot_runs` (blobs) | becomes Phase A segmentation (broadened to keep ambiguous systems/codes) |
| `numbered_names`, `lowercase_tail_names`, `lowercase_lead_system_names`, `lowercase_known_compound` | **mostly deleted** — these are capitalization/known-cache patches that Phase A+ESI subsume |
| `pilot_tokens` keyword suppression (parse-time) | **moved to Phase C** over unreserved tokens |
| system detection (parse-time, minus `pilot_tokens`) | **moved to Phase C** over unreserved tokens |
| `is_distinctive_name`, single-Title-token pass | folded into Phase A name-material rule |
| `cover` (app reconcile) | unchanged in spirit; gains the permutation/TTL details |
| `is_code_lookalike_name` (Luo-xi) | still useful for "is this token an ambiguous system or a name" |

## Transient behaviour (already accepted)

Until ESI answers, a blob shows an animated `…`.

**Decided (Q1): immediate-then-correct.** Status keywords are computed from the
unreserved-so-far tokens right away (so a real threat is never delayed); once ESI confirms a
name that reserves a *soft*-keyword token, that flag is recomputed/cleared on the next reconcile
pass. Threat keywords (principle 6) stand regardless. This matches the transient `…` for names —
the feed updates as verdicts arrive rather than blocking.

## Open questions for you

1. ~~**Status-flag timing**~~ — **decided: immediate-then-correct** (see Transient behaviour).
2. ~~**Systems shown before resolution**~~ — **decided (revised): a token is accepted as a
   system only once ESI confirms it is NOT part of a name.** There is *no* positional rule (the
   first system is not necessarily the location). A system/code token flanked by a name word
   (`Bob Uitra`, `jita trader`) is held as name-material and resolved by ESI; a system token not
   inside any candidate name (`hostiles in Jita`, `N3-JBX Uitra`) is accepted immediately. If
   ESI rejects the name blob, its system tokens are re-accepted as systems in the reconcile.
3. ~~**Offline tests**~~ — **decided: the current suite defines the expected outcomes**; keep
   those intents as the spec and adjust the test *mechanics* to the phase contract as needed
   (Phase A blob → `esi_resolve` → Phase C leftovers). Don't weaken what a test asserts is
   correct; only change how it's expressed.
4. ~~**Scope/sequencing**~~ — **decided: incremental, starting now.**
   - **Step 1 — Phase A:** rebuild segmentation into clean blobs (double-space hard boundary,
     ship/threat keywords as hard entities, no capitalization). Case-insensitive single-word
     candidates: **done**. NB: `extract_pilots` and the lowercase-lead/known-compound patches
     are still load-bearing for **names that contain a system** (`Bob Uitra`) — they can only be
     deleted *after* Phase C makes non-first systems ambiguous + reservable. Until then they
     stay. Double-space hard boundary: still TODO (needs masking to preserve double spaces).
   - **Step 2 — Phase B:** 4 h negative TTL in the resolver (done). `cover` does the
     permutation claim, but **keeps** the all-singles guard as a safety net — prose like
     `I Forgot Who` (three words each coincidentally a real player) must not explode into three
     pilots; a real multi-word name confirms as the longer span (with the TTL re-checking), so
     the guard only blocks the spurious case.
   - **Step 3 — Phase C:** move system + keyword + count parsing to the post-ESI reconcile over
     unreserved tokens; first-system-is-location; soft/threat keyword split.
   - Tests are ported to the phase contract as each step lands; the app stays working between
     steps.

## Not changing

- Log ingestion (`chatlog.rs` strips the `[time] Sender >` framing before `analyze`).
- zKill kill-intel, wormholes, alliances, structures-as-badges, ESS/ping parsing.
- The ESI resolver threading model and persisted known-pilot cache (gains the 4 h negative TTL).
