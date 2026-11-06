// Fanuc 2-axis LATHE post - the first turning .cps we ship (M13.5).
// Provenance: the G-dialect from public RS-274 / ISO 6983 documentation
// and the publicly documented Fanuc 0i-T style vocabulary (G18 ZX
// plane, G96/G97/G50 spindle words, G99 feed per revolution, the G83/
// G84/G85 face cycles, G32 thread cutting, T0101-style offsets) - behavior
// only, per this repo's Clean-Room Protocol; no vendor post text. The hook
// contract is the one assets/posts/AGENTS.md states. Original work.
description = "Fanuc lathe (2-axis turning, G18 ZX, diameter X, G97/G99, G83 face drilling, G32 threading)";
vendor = "Camcraft";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 2; // reference contract: integer feature mask (bit 2 = turning)

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).
//
// The lathe laws this script states, so a reviewer need not infer them:
//  - DIAMETER programming: the X word is twice the stream's radius. The
//    stream speaks world mm with the insert's nose centre on ONE meridian
//    half-plane of the spindle axis (the turning kernel's frame); the
//    radius is that plane's signed in-plane coordinate, origin-relative.
//    A move with BOTH in-plane coordinates off the axis is off every
//    meridian - milling work - and is refused by name, never projected.
//  - G97 constant rpm, from the stream's SpindleSpeed; G96 constant
//    surface speed ONLY when the stream states a SurfaceSpeed (m/min)
//    PARAM - the post never invents a surface speed from an rpm. G50
//    clamps the rpm once per program to the machine's stated maximum.
//  - G99 feed per revolution: F = mm/min / rpm, so the cut speed the
//    generator planned is the cut speed the control runs. No rpm, no
//    per-rev feed - refused by name.
//  - G18: every arc is a ZX-plane arc with I K centre increments in
//    RADIUS values (the 0i-T convention). A meridian arc that rides the
//    kernel's +Y half-plane changes its handedness when it is viewed as
//    ZX from +Y, so a YZ-stated arc flips CW<->CCW; an XZ-stated one
//    does not. An XY arc is a thread half-turn (see G32) or a refusal.
//  - G32: the threading kernel spells a synchronized helix as XY
//    half-turn arcs about the axis, rising half a pitch each. Two
//    half-turns at one radius are one lead of Z travel under G32 with
//    F = lead (mm/rev). Anything else in XY is refused.
//  - Face cycles: G83 (Q peck in whole microns, the 0i-T word; P dwell
//    in ms), G84 tap, G85 bore, cancelled by G80. A cycle off the
//    spindle axis and a back-bore are refused - a 2-axis lathe has no
//    such cycle.
//  - M03 always: the stream carries no spindle sense, and a left-hand
//    tool would need the insert library to say so first.

