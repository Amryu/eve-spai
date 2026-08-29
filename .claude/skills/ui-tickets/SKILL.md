---
name: ui-tickets
description: The eve-spai UI defect workflow. Use when finding, filing, fixing, reviewing or landing a UI issue in this repo, when auditing the UI with the headless harness, or when delegating UI fixes to agents. Covers ticket format, worktree and agent rules, verification, the merge convention, and the resolution report.
---

# UI ticket cycle

Every UI defect gets a folder, a before screenshot, one branch, one merge commit and a written
resolution report. The point is that a year from now the ticket says what was believed at the time,
the review says what was actually true, and `git revert -m 1` backs the whole thing out.

Read `CLAUDE.md` for the harness commands. Read `references/` here as needed, not upfront:

| File | Read it when |
| --- | --- |
| `references/templates.md` | writing a `ticket.md` or `review.md` |
| `references/agent-brief.md` | dispatching a fix agent |
| `references/harness.md` | writing a scene, or a check keeps missing something |
| `references/lessons.md` | something just went wrong, or a rule below looks arbitrary |

## Layout

```
ui-tickets/UI-NNN-slug/
  ticket.md     the problem, written before any fix exists
  before/       screenshots showing the defect
  after/        screenshots showing it gone
  review.md     the resolution report, written when it lands
ui-tickets/README.md   the index, status per ticket, wave schedule, cost totals
```

`UI-NNN` for app defects, `GAP-NNN` for things the harness cannot reach. Numbers are never reused,
including for tickets closed as false positives. Next number: read the directory, do not guess.

## The cycle

1. **Ticket.** Reproduce first. A ticket without a `before/` screenshot containing the actual defect
   is not ready. Size the scene to its whole subject: a scene that crops what it is meant to show is
   worse than no scene, because it reads as coverage.
2. **Fix**, one agent per ticket, in a worktree: `git worktree add --detach <path> main`. If the tree
   is dirty, commit first and seed from that. Tell the agent the region it owns and the regions it
   must not touch, by function name. See `references/agent-brief.md`.
3. **Review the patch before applying it.** Does it address the cause or the symptom? Does it stay in
   its region? Does every comment justify a WHY? Verify the agent's load-bearing claims in the source
   yourself. Several have been subtly wrong, and several have been right in a way that corrected the
   ticket.
4. **Verify.** Apply to a branch, run the suite, re-render, look at the PNG next to `before/`.
   Confirm any new test actually fails without the fix.
5. **Land.** One branch per ticket, merged `--no-ff`, so code, `review.md` and `after/` arrive in one
   revertible merge commit.
6. **Report** in `review.md`, including what it cost.

## Rules that have earned their place

Each of these exists because something failed once. `references/lessons.md` has the incidents.

**Parallelism**
- At most 2 agents at once, and never two whose fixes touch the same region. Most of the UI is in one
  file, so pair by function: `intel_row`, `render_ping`, `battles_view`, `settings_view`, the alert
  viewport callback, the jabber tab bar, the composer, `nav.rs`. Cross-cutting changes run alone.
- Two agents paired by region still collide in `app/src/uitest/scenes.rs`, because every ticket adds
  a test there. Expect `git apply -3` on the second patch of a wave, and check the resolve: a
  conflict boundary can truncate a test mid-function, which surfaces as an unclosed delimiter rather
  than a quietly dropped assertion.
- Apply patches one at a time and re-run the suite between them.
- Stage by path. `git add -A` while a ticket folder is untracked sweeps the code in under the
  ticket's commit message, so the history has no commit describing the fix. Done twice.

**Test counts**
- Measure the count when you write the brief. Never copy it from the previous brief. Quoted floors go
  stale every time a ticket lands.
- Quote it as a FLOOR, never an exact number. Told "must be green (11 passed)", an agent reads it as
  "must not change" and skips the regression test the ticket most needs.

**Recovery**
- **Never discard uncommitted work to recover from a mistake.** The safety classifier blocks
  `git stash drop`, `git stash push`, `git apply -R` and `git checkout -- <files>`, correctly. To
  recover a clobbered worktree, capture every agent's work as a patch, make a FRESH worktree, apply
  the patch there. That discards nothing, so nothing is blocked, and the damaged worktree can be
  abandoned.
- **Never use `git stash` in a worktree.** There is one `refs/stash` in the common `.git`, shared by
  every worktree, so two agents stashing concurrently swap each other's files. To compare against a
  baseline use `git show HEAD:path > /tmp/base.rs` or copy the file aside. Applies to the main tree
  too while any agent is running.

**Verification**
- A fix is not done until a screenshot shows it fixed. The exception is an interaction the harness
  cannot reach cheaply, drag-and-drop being the known one: seed the state and render the result
  rather than simulating the input, and if even that fights back, land the fix and record what is
  uncovered. Verification effort is meant to be proportionate, not total.
- A test that reads a `#[cfg(test)]` hook cannot be teeth-checked by reverting the whole file,
  because the hook goes with it. Revert only the behaviour under test. Reverting a matching *string*
  is not the same thing: an identical line elsewhere absorbs the edit and the check passes vacuously.
- `cargo test` takes ONE filter. Passing two silently matches nothing and reports success.
- Screenshots land in the worktree's own `target/uishots`, because the harness derives that path from
  `CARGO_MANIFEST_DIR`. Reading the main tree's PNGs after an agent runs gives a stale answer.
- `cargo test --workspace` does not compile the `fc-rescue` feature. Rescue tickets need
  `--features fc-rescue`.

**Scope**
- A fix that spawns a new ticket is a good fix, not a failed one. Three of the first fifteen found
  defects the ticket never mentioned.
- A ticket the user overrules is not a failure either. Record why the original reasoning was wrong,
  in the review, under its own heading. UI-003 is the worked example.

## Improving the process

This skill is meant to change. When a round teaches something, add the rule here and the incident to
`references/lessons.md`, in the same commit as the work that taught it, and say in the commit message
what went wrong.
