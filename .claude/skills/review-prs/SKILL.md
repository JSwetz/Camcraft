---
name: review-prs
description: Find open pull requests on NeoTech/Camcraft that need a review (third-party PRs from forks, or any PR whose current head has not been reviewed yet) and dispatch the PR-reviewer agent on each, one at a time. Posts review COMMENTS only - never approves or merges. Use when asked to "review the PRs", "check for new pull requests", "review PR #N", or from the periodic loop (`/loop 10m /review-prs`).
---

# Review open pull requests

The `PR-reviewer` agent (`.claude/agents/PR-reviewer.md`) does the review of
one PR. This skill decides which PRs need it and dispatches it. The maintainer
approves and merges by hand - nothing here approves, merges or pushes.

## 0. One review at a time - the lock

PR reviews never run concurrently (user, 2026-11-07: "make sure we are not
running concurrent PR reviews"): each one builds the whole workspace, and a
loop tick can fire while the previous review is still building. The lock is
the file `.claude/pr-review.lock` in the repo root (gitignored), one line:
`<PR number> <head sha> <UTC ISO time>`.

```bash
lock="$(git rev-parse --show-toplevel)/.claude/pr-review.lock"
if [ -f "$lock" ]; then cat "$lock"; find "$lock" -mmin +90; fi
```

- Lock present and younger than 90 min (the `find` prints nothing): a review
  is running. Say "a PR review is already running (#N since T)" in one line
  and stop - do not dispatch, do not queue.
- Lock older than 90 min: stale (a review that died). Check no
  `../cc-pr*-review` worktree is still building (`git worktree list`); if none,
  delete the lock, say "removed a stale PR-review lock (#N)", and go on.
  If a review worktree remains, remove it (`git worktree remove --force`,
  `git branch -D pr-<N>-review`) before going on.
- Also stop if a `PR-reviewer` agent you launched is still running in this
  session.

## 1. Find PRs that need a review

If the user named a PR (`/review-prs 58`), take just that one and go to step 2.

Otherwise list open, non-draft PRs with their head SHA and reviews:

```bash
gh pr list --repo NeoTech/Camcraft --state open --limit 100 \
  --json number,isDraft,author,headRepositoryOwner,headRefOid,reviews,comments \
  --jq '.[] | select((.isDraft|not) and .headRepositoryOwner.login != "NeoTech")
        | .headRefOid as $s
        | select([.reviews[].body, .comments[].body] | map(select(contains("<!-- pr-reviewer: " + $s))) | length == 0)
        | "\(.number) \(.author.login) \($s)"'
```

Every line printed is a PR that needs a review:
- it comes from a fork - our own issue PRs are reviewed in the release cut by
  the main session (AGENTS.md "A cut goes PR review"), not here, unless the
  user named one;
- no review or comment carries the marker for its current head SHA (a new push
  after our last review means the contributor answered - review again).

Nothing to review: say "no PRs need review" in one line and stop (this is the
common case on a loop tick - keep it quiet).

## 2. Dispatch exactly one PR

Take the oldest PR needing review - **one per invocation**; the next loop tick
takes the next. Never launch two `PR-reviewer` agents at once.

1. Write the lock first:
   `echo "<N> <sha> $(date -u +%Y-%m-%dT%H:%MZ)" > "$(git rev-parse --show-toplevel)/.claude/pr-review.lock"`
2. Agent tool, `subagent_type: "PR-reviewer"`, run in the background, prompt:
   `Review PR #<N> of NeoTech/Camcraft (head <sha>). Follow your definition: comment only, own worktree, clean up, remove .claude/pr-review.lock when done.`
3. On its completion notification, make sure the lock is gone (delete it if
   the agent failed to) and that `../cc-pr<N>-review` is removed. Do not poll
   while it runs.

## 3. Report

One short line per PR: number, author, verdict, link to the posted review.
If a review could not be posted, say why. Do not merge, approve, label, or
close anything; do not reply on the contributor's behalf.

## Periodic run

`/loop 10m /review-prs` re-runs this every 10 minutes for the session
(recurring loops auto-expire after 7 days; cancel with CronDelete). Each tick is
a cheap `gh pr list`; an agent is only spawned when a fork PR has an
unreviewed head.