// ---------------------------------------------------------------- state
var wcsWord = "G54";
var sRpm = 0;              // S/M3 waits for the section (PARAMs, TOOL, SECTION order)
var sCss = 0;              // SurfaceSpeed PARAM (m/min), 0 = none stated
var spindleWait = false;
var toolWait = false;
var tNo = 0;
var inCycle = false;       // a cycle group is open; its chain is suppressed
var sectionOpen = false;
var coolant = "";
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (world -> program)
var clampDone = false;     // G50 once per program
var refused = {};          // each refusal counted once per program, by name
var EPS = 0.0005;          // half a micron: the stream's own lattice

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function pad2(n) { n = Math.round(Number(n) || 0); return n < 10 ? "0" + n : String(n); }
function fu(v) {  // mm at three decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(3);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function fr(v) {  // mm/rev at four decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(4);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
function block(words) {
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function msg(s) { if (typeof message === "function") message(s); else writeln("; " + s); }
// Refuse by name, once per reason per program (per-line is noise).
function refuse(why) {
  if (!refused[why]) { refused[why] = true; msg(why); }
  return null;
}

// The signed meridian coordinate of a world point, origin-relative:
// the lathe's radius. null (and a counted refusal) when the point
// stands off every meridian half-plane - milling work on a lathe post.
function radiusOf(x, y, what) {
  var dx = has(x) ? x - orgX : 0;
  var dy = has(y) ? y - orgY : 0;
  if (Math.abs(dx) <= EPS) return dy;
  if (Math.abs(dy) <= EPS) return dx;
  return refuse(what + " off the spindle axis meridian (both X and Y move) - milling work, not turning");
}
function xWord(r) { return has(r) ? "X" + fu(2 * r) : ""; }         // DIAMETER programming
function zWord(z) { return has(z) ? "Z" + fu(z - orgZ) : ""; }
// G99: feed per revolution from the stream's mm/min and the spindle rpm.
function feedWord(f) {
  if (!has(f) || f <= 0) return "";                                  // modal feed stands
  if (!(sRpm > 0)) { refuse("feed per revolution needs a spindle speed - none stated"); return ""; }
  return "F" + fr(f / sRpm);
}
function posed(p) {
  if (has(p.A) || has(p.B) || has(p.C)) {
    refuse("posed move (A/B/C words) on a 2-axis lathe - no rotary to command");
    return true;
  }
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

// -------------------------------------------------------------- header
function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  writeln("%");
  block(["O0001", "(" + nm + ")"]);
  block(["G21", "G18", "G40", "G80", "G99"]);   // mm, ZX plane, no nose comp, feed/rev
}

// Cutter compensation in the control (M13.11): on a lathe G41/G42 is
// tool NOSE-RADIUS compensation (the insert's ground tip, G41/G42 with
// the tip-direction number) - a different law, out of scope; refused by
// name, never a centre path posted as the part.
function onRadiusCompensation() {
  msg("control cutter compensation on a lathe is nose-radius compensation - not supported; choose In computer");
}

function onParameter(name, value) {
  if (name === "WorkOffset" && value) wcsWord = String(value);
  else if (name === "OriginX" && has(value)) orgX = Number(value);
  else if (name === "OriginY" && has(value)) orgY = Number(value);
  else if (name === "OriginZ" && has(value)) orgZ = Number(value);
  else if (name === "SurfaceSpeed" && has(value)) sCss = Number(value);
}

function onSpindleSpeed() {
  sRpm = Number(properties.SpindleSpeed) || 0;
  spindleWait = true;
  if (typeof machine !== "undefined" && machine.spindleMax > 0 && sRpm > machine.spindleMax) {
    msg("spindle rpm over machine maximum");
  }
}

function onToolChange(number, name) {
  tNo = number;
  toolWait = true;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: offset, tool + wear offset, spindle, coolant.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  block([wcsWord]);
  if (toolWait) {
    block(["T" + pad2(tNo) + pad2(tNo)]);     // T0101: station 1, offset 1
    toolWait = false;
  }
  if (!clampDone && typeof machine !== "undefined" && machine.spindleMax > 0) {
    block(["G50", "S" + ff(machine.spindleMax)]);
    clampDone = true;
  }
  if (spindleWait) {
    if (sCss > 0) block(["G96", "S" + ff(sCss), "M03"]);
    else block(["G97", "S" + ff(sRpm), "M03"]);
    spindleWait = false;
  }
  coolant = coolantCode();
  if (coolant) block([coolant]);
}

function endPreamble() {
  if (!sectionOpen) return;
  sectionOpen = false;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (coolant) block(["M9"]);
  block(["M05"]);
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); }

// A WARN record (only a gate-refused program ever carries one) is
// voiced as a parenthesized comment - the control's own comment
// syntax, never a bare text line it would reject.
function onComment(s) { writeComment(s); }

// ------------------------------------------------------------- motions
function onRapid() {
  startPreamble();
  var p = properties;
  if (posed(p) || inCycle) return;
  var r = radiusOf(p.X, p.Y, "rapid");
  if (r === null) return;
  block(["G00", xWord(r), zWord(p.Z)]);
}
function onLinear() {
  startPreamble();
  var p = properties;
  if (posed(p) || inCycle) return;
  var r = radiusOf(p.X, p.Y, "feed move");
  if (r === null) return;
  block(["G01", xWord(r), zWord(p.Z), feedWord(p.FeedRate)]);
}

// A ZX-plane arc (G18): centre as I K increments from the start, in
// radius values; direction per the view-from-+Y law stated above.
function meridianArc(p) {
  var c = p.IJKIncrement || {};
  var yz = p.CircularPlane === "YZ";
  var r0 = radiusOf(p.StartX, p.StartY, "arc start");
  var r1 = radiusOf(p.X, p.Y, "arc end");
  var rc = yz ? radiusOf(orgX, c.J, "arc centre") : radiusOf(c.I, orgY, "arc centre");
  if (r0 === null || r1 === null || rc === null) return;
  if (!has(c.K)) { refuse("arc without a K centre word"); return; }
  var cw = p.CircularDirection === "CW";
  if (yz) cw = !cw;                       // the +Y meridian seen from +Y is mirrored
  block([
    cw ? "G02" : "G03",
    xWord(r1), zWord(p.Z),
    "I" + fu(rc - r0), "K" + fu(c.K - p.StartZ),
    feedWord(p.FeedRate),
  ]);
}

// An XY arc is a thread half-turn: centre on the axis, one radius at
// both ends, antipodal ends - a synchronized half lead of Z. G32 with
// F = lead; the X word stays modal (the plunge put the tool there).
function threadHalfTurn(p) {
  var c = p.IJKIncrement || {};
  if (!has(c.I) || !has(c.J)) { refuse("thread half-turn without an I J centre"); return; }
  var ok = Math.abs(c.I - orgX) <= EPS && Math.abs(c.J - orgY) <= EPS;
  var rho0 = Math.sqrt(Math.pow(p.StartX - orgX, 2) + Math.pow(p.StartY - orgY, 2));
  var rho1 = Math.sqrt(Math.pow(p.X - orgX, 2) + Math.pow(p.Y - orgY, 2));
  ok = ok && Math.abs(rho0 - rho1) <= EPS;
  ok = ok && Math.abs(p.StartX + p.X - 2 * c.I) <= EPS && Math.abs(p.StartY + p.Y - 2 * c.J) <= EPS;
  if (!ok) { refuse("XY arc that is not a thread half-turn about the spindle axis"); return; }
  var lead = 2 * Math.abs(p.Z - p.StartZ);
  if (!(lead > 0)) { refuse("thread half-turn with no lead"); return; }
  block(["G32", zWord(p.Z), "F" + fr(lead)]);
}

function onCircular() {
  startPreamble();
  var p = properties;
  if (posed(p) || inCycle) return;
  if (p.CircularPlane === "XZ" || p.CircularPlane === "YZ") meridianArc(p);
  else threadHalfTurn(p);
}

// ------------------------------------------------------- canned cycles
// One positioning rapid to the hole's retract plane, then the face
// cycle whose initial level that is (the 0i-T cycle returns there).
// The group's expanded chain is suppressed while the cycle is open.
function onCycle() {
  startPreamble();
  var p = properties;
  var r = radiusOf(p.X, p.Y, "cycle");
  if (r === null) return;
  if (Math.abs(r) > EPS) { refuse("drilling cycle off the spindle axis - a 2-axis lathe drills on centre only"); return; }
  var ms = has(p.P) ? Math.round(p.P * 1000) : 0; // P is seconds
  var g = "G83";
  if (p.CycleType === "TAP") g = "G84";
  else if (p.CycleType === "BORE") g = "G85";
  else if (p.CycleType === "BACKBORE") { refuse("back-boring cycle - no such face cycle on a 2-axis lathe"); return; }
  if (!inCycle) {
    // The chain's own approach rapid usually stands here already;
    // the positioning block is written only when the tool is not.
    var cp = (typeof currentPosition !== "undefined") ? currentPosition : {};
    var there = has(cp.X) && has(cp.Y) && has(cp.Z) && has(p.R)
      && Math.abs(cp.X - orgX) <= EPS && Math.abs(cp.Y - orgY) <= EPS   // on the axis, as the cycle is
      && Math.abs(cp.Z - p.R) <= EPS;
    if (!there) block(["G00", xWord(r), zWord(p.R)]);
  }
  var words = [g, zWord(p.Z)];
  if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
    words.push("Q" + Math.round(p.Q * 1000));    // whole microns, no decimal point
  }
  if (ms > 0 && g !== "G84") words.push("P" + ms);
  words.push(feedWord(p.FeedRate));
  block(words);
  inCycle = true;
}

// ------------------------------------------------------------ footer
function onClose() {
  endPreamble();
  block(["G28", "U0", "W0"]);
  block(["M30"]);
  writeln("%");
}
