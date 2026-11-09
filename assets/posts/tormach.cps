// Tormach PathPilot 3X milling post (#131). Provenance: the G-dialect
// from Tormach's "UM10587: 770MX Operator's Manual" (Version 0626A),
// chapter 10 (Programming: G-codes, canned cycles, M-codes), and §7.2.7 /
// §7.5.10 (G30); Tormach's published PathPilot pages; and the public
// LinuxCNC G-code reference (linuxcnc.org/docs, the interpreter PathPilot
// is built on) - behavior only, per this repo's Clean-Room Protocol.
// Cross-checked against programs PathPilot's own Conversational screens
// write. The hook contract from Autodesk's published post docs. No
// vendor post script text and no CAM post's output used; original work.
// Structure follows our own haas.cps; the goldens lock this file's
// decisions, not any vendor's.
description = "Tormach PathPilot (3-axis mill, G20/G21 from the machine, G73/G81/G82/G83/G84/G86/G89, G2/G3, G98)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";       // from the WorkOffset PARAM
var sRpm = 0;              // spindle/T order follows the stream: PARAMs,
var spindleWait = false;   // TOOL, SECTION - the preamble writes them
var toolWait = false;      // when the first motion needs them legal
var tNo = 0;
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycleSig = "";         // g|Z|R|Q of the open group (modal X Y holes)
var retractSet = false;    // G98 written once per program
var coolant = "";          // the M-code this program started, if any
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var inch = false;          // the machine is programmed in inches (onOpen)

// ------------------------------------------------------------ helpers
// The stream speaks mm and mm/min. An inch machine gets every length and
// feed divided by 25.4 and written at four decimals (a tenth of a
// thousandth); a metric one keeps mm at three. Angles and rpm never scale.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function len(v) { return inch ? v / 25.4 : v; }
function fu(v) {  // a length, in the machine's unit
  if (!has(v)) return "0";
  return trim(Number(len(v)).toFixed(inch ? 4 : 3));
}
function ff(v) {  // a feed: whole mm/min, or in/min at two decimals
  if (!has(v) || v <= 0) return "0";
  return inch ? trim((v / 25.4).toFixed(2)) : String(Math.round(v));
}
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; } // integer word
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
function block(words) {  // join the non-empty words onto one line
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("(" + s + ")");
}
var tapRefused = false;   // the rigid-tap refusal, once per program
var tiltRefused = false;  // one named refusal per program, not per line
function tiltOk(p) {  // this post is 3-axis: a tilted record is refused
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - Tormach PathPilot post has no RTCP output"); }
  return false;
}

// The .mch tool-station coolant list carries names + M-codes; the
// first Flood entry (else the first entry at all) is this machine's
// cutting-fluid-on code. M9 (stop) is the RS-274 fixed pair.
function coolantCode() {
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  var first = "";
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = (typeof c === "string") ? c : (c.name || "");
    if (!code) continue;
    if (name === "Flood") return code;
    if (!first) first = code;
  }
  return first;
}

