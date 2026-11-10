## #523 - A touch probe is never spun

**Decided:** every mill post gates its spindle start on `sRpm > 0`, the rule Fanuc has used since #108. This covers Haas, Tormach, Sinumerik, Heidenhain and Mazak. A section with no spindle speed (a probe) posts no `S` word and no `M3` / `MOTON`. No extra `M5` is needed: every post already ends each section with `M5`, so a probe section never inherits a running spindle.

**Why rpm and not a probe flag:** `Tool::is_sane` refuses rpm 0 on every tool, so S0 reaches a post only from a probe dump. The gate cannot drop a cutter's spindle. `a_cutting_section_still_starts_its_spindle_on_every_mill_post` pins that.

**Reachability:** on main only `fanuc.cps` has `onProbeMove`; the others refuse a probe op by name, so the ungated block was latent. PR #500 (Tormach probe positioning) makes it reachable, which is where the `S0 M3` was seen. #500 should merge after this, or take it in a `git merge origin/main`.

**Considered and dropped:** a separate `Probe` stream parameter for the posts to test. That is more surface for the same guarantee.

**Cost to change later:** low. One condition per post.
