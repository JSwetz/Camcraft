## #41 - ROADMAP.md is retired; the open GitHub issues are the roadmap

**What was decided.** `ROADMAP.md` is deleted from the tree (user,
2026-11-08: "Read the Mx.x Issues, and finish them out as PR's and Retire
the Roadmap.md"). It has been a frozen archive since the Issue Law
(2026-11-07); it now lives on only in git history and in every tag up to
v0.14.0 (`git show v0.14.0:ROADMAP.md`), the way `DONE.md` did before it
(b6c39e6). Live references were repointed: `AGENTS.md` (Issue Law),
`README.md` (Project documents), and the egui-stack notes in
`assets/ui/LICENSES.md` and `Cargo.toml`, which now point to the decision's
real home, `docs/spike-egui-go-no-go.md`.

**Why.** Two ledgers drift. Every item the file still marked owed already
had an issue, so the file added nothing a reader needed except history -
and history is what git keeps.

**The audit (2026-11-08).** Every row and bullet not marked done was
matched to an issue; nothing was found without one, so no issue was filed.

| Roadmap item | Issue(s) |
|---|---|
| Catalog 2D/3D Adaptive, M2.1, M2.4, M8.1 (trochoidal / constant-load, owed under M11.4 C) | #5, #6, #44, #47, #220 (open) |
| M11.4 island late returns (round 6), ring fallback | #7 (open) |
| M11.4 Pocket3D port, rotary wrap ladder, chord lag, arc fit, whole-model Pocket source | #8, #9, #10, #11, #12 (closed) |
| M11.3 bore linear-lead second dive, Rotary Parallel / Swarf entries, Rotary Offset, swarf leftovers | #14, #15, #16, #17 (closed); #48 (open, follow-on) |
| M11.2 oblique entries / Heidenhain vertical arcs | #18 (closed) |
| M5.2 / M13.5 continuous C-axis live tooling | #33 (open) |
| M13.5 G76 threading | #34 (closed) |
| M13.5 ribbon reads `KindInfo::admits`, Setup pane offers the post | shipped in 7205935 |
| M7.1 / M13.1 lathe lift, flute-rotation bound, eikonal | #19, #20, #22 (closed) |
| M13.1 blade-profile sweep variant | #21 (open, in review) |
| M7.2 / M13.1 dual contouring | #23 (open) |
| M13.1 field answers the hidden-rest scan past its budget | #24 (open) |
| M13.1 machined mesh facet highlight and deviation classes | #25 (open) |
| M7.3 dexel retirement | #26 (open) |
| M12.1 Solidcore kernel (also the M1.3 IGES + B-rep operations deferral) | #27 (open) |
| M13.3 .iss compile proven | #28 (closed) |
| M13.3 uninstaller signing (no certificate owned) | #29 (open) |
| M13.3 Docker builds, wasm app/UI, headless binary per crate | #30, #31, #32 (open) |
| M13.6 pocket/slot recognition (Open question "Feature recognition") | #35 (open) |
| M13.9 fold annulus, tilted shelf, reach ceiling | #36, #46, #37 (closed); #212 (open) |
| M13.11 2D Adaptive on a wide Outside band | #13 (closed) |
| M13.11 G41/G42 wear variant, concave-corner check | #38, #40 (closed) |
| M13.11 G41/G42 on the other mill dialects | #39 (open) |

**Not counted as owed**, because the file itself says they are not
promises or are decided: spindle efficiency as a `.mch` key (M13.2, "a
possible follow-on, not a promise"), a cleanup ring around bridged holes
(M13.11, "if wanted"), IGES import (decided: waits for M12.1, #27),
machine kinematics fidelity and a SolidWorks link (Open questions:
out of scope). M13.10 and M11.5 read done with no owed bullet left.

**Considered and dropped.**
- Waiting for every migrated issue to close, as #41's body first planned.
  Dropped on the user's word: an open issue tracks its own work, and the
  file only duplicated it. #41 is referenced by this PR, not closed by it.
- Rewriting the `specced from ROADMAP Mx.y` provenance comments in the
  source. Dropped: they name where a behaviour was specced, which stays
  true and findable in git history; changing them would touch every crate
  for no behaviour.
- Editing `RECOMMENDED.md`, `CHANGELOG.md`, `RELEASE_NOTES.md` and older
  docs (`docs/M4.1-CRATE-SPLIT.md`, `docs/spike-egui-go-no-go.md`) that
  mention the file. Dropped: they are history.

**Cost to change later.** None: `git show v0.14.0:ROADMAP.md` restores
the text; bringing the file back is one commit.
