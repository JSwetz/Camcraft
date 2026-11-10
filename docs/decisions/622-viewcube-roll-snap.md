# One PR, one file - see README.md. Collected into RECOMMENDED.md at the cut.

## #622 - Pole clicks: snap the roll to the nearest quarter turn, not to +X

**Decided.** `Camera::toward` keeps its law - same target, same distance,
eye along `d` - with one extra clause: when the normalized direction is
exactly a Z pole (`n[2].abs() == 1.0`), the yaw comes not from the
vanishing `atan2(n[1], n[0])` (which always reads 0) but from the
pre-click camera's own yaw snapped to the nearest quarter turn,
`(yaw / FRAC_PI_2).round() * FRAC_PI_2`. `basis_right`'s zero-guard,
which pinned screen-right to world +X at the poles, evaluates that yaw
instead: `(-yaw.sin(), yaw.cos(), 0.0)`.

**Why.** A view-cube TOP or BOT click from any yawed approach threw the
roll away: every click landed screen-right on +X, spinning the model up
to half a turn on screen, and the user's framing was gone (issue #622's
repro and claim comment). The old `+X` pin existed because at an exact
pole `cross(forward, +Z)` vanishes and `normalize` answers zero - some
convention had to fill the hole. The near-pole limit says what that
convention should be: for every `|pitch| < pi/2` the guard's own formula
is *exactly* (to rounding) `(-sin y, cos y, 0)`, so evaluating it at the
pole extends the same value to the one point where the cross product underflows
(`cos(pi/2)^2 ~ 3.7e-33` trips `normalize`'s `EPSILON` floor). Snapping
the yaw a quarter turn keeps the surviving half of the old pole law -
screen up and right ride world axes, views sharing a quarter turn roll
alike - while bounding a near-pole click's on-screen rotation to 45°,
the snap's own bound, with views already on a quarter turn not moving at
all. A second click at the same pole is exact idempotence: the yaw is
already snapped.

**Considered and dropped.**
- Keep `+X` and roll the view down into it: state or animation inside a
  one-line orientation law; the Prime Directive does not pay for tweens.
- Carry the pre-click yaw to the pole un-snapped: fully continuous roll,
  but screen up lands on an arbitrary compass bearing and the view keeps
  the tilt the cube law promises to remove.
- Snap whenever `abs(n[2])` merely approaches 1: `hit_dir` classifies
  only exact face centers as poles (edge and corner picks carry x/y
  components, their pitches well inside `asin(1/sqrt(2))`) and orbit
  clamps pitch at 1.5 - the exact pole is the only reachable near-
  singularity, and a near-pole branch would manufacture a discontinuity
  where the math has none.

**Cost to change later.** Two lines: one `if` in `toward`, one
expression in the `basis_right` guard. `view_cube_tests` pins the snap
values, the quarter-turn pole bases, the 45° bound and the idempotence;
basis consumers (mvp, picking, pan, the cube painter) ride any
orthonormal frame and need no touching.
