## #320 - Per-setup stock source and the incoming stock

**Decided: two choices, stored only once picked.** `SetupDef::stock_source`
is `Option<StockSource>` (`Own` | `FromPrevious`); the page shows two
plain options, "Its own stock (the Stock node)" and "What <previous
setup> leaves behind". `None` (every file before v57, every new setup)
means "never picked" and reads #317's implicit rule
(`setup_stock::implicit`):

| Setup 2+ against the setup before it | Unpicked reads as | Sim |
|---|---|---|
| placed relative to it (#316 link, "Same as" included) | What it leaves behind | carry (#317) |
| holds the part differently (any alignment change) | What it leaves behind | carry (#317) |
| holds the part alike, not linked | Its own stock | the M3.2 A clip |

So every existing document simulates, posts and stamps exactly as under
#401: the default IS #401's rule, and nothing is written to a stamp or a
file until a pick is made. Considered and dropped: migrating old files
to an explicit value on load (changes the file bytes on resave, and the
"same hold" outcome is the clip, which neither explicit value would be
by itself if Own meant "a fresh block" everywhere); a third visible
"Automatic" option (the maintainer's wording asks for two).

**Decided: "Its own stock" in the same hold is the clip; in a different
hold it is a new block.** In the same hold the setup's block bounds what
stands and the earlier cuts inside it stay cut (the M3.2 A clip - a
fresh block there would read work the machine already did on the same
clamped piece as uncut). In a different hold the frames differ, the
clip cannot apply, and the setup starts from its own stock: a new block,
legitimate (the user says the machine holds a new piece).

**Decided: a block left behind keeps its gouges in the verdict.** When a
later setup takes a new block, the stock the program leaves is judged in
its own frame first, and its gouge counts and worst gouge fold into the
program verdict (`carry::fold_left`), so a gouge cut before the new
block never disappears from the post gate. Only scalars fold (the cells
are another lattice); the worst point stays in the frame that cut it.

**Decided: refused carries cannot be picked.** `setup_stock::carry_refusal`
runs the same check the walk does (machine kind, `LatticeTurn`), so
"What <setup> leaves behind" is disabled with that reason as its hover
text; if the unpicked default is that refused carry, the page says so in
the error ink and points at "Its own stock". The command refuses it too.

**Decided: the pick is an operation input.** A picked source rides the
setup's operation stamps (written only once picked), so switching marks
the setup's operations Stale; undo restores the stamp.

**Incoming stock display:** the setup page shows the carried body's box
and volume when the setup takes what the previous one leaves (measured
once per document version, never per frame) and says the setup's own
Stock node is not cut from - the reviewer note on #401 (the later
setup's own stock was silently ignored). The setup sheet prints the
same `source` / `incoming` lines under STOCK for setups after the first.
The viewport preview of the incoming body is follow-up #406 (needs a
mesh overlay path in the viewport; not cheap here).

**Walk scope:** the program walk visits the setups that have operations;
a setup with no operations cuts nothing and its pick takes effect once
it has some. "Previous setup" on the page is the previous by setup order.

**Project v57:** `"stock_source": "own" | "from_previous"`, absent when
never picked; an unknown word is a corrupt file. Not breaking: every v56
file loads and simulates unchanged.

**Rest across setups (#318, merged meanwhile) follows the pick.**
`rest::across::chain_of` reads the same `carry::entry`, so "What <setup>
leaves behind" extends the rest chain and "Its own stock" breaks it - a
new block (different hold) exactly like the clip (same hold): the rest
starts from the setup's own block. Considered and dropped: extending the
chain through a new block (rest would assume setup-1 cuts gone from a
blank that never had them - gouge-class). This settles the "fresh block
or same part, not stated until #320" open point in
`docs/decisions/318-rest-across-setups.md`.
