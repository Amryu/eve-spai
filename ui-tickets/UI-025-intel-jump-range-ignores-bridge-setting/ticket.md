# UI-025 &mdash; Intel cards count jump bridges regardless of the setting, and the setting is buried

| | |
|---|---|
| **Severity** | High |
| **Status** | Fixed, see `review.md` |
| **Region** | `jumps_from_you`, `intel_row`, the intel toolbar |
| **Reported by** | user |
| **Run before** | UI-026, which builds on the same plumbing |

## What the user reported

Intel cards show jump ranges computed with jump bridges in mind. There is a bridge on/off option
somewhere, but it is not on the intel tab, which is confusing.

## What is actually wrong, which is worse

The setting exists, and **the intel card does not obey it.**

- `count_bridges` is a **per-alert-rule** field (`settings.rs:295`), default **false**, toggled in
  the alert rules editor at `app.rs:15349`. Its tooltip already states the real semantics: *"Count
  jump bridges in the range. Off = gate-only (how far a hostile, who can't use your bridges, really
  is)."*
- Alerts honour it: `app.rs:17080` calls `min_jumps_from(.., ru.count_bridges)`.
- **The intel card ignores it.** `jumps_from_you` (`app.rs:18109`) calls `sys.jumps(t, p, 50)`
  unconditionally, and `Systems::jumps` walks `adjacency`, which has bridges folded in via
  `add_bridges`. There is a `jumps_gates_only` using `gate_adjacency`, and the card never calls it.

So with the default settings, the same report can raise an alert computed at 7 gate jumps while its
card reads "3j". The number a user acts on is the one that assumes hostiles can use your bridges,
which is the assumption the alert tooltip explicitly warns against.

That is a correctness problem, not only a discoverability one.

## Decisions, made by the user

1. **Default: gate-only**, matching what alert rules already default to. This removes the
   card/alert mismatch and gives the accurate distance for a hostile who cannot use your bridges.
   **Existing numbers will get larger**, for example a card reading 3j today may read 7j. That is
   accepted and expected, not a regression to report.
2. **Scope: a toggle on the intel toolbar**, owning the feed's own setting, next to the existing
   jumps and severity controls. Alert rules keep their separate per-rule `count_bridges` flag.

The original framing of these as open questions is kept below for the record.

## Two things that were decided, originally open

1. **Scope.** `count_bridges` is per-rule. The intel feed is not per-rule. Does the feed get its own
   setting, or does it follow a global default, or does the per-rule flag get promoted? Do not
   silently invent a third source of truth.
2. **Default.** The alert default is gate-only, and the tooltip argues that is the honest reading for
   threat distance. If the feed adopts the same default, existing users will see numbers get larger.
   That is arguably correct but it is a visible change, so it should be a deliberate choice.

Pick one, state the reasoning, and flag it clearly for the user rather than burying it.

## Requirement from the user

The control must be easily visible and adjustable from the intel tab. The intel toolbar already
carries `<= jumps`, `outdated after`, a severity picker and the zKill range control, so it is the
obvious home. UI-004 reshaped that row recently; match its shape.

## How to verify

`view_intel` currently renders the "EVE chat logs not found" placeholder, so the intel toolbar is
**not covered by any permanent scene** (this is GAP-002, and UI-004 hit the same wall). Seeding
`chat_dir` is part of this ticket's cost.

`intel_row` scenes exist and cover the jump chip. Prove the chip's number changes with the setting,
by asserting both values rather than eyeballing one.
