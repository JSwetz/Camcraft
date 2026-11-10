// Heidenhain TNC 3X milling post - the control family's documented
// DIN/ISO programming mode, not the native conversational language
// (a KLAR-style post is a separate, later candidate; see
// RECOMMENDED.md M3.3 B). Provenance: HEIDENHAIN "TNC 640 User's
// Manual, ISO programming", ID 892909-25, 9/2016 (public user
// documentation, behavior only, per this repo's Clean-Room Protocol)
// for the block form, words and functions (#49, #50); the hook
// contract from Autodesk's published post docs. No vendor script
// text; original work. Where the manual leaves a hole, the post
// refuses by name or the house style decides, and the conformance
// goldens lock that decision, not any vendor's.
description = "Heidenhain TNC (3-axis mill, DIN/ISO mode, numbered * blocks, drilling as G00/G01)";
vendor = "CamCraft (independent, unaffiliated)";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
// TNC 640 User's Manual, ISO programming (ID 892909-25) p. 143: an
// ISO program carries the .I extension (#52).
extension = "I";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)
coolantModes = true; // reads PARAM Coolant (#112/#150): the engine refuses non-Flood ops for a post without it
afterModes = true; // reads PARAM AfterOperation (#110): the engine refuses a stop or home ask for a post without it

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";
var sRpm = 0;
var spindleWait = false;
var toolWait = false;
var tNo = 0;
var coolant = "";
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;
var modalWait = false;  // G40 G90 rides the section's first motion block
var pendingDwell = null; // {z, s}: the open hole's dwell, owed at its bottom
var orgX = 0, orgY = 0, orgZ = 0;
var pgmName = "CAMCRAFT";
var blockNo = 0;

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function fu(v) {  // mm at three decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(3);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
function ni(v) { return has(v) ? String(Math.round(v)) : "0"; }
// #50 - the block form of the manual (ID 892909-25): "The TNC numbers
// the blocks of a part program" (p. 130), each block a line ending in
// `*`, the first `%name G71 *`, the last `N99999999 %name G71 *`, and
// every coordinate word signed (`X+0 Y+38.5`, `Z-40`; pp. 131, 134).
// N steps by 10 as the manual's own programs do; a program long
// enough to reach the end block's number is refused, never wrapped.
var N_STEP = 10, N_LAST = 99999999;
var nRefused = false;
function sgn(letter, v) {  // a signed coordinate word, e.g. X+25 / Z-1.5
  var s = fu(v);
  return letter + (s.charAt(0) === "-" ? s : "+" + s);
}
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
function block(words) {  // one numbered block: N.. words *
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length === 0) return;
  blockNo += N_STEP;
  if (blockNo >= N_LAST && !nRefused) {
    nRefused = true;
    msg("program too long for TNC block numbers (N reaches 99999999, the end block's own number)");
  }
  writeln("N" + blockNo + " " + a.join(" ") + "*");
}
// A motion block: the section's first one also states G40 G90 (the
// manual's positioning form, `G00 G40 G90 Z+250*`, p. 66).
function motion(g, words) {
  var head = [g];
  if (modalWait) { head.push("G40", "G90"); modalWait = false; }
  block(head.concat(words));
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? sgn(letter, v - o) : ""; }
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("; " + s);
}
var tiltRefused = false;  // stage D: one named refusal per program, not per line
function tiltOk(p) {  // RTCP words are Fanuc-family; this dialect refuses them
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - Heidenhain post has no RTCP output"); }
  return false;
}

// The .mch tool-station coolant list carries names + M-codes. A
// flood ask takes the machine's Flood entry; failing one, the first
// coded entry of no other known family (a shop's own "Brine" is its
// flood); a stated codeless Flood is the operator's own valve, silent.
// A list that states only other kinds (Mist, Through-spindle) has no
// flood code: refused by name (#150), never posted as mist or dry. An
// unstated list stays silent (the unstated-limit law). M9 (stop) is
// the RS-274 fixed pair.
var COOLANT_KINDS = ["flood", "mist", "through-spindle", "through-tool"];
function coolantWho() { return coolantOp ? "operation " + coolantOp + ": " : ""; }
function coolantCode() {
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  var other = "", floodNamed = false;
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
    if (name === "flood") { if (code) return code; floodNamed = true; continue; }
    if (code && !other && COOLANT_KINDS.indexOf(name) < 0) other = code;
  }
  if (other || floodNamed) return other;
  if (cs.length > 0) msg(coolantWho() + "the machine states no flood coolant code - pick another coolant for the operation (Off cuts dry) or add a Flood entry to the machine");
  return "";
}

