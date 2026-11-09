## #170 - Machine files for the Mach3-era PCNC 770 and PCNC 1100, and the 1100M (2026-11-08)

- **Chosen:** six more inch `.mch` files, one per manual in hand, each
  describing a distinct machine. The 1100M's figures are in their own
  entry further down. For the Mach3-era mills, each figure comes from
  the manual's § 9.4 "Specifications" (§ 9.1 in UM10087):

  | File / model | Manual (part no., rev, date) | Spec page |
  |---|---|---|
  | `tormach-pcnc770.mch`, "Tormach PCNC 770" | 31725 Rev B1-4, 12 Aug 2011 | p. 9-2 (PDF p. 142) |
  | `tormach-pcnc770-s3.mch`, "Tormach PCNC 770 (Series 3)" | 32426 Rev C1-1, 17 Oct 2011 (cover: "0412A") | p. 9-2 (PDF p. 142) |
  | `tormach-pcnc1100-s1.mch`, "Tormach PCNC 1100 (Series I)" | 30625 Rev C4-1, 5 Sept 2008 | p. 9-2 (PDF p. 134) |
  | `tormach-pcnc1100-s2.mch`, "Tormach PCNC 1100 (Series II)" | 31138 Rev B1-4, 12 Aug 2011 | p. 9-2 (PDF p. 144) |
  | `tormach-pcnc1100-s3.mch`, "Tormach PCNC 1100 (Series 3)" | UM10087 1013A, 2013 | p. 121 (PDF p. 121) |

  | Figure | PCNC 770 (B1-4) | PCNC 770 S3 | PCNC 1100 S I | PCNC 1100 S II | PCNC 1100 S3 |
  |---|---|---|---|---|---|
  | Travel X x Y x Z (in) | 14 x 7.5 x 13.25 | 14 x 7.5 x 13.25 | 18 x 9.5 x 16.5 | 18 x 9.5 x 16.5 | 18 x 9.5 x 16.5 |
  | Spindle rpm | 175-10200 | 175-10200 | 300-4500 | 100-5140 | 100-5140 |
  | Low / high belt (rpm) | 175-3250 / 525-10200 | 175-3250 / 525-10200 | 300-1750 / 800-4500 | 100-2000 / 250-5140 | 100-2000 / 250-5140 |
  | Motor (stated) | 1 hp (1.5 hp peak) | 1 hp (1.5 hp peak) | 1.5 hp (2 hp peak) | 1.5 hp (2 hp peak) | 1.5 hp (2 hp peak) |
  | `spindle.power` (kW) | 0.75 | 0.75 | 1.12 | 1.12 | 1.12 |
  | Rapid X, Y / Z (IPM) | 130 / 100 | 135 / 100 | 65 / 65 | 90 / 65 | 110 / 90 |
  | Taper | R8 | R8 | R8 | R8 | R8 |
  | Rigid tapping | no | no | no | no | no |
  | Coolant | M7 mist, M8 flood | same | same | same | same |

- **Which machine each manual covers.** The task named two of the
  manuals differently from what they say, so the names come from each
  manual's own text:
  - 30625 Rev C4-1 says only "PCNC 1100". The Series II manual's revision
    history (31138 p. 10-19, PDF p. 217) says "Rev C4-2 ... This version
    of PCNC Series I is base manual for this documentation", so the C4
    manual is the Series I's. Its spindle (300-4500 rpm, DC injection
    braking) and its 65 IPM rapid on all three axes differ from every
    later 1100, so it ships as its own file. It is not the Series 3.
  - UM10087 1013A names itself "PCNC_1100_SERIES_3" in every page footer.
    It is the Series 3 manual, and the Series 3 file comes from it. It has
    the same spindle as the Series II, but its rapids are faster
    (110/90 against 90/65 IPM), so the two are separate files.
  - 31725 Rev B1-4 (12 Aug 2011) is the PCNC 770 manual from before the
    Series 3 one. 32426 Rev C1-1's revision history (p. 10-21, PDF
    p. 220) reads "Rev C1-1 17 October 2011 Initial release of Series 3
    manual", directly above B1-4.
