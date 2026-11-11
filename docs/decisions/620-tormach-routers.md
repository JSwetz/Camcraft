## #620 - Machine files for the Tormach 24R, xsTECH Pro and xsTECH routers (2026-11-09)

The 24R and xsTECH Pro came first, from the documents in hand; the plain
xsTECH Router followed from Tormach's public pages (its section below).

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

### The plain xsTECH Router (2026-11-10)

**No manual is cited:** none exists. Tormach told the contributor:
"Xstech we never made a separate manual for, we tried putting everything
in the knowledgebase but it didn't really work well." The figures
therefore come from Tormach's own public pages, retrieved 2026-11-09/10:

- [S] spec sheet `xstech_spec_sheet_0225.pdf` ("©2025 Tormach (02/25)"),
  served by the product page's "Technical Specifications" link,
  https://tormach.com/docs/download/assetlink/asset_id/674 (redirects to
  https://tormach.com/media/asset/x/s/xstech_spec_sheet_0225.pdf).
- [P] product page, https://tormach.com/machines/routers/xstech-router.html
  (https://cnc.tormach.com/xstech-router redirects there) - same figures as [S].
- [G] package page "xsTECH Router - 230V (Europe)", PN 50614,
  https://tormach.com/xstech-cnc-router-package-50614.html ("© 2026").
- [K] knowledge base "xsTECH Machine Specifications",
  https://knowledgebase.tormach.com/xstech/xstech-machine-specifications,
  mirrored at https://tormach.atlassian.net/wiki/spaces/XST/pages/1967259722
  ("Technical specifications of the xsTECH Router?", published 2021-01-22).
- [K2] knowledge base "How do I install tools on the xsTECH Router?" and
  "What size cutting tools can I use with the xsTECH Router?",
  https://knowledgebase.tormach.com/xstech/how-do-i-install-tools-on-the-xstech-router,
  https://knowledgebase.tormach.com/xstech/what-size-cutting-tools-can-i-use-with-the-xstech-

The xsTECH Pro documents cited above describe the Pro only; nothing in
them was used for the plain machine.

Decisions:

- **Chosen: `tormach-xstech.mch`, model "Tormach xsTECH"**, inch,
  registered in `posting::MACHINES` after the Pro, `kind: "mill"`, so
  the library offers the PathPilot post for it ([P]: "the xsTECH uses
  our award-winning PathPilot control software").
- **Travel X 8.6, Y 6.3, Z 2.4 in**, ±the full stroke as on every
  Tormach file. All sources agree ([S], [P]: X 8.6 in (220 mm), Y 6.3 in
  (160 mm), Z 2.4 in (61 mm); [G], [K]: "working area 8.6 x 6.3 x 2.4 in").
- **Spindle: the sources disagree, and the spindle evidently changed
  between builds** - [K] (2021): "Direct Drive 80 W Brushless DC",
  "3000 - 15000 RPM"; [S]/[P]: "120 W (0.12 kW) Brushless DC", "Maximum
  Speed 20,000 RPM", no minimum; [G]: "5000-20000 RPM - 125W BLDC direct
  drive". **Chosen: the narrowest window and the smallest power** -
  5000-15000 rpm, 0.08 kW - the smaller-figure rule of the Pro's travels
  applied to each limit in the direction that refuses: the larger stated
  floor (a 3000 floor would pass a speed the 5000-floor spindle cannot
  hold), the smaller stated ceiling, the smaller stated motor. Every one
  is a refusal limit (speed window and power estimate in the machine
  check, `spindleMax` in the post), so the narrow figure can only refuse a
  job the newer spindle could run, never pass one the older could not.
  Nothing on the machine says which spindle it carries, so the current
  sheet was not taken to govern. Cost: a 15000-20000 rpm job on a newer
  machine is refused by name. Cost to change: three numbers and the spec
  row, if the spindle generations are confirmed separable (or ship the
  120 W build as its own entry, as #170 did for the 770MX ML).
  Direct drive: no belt ranges.
- **Feed: 120 IPM on X, Y and Z**, normal and rapid, from [G] ("Belt
  drive, maximum speed 120 IPM") - the only feed figure Tormach publishes
  for the machine ([S], [P], [K] state none). It is one machine-wide
  figure, so it is stated per axis as the 24R's is.
- **Tool change: manual, no ATC** - [K2]: "The xsTECH Router uses ER11
  collets to hold tools", tightened "using the collet wrenches"; [S]
  lists an "Integrated Tool Setter", not a changer. So
  `tooling.number_of_tools` is unstated (as on the 24R and 1100MX): no
  magazine limit exists to refuse against; PathPilot prompts each change.
- **Rigid tapping: `false`** - no source documents it, and the 80-125 W
  BLDC spindle is not documented as spindle-synchronised, so a G84 is
  refused by name, as on the other routers.
- **Coolant: none** - no source lists flood or mist; a Flood op is
  refused by name.
- **Safe retract Z0**, the Tormach convention (#495), so
  `tormach_safe_retract`'s every-Tormach-file law holds for it.
- **Left unstated** (not found in any public Tormach source): a separate
  rapid rate (the 120 IPM is used for both), per-axis feed differences, a
  maximum tool length / diameter beyond the ER11 shank range, and any
  4th-axis figures (the optional 4th-axis kit is not modelled).
- **Tests:** `tormach_routers` pins the figures (the `SPECS` row), the
  PathPilot post for all three routers (now at S12000, inside every
  router's window: the xsTECH's 15000 ceiling refuses the old S18000),
  and `the_xstech_spindle_window_is_the_narrowest_stated` (18000 rpm and
  4000 rpm refused by name, 12000 passes).
