# #438 - the tool-number clash is judged per machine


- **Chosen:** the #221 clash law (`tool_numbers::clash_per_machine`) groups
  the program's ops by the machine their setup mounts
  (`program_mounts(world, false)` - machine alone, not machine+post) and
  judges each group's tools on their own. A multi-machine refusal is
  prefixed with the mount's label; a one-machine program refuses with the
  exact #221 message, unchanged. Per-op-files mode reads the same grouping.
- **Why per machine, not per file:** a tool number names a pocket in one
  machine's changer. Two machines are two tool tables, so T2 on each is
  correct; two posts on one machine are two files but one carousel, so a
  shared number there is still a wrong tool load and stays refused.
- **Dropped:** per NC file (machine+post) - would pass a real clash on one
  machine posted through two dialects; whole program (the old law) - refused
  correct multi-machine jobs.
