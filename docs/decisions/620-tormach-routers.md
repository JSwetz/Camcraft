## #620 - Machine files for the Tormach 24R and xsTECH Pro routers (2026-11-09)

First slice of #620: the two routers whose documents were in hand. The
plain xsTECH Router waits for its manual (a file built from memory would
be the silent wrong answer, as #170 held for the non-MX 1100).

Sources (contributor-supplied, Tormach's published documents, figures
only):
- **Tormach 24R:** operator's manual UM10564 0826A, § 2.2 "Machine
  Specifications".
- **Tormach xsTECH Pro:** spec sheet `xstech_pro_spec_sheet_final.pdf`
  (©2026, US) and the CE operator's manual UM10859, § 2.

Decisions:

- **Chosen: two inch `.mch` files**, `tormach-24r.mch` ("Tormach 24R") and
  `tormach-xstech-pro.mch` ("Tormach xsTECH Pro"), registered in
  `posting::MACHINES` after the mills. The model name is the documents'
  own ("xsTECH Pro"); the issue's "xsTECH Pro Router" wording was dropped
  so the picker reads what the spec sheet says.
- **Travel** is ±the full stroke in program coordinates, as on every
  Tormach file (#131 / #457): 24R X 24.75, Y 55.75, Z 6.7 in; xsTECH Pro
  X 10.2 in (the spec sheet's figure with the tool changer fitted - the
  ATC is standard, so its 12.3 in changer-removed figure is not the
  machine as shipped).
- **xsTECH Pro Y and Z disagree between documents:** Y 9.3 in (US spec
  sheet) vs 7.9 in (CE manual); Z 6.3 in (sheet) vs 6.5 in (CE). Chosen:
  the smaller of each, Y 7.9 and Z 6.3. The travel band is a refusal
  limit, so the smaller figure can only refuse a move the larger machine
  could make, never pass one the smaller could not. Nothing in either
  document says which build is sold where beyond the CE marking, so the US
  sheet was not taken to govern. Cost to change: two numbers and the
  spec test row, if a US 9.3 in machine is confirmed (or ship it as its
  own entry, as #170 did for the 770MX ML).
- **Spindle window:** 24R 10000-24000 rpm, 1.5 kW (2 hp liquid-cooled
  electrospindle, ER20); xsTECH Pro max 20000 rpm, 0.35 kW, **no minimum
  stated, so none in the file** (absent is not zero). No belt ranges:
  both are direct-drive, so the post writes no belt note.
- **24R's 10000 rpm floor is enforced:** a mill-speed job is refused by
  the machine check ("Face1: tool ... runs 5000 rpm, below the machine's
  10000"), pinned by `tormach_routers::a_mill_speed_job_is_refused_on_the_24r`.
  That is the intended behaviour, not a bug: the spindle cannot run there.
- **Feed rates:** 24R 200 IPM on X, Y and Z (§ 2.2), stated as both
  normal and rapid as the mill files do. xsTECH Pro: neither document
  states a feed, so the file states none - the controller table carries
  only the axes' `coordinate` order (so X, Y, Z list in the usual order)
  and no speeds; `feedRateMax` reads 0 and the post's `> 0` guards stay
  silent. Considered and dropped: borrowing the 24R's or a mill's figure
  (a guessed ceiling is a fake limit).
- **Tool changer:** 24R - the ATC is optional and § 2.2 states no pocket
  count, so `tooling.number_of_tools` is unstated (as on the 1100MX).
  xsTECH Pro - the spec sheet states a 4-station ATC as standard, so the
  file states 4, as the 770MX states its 10. Consequence: a program
  calling T5 or above on the xsTECH Pro is a checker refusal by name.
  Considered: leaving it unstated because PathPilot can prompt a manual
  change for a tool outside the carousel. Dropped to follow the 770MX
  precedent; cost to change is deleting one line if Hub runs show
  manual-change tool numbers are wanted.
- **Rigid tapping: `false` on both.** Neither document lists rigid
  tapping; a router electrospindle (24R) and the 350 W ISO 10 spindle
  (xsTECH Pro) are not documented as spindle-synchronised. The PathPilot
  post therefore refuses a G84 by name on them, rather than writing a
  cycle the spindle may not follow. Cost to change: flip the flag if a
  manual later documents rigid tapping.
- **Coolant: none stated.** Neither spec table lists flood or mist, so
  the files list no coolants; a Flood op on either is refused by name.
- **Safe retract `0.0`** on Z, as every Tormach file states (#495):
  PathPilot homes Z at the top of travel on its routers as on its mills,
  and `tormach_safe_retract`'s shipped-file law (every `Tormach*` entry
  states Z0 and posts byte-identical) holds for both.
- **Post:** both run PathPilot, so they are `kind: "mill"` and the
  library offers "Tormach PathPilot" for them as for every Tormach mill.
  There is no per-machine post field in the `.mch` grammar; the post
  choice stays with the setup (`SelectPost`), as for the mills. A Hub run
  of a simple profile on one router is still owed for #620's "Done
  means".
