## #266 - The program sim sweeps a tap at its thread's minor diameter

**What was decided.** In the program sim (`stock_sim::plan_program`), a
Tap-kind tool sweeps a disc of radius `diameter / 2 - 0.5413 x pitch`:
the thread's minor (core) cylinder. Every other mill tool still sweeps half
its diameter. A tap whose pitch leaves no core is refused by name. The rule
lives in `operation::tool_law::cutting_radius`, so a later change to the
generator's verify can read the same number.

**Why.** CAD models a tapped hole at the thread's minor or tap-drill size,
and a tap only cuts the thread band outside that wall. Sweeping the full
major diameter read that band as a gouge, so the program-sim clause of the
post gate refused every tap on a normally modelled hole. `0.5413 x pitch` is
the ISO 724 radial thread depth the thread-mill generator already uses
(`machining2d::THREAD_DEPTH_PER_PITCH`), so both read one number. A tap
over a hole smaller than its core, or over solid metal, still gouges.

**Partial: the generator's verify is not changed.** With this fix the
program gate passes, but the tap op's own verify (the tangency law against
the hole's rim, and the containment solid law) still judges the tap at its
full diameter and flags its feed in and out as gouges, so the post is still
blocked. That verify feeds one tool radius to every law, rapids included,
and a tap's full diameter is the right envelope for rapids (a tap must never
rapid into the thread band). It also meets an existing convention:
`solid_law_goldens` models a tapped hole at the tap's major diameter.
Splitting the cutting radius from the rapid envelope changes the verifier's
shared inputs and that convention, so it is left for the maintainer to
decide on the issue.

**Considered and dropped.**
- `diameter - pitch` (the common tap-drill rule, as the issue suggested):
  it is 0.04 mm per side wider than the ISO minor diameter for M6 x 1, so a
  hole modelled at the exact minor diameter would still read as a gouge.
- Keep the full diameter and exempt taps from the gouge count: a tap into
  an undrilled spot would then post silently, which breaks the tap.
- Shrinking only the containment law's cutter: tried; the tangency law still
  convicts the same moves, so on its own it changes nothing a user sees.

**Cost to change later.** One function (`tool_law::cutting_radius`), its use
in `stock_sim::plan_program`, and the two tests in
`crates/app/tests/tap_sim.rs`.