// The op's coolant ask (#112): the stream says nothing for flood (every
// older stream), or carries PARAM Coolant off / mist / through-tool. The
// code is the machine's own entry of that family - the machine is truth;
// an ask the machine states no code for is refused by name, naming the
// operation (#150), never silently posted dry.
function coolantFor(ask) {
  if (ask === "off") return "";
  if (!ask || ask === "flood") return coolantCode();
  var want = ask === "mist" ? ["mist"] : (ask === "through-tool" ? ["through-spindle", "through-tool"] : []);
  if (want.length === 0) { msg(coolantWho() + "unknown coolant ask " + ask); return ""; }
  var cs = (typeof machine !== "undefined" && machine.coolants) || [];
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i];
    var code = (typeof c === "string") ? "" : (c.mCode || "");
    var name = String((typeof c === "string") ? c : (c.name || "")).trim().toLowerCase().replace(/[\s_]+/g, "-");
    if (code && want.indexOf(name) >= 0) return code;
  }
  msg(coolantWho() + "the machine states no " + ask + " coolant code - pick another coolant for the operation or add it to the machine");
  return "";
}

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// The stream's PARAMs OperationName / OperationKind / SetupName /
// ToolDiameter / ToolKind / ToolCornerRadius and the TOOL record's name
// feed it; a stream without them (a legacy .cl) writes none. User text
// is cleaned for this dialect: the block ends at * and the comment
// opens at ;, so neither (nor %, parentheses) survives in the text; a
// comment is a numbered block of its own, `N.. ; text*`, as the TNC
// numbers every block; the work offset is left out, since this dialect
// posts the active preset, not a G54 word; printable ASCII only, capped
// at 40 characters.
var setupName = "", setupSaid = "", opName = "", opKind = "";
var toolName = "", toolKind = "", toolDia = 0, toolCr = 0;
function clean(s) {
  var t = String(s === null || s === undefined ? "" : s);
  t = t.replace(/[()%;*]/g, " ").replace(/[^\x20-\x7E]/g, "?").replace(/\s+/g, " ").trim();
  return t.length > 40 ? t.substring(0, 40).trim() : t;
}
function joinDash(parts) {  // the non-empty parts, " - " between
  var a = [];
  for (var i = 0; i < parts.length; i++) if (parts[i]) a.push(parts[i]);
  return a.join(" - ");
}
function note(s) { block(["; " + s]); }
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
  if (opName) note("OPERATION: " + joinDash([opName, opKind]));
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
// #50: "The first block of a program is identified by %, the program
// name and the active unit of measure" (p. 130): `%NAME G71 *`, G71 =
// mm (p. 755, "at start of program"). The stream carries no program
// name, so the machine's name stands in, cut to the file-name
// characters the manual permits (A-Z a-z 0-9 . _ -, p. 143). The old
// parenthetical identity line is gone: `( )` is no TNC comment form.
function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  pgmName = String(nm).replace(/[^A-Za-z0-9._-]/g, "_") || "CAMCRAFT";
  writeln("%" + pgmName + " G71 *");
}

// Cutter compensation in the control (M13.11, #39): refused by name,
// never a centre path under a Control op (the stream's contour is the
// PART contour). The TNC's documented engage rule (the manuals: the
// block that activates G41/G42 ends with the tool perpendicular to the
// programmed point, compensation taking effect only at its end) is
// not stated against the next element the way the Fanuc start-up is,
// so a perpendicular lead's engage point would be a guess; and the
// radius is always the tool table's R plus DR, so the wear variant's
// "the register holds only the wear delta" has no TNC form.
// docs/decisions/39-comp-dialects.md records the call.
function onRadiusCompensation() {
  msg("control cutter compensation (G41/G42) is not supported by this post - choose In computer, or the Fanuc, Haas, Mazak or Siemens 840D post");
}

