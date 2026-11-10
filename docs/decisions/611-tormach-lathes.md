## #611 - Tormach 8L and 15L Slant-PRO lathe definitions

Requested by the contributor (2026-11-09): PathPilot Hub simulates both lathes, and CamCraft shipped only a generic `lathe.mch`.

**Sources:** Tormach's operator manuals, supplied by the contributor: **UM10753** (8L, §3.1 Specifications) and **UM10225** (15L Slant-PRO, §1.1 Specifications). Both confirm every figure below, and the 8L manual settles its X stroke at 4.5". Tormach's public web pages were cross-checked:
- 8L: Tormach knowledge base "8L Machine Specifications" (knowledgebase.tormach.com/8l/machine-specifications), the 8L spec sheet, and Tormach's "8L / 15L / RapidTurn package comparison" PDF.
- 15L: Tormach "15L Slant-PRO Lathe Technical Specifications" (tormach.com/support/lathe/15l-slant-pro-lathe-technical-specifications) and the knowledge base "15L Machine Specifications".

| | 8L | 15L Slant-PRO |
|---|---|---|
| X stroke | 4.5 in | 10 in |
| Z stroke | 10 in | 12 in |
| Spindle | 180–2500 / 350–5000 rpm (two belt ranges, 5C nose) | 180–2500 rpm (6 in chuck) |
| Feed / rapid | 150 IPM | 60 IPM |
| Power | 1.5 hp ≈ 1.1 kW (Tormach web; the 8L manual states none) | 3 hp continuous ≈ 2.24 kW (UM10225 §1.1) |
| Tools | 99 (gang / quick-change post, PathPilot tool table) | 8 (8-station turret) |

**Decisions:**
- **8L X travel 4.5 in, not the knowledge base's 10 in.**
  - **Why:** the spec sheet and the package comparison both say 4.5 in. 10 in is the 15L's X and matches the 8L's Z, so the knowledge-base line reads like a copy error.
  - **Effect:** 4.5 in is the conservative choice; it refuses more, never less.
- **15L spindle max 2500 rpm (the 6 in chuck), not the 5C collet's 3500.**
  - **Why:** the chuck is the recommended and default workholding, and a chuck spun past its rating is a hazard.
  - **Effect:** a shop running the 5C collet can raise it in its own copy of the machine. The default never permits a speed the default workholding can't take.
- **Travel convention:** travel is the full stroke either way of the work zero (`-s..+s`), the convention of the shipped Tormach mills (`tormach_machines`). The work zero's place on the lathe is not known to the file, so this catches a gross over-travel without refusing a legitimate zero.
- **No spindle-body geometry** (`body_diameter` / `body_length` 0): the public sheets give none, and the unstated-limit law checks nothing it isn't told.
- **Units:** inch, the machines' native units, like the Tormach mills.
- **Post:** the default post is the registry's lathe default (`fanuc-lathe` today). It switches to the PathPilot lathe post when #612 lands. Until then the machines judge envelope, rpm and feed, but a posted program is Fanuc-dialect and won't run on PathPilot.
- **Dropped:** the 15LX (a newer model with different specs). It can follow on request.
