// Makera Carvera / Carvera Air 3-axis milling post (#707). Provenance:
// the words from the Carvera firmware's supported-codes tables (Makera's
// Smoothieware-based firmware; the tables as documented at
// carvera-community.gitbook.io, firmware -> supported commands -> GCodes
// and MCodes, read 2026-11-10) and the Carvera instruction manual
// (coordinates, tool changer, wireless probe) - behavior only, per this
// repo's Clean-Room Protocol. The hook contract from Autodesk's published
// post docs. No vendor post script text and no CAM post's output used;
// original work. Structure follows our own tormach.cps; drilling as its
// expanded chain follows our own heidenhain.cps.
description = "Makera Carvera (3-axis mill, G21 only, M6 Tn auto tool change, drilling as plain moves, G2/G3 in G17/G18/G19)";
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
var coolant = "";          // the M-code this section started, if any
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs)
var pendingDwell = null;   // a drilling dwell owed at its hole's bottom

// ------------------------------------------------------------ helpers
// The firmware is millimetres only (its G20 "will halt the machine"), so
// every length is mm at three decimals and every feed whole mm/min.
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function trim(s) {
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function fu(v) { return has(v) ? trim(Number(v).toFixed(3)) : "0"; }
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
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
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }  // axis word, origin-relative
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("(" + s + ")");
}
var tiltRefused = false;  // one named refusal per program, not per line
function tiltOk(p) {  // this post is 3-axis: a tilted record is refused
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - the Carvera post is 3-axis (the 4th axis module is not supported yet)"); }
  return false;
}

// The machine file's coolant list: the Carvera's only entry is its air
// blast (M7 "starts the airflow", M9 stops it). The first Flood entry,
// else the first entry at all, is the section's on code.
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
// G17 / G18 / G19 are listed; written only on change, back to G17 at
// section end. The firmware's arc centres are relative to the start
// point (the I J K increments), its default - no G91.1 word exists.
var planeG = 17;
function planeOf(p) {
  return p.CircularPlane === "XZ" ? 18 : (p.CircularPlane === "YZ" ? 19 : 17);
}
function planeWord(g) {
  if (g === planeG) return "";
  planeG = g;
  return "G" + g;
}
function restorePlane() { var pw = planeWord(17); if (pw) block([pw]); }
function centerWords(p) {
  var c = p.IJKIncrement || {};
  return [
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    w("K", has(c.K) && has(p.StartZ) ? c.K - p.StartZ : null),
  ];
}

// ------------------------------------------------------------ comments
// Program commentary (#257): setup, operation and tool, for the operator.
// User text is cleaned: a parenthesis would end the comment early and ;
// opens a line comment, so both become spaces; printable ASCII only,
// capped at 40 characters.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function joinDash(parts) {
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
function opNotes() {
  if (setupName && setupName !== setupSaid) { note("SETUP: " + setupName); setupSaid = setupName; }
  if (opName) note("OPERATION: " + joinDash([opName, opKind, wcsWord]));
  opName = ""; opKind = "";
}
function toolNote() {
  var desc = [];
  if (toolKind) desc.push(toolKind);
  if (toolDia > 0) desc.push("D" + fu(toolDia));
  if (toolCr > 0) desc.push("R" + fu(toolCr));
  note(joinDash(["T" + ni(tNo) + (toolName ? " " + toolName : ""), desc.join(" ")]));
  toolKind = ""; toolCr = 0;
}

// -------------------------------------------------------------- header
// One identity comment, then the modal state every later line relies on
// - only words the supported-codes table lists: G21 (mm), G17, G90
// (absolute), G94 (feed per minute). No G40 / G49 / G80: the table lists
// none of them (no cutter comp, no tool length word, no canned cycles).
function onOpen() {
  if (typeof machine !== "undefined" && machine.units === "inch") {
    msg("the Carvera firmware runs in millimetres only (G20 halts the machine) - the machine file must be metric");
  }
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  note(clean(nm) || "CAMCRAFT");
  block(["G21", "G17", "G90", "G94"]);
}

// No G41 / G42 in the firmware's table: refused by name, never a centre
// path under a Control op.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not in the Carvera firmware - choose In computer");
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

// M6 Tn: T0 is the wireless probe and T-1 "no tool" in the firmware's
// numbering, so a cutting tool is 1 or above; a tool number the
// changer does not hold is the machine check's refusal (the C1's
// tooling.number_of_tools).
function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
  if (!has(number) || number < 1) {
    msg("tool number " + ni(number) + " is not a Carvera cutting tool (T0 is the wireless probe) - number tools from 1");
  }
}

