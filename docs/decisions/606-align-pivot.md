## #606 - Part alignment: the typed turn pivots on the WCS origin; a picked face lays face down

Split from #330's reopen report. Three items, each decided unattended
(Autonomous Work Law). Stacked on PR #603 (#330), which carries the
structural test this tightens and the short-line plane rows.

### 1. The typed-angle Apply turns about the WCS origin (with a choice)

**What was decided.** The Setups page's Part alignment row's typed
angles now turn the part about the setup's **WCS origin** by default:
the point of the part that sits on the origin now stays on it, and the
rest turns around it (`setup_turn::turned`, `AlignPartToSetup::turn`).
The origin is the setup's own WCS entity's position in the setup frame,
or the frame's zero when none is placed - the same reading the post
makes (`posting::wcs_context`). The angles stay absolute (X, then Y,
then Z from as modelled); the pivot point is the part point under the
origin in the CURRENT placement (`A_prev^-1(O)`), so re-applying the
same angles is "already holds the part that way", never a drift. All
zero is "as modelled" whatever the pivot, so the row's words
("As modelled", a preset's name) never hide a shift.

The row says which: a small "about [WCS origin | part centre]" combo
with the Apply, on a line of its own under the angles (the pane is
narrow - #330), and Apply's tooltip names the pivot. Choosing
"part centre" gives the old turn. The quick-choice presets (Flip over
X, Turn Z 90, ...) keep turning about the part's centre - a flip about
the origin would put a part sitting on Z 0 under the table - and their
tooltip now says so.

**Why.** The maintainer's report: alignment "shifts the model relative
to the origin"; their expectation in #330: "the part keeps its
relationship to the origin as the setup's alignment defines it, without
a stray shift". A pivot choice was cheap (one enum, one combo, one
field on `CommandKind::AlignPart`), so the old behaviour stays one
click away instead of being lost.

**Considered and dropped.** Bounds centre only (the old behaviour - the
reported fault). WCS origin only, with no choice (loses the centre
turn, which is the natural one for a part not yet placed against the
origin). Making the presets pivot on the origin too (a flip lands the
part below the table when the origin is on its bottom face; presets are
"how it sits on the table"). Pivoting on the as-modelled point at the
origin (`O` itself) rather than the part point currently on it: after a
first turn that drifts the part on every Apply.

**Cost to change later.** `TurnPivot` is view state on the command
queue; nothing is saved (the stored alignment is the resulting
transform), so changing the default or dropping the choice is a UI
edit with no project-file effect.

### 2. A picked face lays face down by default

**What was decided.** The "Lay on a plane" face row's toggle is now
"Face down", **on** for a new pick: the picked face's outside faces -Z
for G17 (-Y G18, -X G19), so it goes onto the table and becomes the
part's bottom with the part above it. Off lays it face up (the old
default). The three-points row gets the same: "Points down", on.

**Why.** "Lay a face on the plane" reads as "put this face on the
table" in most CAM tools and in fixture practice; the old default left
a picked G17 face looking up with the part below Z 0 (#330's report).

**Saved projects are untouched.** Only the UI draft's default changed.
`PlaneLay::flip` keeps its meaning in the app, the MCP tools and
scripts (they pass the flag explicitly), and a setup stores the
resulting transform, never the flag - so every saved project loads and
posts identically. That is why flipping the default was safe; had the
flag been saved, the default would have stayed.

**Considered and dropped.** Inverting `PlaneLay::flip`'s meaning in
the app (would silently change MCP/script callers and every app test's
fixtures). Keeping a checkbox labelled "Flip" that starts ticked
(reads backwards).

### 3. The structural test asserts the pane rect, not the window

`setup_page_alignment_rows_structure` now reads the properties leaf's
tab-body rect from the dock and asserts every alignment-row option -
and now the Part alignment row's pivot combo and Apply - lies inside
it. Checked against the #330 regression (the face row's controls put
back on one line): it fails with `"Face down" lies inside the
properties pane [[990 101] - [1264 682]]: [[1349.8 344] - [1429.4
362]]`. The pane ends 16 px inside the window, so a spill that stops
between the pane edge and the window edge (clipped, unreachable) now
fails where the window check let it pass.
