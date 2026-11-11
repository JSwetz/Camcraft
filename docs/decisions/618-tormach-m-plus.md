# #618 - Tormach 770M+ and 1100M+

**Chosen:** each M+ is its M machine with the matching MX's axis speeds:
- **770M+:** the 770M's travels (14 / 7.5 / 13.25 in), its 175-10000 rpm over two belts, 1.1 kW and no rigid tapping. Speeds are the 770MX's: 300 IPM on X and Y, 250 on Z.
- **1100M+:** the 1100M's travels (18 / 11 / 16.25 in), its 70-7500 rpm over two belts, 1.5 kW and no rigid tapping. Speeds are the 1100MX's: 300 IPM on X and Y, 230 on Z.

**Why:** Tormach never published an M+ manual. The M+ is an upgrade kit for an M, and the contributor relayed Tormach's word on 2026-11-09: "Everything is M except they have MX axis speeds". So the issue's "cite the manual" is met by the M and MX manuals already cited in `tormach_machines.rs` (UM10539 770M, UM10540 1100M, UM10587 770MX, UM10586 1100MX).

**Pinned** by `an_m_plus_is_its_m_with_the_mx_axis_speeds`, which compares the shipped files, not copied numbers. A later fix to an M or MX file is caught as a mismatch.

**Considered and dropped:**
- **Giving the M+ the MX's rigid tapping or 10000 rpm 1100 spindle:** that's not part of the kit per the contributor, so tapping stays refused by name, like the M.
- **A tool-changer count:** the M files state none, so neither do these.
