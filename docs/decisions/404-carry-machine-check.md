## #404 - carried stock: refuse a machine change, pin off-grid alignment

- **Chosen: the machine is each setup's own, checked at the setup change.**
  `stock_sim::carry::one_machine` compares the two setups' machines through
  `setup_mount::machine_of` (#343: the setup's own machine, else the
  document's) and refuses by name with `CARRY_MACHINE_CHANGE_REFUSAL`, naming both
  setups and both machines. The lathe / mill-turn gate now reads that shared
  machine's kind instead of `machine::kind_of` (the ACTIVE setup's), which is
  what let a carry across a machine change through before.
- **Chosen: where it fires.** `carry::entry` refuses a CARRY across machines,
  so the rest reading (#318, `rest::across::chain_of`) refuses it too: an op
  in the later setup is refused at generation with `REST_CARRY_REFUSAL` plus
  the machine reason. The program walk (`carry::walk`, behind
  `simulate_document` and `incoming_body`) refuses EVERY setup change onto
  another machine, the M3.2 A clip included, because the clip keeps what the
  earlier setup left standing and so hands material across too.
- **Considered and dropped: refusing the clip inside `entry`.** `entry` is
  shared with the rest chain, where a clip starts the later setup from its
  own block and reads nothing across the boundary. Refusing it there would
  have refused generation of every op in a two-machine document whose setups
  hold the part alike - work #343 allows (each machine's setups post on
  their own).
- **Notes corrected.** #401's PR notes and `docs/decisions/317-carry-body.md`
  say "setups with different machines (#390's refusal)". No such refusal
  existed in the sim: #390 is the browser tree, and the only mount refusals
  (#343) are at generation (op vs active setup) and at post. That refusal is
  now `CARRY_MACHINE_CHANGE_REFUSAL` (this entry). The 317 file is left as written
  (one PR, one file); the module docs of `stock_sim_carry.rs` state the law.
- **Off-grid alignment: pinned, not refused.** `Body::carry` derives the image
  origin from the turned lattice corners, so an image cell centre IS a
  transformed source centre for any translation; nothing is snapped to the
  old grid and no material moves by a fraction of a cell. Pinned in the
  engine (`an_odd_lattice_carried_off_the_grid_keeps_whole_cells`: a 7 x 5
  lattice at 0.5 mm, distinct heights per column, a flip and a quarter turn
  each translated by (0.37, -1.13): one-to-one column map, exact heights,
  origin and bounds) and in the app (`a_flip_about_a_centre_off_the_grid_carries_whole_cells`:
  a part centred at (0.37, -0.21) in a 61 x 47 block, flipped over X). A
  refusal was considered and dropped: the case is represented exactly.
- **Cost to change later:** carrying across machines (a part moved from a
  mill to another mill) needs the two machines' frames and fixtures
  related; lifting the refusal is one check in `one_machine`.

## Review fix: a new block may start on another machine

The review on PR #411 found that the machine check ran before the entry was chosen, so it also refused `Entry::NewBlock`, where the later setup takes its own block and nothing crosses. The check now runs only where material crosses: in `carry_turn` (the carry) and in the `Clip` arm of `walk`. A new block on another machine is judged, with the earlier setup's verdict folded in.

- **Considered:** skipping the check only when `entry` returns `NewBlock`. That is the same rule, but it's easier to read when each arm states its own reason.
- **Pinned by:** `setup_carry::a_new_block_on_another_machine_is_judged_not_refused`.
