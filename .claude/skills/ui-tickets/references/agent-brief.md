# Dispatching a fix agent

One agent per ticket. Two at a time at most, never two in the same region.

## Setup

```bash
cd /var/home/smense/eve-spai
git status --short                     # commit first if dirty; agents seed from a clean main
git worktree add --detach /var/home/smense/eve-spai/.claude/worktrees/ui-NNN main
cargo test --workspace 2>&1 | tail -3  # MEASURE the count now, do not copy it
```

## Brief template

```
You are fixing UI-NNN in a git worktree at <path>. Work only in that worktree.

Read /var/home/smense/eve-spai/ui-tickets/UI-NNN-slug/ticket.md first. It is the
statement of the problem, not a spec for the fix. If it is wrong, say so and fix the
real thing.

YOUR REGION: <function names you may change>
DO NOT TOUCH: <adjacent function names, and any region another agent owns right now>

Read CLAUDE.md for the harness, the comment rules and the writing rules.

Required:
- A scene or assertion in app/src/uitest/scenes.rs that FAILS without your fix.
  Prove it: revert the behaviour (not the whole file, not a matching string) and
  show the assertion failing.
- cargo test --workspace stays green, at least <N> passing. That is a FLOOR. Adding
  tests is expected.
- Render the scene: cargo test --bin eve-spai uitest_screenshots -- --ignored
  Screenshots land in YOUR worktree's target/uishots. Look at them.
- Comments only where they justify a WHY. Default to none.

Never run git stash in a worktree. There is one refs/stash shared across all of them
and it will swap your files with another agent's.

Return: the diff, the test you added, what the screenshot shows, what you rejected and
why, and anything the ticket got wrong. Report your tool call count.
```

## After it returns

Review the patch before applying it, not after. In order:

1. Does it fix the cause or paper over the symptom?
2. Did it stay in its region? `git diff --stat` in the worktree answers this in one line.
3. Verify every load-bearing claim in the source yourself. Agents have been confidently wrong about
   which code path is hot, which caller exists, and which platform a branch runs on. They have also
   been right in a way that corrected the ticket, which is the more valuable case and belongs in the
   review.
4. Do the comments justify WHY, or restate WHAT?

Then apply to a branch in the main tree, run the suite, re-render, and look at the PNG next to
`before/`.

```bash
git -C <worktree> diff main > /tmp/ui-NNN.patch
git checkout -b fix/ui-nnn-slug main
git apply -3 /tmp/ui-NNN.patch        # -3 expected on the second patch of a wave
cargo test --workspace
```

Check any three-way resolve in `scenes.rs` by eye. A conflict boundary can truncate a test
mid-function, which surfaces as an unclosed delimiter rather than a quietly dropped assertion.

## Landing

Stage by path, never `git add -A`. The ticket, the fix and the review are three commits, and a
sweeping add puts the code under the ticket's message, where nothing describes what changed:

```bash
git add ui-tickets/UI-NNN-slug/ticket.md && git commit   # filed before the fix exists
git add <the source files> && git commit                 # the fix and its tests
git add ui-tickets/UI-NNN-slug/ && git commit            # review.md and after/
git checkout main
git merge --no-ff fix/ui-nnn-slug
git worktree remove <worktree>
```

The merge commit is the revert unit: `git revert -m 1 <sha>` backs out code, tests and docs
together. Do not fast-forward. Do not push unless asked.
