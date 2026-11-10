# docs/decisions - one file per PR, gathered into RECOMMENDED.md at the cut

Issue branches never edit `RECOMMENDED.md`. Every branch appending to the
end of one shared file made each merge re-conflict every other open PR
(user, 2026-11-08: "this is getting annoying"). Instead, a PR records its
decisions in its own file here:

    docs/decisions/<issue>-<slug>.md      e.g. docs/decisions/208-axis-point.md

Use the same entry format as `RECOMMENDED.md`: a `## #N - <title>` heading,
then **what was decided**, **why**, what was considered and dropped, and
what it would cost to change later. One PR, one file. A review fix on the
same PR edits that PR's file, never another PR's.

At a minor or major cut, `cargo xtask decisions --version X.Y.Z` (or its
wrappers `scripts/collect-decisions.ps1` / `.sh`) appends every file here, in issue-number
order, to `RECOMMENDED.md` under a `## vX.Y.Z` heading, then deletes those
files. This file, `README.md`, stays.
