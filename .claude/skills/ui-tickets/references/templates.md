# ticket.md and review.md

## ticket.md

Written before the fix, and never rewritten to match what got built. If the ticket turns out to be
wrong, the correction goes in `review.md` and the ticket gets a `Status` pointer to it. Seeing what
was believed at the time is the point.

```markdown
# UI-NNN &mdash; one line, the defect not the fix

| | |
|---|---|
| **Severity** | Critical / High / Medium / Low |
| **Status** | Open |
| **Region** | the function or view that owns it |
| **Reported by** | harness / user / spun off UI-NNN |

## Symptom

What is observable, in the terms someone using the app would describe it. No cause here.

## Measured

Numbers from the AccessKit tree or the census, in a table when there is more than one. A ticket
that says "too cramped" is an opinion; a ticket that says "1092px of controls in a 1280px row,
leaving 100px for the search field" is a bug report. Measure before claiming.

## Cause

`app/src/app.rs:NNNN`, with the mechanism. If it is an egui behaviour rather than a mistake in this
repo, say which one, because the fixer will otherwise rediscover it.

## Notes

Anything a fixer would otherwise have to work out from scratch: adjacent tickets that touch the same
lines, an egui trap that applies, why the obvious fix does not work.

## How to verify

The scene and the command. State what would make the fix WRONG, not only what would make it right.
That is the sentence that stops an agent from satisfying the assertion by deleting the widget.
```

Severity, as used here: **Critical** = wrong information or an unrecoverable state (a spinner with
no exit, a jump count computed against the wrong setting). **High** = a control that cannot be hit
or read. **Medium** = layout that misleads or wastes the user's attention. **Low** = polish.

## review.md

Opens with the Resolution table, then the narrative.

```markdown
# UI-NNN review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-nnn-slug`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed / Fixed differently than specified / Closed, ticket was wrong |
| **Agent time** | N min across N rounds, N tool calls |
| **Patches rejected on review** | N |
| **App code changed** | added/removed lines, excluding the harness |
| **Harness code changed** | added/removed lines |
| **Suite** | before to after |
| **Follow-ups** | UI-NNN, or none |

Add a row per measured outcome when there is one: "Controls at 1280px | 1135px to 888px".
```

Then, as prose:

- What changed and why that approach.
- What was rejected and why. An option declined with a measurement behind it is worth more than the
  fix itself, because it stops the next round relitigating it.
- How the tests were proven to have teeth. Name the line that was reverted and the assertion that
  then failed.
- What the screenshots show, described by someone who actually looked at them.
- Residual risk and known limits.
- If the ticket was wrong, a heading saying so, and what the correct reading is.

Record effort honestly even when it is embarrassing. A two-line fix that took twelve minutes and 165
lines of test is useful information about where cost actually goes.

## README.md index

One row per ticket: id, title, severity, status, branch. Plus the wave schedule (which pairs ran
together) and running cost totals. Update it in the same commit that lands the ticket, not later.
