## #170 - Machine files for the Mach3-era PCNC 770 and PCNC 1100 (2026-11-08)

- **Chosen:** four more inch `.mch` files, one per distinct machine the
  five manuals in hand describe. Each figure comes from the manual's
  § 9.4 "Specifications" (§ 9.1 in UM10087). The figures:

  | File / model | Manual (part no., rev, date) | Spec page |
  |---|---|---|
  | `tormach-pcnc770-s3.mch`, "Tormach PCNC 770 (Series 3)" | 32426 Rev C1-1, 17 Oct 2011 (cover: "0412A") | p. 9-2 (PDF p. 142) |
  | `tormach-pcnc1100-s1.mch`, "Tormach PCNC 1100 (Series I)" | 30625 Rev C4-1, 5 Sept 2008 | p. 9-2 (PDF p. 134) |
  | `tormach-pcnc1100-s2.mch`, "Tormach PCNC 1100 (Series II)" | 31138 Rev B1-4, 12 Aug 2011 | p. 9-2 (PDF p. 144) |
  | `tormach-pcnc1100-s3.mch`, "Tormach PCNC 1100 (Series 3)" | UM10087 1013A, 2013 | p. 121 (PDF p. 121) |

  | Figure | PCNC 770 S3 | PCNC 1100 S I | PCNC 1100 S II | PCNC 1100 S3 |
  |---|---|---|---|---|
  | Travel X x Y x Z (in) | 14 x 7.5 x 13.25 | 18 x 9.5 x 16.5 | 18 x 9.5 x 16.5 | 18 x 9.5 x 16.5 |
  | Spindle rpm | 175-10200 | 300-4500 | 100-5140 | 100-5140 |
  | Low / high belt (rpm) | 175-3250 / 525-10200 | 300-1750 / 800-4500 | 100-2000 / 250-5140 | 100-2000 / 250-5140 |
  | Motor (stated) | 1 hp (1.5 hp peak) | 1.5 hp (2 hp peak) | 1.5 hp (2 hp peak) | 1.5 hp (2 hp peak) |
  | `spindle.power` (kW) | 0.75 | 1.12 | 1.12 | 1.12 |
  | Rapid X, Y / Z (IPM) | 135 / 100 | 65 / 65 | 90 / 65 | 110 / 90 |
  | Taper | R8 | R8 | R8 | R8 |
  | Rigid tapping | no | no | no | no |
  | Coolant | M7 mist, M8 flood | same | same | same |

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
  B1-4 says X/Y 130 IPM and C1-1 says 135 IPM (both PDF p. 142); Z is
  100 IPM in both. Every other spec line is word for word the same,
  except that C1-1 adds "polyphase" to the stepper motors. Resolution:
  the later C1-1 wins (135/100), and the file is named for the Series 3
  that C1-1 covers. The earlier 770 is 5 IPM slower in X/Y, which is not
  enough to justify a file of its own. Considered and dropped: a fifth
  file for the B1-4 770. If an earlier-770 owner needs it, that file
  would differ only in the X/Y rapid.
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
- **PathPilot Hub / later models:** none of these manuals covers a
  PathPilot-era model such as the "1100M" that PathPilot Hub lists, so no
  file is made for one, and whether its specs match the Series 3 is not
  stated in anything in hand. The 770M (UM10539, already shipped) has the
  same belt ranges as the PCNC 770 S3 at the low end, but stops at
  10000 rpm against 10200 and has a 110 IPM Z against 100. They are
  different machines and keep different files.
- **Still not covered from #170's title:** the PCNC 1100 Series 3 is
  covered now, and so is the earlier "1100 (non-MX)". The "770 (non-MX)"
  is covered as the PCNC 770 Series 3. Every model named in the title now
  has a file.
- **Cost to change later:** one figure in a `.mch` plus its row in
  `tests/tormach_machines.rs`.
