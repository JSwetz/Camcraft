<!-- moved from RECOMMENDED.md by PR #218 (docs/decisions rule, PR #250) -->
## #37 - the scallop law's reach ceiling: the ball diameter at the generator; the scan's reach stays, measured (2026-11-08)

**What "the reach ceiling" was.** The ridge flank law (`stock_sim/ridge.rs`)
reads a column only when both feet stand within `reach = band + 3·tol`, so on
flat ground it sees a cusp only to `4·tol` past the ball (s ≈ 2.9 on an r3
ball at tol 0.1). A stepover of 4 (floor 0.606 mm outside both balls) stands
past it and verifies silent on scallop: `rest_mm` needs two band contacts,
and the "missing-pass law" the comments deferred to does not exist. The
generator accepted any positive stepover, so the silence was reachable.

**Decision (1) - the generator's ceiling is the ball diameter**
(`finishing3d::params::scallop_ceiling`, in the shared `validate`, so every
3D finish door): a stated `stepover` or `steep_stepover` at or past the ball
diameter is refused by name ("reaches the scallop ceiling"); past `s = 2r`
the cusp formula `h = r − √(r² − (s/2)²)` describes no cusp, only uncut
ground. A tolerance at or past `r/2` is refused too: there the auto law
`√(8·r·tol)` itself reaches `2r`. Inputs under the ceiling are untouched.
No clamp: a silently shrunk stepover is a parameter the user did not write.
One test moved: the authored-ladder test coarsened with steep_stepover 8 on
the r3 box; it now uses 5 (still coarser than 2, still under the ceiling).

**Decision (2) - the scan's reach stays `band + 3·tol`, and its validity
range is documented** (`ridge::reach`). A far tier was built and measured:
columns past the band that the near flank left unflanked asked again within
`2r`, distinct passes required. It read every flat-raster spacing up to 5.9
exactly, but over the release verdict lanes it convicted 11 finish3d tests:
cone apex 1.68 mm, box walls 0.42, dome waterline 1.10, blind pocket 0.16,
dome scallop/multidirection 0.20, hill contour 0.18, hill radial 0.17, ramp
hill 0.18 - the same one-pass crown/corner ground the M13.8 full-diameter try
convicted, which the distinct-rings rule (M13.9) does not excuse (a crown is
flanked by DIFFERENT rings of the ladder). Shipping it would have redded the
heavy lane or raised every ratchet on ground no spacing fixes. Reverted.
Owed: a crown/corner exclusion (a column whose flanking feet all lie BELOW
its tangent plane on a convex crown, or across a convex edge, is rest's
ground) before the reach can grow - and then the far tier above, whose
lattice-slack fix (asking the unflanked columns inside the reach too) made
s = 3 read 0.354 where today it reads 0.

Dropped: an unbounded far window (two passes on opposite sides of a region a
boundary excluded would "flank" it); the exact cusp law for the auto
stepover (`2√(2r·tol − tol²)`, every golden changes for a ≤1% difference at
sane tolerances); a clamp-with-warning at the generator.