// The preamble closes over everything a raw motion line needs: the work
// offset, the tool, the spindle, the air. M6 Tn is the firmware's auto
// tool change (the Air's quick changer prompts the operator through the
// same word): the firmware fetches the tool and measures its length on
// its own, so no length-offset word follows.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  opNotes();
  block(["G17", "G90", wcsWord]);
  planeG = 17;
  if (toolWait) {
    toolNote();
    xyUnknown = true;          // the changer moved X and Y
    block(["M6", "T" + ni(tNo)]);
    toolWait = false;
  }
  if (spindleWait && sRpm > 0) block(["M3", "S" + ni(sRpm)]);
  spindleWait = false;
  coolant = coolantCode();
  if (coolant) block([coolant]);
}

function endPreamble() {
  if (!sectionOpen) return;
  owedDwell();
  restorePlane();
  sectionOpen = false;
  if (coolant) block(["M9"]);
  block(["M5"]);
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ------------------------------------------------------------- motions
// X and Y are unknown at the program's start and after a tool change: the
// first move positions X Y alone at the current (changer-raised) height,
// and only then does Z come down - never a diagonal from wherever the
// changer left the spindle. A feed's first move rapids Z to the height
// the generator says the tool starts from, then feeds the stretch planned.
var xyUnknown = true;
function xyFirst(x, y) {
  if (!xyUnknown || !has(x) || !has(y)) return;
  block(["G0", wo("X", x, orgX), wo("Y", y, orgY)]);
  xyUnknown = false;
}
function startXY(p) {
  if (!xyUnknown) return;
  if (has(p.StartX) && has(p.StartY)) xyFirst(p.StartX, p.StartY);
  else xyFirst(p.X, p.Y);
  if (!xyUnknown && has(p.StartZ) && has(p.Z) && p.StartZ > p.Z) {
    block(["G0", wo("Z", p.StartZ, orgZ)]);
  }
}

function onRapid() {
  startPreamble();
  if (!tiltOk(properties)) return;
  xyFirst(properties.X, properties.Y);
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties)) return;
  startXY(properties);
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
  dwellAtBottom(properties.Z);
}
function onCircular() {
  startPreamble();
  if (!tiltOk(properties)) return;
  var p = properties;
  startXY(p);
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}

// ------------------------------------------------------------ drilling
// The firmware's G-code table lists no canned cycle (no G73 / G81-G89),
// so no cycle is written: every hole posts as its expanded chain - the
// generator's own plunges, peck retracts and exits, the exact motion the
// simulation verified. A dwell becomes G4 P<seconds> (the table's
// "Dwell P<seconds>") right after the move that reaches the hole's
// bottom. Tapping cannot be plain moves (the feed out of a tap needs the
// spindle reversed in step with it, and the stock machine has no M4), so
// TAP is refused by name.
var DWELL_TOL = 1e-6;
function onCycle() {
  startPreamble();
  owedDwell();
  var p = properties;
  if (p.CycleType === "TAP") {
    msg("tapping is not possible on the Carvera (no canned cycles, and the spindle cannot reverse) - refused, not posted as plain moves");
    return;
  }
  if (has(p.P) && p.P > 0) pendingDwell = { z: Number(p.Z), s: Number(p.P) };
}
function dwellAtBottom(z) {
  if (pendingDwell === null || !has(z) || Math.abs(z - pendingDwell.z) > DWELL_TOL) return;
  block(["G4", "P" + trim(Number(pendingDwell.s).toFixed(3))]);
  pendingDwell = null;
}
// A dwell whose chain never reached its bottom is refused, never lost.
function owedDwell() {
  if (pendingDwell === null) return;
  pendingDwell = null;
  msg("drilling dwell without a feed move to the hole bottom - refused, not dropped");
}

// ------------------------------------------------------------ probing
// Deliberately no onProbeMove / onProbeGuard / onProbeResult. The
// firmware probes with G38.2-G38.5 and its own M46x macros, but how it
// reports a trip (which variables, which frame) is not in the sources
// this post is specced from - so the engine refuses a Probe operation by
// name for this post rather than guess.

// ------------------------------------------------------------ footer
// The table's program end is M2 ("end program, stop spindle and air");
// M30 is listed as "no action on the Carvera", so it is not the end word.
// The program ends where its last retract left the tool - no machine-
// coordinate move is guessed (the manual puts machine zero at the top
// right, but states no safe retract height for a G53 move).
function onClose() {
  endPreamble();
  block(["M2"]);
}