- **The two 770 manuals disagree in one place:** the rapid traverse.
  B1-4 says "Rapid traverse X and Y - 130 IPM, Z - 100 IPM" and C1-1
  says 135 IPM for X/Y (both § 9.4.1, p. 9-2, PDF p. 142); Z is 100 IPM
  in both. Every other spec line is word for word the same, except that
  C1-1 adds "polyphase" to the stepper motors.
  - First pass: one file, `tormach-pcnc770-s3.mch`, from C1-1, with a
    fifth file for the B1-4 770 considered and dropped.
  - Revised (the user asked for the earlier 770 to be covered): the B1-4
    770 ships too, as `tormach-pcnc770.mch`, "Tormach PCNC 770". Every
    figure in it was re-read from B1-4's own § 9.4.1 (PDF p. 142), not
    copied from the S3 file: travel 14" x 7.5" x 13.25"; "Spindle Speed
    Range RPM - 175 to 10200 ... from a 1 hp induction motor (1.5 hp
    peak)"; "Low belt speed is 175 - 3250 RPM, high belt is 525 - 10200
    RPM"; R8; rapid 130 / 100 IPM. It matches the S3 file in every
    figure except the X/Y rapid. Coolant is § 7.8.4 (p. 7-34, PDF p. 120:
    "To turn mist coolant on, program: M7", M8 flood). Tapping is § 7.9.1
    "Self-reversing Tapping Cycles" (p. 7-34, PDF p. 120: M871-M874 for
    the auto-reverse tapping heads, which § 8 lists as supplied
    accessories, p. 8-14, PDF p. 138). G84 appears only in the modal-group
    list (p. 7-10). So `rigid_tapping` is false. B1-4's revision history
    (§ 10.4, p. 10-19, PDF p. 215) dates it 12 August 2011 and names
    Rev A1-1 (31 Dec 2009) as the "Initial release of PCNC 770 manual".
    The name has no series word because B1-4 itself says only
    "PCNC 770".
- **Power is the rated figure, not the peak.** `spindle.power` is the
  ceiling the cutting-power check refuses above, so the continuous
  rating is the safe one: 1 hp = 0.75 kW, 1.5 hp = 1.12 kW, at
  0.7457 kW/hp. The PCNC 440 file already does this (0.75 hp, 0.56 kW).
  Peak power would let a cut through that the motor can only hold
  briefly.
- **Feeds:** each manual states only the rapid traverse. As in the
  other Tormach files, it is both `max_rapid_speed` and the feed ceiling
  `max_normal_speed`. No manual states a separate maximum cutting feed.
- **Rigid tapping: `false` on all four.** These are Mach3-controlled R8
  mills. The tapping the manuals document is the auto-reversing tapping
  head driven by the M871-M874 macros (§ 7.9.1: 31138 p. 7-34, PDF
  p. 122; 32426 p. 7-34, PDF p. 120; 30625 p. 7-30, PDF p. 114; UM10087 p. 105),
  and "tapping heads" are listed as accessories. G84 appears only in the
  modal-group list, with no spindle-synchronised cycle described. So the
  Tormach post refuses a tap on these machines by name, as it does on
  the 770M and the PCNC 440.
- **Coolant:** each manual's § 7.8.4 "Coolant Control - M7, M8 and M9"
  says "To turn mist coolant on, program: M7" (same pages as the tapping
  section above), and M8 is flood. Every file has Flood M8 and Mist M7.
- **ATC:** none stated. UM10087 lists an "Automatic Tool Changer" only
  as an available accessory, with no pocket count. `tooling` stays
  unstated, as on the 770M and the 1100MX.