// #50: in this mode G54 is DATUM SHIFT with axis values and G55 a
// touch-probe cycle (pp. 753-756: "G54 X+20 Y+30 Z+10"), not a
// work-offset register. The first work offset is therefore the
// control's active preset - no word; any other is refused by name
// (preset selection is cycle G247, whose parameters this post was
// not specced from), never posted as a word the TNC reads otherwise.
function onParameter(name, value) {
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
  if (name === "WorkOffset" && value) {
    wcsWord = String(value);
    if (wcsWord !== "G54") {
      msg("work offset " + wcsWord + " has no TNC DIN/ISO form in this post - use the first work offset (the active preset)");
    }
  }
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

function onToolChange(number, name) {
  toolName = clean(name);
  tNo = number;
  toolWait = true;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program. #50: the tool call is the T block, `T5 G17 S4500*`
// (pp. 71, 218: number, spindle axis, speed; it applies the tool's
// length) - M6 is machine-dependent (p. 392) and not written. A speed
// change without a tool change is a T block of the speed only (p.
// 201). G40 G90 ride the first positioning block. The old preamble
// words are gone: G54 (datum shift), G80 (WORKING PLANE, p. 756) are
// not what a TNC reads them as.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  opNotes();
  modalWait = true;
  if (toolWait) {
    toolNote();
    block(["T" + ni(tNo), "G17", spindleWait && sRpm > 0 ? "S" + ff(sRpm) : ""]);
    toolWait = false;
  } else if (spindleWait && sRpm > 0) {
    block(["S" + ff(sRpm)]);
  }
  if (spindleWait && sRpm > 0) {   // a touch probe (#523) states no spindle
    block(["M3"]);
  }
  spindleWait = false;
  coolant = coolantFor(coolantAsk);
  if (coolant) block([coolant]);
}

// The op's after-action (#110), written once its spindle has stopped:
// the stream says nothing to carry on, or PARAM AfterOperation
// optional-stop / stop / home. The ask resets here, so the next
// section states its own.
function afterOp() {
  var ask = afterAsk;
  afterAsk = "";
  if (!ask) return;
  if (ask === "optional-stop") { block(["M1"]); return; }
  if (ask === "stop") { block(["M0"]); return; }
  if (ask === "home") {
    // #50: this dialect has no home move it can trust - G53 is a
    // datum-table shift and M91 rides a builder-set datum - so a
    // trip home is refused by name, never guessed.
    msg(coolantWho() + "the Heidenhain post cannot go home after an operation (no machine-referenced home it can trust) - choose a stop instead");
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  if (!sectionOpen) return;
  sectionOpen = false;
  owedDwell();
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  block(["M5"]);
  afterOp();
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ------------------------------------------------------------- motions
// #50: the words are the manual's two-digit G00/G01/G02/G03 (the
// function overview, p. 752). Drilling posts as its expanded chain
// (see onCycle), so no motion is ever suppressed here.
function onRapid() {
  startPreamble();
  if (!tiltOk(properties)) return;
  motion("G00", xyz(properties));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties)) return;
  motion("G01", xyz(properties).concat([fw(properties.FeedRate)]));
  dwellAtBottom(properties.Z);
}
// M11.2 A - arcs off the XY plane. This family's DIN/ISO mode binds
// the main plane to the spindle axis the T block sets (the published
// ISO programming manuals: a circle belongs to the main plane defined
// with the tool axis - G17 for a Z spindle; circles off it are the
// tilted-plane function's or Q-parameter work). A free G18/G19 word is
// therefore not this dialect's way to state an XZ/YZ arc, and writing
// one would be re-interpreting the program's tool axis. The house
// choice: a vertical-plane arc posts as its chord polyline, G01 blocks
// whose sagitta stays within ARC_CHORD_TOL (1 um - under the 3-decimal
// word resolution), the normal coordinate linear along the sweep. The
// motion is the arc's own to the output resolution; the plane word
// is never touched, so G17 stays the program's only plane.
// #18 made this final: the public manuals allow G2/G3 with axes off
// the working plane only as a spatial-arc case and state no rotation
// sense for it (its I/J/K are absolute centres, #49) -
// a native XZ/YZ arc would be a guess at a mirrorable direction.
var ARC_CHORD_TOL = 0.001;
var ARC_MAX_CHORDS = 1024;
function verticalArc(p) {
  var c = p.IJKIncrement || {};
  var yz = p.CircularPlane === "YZ";
  // The right-handed in-plane frame (u, v) with u x v = +normal:
  // XZ is (Z, X) about +Y, YZ is (Y, Z) about +X - so CCW (G3, seen
  // from the positive normal) is the ordinary atan2 sense in both.
  var cu = yz ? c.J : c.K, cv = yz ? c.K : c.I;
  var u0 = yz ? p.StartY : p.StartZ, v0 = yz ? p.StartZ : p.StartX;
  var w0 = yz ? p.StartX : p.StartY;
  var u1 = yz ? p.Y : p.Z, v1 = yz ? p.Z : p.X, w1 = yz ? p.X : p.Y;
  var ra = Math.sqrt((u0 - cu) * (u0 - cu) + (v0 - cv) * (v0 - cv));
  var rb = Math.sqrt((u1 - cu) * (u1 - cu) + (v1 - cv) * (v1 - cv));
  var r = Math.max(ra, rb);
  var a0 = Math.atan2(v0 - cv, u0 - cu), a1 = Math.atan2(v1 - cv, u1 - cu);
  var ccw = p.CircularDirection !== "CW";
  var d = ccw ? a1 - a0 : a0 - a1;
  d = ((d % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  var n = 1;
  if (has(r) && has(d) && r > 1e-12 && d > 0) {
    var step = 2 * Math.acos(Math.max(-1, Math.min(1, 1 - Math.min(ARC_CHORD_TOL / r, 1))));
    n = step > 0 ? Math.ceil(d / step - 1e-9) : ARC_MAX_CHORDS;
    n = Math.max(1, Math.min(ARC_MAX_CHORDS, n));
  }
  for (var j = 1; j <= n; j++) {
    var x = p.X, y = p.Y, z = p.Z;
    if (j < n) {
      var f = j / n, th = a0 + (ccw ? d * f : -d * f);
      var u = cu + r * Math.cos(th), v = cv + r * Math.sin(th), wn = w0 + (w1 - w0) * f;
      if (yz) { x = wn; y = u; z = v; } else { x = v; y = wn; z = u; }
    }
    motion("G01", [wo("X", x, orgX), wo("Y", y, orgY), wo("Z", z, orgZ), fw(p.FeedRate)]);
  }
}

function onCircular() {
  startPreamble();
  if (!tiltOk(properties)) return;
  var p = properties;
  if (p.CircularPlane === "XZ" || p.CircularPlane === "YZ") {
    verticalArc(p);
    return;
  }
  // #49: in this dialect I/J are the circle centre's Cartesian
  // coordinates - ABSOLUTE unless the word carries G91 (TNC 640 ISO
  // Programming User's Manual, ID 892909-25, 9/2016, pp. 260-261:
  // "Circle center I, J"; "Entering the circle center incrementally"
  // is relative to the last programmed position only when entered
  // incrementally; G91 is written before the word, p. 128). The
  // manual's form sets the centre in its own block before the
  // G02/G03 ("you must first enter the circle center I, J"), so it is
  // posted that way. Absolute: the work origin shifts the centre like
  // any position word (wo), never a start-relative difference.
  var c = p.IJKIncrement || {};
  if (!has(c.I) || !has(c.J)) {
    msg("arc without an XY centre - refused, not guessed");
    return;
  }
  block([wo("I", c.I, orgX), wo("J", c.J, orgY)]);
  motion(p.CircularDirection === "CW" ? "G02" : "G03", [
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
    fw(p.FeedRate),
  ]);
}

// ------------------------------------------------------------ drilling
// #50: this mode has no G81-G89. Its drilling cycles are G200-G209
// (p. 752), defined with Q parameters (`G200 DRILLING Q200=..`, p. 71)
// and called with M99/G79 - and G73 is ROTATION, G80 WORKING PLANE
// (p. 756), so the old `G73 X.. Q..` peck would have rotated the
// coordinate system. The Q parameters' meanings live in the separate
// Cycle Programming manual, which this post was not specced from, so
// no cycle is written: every hole posts as its expanded chain - the
// generator's own plunges, peck retracts and exits, the exact motion
// the simulation verified. A dwell becomes FUNCTION DWELL TIME<s>
// (seconds, p. 456) right after the move that reaches the hole's
// bottom. Tapping cannot be plain moves (the feed out of a tap needs
// the spindle reversed in step with it), so TAP is refused by name.
var DWELL_TOL = 1e-6;
function onCycle() {
  startPreamble();
  owedDwell();
  var p = properties;
  if (p.CycleType === "TAP") {
    msg("tapping has no TNC DIN/ISO form in this post (cycles G206/G207 are not specced) - refused, not posted as plain moves");
    return;
  }
  if (has(p.P) && p.P > 0) pendingDwell = { z: Number(p.Z), s: Number(p.P) };
}
function dwellAtBottom(z) {
  if (pendingDwell === null || !has(z) || Math.abs(z - pendingDwell.z) > DWELL_TOL) return;
  block(["FUNCTION DWELL TIME" + fu(pendingDwell.s)]);
  pendingDwell = null;
}
// A dwell whose chain never reached its bottom is refused, never lost.
function owedDwell() {
  if (pendingDwell === null) return;
  pendingDwell = null;
  msg("drilling dwell without a feed move to the hole bottom - refused, not dropped");
}

// ------------------------------------------------------------ probing
// #173: deliberately no onProbeMove / onProbeRetreat / onProbeResult.
// The ISO user's manual this post is specced from documents no free
// skip/measuring move and no variable holding a trip position; TNC
// probing is the touch-probe cycles, whose call form and result Q
// parameters live in the separate touch-probe cycle manual, not the
// source of this post. Rather than guess a cycle number or a Q
// parameter, the engine refuses a Probe operation by name for this
// post (crates/app/tests/probe_dialects.rs pins it).

// ------------------------------------------------------------ footer
// #50: the program ends M2 (p. 66: `G00 Z+250 M2*`), then the end
// block `N99999999 %NAME G71 *` (p. 130). The old optional `/G53 Z0`
// homing is gone: G53 is a datum-table shift in this mode (p. 753),
// and machine-referenced moves (M91, p. 393) depend on a datum the
// machine builder sets - the program ends where its last retract left
// the tool, as the manual's own programs do.
function onClose() {
  endPreamble();
  block(["M2"]);
  writeln("N" + N_LAST + " %" + pgmName + " G71 *");
}
