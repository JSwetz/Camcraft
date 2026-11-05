// Heidenhain TNC 3X milling post - the control family's documented
// DIN/ISO programming mode, not the native conversational language
// (a KLAR-style post is a separate, later candidate; see
// RECOMMENDED.md M3.3 B). Provenance: the G-dialect from public DIN
// 66025 / ISO 6983 documentation and the family's published operator
// conventions (behavior only, per this repo's Clean-Room Protocol);
// the hook contract from Autodesk's published post docs. No vendor
// script text; original work. Where a public convention leaves a
// hole, the house style (see fanuc.cps) decides, and the conformance
// goldens lock that decision, not any vendor's.
description = "Heidenhain TNC (3-axis mill, DIN/ISO mode, cycles G73-G89, G2/G3)";
vendor = "CamCraft (independent, unaffiliated)";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
setCodePage = "utf-8";
capabilities = 1; // reference contract: integer feature mask (bit 1 = milling)

// Determinism law: no timestamps, no randomness - the same stream
// always posts to the same text (golden files depend on it).

// ---------------------------------------------------------------- state
var wcsWord = "G54";
var sRpm = 0;
var spindleWait = false;
var toolWait = false;
var tNo = 0;
var inCycle = false;
var coolant = "";
var sectionOpen = false;
var orgX = 0, orgY = 0, orgZ = 0;

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
function w(letter, v) { return has(v) ? letter + fu(v) : ""; }
function fw(v) { return has(v) ? "F" + ff(v) : ""; }
function block(words) {  // join the non-empty words onto one line
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln(a.join(" "));
}
function skip(words) {  // an optional block: '/' prefix, the family law
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) writeln("/" + a.join(" "));
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("; " + s);
}
var tiltRefused = false;  // stage D: one named refusal per program, not per line
function tiltOk(p) {  // RTCP words are Fanuc-family; this dialect refuses them
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - Heidenhain post has no RTCP output"); }
  return false;
}

// The .mch tool-station coolant list carries names + M-codes; the
// first Flood entry (else the first entry at all) is this machine's
// cutting-fluid-on code. M9 (stop) is the fixed pair - the machine
// file speaks, the post transcribes.
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
// Identity in one parenthetical comment - the mode needs no fence and
// the post writes no program number (the control names its programs).
function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  block(["(" + nm + ")"]);
}

function onParameter(name, value) {
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

function onToolChange(number, name) {
  tNo = number;
  toolWait = true;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program. The tool call is one word plus the change - the mode
// selects tool geometry with the T word itself.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  block(["G17", "G90", wcsWord, "G40", "G80"]);
  if (toolWait) {
    block(["T" + ni(tNo), "M6"]);
    toolWait = false;
  }
  if (spindleWait) {
    block(["S" + ff(sRpm), "M3"]);
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
  block(["M5"]);
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ------------------------------------------------------------- motions
// While a canned-cycle group is open its expanded hole chain is the
// cycle block's business only - the chain is the generator's truth,
// not a second copy of output.
function onRapid() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  block(["G1"].concat(xyz(properties), [fw(properties.FeedRate)]));
}
// M11.2 A - arcs off the XY plane. This family's DIN/ISO mode binds
// the main plane to the spindle axis the T block sets (the published
// ISO programming manuals: a circle belongs to the main plane defined
// with the tool axis - G17 for a Z spindle; circles off it are the
// tilted-plane function's or Q-parameter work). A free G18/G19 word is
// therefore not this dialect's way to state an XZ/YZ arc, and writing
// one would be re-interpreting the program's tool axis. The house
// choice: a vertical-plane arc posts as its chord polyline, G1 blocks
// whose sagitta stays within ARC_CHORD_TOL (1 um - under the 3-decimal
// word resolution), the normal coordinate linear along the sweep. The
// motion is the arc's own to the output resolution; the plane word
// is never touched, so G17 stays the program's only plane.
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
    block(["G1", wo("X", x, orgX), wo("Y", y, orgY), wo("Z", z, orgZ), fw(p.FeedRate)]);
  }
}

function onCircular() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  var p = properties;
  if (p.CircularPlane === "XZ" || p.CircularPlane === "YZ") {
    verticalArc(p);
    return;
  }
  var c = p.IJKIncrement || {};
  block([
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
    // I/J are increments: a shared translation cancels, so the origin
    // must NOT enter them - center minus start in the stream's frame.
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    fw(p.FeedRate),
  ]);
}

// ------------------------------------------------------- canned cycles
// The cycle record replaces its whole move group. The mode's plain
// G81-G89 cycles retract to the R plane by documented default - no
// G98 word exists to write, so none is invented; each hole restates
// its position, which is also what the family's own examples show.
function onCycle() {
  startPreamble();
  var p = properties;
  var ms = has(p.P) ? Math.round(p.P * 1000) : 0; // P is seconds
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = ms > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "TAP") g = "G84";
  else if (p.CycleType === "BORE") g = "G86";
  else if (p.CycleType === "BACKBORE") g = "G89";
  var words = [g, wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), wo("R", p.R, orgZ)];
  if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
    words.push(w("Q", p.Q)); // an increment (peck depth): no origin shift
  }
  if (ms > 0 && g !== "G73" && g !== "G84") words.push("P" + ms);
  words.push(fw(p.FeedRate));
  block(words);
  inCycle = true;
}

// ------------------------------------------------------------ footer
// Homing rides machine coordinates as an optional block, the family's
// documented convention for a line the operator may choose to skip.
function onClose() {
  endPreamble();
  skip(["G53", "Z0"]);
  block(["M2"]);
}
