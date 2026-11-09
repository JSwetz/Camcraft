## #415 - a new block after a flip: the pin compared a verdict that cannot differ

**What was decided.** The test was wrong, not the code. The test
`setup_stock_source::a_new_block_after_a_flip_starts_from_its_own_stock_and_undoes`
now gives Setup2 its own block, 60 x 60 x 24 with its top at Z22. That
is 4 mm higher than the Z18 top of the body Setup1 leaves. Face2 stops at
Z19, above the part. The carried run leaves 2 mm of rest (Face2 cuts
air over the Z18 body), and the new-block run leaves 3 mm. The test checks
both values (`max_rest` 2 and 3), and still checks that the verdict bits
differ and that undo and redo work. No production code changed.

**Why.** Neither #410 (#319) nor the merge broke it. The test fails on
#408's own branch head (d0e5d074) and on its first commit (1bb313d8),
both at the merge base, on macOS debug. It never held on this fixture.
`bits()` compares `Deviation::dev`, which is `top - part` for each column.
Both starting stocks (the carried body, Z 0..18, and Setup2's own block,
Z -2..18) are 60 x 60 with a Z18 top. They differ only below the part,
which a top-down height verdict cannot see. Face2 faced both to Z16, the
part top, so every column read exactly 0.0 in both runs and the arrays
matched. The code was right: `stock_sim_carry::walk` takes
`Entry::NewBlock` and restarts on `stock_grid(Setup2)`, and the
`incoming_body` checks already in the test show it (volume and top of
the new block).

**Considered and dropped.**
- *Fix the code.* Nothing to fix: the walk starts on the new block, and
  #410 does not touch `stock_sim` or `stock_sim_carry`.
- *Compare the simulated surface instead.* `SimSurface` is the top sheet
  too, and was identical (Z16 everywhere).
- *Delete or loosen the assertion.* Not allowed. The new fixture makes it
  stronger: it checks exact rest values that change if the sim carries
  when it should start on a new block.

**Cost to change later.** None. Only the test's fixture changed.
