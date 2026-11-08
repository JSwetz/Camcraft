<!-- moved from RECOMMENDED.md by PR #211 (docs/decisions rule, PR #250) -->

## #34 - G76 threading on the Fanuc Lathe post, from a thread-cycle record in the stream (2026-11-08)

Lifts the M13.5 "G32, not G76" limit ("the first lathe post" entry).

**The record rides beside the passes, not instead of them.** The thread
kernel keeps its explicit helical passes (the sim, verify and every G32
program read them, unchanged) and adds one `cl::ThreadCycle` per thread
(`Toolpath.threads`): start point (clearance radius, first centre), end
at the root on the last centre, major / minor diameter, lead, thread
height, first-pass and minimum depth, finish allowance, infeed angle,
spring passes, pull-out, and the move span `first..=last` it stands
for - the drilling `Cycle` law (intent beside the expanded chain), with
a sanity door (`is_sane`: in range, opens on its own move) read by the
dump and the project loader. The stream carries it as `THREAD ...` /
`THREAD END` around the passes; the cache as an additive `threads` key.
Dropped: a CL-only cycle with no passes (the sim and verify would need a
second reading of the same thread), and re-deriving the cycle from the
G32 arcs in the script (the "re-deriving is what the record replaces"
law).

**The kernel's honest numbers.** The ladder plunges RADIALLY in equal
rungs of `infeed` to the root, so the record says infeed angle 0, first
depth = minimum depth = the rung, no finish allowance, no spring pass,
square pull-out. Under Fanuc's G76 depth law (n-th cut at d*sqrt(n),
increments clamped to dmin) dmin = d makes every increment the rung - the
control cuts the kernel's own ladder. `m` = 1 + spring passes (the
finishing repetition count is at least one), so G76 runs one finishing
pass at full depth the explicit passes do not show - a spring pass, no
deeper than the root the verdict judged. Flank or modified-flank infeed,
finish allowance and pull-out are record fields the kernel does not set
yet; the post already writes and range-checks them.

**The choice is a document post option, default G32.** A new singleton
`PostOptions { threading: G32Passes | G76Cycle }`, moved only by the
undoable `SetPostOptions` (UI: a "Threading" combo under the post picker
while Fanuc Lathe is the post), persisted as an additive `post_options`
key (absent = default, no version bump - the comps law), handed to the
script as the global `postOptions` by `engine::run_with` (`run` and the
CLI pass none = every default). Default G32 keeps every existing golden
byte-identical, and a script without `onThreadCycle` (every mill post)
ignores the record. Dropped: a per-op threading parameter (the cycle is
a dialect choice of the post, not of the cut), and a stream PARAM (it
would change every stream golden).

**G76 words** (public Fanuc 0i-T style lathe documentation, behaviour
only): `G76 P(m)(r)(a) Q(dmin) R(d)` / `G76 X(root dia) Z(end) P(k)
Q(first depth) F(lead)` after one positioning rapid to the start point;
Q/P in whole microns (radius values), R in mm, X diameter, F mm/rev; R(i)
taper omitted (the kernel threads plumb only). Refused by name, never
posted as G32 under a G76 choice: passes with no record ("regenerate the
thread, or choose G32 passes"), an infeed angle outside 0/29/30/55/60/80,
spring passes over 98, a pull-out that is not 0..9.9 leads in tenths,
first/minimum depth outside the height, a finish allowance at or over it,
an internal thread (minor >= major), an end off the minor diameter, a
start inside the crest, no lead, no length. Golden:
`crates/app/tests/golden/nc/fanuc-lathe/thread_g76.nc`.
