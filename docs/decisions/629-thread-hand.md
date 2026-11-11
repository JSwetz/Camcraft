## #629 - lathe threads cut right-hand toward the chuck; the hand is stated, never dropped

**Decided.**

- **The hand law, stated once (`cl::ThreadHand`).** A helix is right-hand
  when it advances +Z turning counter-clockwise seen from +Z, or -Z turning
  clockwise (the screw-thread law of any machining handbook). The turning
  frame stands the free end at +Z and the chuck below (the axial-drill and
  radial-rapid laws already read it so). M03 is clockwise seen from the
  headstock (RS-274 / ISO 6983), so the tool's motion about a part turning
  M03 is clockwise seen from +Z: clockwise half-turns are M03's,
  counter-clockwise ones M04's.
- **One feed direction: toward the chuck.** Every pass starts on the top
  lattice point and falls; the hand alone picks the arc sense. RH toward
  the chuck = clockwise = M03 (the shop standard); LH toward the chuck =
  counter-clockwise = M04. Behaviour confirmed against the control vendor's
  own conversational output (PathPilot Hub 8L conversational v2.14.4,
  2026-11-09, syntax/behaviour only, nothing copied): its RH external
  threads run G76 from Z+ to Z-1.0 under M3, i.e. toward the chuck.
- **Threading gets `hand` (Right default, Left)** on `OpParams::Threading`
  and `ThreadParams`, edited through EditOperationParams (undoable), shown
  on the Passes tab as "Hand: Right hand / Left hand", and on MCP through
  the params DTO (`"hand": "right" | "left"`).
- **The record carries the hand.** `ThreadCycle.hand`; `is_sane` requires
  every helical half-turn in the span to lay it, so a record never
  contradicts its passes. Direction is the record's own start/end Z
  (`toward_minus_z`), and `reverse_spindle()` is derived from both.
- **The stream states it twice.** `THREAD ... HAND right|left` (required -
  a record without it is refused by name), and `PARAM SpindleDirection
  reverse` before the section opens when the passes need M04 (absent =
  forward, the coolant law). A script must declare `threadHands = true` to
  receive a reverse section; otherwise the engine refuses it by name (the
  mill posts, and the PathPilot lathe post in #626 until it adopts it).
- **Fanuc Lathe:** M03 or M04 from the PARAM (M04 with an operator note to
  mount the insert for reverse rotation); every G32 half-turn is checked
  against the spindle the section started - a half-turn turning against it
  is refused ("would cut the other hand"); a G76 record whose hand and Z
  direction need the other spindle, or states no hand, is refused.
- **The verdict:** `thread_hand_warnings` convicts (as a gouge at the
  first offending half-turn) any helix that does not wind the asked hand,
  so the post gate blocks it.
- **Old files: no project-version step.** The key is optional and absent
  reads right-hand - what the parts were drawn as; the old files POSTED
  left-hand by this bug, and load as the right-hand thread they modeled.
  The stamp now digests the hand for both hands, so every threading cache
  made before the fix goes Stale once and regenerates toward the chuck. A
  v58 reader dropping `hand` cannot do worse than its own behaviour (it
  cut every thread left-hand), and the coolant (#112) / after-op (#110)
  keys set the precedent of an additive optional key without a step.

**Considered and dropped.**

- A feed-direction parameter (toward / away from the chuck): LH work is
  often cut M03 away from the chuck from a relief groove. Dropped for now -
  one direction keeps the approach in free air for both hands and the
  post rule simple; M04 expresses LH correctly on any Fanuc-style lathe.
  Follow-up worth filing: "Threading: optional feed away from the chuck
  (LH under M03)".
- Inferring the hand in the post from arcs alone: the spindle word is
  written at the section start, before any arc is seen; hence the PARAM.
- Refusing LH outright: the post can express it (M04), so refusal would
  be a needless gap.

**Cost to change later.** A direction parameter adds one field and flips
`ccw_for(true)` to `ccw_for(toward)`; the posts already read direction from
the record's Z and the spindle from the PARAM, so they need no change.
