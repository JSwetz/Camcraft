## #21 - the lathe field sweeps the blade's own profile (2026-11-08)

(1) **One convex footprint per tool kind** (`stock_sim::LatheProfile`, new
`field_blade.rs`). The nose disc keeps its bare-chord stadium (no nose golden
moves). A grooving/parting blade is its rectangle (`width` along the axis,
corners rounded by `corner_r`), a threading insert its V of `tip_angle` cut off
at the `width` crest, a form gauge its crown of `nose_radius` across `width`.
The sweep of a convex polygon along a chord is the hull of the polygon at both
ends; its signed distance is closed form, read in the query's meridian both
signs (the same half-plane law the stadium uses). Dropped: a golden-section
sweep like the mill's `Revolve` (a residual where none is needed); stamping
the profile at steps in the engine (that is the test's brute-force reference,
not a law).

(2) **One reading with #103's meridian rapid law.** A blade's move carries its
tip at the band's centre, the body outward (larger radius); the V widens by
`tan(angle/2)` per mm to the band; the crown rises by the sagitta. The body's
outward reach is the tool's `flute_len` (the field is bounded; the rapid law's
`Some(INFINITY)` would be an infinite window). Dropped: the holder above the
blade (not modelled for the insert either).

(3) **The crown is chorded inside the arc**, vertices on the arc, so the
polygon stays on the never-over-claim side of the true crown by at most
`CROWN_SAG_TOL` (1e-4 mm, far under any cell). Dropped: a circumscribed
polygon (it would over-claim by the same amount). A profile that is no footprint
(a NaN or oversize blade corner, a form band wider than its crown) refuses by
name at plan time; the tool's raw numbers reach the gate, never a clamp or a
rectangle stand-in (review of fc2b505: `f64::min` hid a NaN corner as a full
round, and the rectangle over-claimed a form band).

(4) **`LATHE_BLADE_REFUSAL` is gone**: blades simulate. The plan's display
radius for a blade is half its band (`LathePlan::feature`), the cutting feature
`auto_cell` sizes the picture by. The column verdict still refuses every lathe
program by name. The lathe entry points moved from `backend.rs` to
`field_lathe.rs` (400-line law).

(5) **Threads: only meridian passes are cut; a helix refuses by name** (review
of fc2b505, option (a)). The Threading kernel writes each pass as half-turn
helical arcs about the spindle. Revolving a helix removes a full ring at every
station - the picture would show crests cut that the insert leaves - and the
chord walk would refuse it as a "skewed chord", which blames the geometry for
an unsupported program kind. So `lathe_plan` refuses, at plan time and by the
op's name, any turning program with an arc whose plane does not hold the fitted
spindle line: `LATHE_THREAD_REFUSAL` for a thread tool, `LATHE_HELIX_REFUSAL`
for any other. Axial thread passes (meridian moves) are cut with the V
footprint. Dropped: (b) modelling the helical V sweep (a non-revolved solid -
real work, its own issue), and refusing every Thread-tool program (the axial
V pass is honest and pinned in the sim suite).
