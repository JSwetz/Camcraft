## #669 - a plasma machine kind and the Tormach 1300PL

Sources (clean room, behaviour only): Tormach *1300PL Operator's Manual*
UM10720 v1223A ([UM]), the 1300PL spec sheet 03/24 ([SS]), Tormach's
*PathPilot Post Processor Guidelines* ([PG]) - as cited in the #621
research comment. No commercial CAM post or its output was read.

**1. `Kind::Plasma` is declared, never inferred.** A plasma table's axis
inventory (X/Y/Z, no rotary) is a mill's; inferring plasma from axes would
turn every three-axis file into an ambiguity. So `kind: "plasma"` must be
written, and the inference keeps reading such axes as a mill. *Dropped:*
inferring plasma from "no spindle stated" - most vendor-shaped mill files
state none, and they would silently flip kind. *Cost to change:* none; the
inference table is one match.

**2. The plasma rules refuse by name, at both doors, from one list.**
`post::machine::plasma_refusals` is the single list: no `spindle` on any
head (each stated key named - `spindle.max_speed`, `spindle.power`), no
`live_tool`, no coolant list (station or tooling root), no rotary. The
reader refuses the first, the Machines pane's validator lists all; the
parsed door (`Machine::is_sane` -> `plasma_sane`, the project reader and
`SetSetupMachine`) holds the same rules over the parsed numbers, so a
hand-edited project that grew an rpm on a plasma table is a corrupt file.
*Why refuse rather than ignore spindle keys:* a plasma file stating a
spindle is a malformed file - either the kind or the spindle is wrong, and
guessing which is the silent wrong answer. Coolant is refused for the same
reason ([PG]: no M7/M8/M9 on plasma). *Dropped:* refusing
`tool_station.max_tool_length/diameter` (harmless, unstated anyway) and
`number_of_tools > 1` (a scribe or marker head is a real plasma-table
option; not ours to forbid).

**3. Kerf lives on the TORCH TOOL, as its diameter.** [UM]/Tormach KB:
kerf is ~1.5x the nozzle orifice and is set by the consumables, amperage
and material - confirmed by a test cut. None of that is the table's, so
kerf is not a machine number. Carrying it as the torch tool's `diameter`
means every offset path (tool-radius compensation, G41/G42 with a D
register, the 2D offset engine) works unchanged with radius = kerf/2.
*For #670:* add a torch tool kind whose diameter field is labelled "kerf
width", whose `rpm` is absent (today `Tool::is_sane` requires rpm > 0 -
relax it for torches only), and let the plasma op override kerf per
material if needed (op-level override, defaulting to the tool's). *Dropped:*
a machine-level kerf (wrong owner - one table runs many nozzles), an
op-only kerf (every op would restate the same consumable fact).

**4. Tool 1 convention: stated as `tooling.number_of_tools: 1`.** [UM]
§7: PathPilot plasma always calls `G43 H1`; the torch is Tool 1. The
1300PL file states one tool, so the changer check refuses a program
calling two distinct tools ("carries 1 torch"). Writing `T1`/`H1` is the
post's job (#671).

**5. The machine check on plasma: XY judged, Z and rpm skipped with a
note.** `check_ops` branches to `machine::plasma` for a plasma machine: it
judges X/Y travel (arc quarter points included) and the X/Y share of each
feed against the slowest X/Y ceiling, the changer count, and refuses by
name: any op that is not a plasma op ("a Face operation is milling work - a
plasma table cuts only plasma operations"), any tool stating rpm ("a plasma
table has no spindle to turn"), any tilted or C-turned move. Z travel,
Z feed, depth, rpm and spindle power are skipped, and the skip is named on
the new `MachineCheck.notes` (non-blocking), which the status line's
machine vote prints ("- not judged: ..."). *Why notes, not problems:* the
lifter's Z is the control's ([UM] §6.3 - G15 probe, G16 pierce/cut height,
THC); failing it would block every plasma program for a number the CAM
never writes. *Why not silent:* a green "within limits" that never looked at
Z would be the friendly silent wrong answer.

**6. No op is postable on a plasma table until #670.** The command door
(`family_gate`) refuses creating a milling/turning op on a plasma setup by
name, the ribbon shows no section (`shown_for(Plasma)` is empty), and an op
that was there before the table was mounted blocks the post at the
verification gate (machine check refusal).

**7. The 1300PL figures: the smaller of [UM] and [SS].** Travel - [UM]
§2.2.1 p.17 says X 50 / Y 51.5 / Z 3.9 in; [SS] (03/24) says X 48.5 / Y 50
/ Z 3.9 in. The file states X 48.5 / Y 50 / Z 3.9 (written `-L..L`, the
#457 law): a smaller stroke can only refuse a job the table might have run,
never pass one it cannot. Feeds 1000 IPM X/Y, 120 IPM Z ([UM] p.16).
No rapid is published, so `max_rapid_speed` is left unstated (the
unstated-limit law). Units inch. No `safe_retract`: the plasma post
retracts with G30 ([UM] §4.13), not the mill post's `G53 G0 Z0`.

**8. The post pairing is the registry's, and empty until #671.** The `.mch`
grammar has no post key - machine/post pairing lives in
`posting::post_serves`. A new `PLASMA_POSTS` list (empty) makes every
cutting post decline a plasma table (a mill post would write S/M3 words a
torch has not), `default_post_for(Plasma)` names the first plasma post when
one exists, and `post_offer` offers nothing that would not serve the mount.
So the 1300PL lists no built-in post today; #671 adds "Tormach PathPilot
Plasma" (or its chosen name) to `BUILTINS` and `PLASMA_POSTS`, and the
`the_1300pl_states_its_manuals_figures` pin flips.

**9. `Kind` moved to `post/src/machine/kind.rs`.** `def.rs` was at the
400-line law; the kind enum, its inference and `can_tilt` are their own
seam, re-exported so every `machine::Kind` path reads as before. Likewise
the machine check's per-move helpers moved to `app/src/machine/judge.rs`,
shared by the cutting and the plasma checks.

**Seams left:** #670 - a `Family` arm in `machine::plasma::cuts_plasma`
(exhaustive match, so a new family must answer), `Family::admits` /
`KindInfo::admits` for `Kind::Plasma`, the torch tool kind (kerf =
diameter, no rpm), and the family-gate wording. #671 - `PLASMA_POSTS` +
`BUILTINS` entry; T1/H1, G30 retracts, M200/G15/G16/M205. #672 - the
plasma verification laws own what the machine check skips (through-cut
instead of the stock sim's depth model, cut order, kerf crossings); the
`MachineCheck.notes` lines name what they will judge.
