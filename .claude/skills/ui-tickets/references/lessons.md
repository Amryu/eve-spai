# Incident log

Every rule in `SKILL.md` traces to one of these. Kept so the rules can be argued with rather than
just obeyed, and so the same mistake is recognisable the second time.

Add to this file in the same commit as the work that taught the lesson.

## Process

**An agent skipped the regression test to protect a number.** The brief said "must be green (11
passed)". It read that as "must not change" and shipped the fix with no test. Test counts are floors
now, and the word FLOOR is in the brief.

**Two briefs quoted a stale test count, off by 3 both times.** Both agents caught it and had to stop
and work out whether they had broken something. Measure the count when writing the brief; never copy
it from the previous one.

**A three-way apply truncated a test mid-function.** Two agents paired by region still both edited
`scenes.rs`, because every ticket adds a test there. The conflict boundary cut a test in half, which
showed up as an unclosed delimiter rather than a missing assertion. Check the resolve by eye.

**Two `before/` screenshots did not contain their own bug.** One scene never rendered the chip; the
other put the footer 350px below the frame. Both were caught by the fixing agent, not the review. A
scene that crops its subject reads as coverage while providing none.

**Reviews recorded what changed but not what it cost.** Added the Resolution table. A two-line fix
that took twelve minutes and 165 lines of test is the interesting data point.

## Git

**Two concurrent agents swapped each other's `app.rs` through the shared stash ref.** There is one
`refs/stash` in the common `.git`, shared by every worktree. One agent's file landed in the other's
worktree and the second agent's work was consumed. Recovered by capturing patches and building a
fresh worktree. `git stash` is banned in worktrees.

**Six recovery commands denied in a row.** The obvious repair for the above, `git checkout -- <files>`
then reapply, plus `git apply -R` and `git stash drop`, are all blocked by the safety classifier
because they discard uncommitted work. The classifier was right. Recover by ADDING: capture a patch,
make a fresh worktree, apply there, abandon the damaged one. Nothing is discarded so nothing is
blocked.

Also: when a command is reported blocked, grep the agent transcript before answering. Guessing which
command it was produced a confidently wrong answer and cost the user three prompts to correct.

## Verification

**Two filters passed to `cargo test`.** It takes one. The second was silently ignored, nothing
matched, and it reported success.

**A patch landed on the wrong line.** The target string appeared identically elsewhere in the file,
and the edit was absorbed by a different ticket's row, reverting that ticket instead. Match on
enough context to be unique, and check what actually changed.

**A teeth-check passed vacuously.** The test read a `#[cfg(test)]` hook, and the check reverted the
whole file, which took the hook with it. Revert only the behaviour under test.

**A shell `&&` chain committed even though the edit inside it failed its own assertion.** The
"landed" output was from the commit, not the edit. Check the file, not the exit status.

## Claims that were wrong

Mine, each corrected by an agent or the user, each recorded in the relevant review:

- UI-022's headline blamed a per-frame clone. It was ~1.3% of the cost. The real cause was missing
  virtualization.
- UI-018 claimed a third caller in the jabber renderer. No such caller ever existed.
- UI-008 listed the alert countdown as a `.small()` offender. It is `.weak()` only.
- GAP-006 said expiry issues `Visible(false)`. Windows-only.
- UI-020's review concluded the dialog family was fine. It was not, and became UI-033.
- UI-003 argued a fleet call's description should outrank its own metadata rows. The user reverted
  it. The contrast measurement was correct and the conclusion drawn from it was not, because the
  card it was compared against has no metadata rows and therefore no shared hierarchy.

The pattern: a measurement can be right and the inference from it wrong. State the mechanism in the
ticket so the fixer can disagree with it.

## Ways the app broke that no ticket predicted

Found while looking for something else, which is the argument for rendering scenes wider than the
ticket needs:

- `brview.rs:439` skipped the loop while battles were disabled, so the spinner never cleared. A
  production hang.
- A wormhole table 89px wider than the app's own minimum window.
- A toolbar running 511px past a 720px window.
