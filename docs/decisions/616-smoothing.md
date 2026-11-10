## #616 - per-operation smoothing, opt-in, per dialect

Requested by @spikeypineapple (#616): smoothing modes differ per control, and the "auto" behaviour is maximum accuracy on finishing and aggressiveness following the stock left on roughing.

- **Opt-in, document-wide:** `PostOptions.smoothing`, default **off**.
  - **Off:** the stream carries no smoothing PARAM, so every program posts byte-identical to before.
  - **On:** each operation's section carries `PARAM Smoothing <mm>` and `PARAM SmoothingMode finish|rough`.
  - **Why opt-in:** several controls need a paid option for smoothing (Fanuc's AI contour control), and a word an unequipped control rejects is worse than none. It also keeps the golden programs unchanged.
  - **Persistence:** saved as the additive `smoothing` project key, absent when off, the same pattern as `skip_stops` / `per_op_files`. It's a checkbox under the post picker for every mill post, undoable via `SetPostOptions`.
- **The ask per operation** (`app::smoothing::of`):
  - **Finish:** an op stating a contour `tolerance` and leaving no stock (Contour, Chamfer2D, Finish3D, Contour5D, Parallel5D, Swarf, AdvancedSwarf, RotaryContour) asks its tolerance, with a 0.01 mm floor if it states none.
  - **Rough:** an op leaving stock (Contour with leave, Pocket, Adaptive2D, Offset2D, Slot, Circular, Bore, Pocket3D, RotaryOffset, MultiAxisRough) asks **stock_to_leave × 0.2, clamped to 0.01–0.25 mm**. That's a fifth of the allowance, so a rounded corner never eats more than 20 % of what the finish pass will remove, capped at 0.25 mm. Dropped: a direct stock-to-leave = tolerance mapping, which would let a 0.5 mm-leave rough round corners by the whole allowance.
  - **Nothing:** drilling, tapping, probing, threading, facing and the lathe kinds.
- **Words per dialect**, each from the control's public programming documentation. Each is written after the section preamble and undone at the section end, so no tolerance leaks into the next op:

  | Post | On | Off |
  |---|---|---|
  | Tormach / LinuxCNC | `G64 P<tol>` | `G64 P0.001` / `P0.025` (the header's) |
  | Fanuc | `G5.1 Q1` | `G5.1 Q0` |
  | Haas | `G187 P3 E<tol>` (finish) / `G187 P1 E<tol>` (rough) | `G187` |
  | Siemens 840D | `CYCLE832(<tol>,_FINISH\|_ROUGH,1)` | `CYCLE832()` |
  | Heidenhain (ISO) | `G62 T<tol>` | `G62` |
  | Mazak | one note "SMOOTHING NOT WRITTEN - NO MAZAK FORM STATED" | nothing |

- **Fanuc R level not written:** `G5.1 Q1 R<1-10>`'s R range is control-version dependent (as the requester noted), so only Q1/Q0 is written. A machine-file or post property for the R level can follow with #614 (the Doosan DNM), whose sample program will show the range that control accepts.
- **Mazak named, not guessed:** no smoothing form is stated from public Mazak documentation yet, and a guessed word could alarm the control.
- **Not done here:** the lathe post (Fanuc Lathe writes no smoothing; the option isn't offered for it), and a per-op override (the ask is derived; an explicit per-op field can follow if wanted).
