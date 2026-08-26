# UI-025 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-025-intel-bridge-setting`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 10.9 min across 1 round, 71 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 62/11 lines (added/removed), excluding the harness |
| **Harness code changed** | 119/1 lines |
| **Suite** | 449 to 452 passing |
| **Follow-ups** | partly closed GAP-002 |

## The change

`jumps_from_you` takes `use_bridges: bool` and picks `sys.jumps` or `sys.jumps_gates_only`, the
same branch `min_jumps_from` already used for alerts.

**It went wider than the ticket, correctly.** All nine call sites pass the setting, so the card chip,
the `<= jumps` filter, the dashboard's "Nearest hostile", the system-info panel, the rule feeds and
the alert overlay now read one number. Fixing only the chip would have replaced one inconsistency
with several. Threading it to the overlay needed one field each on `AlertConfig` and
`AlertWindowState`, set where those structs already sync.

`Settings::intel_count_bridges`, a new field with `#[serde(default)]` and `false` in `Default`. No
existing field changed type, which is the rule that matters here: a failed parse resets every
setting. A test parses a legacy `{"jabber_jid":"a@b"}` config and confirms it defaults to gate-only
without losing the other field.

## The decisions, as instructed

Gate-only default and a toolbar toggle, both as chosen. Alert rules keep their separate per-rule
`count_bridges`; the two were not merged.

## Copy

Checkbox **count jump bridges**, placed immediately after the `<= jumps` spinner inside the same
divider group, so the two jump-distance controls read as one unit.

> Count your jump bridges in the card distances and the ≤ jumps filter. Off = gate-only, how far a
> hostile, who can't use your bridges, really is.

It names both things the setting moves, since it drives the filter as well as the chip. The second
sentence is lifted from the alert rule tooltip, so a user who has seen either recognises the other.
Lowercase label matches its neighbours.

## Proof the setting bites

`fixtures::systems_bridged()` is the existing 1DQ1-A / 319-3D / 7-K5EL chain plus one bridge
1DQ1-A to 7-K5EL. Player in 1DQ1-A, hostile in 7-K5EL:

| | jumps |
|---|---|
| gate-only, the new default | **2j** |
| counting bridges | **1j** |

`uitest_intel_card_jumps_follow_the_bridge_setting` renders the whole app scene both ways and
asserts both chip labels. **I teeth-checked it myself** by reverting only the branch in
`jumps_from_you`: it fails with `left: ["1j"], right: ["2j"]`, which is precisely the under-report
the ticket describes.

## GAP-002 partly closed

`view_intel` renders for the first time. The unlock needed **no store seeding at all**, just
`chat_dir` pointed at a scratch dir plus `settings`, `intel_state` and `player` opened to
`pub(crate)`. Census went from the ~12-target chrome baseline to **24 hit targets**.

Two consequences worth recording:

- **UI-004's second call site is now covered.** That fix touched the intel toolbar's
  `kill_intel_range` and only the alerts one had a scene, so half of it rested on throwaway scenes.
  GAP-002 named it as a concrete consumer; it is closed.
- `dashboard_view` reads the same three fields, so seeding them there would populate it without
  touching the store. Map, Battles and Characters still need the store seam.

`chat_dir` is seeded only for `View::Intel`, because `battles_view` reads it too and seeding it
globally would delete the "configure intel channels" hint the battles scenes cover.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 449 passed, 4 ignored (+32) | **452 passed**, 4 ignored (+32) |
| with `--features fc-rescue` | 475 passed | 478 passed |
| `cargo test --bin eve-spai uitest` | 40 passed | 42 passed |
| `view_intel` census | ~12, chrome only | **24** |

`cargo check --workspace --all-targets --all-features`: only the pre-existing `unused_mut` at
`app/src/intel.rs:5605`.

## Screenshots

- `after/view_intel.png`: the real toolbar at last, not the chat-logs placeholder.
  `All Hostile Clear Kill Threat | <= jumps [any] [ ] count jump bridges | outdated after [300s] |
  Severity... | [x] zKill intel [within the intel feed's range] | Filter...`. The new checkbox sits
  unchecked between the jumps spinner and the divider, nothing overlapping or escaping.
- `after/view_intel_feed.png`: the same toolbar with a seeded card, chip reading **2j** beside a
  7-K5EL system chip. That is the gate-only default rendering in the real feed.

## Note for the user

Existing installs will see intel distances **increase** on first run with this build, since the
default flips to gate-only. That is the accepted consequence of the decision, not a regression. The
toggle restores the old numbers.