// ------------------------------------------------------------ the plane
// The arc's plane (G17 XY, G18 XZ, G19 YZ), stated modally: written only
// on change, taken back to G17 before any canned cycle and at section end.
var planeG = 17;
function planeOf(p) {
  return p.CircularPlane === "XZ" ? 18 : (p.CircularPlane === "YZ" ? 19 : 17);
}
function planeWord(g) {  // "" while modal; the word (and the new state) on change
  if (g === planeG) return "";
  planeG = g;
  return "G" + g;
}
function restorePlane() { var pw = planeWord(17); if (pw) block([pw]); }
// The center words of the arc's plane as increments from the start
// point (G91.1, stated in the header): a shared translation cancels in
// an increment, so the origin must NOT enter them.
function centerWords(p) {
  var c = p.IJKIncrement || {};
  return [
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    w("K", has(c.K) && has(p.StartZ) ? c.K - p.StartZ : null),
  ];
}

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// The stream's PARAMs OperationName / OperationKind / SetupName /
// ToolDiameter / ToolKind / ToolCornerRadius and the TOOL record's name
// feed it; a stream without them (a legacy .cl) writes none. User text
// is cleaned for this dialect: a parenthesis would end the comment
// early, % is the tape's end mark and ; opens a comment on some
// controls, so all three become spaces; printable ASCII only, capped at
// 40 characters.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function joinDash(parts) {  // the non-empty parts, " - " between
  var a = [];
  for (var i = 0; i < parts.length; i++) if (parts[i]) a.push(parts[i]);
  return a.join(" - ");
}
function note(s) { block(["(" + s + ")"]); }
function commentParam(name, value) {
  if (name === "SetupName") setupName = clean(value);
  else if (name === "OperationName") opName = clean(value);
  else if (name === "OperationKind") opKind = clean(value).toUpperCase();
  else if (name === "ToolKind") toolKind = clean(value).toUpperCase();
  else if (name === "ToolDiameter" && has(value)) toolDia = Number(value);
  else if (name === "ToolCornerRadius" && has(value)) toolCr = Number(value);
}
function opNotes() {  // at the section's preamble: the setup once, the op each time
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + joinDash([opName, opKind, wcsWord]));
  opName = ""; opKind = "";
}
function toolNote() {  // just before the tool change it describes
  var desc = [];
  if (toolKind) desc.push(toolKind);
  if (toolDia > 0) desc.push("D" + fu(toolDia));
  if (toolCr > 0) desc.push("R" + fu(toolCr));
  note(joinDash(["T" + ni(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}

// -------------------------------------------------------------- header
// Tormach's post guidelines ask for no % or $ at the program's start or
// end, and LinuxCNC reads an O word as a subroutine label, not a program
// number - so the header is one identity comment and the modal state
// every later line relies on: the unit, absolute positions, incremental
// arc centers (G91.1, stated, never assumed), feed per minute. Path
// blending gets an explicit tolerance on its own line: a bare G64 is
// "no tolerance" in plain LinuxCNC, and a stated P reads the same on
// every build (0.001 in, or its 0.025 mm twin).
function onOpen() {
  inch = typeof machine !== "undefined" && machine.units === "inch";
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  block(["(" + nm + ")"]);
  block([inch ? "G20" : "G21", "G17", "G90", "G91.1", "G94", "G40", "G49", "G80"]);
  block(["G64", inch ? "P0.001" : "P0.025"]);
  var z = safeZ();
  if (z !== null && z !== 0) block(["(SAFE RETRACT G53 Z" + fu(z) + " - STATED BY THE MACHINE)"]);
}

// #495: where "straight up" is, in machine coordinates. G53 G0 Z0 is the
// top of Z travel only where Z homes at the top with machine Z0 there -
// the PathPilot convention this post was written for (a Tormach homes Z
// at the top of its travel). A LinuxCNC machine homed with Z0 at the bottom
// would rapid DOWN into the part on that line. So the machine file
// states the height (its Z part's `safe_retract`, `machine.safeRetractZ`,
// mm); a document with no machine, or a Tormach model that states none,
// keeps the PathPilot convention (0); any other machine stating none is
// refused by name - never a guess about which way is up. Judged once.
var safeZDone = false;
var safeZValue = null;     // null = refused, nothing is written
function safeZ() {
  if (safeZDone) return safeZValue;
  safeZDone = true;
  var m = typeof machine !== "undefined" ? machine : null;
  var stated = m ? m.safeRetractZ : null;
  if (has(stated)) {
    safeZValue = Number(stated);
    return safeZValue;
  }
  var nm = (m && (m.name || m.model)) || "";
  if (nm === "" || /^\s*tormach\b/i.test(nm)) {
    safeZValue = 0;
  } else {
    msg("machine " + nm + " does not state its safe Z retract - this post lifts Z with G53 G0 before each tool change and at the end, and machine Z0 is the top of travel only on a Tormach; state the Z axis's safe_retract (machine coordinates) in the machine file");
  }
  return safeZValue;
}

// Z straight up in machine coordinates, then G30 to the operator's preset
// tool-change spot. The G30 rides PathPilot's convention with the Z0
// retract: a machine that states a different safe Z is not on it, and an
// unset G30 position (LinuxCNC's default is the machine origin) could
// drive Z back down - so there the retract stands alone and M6 / the
// control's own tool-change position does the rest.
function retract() {
  var z = safeZ();
  if (z === null) return;
  block(["G53", "G0", "Z" + fu(z)]);
  if (z === 0) block(["G30"]);
}

// Cutter compensation in the control: not stated by this post yet -
// refused by name, never a centre path under a Control op.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not supported by this post - choose In computer, or the Fanuc post");
}

function onParameter(name, value) {
  commentParam(name, value);
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
}

function onSpindleSpeed() {
  sRpm = Number(properties.SpindleSpeed) || 0;
  spindleWait = true;
  if (typeof machine !== "undefined" && machine.spindleMax > 0 && sRpm > machine.spindleMax) {
    msg("spindle rpm over machine maximum");
  }
}

// PathPilot's tool table numbers tools 1-1000; anything else has no
// offset to apply, so it is refused by name, never posted.
function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  if (!has(number) || number < 1 || number > 1000) {
    msg("tool number " + ni(number) + " is outside PathPilot's tool table (1-1000)");
  }
}

// The belt range is a physical belt move that no G or M code can make,
// and PathPilot's Spindle Range setting must match it. The ranges are
// the machine's own (`.mch` spindle.ranges, #170 - e.g. the 770MX's
// low 175-3250 rpm and high 525-10000 rpm). When a speed fits only one
// range the program says which, as a comment the operator sees before
// the spindle starts; a speed several ranges serve says nothing, and a
// machine stating fewer than two ranges has no belt to name. A speed
// past every range names the nearest one (the rpm check refuses it).
function beltNote(rpm) {
  var rs = (typeof machine !== "undefined" && machine.spindleRanges) || [];
  if (rs.length < 2 || !(rpm > 0)) return "";
  var lo = 0, hi = 0, serving = [];
  for (var i = 0; i < rs.length; i++) {
    if (rs[i][0] < rs[lo][0]) lo = i;
    if (rs[i][1] > rs[hi][1]) hi = i;
    if (rpm >= rs[i][0] && rpm <= rs[i][1]) serving.push(i);
  }
  var pick = -1;
  if (serving.length === 1) pick = serving[0];
  else if (serving.length === 0) pick = rpm > rs[hi][1] ? hi : (rpm < rs[lo][0] ? lo : -1);
  if (pick < 0) return "";
  var name = rs.length === 2 ? (pick === lo ? "LOW" : "HIGH")
    : ni(rs[pick][0]) + "-" + ni(rs[pick][1]) + " RPM";
  return "(SPINDLE " + ni(rpm) + " RPM NEEDS THE " + name + " BELT RANGE)";
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: plane, offsets, tool, spindle, coolant. Tormach
// recommends a G30 before every tool change (no Tormach document says
// M6 moves to a safe position on its own), run in G90 as the guidelines
// require. A bare G30 may move X, Y and Z together, so Z goes straight
// up first (G53 G0 Z0, the top of travel in machine coordinates, or the
// machine's stated safe Z - #495, safeZ) - the
// same Z-first order PathPilot's own programs use between tools - and
// only then G30. M6 then runs the changer, or pauses for a manual change
// when the tool is not in the tray, and G43 H applies the length the
// tool table holds (measured by the tool setter).
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  opNotes();
  block(["G17", "G90", wcsWord, "G40", "G49", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    toolNote();
    retract();
    xyUnknown = true;        // G30 (or the changer) may have moved X and Y
    block(["T" + ni(tNo), "M6"]);
    block(["G43", "H" + ni(tNo)]);
    toolWait = false;
  }
  if (spindleWait) {
    block([beltNote(sRpm)]);
    block(["S" + ni(sRpm), "M3"]);
    spindleWait = false;
  }
  coolant = coolantCode();
  if (coolant) block([coolant]);
}

function endPreamble() {
  if (!sectionOpen) return;
  restorePlane();            // the plane never leaks past its section
  sectionOpen = false;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (coolant) block(["M9"]);
  block(["M5"]);
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ------------------------------------------------------------- motions
// X and Y are unknown at the program's start and after every G30: the
// first move from there would start wherever the table happens to be, a
// diagonal from an unknown point (PathPilot warns "Axis XY not specified
// by this or previous motions"). So that move is preceded by a G0 to its
// X Y alone, made at the current - safe, Z-up - height, and only then
// does Z come down: PathPilot's own XY-first order. A rapid positions at
// its target; a feed or arc at the point the stream says it starts from
// (the target when the stream names none); a cycle at its first hole.
var xyUnknown = true;
function xyFirst(x, y) {
  if (!xyUnknown || !has(x) || !has(y)) return;
  block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  xyUnknown = false;
}
// A feed or arc as the first move after a tool change starts, in the
// stream, from the generator's own start point - its clearance height
// (a face plunges from START). After the G30 the tool stands at the top
// of Z travel, so feeding from there would crawl the whole stroke at the
// plunge rate. Once X Y are positioned, a rapid takes Z down to that
// start height - where the generator says the tool already is - and the
// feed runs only the stretch it planned (proven on the 770MX, #131).
function startXY(p) {
  if (!xyUnknown) return;
  if (has(p.StartX) && has(p.StartY)) xyFirst(p.StartX, p.StartY);
  else xyFirst(p.X, p.Y);
  if (!xyUnknown && has(p.StartZ) && has(p.Z) && p.StartZ > p.Z) {
    block(["G0", wo("Z", p.StartZ, orgZ)]);
  }
}

// While a canned-cycle group is open its expanded hole chain is the
// cycle block's business only.
function onRapid() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  xyFirst(properties.X, properties.Y);
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  startXY(properties);
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  var p = properties;
  startXY(p);
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}

// ------------------------------------------- protected probe moves
// #174. The positioning feeds of a probe cycle (the descent to a touch's
// start, the travel between touches, the rise) are protected moves:
// G38.3 - the LinuxCNC G-code reference's "G38.n Straight Probe": probe
// toward the point, stop on contact, NO error when no contact comes -
// then #5070 (G38 probe result: 1 when the probe tripped, set by
// G38.3/G38.5) is tested in an `o<n> if [...]` / `o<n> endif` block
// (O-codes, same reference), and a trip shows a (MSG,...) and ends the
// program with M2: an obstacle where the model has none stops the run
// instead of the stylus being driven into it, and is never resumed past
// as M0 could be. G38.3 is itself an error when the probe is already
// tripped at the start - the control's own guard. Tormach's PathPilot
// machine-code reference lists G38.2-G38.5 and M2 as supported. The
// O-word numbers run from 5001, a range of their own.
var guardO = 5000;
function onProbeGuard() {
  startPreamble();
  var p = properties;
  startXY(p);
  block(["G38.3", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  guardO += 1;
  block(["o" + guardO, "if", "[#5070 EQ 1]"]);
  block(["(MSG, PROBE HIT ON POSITIONING)"]);
  block(["M2"]);
  block(["o" + guardO, "endif"]);
}

// ------------------------------------------------------- canned cycles
// One block per hole, holes of an unchanged cycle continuing modally
// with bare X Y lines. LinuxCNC's dwell P is in SECONDS (not the
// milliseconds Fanuc-family controls read).
function secs(s) { return trim(Number(s).toFixed(3)); }
function onCycle() {
  startPreamble();
  restorePlane();            // a cycle drills along G17's Z
  var p = properties;
  var dwell = has(p.P) && p.P > 0 ? p.P : 0; // P is seconds
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = dwell > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "BORE") g = "G86";
  else if (p.CycleType === "BACKBORE") g = "G89";
  // Tormach documents G84 for PathPilot: F is the pitch times the rpm
  // (the stream's synthesized tap feed), no P - PathPilot sizes the
  // bottom dwell itself, and the MX mills tap from the spindle encoder.
  else if (p.CycleType === "TAP") {
    // A machine that says it cannot rigid-tap (the R8 770M, 1100M, PCNC 440
    // and the Mach3-era PCNC 770 / 1100: tapping heads, tension/compression
    // holders and thread milling only) refuses the
    // cycle by name rather than posting a G84 its spindle cannot follow.
    if (typeof machine !== "undefined" && machine.rigidTapping === false) {
      if (!tapRefused) tapRefused = true; else return;
      msg("rigid tapping (G84) is not available on " + (machine.name || "this machine") + " - use a tapping head or thread milling");
      return;
    }
    g = "G84";
  }
  xyFirst(p.X, p.Y);         // a cycle as the first move: its first hole
  var sig = g + "|" + p.Z + "|" + p.R + "|" + p.Q;
  if (inCycle && sig === cycleSig) {
    block([wo("X", p.X, orgX), wo("Y", p.Y, orgY)]);
  } else {
    if (inCycle) block(["G80"]);
    // The chain retracts to its initial (clearance) level each hole.
    if (!retractSet) { block(["G98"]); retractSet = true; }
    // Q is an increment (peck depth), not a position: no origin shift.
    var words = [g, wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), wo("R", p.R, orgZ)];
    if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
      words.push(w("Q", p.Q));
    }
    if (dwell > 0 && (g === "G82" || g === "G86" || g === "G89")) words.push("P" + secs(dwell));
    words.push(fw(p.FeedRate));
    block(words);
    cycleSig = sig;
  }
  inCycle = true;
}

// ------------------------------------------------------------ footer
// G53 G0 Z0 lifts the spindle straight up to the top of Z travel in
// machine coordinates (Tormach's Z home is at the top; another machine
// states its own height, #495 - see safeZ), whatever the
// stored G30 position holds; then G30 takes the machine to the
// operator's preset G30 location (the unload spot), the way PathPilot's
// own programs end. M30 ends the program (no % fence).
function onClose() {
  endPreamble();
  retract();
  block(["M30"]);
}
