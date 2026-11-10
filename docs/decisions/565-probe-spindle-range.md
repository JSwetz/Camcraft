## #565 - The machine's spindle-range law judges the commanded speed

**Decided:** `posting::posted_rpm(tool)` is the single answer to "what spindle speed does the program command": 0 for a touch probe, the tool's own rpm for every cutter. The post and the machine check's spindle-range clause both read it. A probe is no longer judged against the spindle window; every cutter still is.

**Why:** the check compared the probe's token catalog rpm (1) with the machine's minimum (175 on the Tormach 770MX), so every probing program on a machine that states a spindle minimum was refused at post. The post never commands that number, and since #523 it starts no spindle for a probe.

**Considered and dropped:**
- Giving the catalog probe a fake in-range rpm: it would hide the mismatch and lie in the tool table.
- Excluding the Probe *op kind*: the speed belongs to the tool, and a probe tool is what the post zeroes.

**Cost to change later:** none. One function, two callers.
