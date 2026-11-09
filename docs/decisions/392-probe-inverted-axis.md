## #392 - Probing on a machine with an inverted X / Y axis

Part of #372; follows #374 (PR that mirrored the motion stream and refused
probing by name). Decisions taken unattended.

### The translation: negate every probing coordinate on the inverted axis

**Decided:** `post::invert` mirrors TOUCH (target, expected trip), RETREAT
(incremental delta, nominal end) and FOUND (each axis's expected value) on
an inverted X / Y, exactly like a move: `v -> -v`. The scripts' macro code is
unchanged.

**Why (the frame):** the controller's machine position is `M = G54 + P` and
the tip physically stands at `S * M` (`S = -1` on the inverted axis). For the
setup's origin at physical machine position `D`, the posted `P = S (W -
origin)` lands the tip at `D + W - origin` only if G54 holds `S * D` - the
part datum in controller coordinates. A part sitting `d` off the model then
trips at controller `#5061 = S (W_trip + d - origin)`; the translated expected
value is `S (W_trip - origin)`, so the deviation `found - expected` is `S d`
and `G10 L2 X[#5221 + dev]` writes `S (D + d)`: the real part's datum in
controller coordinates. The back-off is an increment, which flips with the
axis exactly as a point does. The sign-blind parts of the macro (which axis a
touch travels along, the tolerance on `ABS[dev]`) are unaffected.

**Proved, not assumed:** `crates/app/tests/inverted_probe.rs` runs the
posted program on a test-only macro B controller (`tests/macro_b/`: G0/G1
swept for collisions, G31 tripping where the stylus first meets a simulated
part, G90/G91, G54..G59 registers `#5221+20k`, G10 L2, `#` arithmetic with
brackets, ABS and unary minus, IF..THEN / GOTO, `#3000` alarm, `#3006` / M0
stop; anything else is an error). For face (-Z, -X), edge, both corner
senses, bore and boss, on a standard, inverted-X, inverted-Y and inverted-XY
machine, with the part shifted (0.37, -0.21, -0.13) mm: G55 ends equal to the
physical datum in controller coordinates on every found axis, G54 untouched,
other axes unwritten. A part out of reach alarms "no contact" and writes
nothing; one past the tolerance alarms. The converse: the standard-sign
program on an inverted machine does not find the part, and undoing any one
translated word (touch target, back-off delta, found expected) after the
engine's translation crashes the stylus into the part, alarms, or probes past
its overtravel instead of reporting no contact.

**Dropped:** writing the deviation with a sign flip in the scripts (a second
copy of the mirror law in every dialect, the shape #374 rejected); modelling
tool length (Z is never inverted, so G43 shifts every program alike).

### Who may post it: a second manifest flag, Fanuc only

**Decided:** a script declares `probeAxisInversion = true` once its probing
blocks pass the interpreter test; only `fanuc.cps` does. Haas declares
`axisInversion` (motion verified) but not this, so it refuses probing on an
inverted axis by name: "probing with a post script that does not declare
probeAxisInversion = true (Fanuc does) is not supported for an inverted axis
yet". Mazak, Sinumerik, Heidenhain, Tormach and the lathe already refuse the
whole inverted machine (no `axisInversion`). `invert::translate` keeps
refusing probing; `invert::translate_with(.., probing)` is the flagged form.

**Why:** Haas's probing hooks live in draft PR #360 (not on main), so there
is nothing on main to verify; when #360 lands, Haas adds the flag together
with its own interpreter cases (the reader already covers the Fanuc-family
macro subset), which also closes #392 (this PR is `Refs #392`). A flag keeps
"verified" a stated claim per script rather than inherited from the motion
flag.

**Dropped:** reusing `axisInversion` for probing (would silently opt Haas -
and any user script - into unverified probing the moment it gained hooks).

### PR #386 (#377) compatibility

Main writes a negative expected value as `#190=#100--69.5`; on an inverted
axis that is the common case, since positive model coordinates turn
negative. #386 folds it into `#190=#100+69.5`. The reader accepts both
(binary minus then unary minus), and the suite was run green against #386's
`fanuc.cps` as well. Whether a real control accepts `--` is #377's question,
not this one; the fold in #386 removes it for both machine kinds.

### Cost to change later

Adding a dialect: its probing goldens plus a case list in `inverted_probe.rs`
run against its script, then the flag. Lifting the Haas refusal is one
manifest line once #360 is on main and the cases pass.