- **Travel** is ±the full stroke in program coordinates, as on every
  Tormach file (#131's finding).
- **Post:** these manuals document Mach3 controls, and the shipped
  Tormach post speaks PathPilot. The files describe the iron, which is
  the same after a PathPilot retrofit. A Mach3-specific post is out of
  scope here.
- **PathPilot Hub / later models:** the first pass had no PathPilot-era
  1100M manual, so it made no 1100M file. That manual is now in hand and
  the 1100M has its own file (next entry). The 770M (UM10539, already shipped) has the
  same belt ranges as the PCNC 770 S3 at the low end, but stops at
  10000 rpm against 10200 and has a 110 IPM Z against 100. They are
  different machines and keep different files.
- **Tormach 1100M (PathPilot era):** `tormach-1100m.mch`, "Tormach
  1100M", from UM10540 "1100M Operator's Manual", Version 0626A
  (copyright 2026). Its printed page numbers equal its PDF page numbers.

  | Figure | Value | Where |
  |---|---|---|
  | Travel X x Y x Z (in) | 18 x 11 x 16.25 | § 2.2 Machine Specifications, p. 33; § 5.1.2, p. 119 |
  | Spindle rpm | 70-7500 | § 2.1.1, p. 32 (ranges); § 2.2, p. 33 ("Spindle Maximum Speed 7500 rpm") |
  | Low / high belt (rpm) | 70-2000 / 250-7500 | § 2.1.1, p. 32; § 5.1.2, p. 119; Spindle Range button, p. 192 |
  | Spindle power | 2 hp (1.5 kW) -> `power` 1.5 | § 2.1.1 "Spindle Power Rating", p. 32; § 2.2, p. 33 |
  | Max feed X, Y / Z (IPM) | 110 / 90 | § 2.1.1 and § 2.2 "Maximum Feed Rate", pp. 32-33 |
  | Transmission / taper | Poly-V belt / R8 | § 2.2, p. 33 |
  | Thread machining | "Tension/Compression, Auto-Reversing, Thread Mill" | § 2.2, p. 33 |
  | Coolant | M07 coolant on, M08 flood, M09 off | § 10.4.5, p. 245 |

  - **Power:** the manual states 2 hp (1.5 kW) as the "Spindle Power
    Rating" and gives no peak figure, so 1.5 kW is its rated figure and
    goes in as stated. This is the same as the 1100MX file.
  - **Rapids:** the manual labels 110/90 IPM "Maximum Feed Rate". As in
    every Tormach file, it is both `max_rapid_speed` and
    `max_normal_speed`.
  - **Rigid tapping: `false`.** The specification lists thread machining
    as "Tension/Compression, Auto-Reversing, Thread Mill", word for word
    the 770M's line, and does not list rigid tapping. The PathPilot G84
    on this machine (§ 10.3.7, p. 241; conversational Tap tab, p. 146)
    starts the spindle forward, feeds to depth, reverses the spindle and
    feeds out. Feed override is ignored and the dwell is automatic. That
    is a cycle for a tension/compression holder, and nothing in the
    manual ties it to a spindle encoder. "G33.1 Rigid tapping" appears
    only in PathPilot's generic G-code list (G-code table, p. 224) and is not
    claimed for this machine anywhere. The post therefore refuses a tap
    on the 1100M by name, as on the 770M. Considered and dropped:
    `rigid_tapping: true` on the strength of the G-code list. That would
    post a spindle-synchronised tap for a machine whose own spec sheet
    does not claim one.
  - **Belts:** two ranges, set by moving the Poly-V belt. PathPilot's
    Spindle Range button must match the belt position (p. 192). The
    post's belt note reads them.
  - **Coolant:** § 10.4.5 says "To turn coolant on, program: M07" and
    "To turn flood coolant on, program: M08". The safety section (p. 27)
    speaks of "flood or MQL (mist) coolant", so M07 is the mist/MQL
    output, and the file carries Flood M8 and Mist M7 like every other
    Tormach file.
  - **ATC:** the manual installs an optional Automatic Tool Changer
    (§ 4.7.4, p. 71) but states no pocket count. `tooling` stays
    unstated, so the tool-count check is off.
  - **1100M+: no file.** UM10540 never mentions an "1100M+" or any
    variant of the 1100M with other figures. "1100M/MX" appears only
    where shared accessories are named. No figure for the M+ is in hand,
    so making a file for it would mean guessing, and that is a silent
    wrong answer. Considered and dropped: copying the 1100M file under
    the M+ name. It can be added the same way once its manual is in
    hand.
  - **PathPilot Hub:** the Hub's "1100M" simulator is this machine. The
    Hub's "1100M+" has no matching file (above). The Mach3-era PCNC
    files have no Hub simulator by their own names.
- **Coverage of #170's title** ("1500MX, 1100MX, 1100, 770, 770M, PCNC
  440"): 1500MX, 1100MX, 770M and PCNC 440 shipped in the first pass.
  "1100" is covered by the PCNC 1100 Series I, II and 3, and "770" by
  the PCNC 770 (B1-4) and PCNC 770 Series 3. The 1100M has a file now
  too. Every model named in the title, plus the 1100M, has a file. The
  1100M+ is the only Hub machine without one, and it is not in the
  title.
- **Cost to change later:** one figure in a `.mch` plus its row in
  `tests/tormach_machines.rs`.
