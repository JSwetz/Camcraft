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
  - **Effect:** 4.5 in is the smaller of the two published figures. Note that the stated band is `±4.5` (the convention below), so it catches gross over-travel; it does not bound the true slide position, which needs the work zero's place on the machine.
- **15L spindle max 2500 rpm (the 6 in chuck), not the 5C collet's 3500.**
  - **Why:** the chuck is the recommended and default workholding, and a chuck spun past its rating is a hazard.
  - **Effect:** a shop running the 5C collet can raise it in its own copy of the machine. The default never permits a speed the default workholding can't take.
- **Travel convention:** travel is the full stroke either way of the work zero (`-s..+s`), the convention of the shipped Tormach mills (`tormach_machines`). It differs from the generic `lathe.mch`, which states a machine-frame `0..320`. The work zero's place on the lathe is not known to the file, so the `±s` band admits up to twice the stroke of span: it catches a gross over-travel without refusing a legitimate zero, and it is not a precise envelope.
- **No spindle-body geometry** (`body_diameter` / `body_length` 0): the public sheets give none, and the unstated-limit law checks nothing it isn't told.
- **Units:** inch, the machines' native units, like the Tormach mills.
- **Post:** the default post is the registry's lathe default (`fanuc-lathe` today). It switches to the PathPilot lathe post when #612 lands. Until then the machines judge envelope, rpm and feed, but a posted program is Fanuc-dialect and won't run on PathPilot.
- **Dropped:** the 15LX (a newer model with different specs). It can follow on request.

## #619 - the 15LX Slant-PRO (stacked on #611)

**Source:** UM10860 15LX operator manual, §3.1 Specifications (contributor-supplied).

| | 15LX Slant-PRO |
|---|---|
| X / Z stroke | 10" / 14" |
| Spindle | max 3500 rpm; **no minimum stated** (servo spindle), so none is stated in the file (unstated-limit law) |
| Power | 6 hp continuous ≈ 4.5 kW (10 hp peak, not used: the machine check judges sustained power) |
| Feed / rapid | 360 IPM (EtherCAT servos) |
| Tools | 99: gang tooling is standard, and the 8-position turret is a kit (PN 54578). A turret shop can set 8. |

**Why 3500 rpm here but 2500 on the 15L:** the 15LX manual states one maximum (3500) without a chuck-specific lower figure, while the 15L manual gives 2500 rpm for its 6" chuck explicitly.
