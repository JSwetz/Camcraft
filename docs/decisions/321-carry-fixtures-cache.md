## #321 - Carried stock: workholding, the lathe second op, the cache

**Decided (1) - workholding reads the carried stock.** A setup entered
with a carried body (#317) holds that body, so its fixtures are read
against it: a tagged fixture of that setup standing INSIDE the material
the program carries in (a jaw modelled into a wall setup 1 left, a clamp
on a face that is still there) refuses the post by name
(`fixture_carried::FIXTURE_IN_CARRIED`, naming both setups, the fixture
and how deep). Only the body the setup is entered with is read - a clamp
seated on a face or in a pocket setup 1 cut is clear even where the
declared block still reaches through it. Penetration, not contact:
column centres strictly inside the footprint, Z overlap beyond the 1 nm
fixture tolerance - jaws gripping a side and a part seated on a plate are
contact by design. Live at the post gate, like every workholding law
(never in a stamp). The machine table is not read (the stock stands on
it).

The tool and holder against the carried stock were already judged by
#319 (PR #410): every stock-reading law of a carried setup's generation -
the rapid, entry, reach (holder) and linking laws - reads the box
enclosing the declared stock and the carried body, and the post's WARN
records are those cached warnings. The fixture laws (c2)/(d2) judge the
same stack against the setup's steel, live. Together that is "tool and
holder against the carried stock plus that setup's fixtures"; what was
missing was the fixtures themselves being read against the carried body,
which is what this adds.

**Why.** Prime Directive: a fixture verdict judged against steel the
machine cannot hold where it is modelled is a verdict about a different
setup. A collision check that sees less than reality is crash-class, so
a modelled hold that contradicts the material is refused, not trusted.

**Considered and dropped.**
- *Carried material outside the declared stock as fixture boxes* for the
  (c2)/(d2) laws: the flute's own law would then convict every op that
  cuts along a declared block smaller than the carried body - the common
  "declare the finished size" habit #319 chose to note, not refuse. The
  holder and rapids against that material are #319's union box already.
- *The spindle housing against the carried stock*: it stands down
  against stock by the kerf reasoning on every setup, carried or not;
  changing that is a law change for all setups, not this issue's.
- *Checking the declared stock too* (single-setup documents): a new
  refusal on every existing document whose fixture boxes overlap a
  loosely declared block; out of scope.

**Limits (stated).** The column sampling resolves to the carried grid's
pitch: an overlap narrower than half a cell beside a face can pass. The
law is a model-consistency refusal; the tool's own steel and stock laws
still judge every move.

**Decided (2) - the lathe / mill-turn second op stays refused by name.**
A part flip or sub-spindle hand-off is not implemented: the column
lattice carries milled bodies only, and a turned body lives in the
meridian field (#19), whose flip is a different transform (about the
spindle normal plus a Z shift) on a different representation. Implementing
it is a turning-field carry with its own verification, larger than this
issue's remaining scope. The refusal is now one named constant
(`stock_sim::CARRY_MACHINE_REFUSAL`) and is pinned on every path:
generation and the rest chain name it (both read `carry::entry`); a path
cut before the machine changed goes stale because its stamp digests the
refused carry, so the program sim and the post refuse that op by name; a
program sim over fresh paths meets the same `carry::entry` refusal.

**Considered and dropped.** *Treating the lathe flip as the milling carry
with a flip transform*: the column grid cannot hold a revolved body's
undercuts, so it would judge less material than stands - refused.

**Decided (3) - the carried body is cached by content.** The body the
chain leaves (`rest::across::carried_body`) is the costliest read of a
carried setup and is read by every op's judged stock, its rest mask and
the post's workholding clause. It is memoized (thread-local, the 4 most
recently used bodies) under a key digesting everything the sweep reads:
the origin setup's stock (placement, mesh signature, shape), the lattice
radius, every turn of the chain, and every cutting op's exact moves and
tool atom in order (the consumer setup's own ops only when the rest mask
asks through them). Op names, the consumer's own parameters, the
viewport and the post gate's reading never enter it, so an unrelated edit
is a hit; any input of the sweep moves the key, so a stale body is never
served. A stock without a geometry signature is never cached. Opening a
document or making a new world clears it, so nothing of a closed document
outlives it. A test-visible counter (`rest::carried_body_computes`) pins
both directions.

**Considered and dropped.**
- *Keying by the stamp digests* (`prior_digest`): they include op names,
  so a rename would recompute; and they digest path signatures rather
  than the moves the sweep reads. Hashing the moves themselves is a
  cheap linear pass beside the sweep and leaves no input out.
- *A flecs singleton per setup*: a write from the read-only collect phase
  of generation; the content key makes the per-setup binding unnecessary.
- *A process-global cache*: would outlive documents; thread-local and
  cleared on document change instead.

**Not done here (owed).** `stock_sim::incoming_body` (the program walk
the #320 display reads) is not routed through the cache - it walks the
whole program, not the rest chain. Heavy-lane measurement on the big
fixtures was not added; the counter pins the behaviour.

**#320 / PR #408.** Its `carry::Entry::NewBlock` (a fresh blank) breaks the
chain like the clip; the cache key reads only the chain #318 builds, so
whichever lands second needs no further rule here.

**Cost to change later.** Local: `fixture_carried`, `rest::carried_cache`
and `carried_body` in `rest::across`.
