// Fanuc 3X milling post - the first .cps we ship. Provenance: the
// G-dialect from public RS-274/ISO 6983 documentation; the hook
// contract from Autodesk's published post docs (behavior only) per
// this repo's Clean-Room Protocol. No vendor script text; original
// work. A .cps is plain JS: the manifest is assignments and the
// helpers below are local - the script needs only writeln/write and
// the properties object from any host (our engine publishes more,
// the reference base less; this text runs on either unchanged).
description = "Fanuc (3-axis mill, G73/G81/G82/G83/G84/G86/G89, G2/G3, G98)";
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
var wcsWord = "G54";       // from the WorkOffset PARAM
var sRpm = 0;              // S/M3 waits for the section: the stream
var spindleWait = false;   // order is PARAMs, TOOL, SECTION
var toolWait = false;      // T../M6 and G43 wait the same way
var tNo = 0;
var inCycle = false;       // a cycle group is open; its chain is suppressed
var cycleSig = "";         // g|Z|R|Q of the open group (modal X Y holes)
var retractSet = false;    // G98 written once per program
var coolant = "";          // the M-code this program started, if any
var sectionOpen = false;   // preamble written (a legacy .cl carries no SECTION)
var tcpmOn = false;        // G43.4 armed (stage D RTCP): re-armed at tool change
var lastA = 0, lastB = 0, lastC = 0; // angle words the last block commanded (deg)
var tiltRefused = false;   // a refusal the gate counts once, not per line
var orgX = 0, orgY = 0, orgZ = 0; // the WCS slot origin (OriginX/Y/Z PARAMs):
// the stream speaks world coordinates; the machine measures its program
// from the work offset. A legacy .cl carries no PARAMs, so these stay 0.

// ------------------------------------------------------------ helpers
function has(v) { return v !== null && v !== undefined && isFinite(v); }
function pad2(n) { n = Math.round(Number(n) || 0); return n < 10 ? "0" + n : String(n); }
function fu(v) {  // mm at three decimals, trailing zeros trimmed
  if (!has(v)) return "0";
  var s = Number(v).toFixed(3);
  if (s.indexOf(".") >= 0) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s === "-0" ? "0" : s;
}
function ff(v) { return has(v) && v > 0 ? String(Math.round(v)) : "0"; }
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
  if (typeof message === "function") message(s); else writeln("; " + s);
}

// RTCP output (stage D). A tilted record locates the TOOL TIP in the
// work offset while the machine turns - G43.4 delegates the kinematics
// to the controller, so the post never does pivot math; it arms the
// mode, voices the commanded angles modally, and refuses by name
// (counted warning, gate-blocked) wherever it cannot honor one. Law:
// a tilted record states all three angles (the stream's parser zeros
// unstated letters), so the presence of A decides; angle memory
// resets with the arm, so re-arming always commands all three words.
// A prismatic record never touches the mode - the tilt stays modal.
function tilt(p) {  // words to append, [] if none, null if refused
  if (!has(p.A)) return [];
  if (typeof machine === "undefined" || machine.rtcp !== true) {
    if (!tiltRefused) { tiltRefused = true; msg("tilted move on a machine without a tilting rotary"); }
    return null;
  }
  if (inCycle) { msg("tilted move inside a canned-cycle group"); return null; }
  if (!tcpmOn) {
    block(["G43.4", "H" + pad2(tNo)]);
    tcpmOn = true;
    lastA = p.A; lastB = p.B; lastC = p.C;
    return [w("A", p.A), w("B", p.B), w("C", p.C)];
  }
  var words = [];
  if (p.A !== lastA) { words.push("A" + fu(p.A)); lastA = p.A; }
  if (p.B !== lastB) { words.push("B" + fu(p.B)); lastB = p.B; }
  if (p.C !== lastC) { words.push("C" + fu(p.C)); lastC = p.C; }
  return words;
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
// M11.2 A: the arc's plane (RS-274 / ISO 6983 G17 XY, G18 XZ, G19 YZ),
// stated modally. G17 is the state every program starts in (the
// control's power-on plane; a preamble that writes G17 restates it),
// so a program whose arcs are all XY writes no new word. A G18/G19 is
// taken back to G17 before the next XY arc, before any canned cycle
// (a drilling cycle runs along the plane's normal axis) and at section
// end (the next tool's length offset must not meet a stale plane).
// Direction needs no translation: the stream's CW/CCW is G2/G3 by the
// same view-from-the-positive-normal rule the control applies.
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
// The center words of the arc's plane - I J (XY), I K (XZ), J K (YZ) -
// as increments from the start point: a shared translation cancels in
// an increment, so the origin must NOT enter them.
function centerWords(p) {
  var c = p.IJKIncrement || {};
  return [
    w("I", has(c.I) && has(p.StartX) ? c.I - p.StartX : null),
    w("J", has(c.J) && has(p.StartY) ? c.J - p.StartY : null),
    w("K", has(c.K) && has(p.StartZ) ? c.K - p.StartZ : null),
  ];
}

// -------------------------------------------------------------- header
function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  writeln("%");
  block(["O0001", "(" + nm + ")"]);
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
  tcpmOn = false;              // a new tool re-arms TCPM (stage D)
  lastA = 0; lastB = 0; lastC = 0;
}

// The preamble closes over everything a raw motion line needs to be a
// legal program: offsets, tool, spindle, coolant. v1 streams deliver
// it at onSection; a legacy .cl (no records at all) gets it at its
// first motion - a posted program without an offset or a spindle is
// not a posted program.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  block(["G90", wcsWord, "G40", "G49", "G80"]);
  if (toolWait) {
    block(["T" + pad2(tNo), "M6"]);
    block(["G43", "H" + pad2(tNo)]);
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
  restorePlane();            // M11.2 A: the plane never leaks past its section
  sectionOpen = false;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (coolant) block(["M9"]);
  block(["M5"]);
  // The next preamble carries G49, which cancels TCPM too - so the
  // arm and the angle memory reset here, and a later tilt re-arms by
  // commanding all three words (stage D).
  tcpmOn = false;
  lastA = 0; lastB = 0; lastC = 0;
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ------------------------------------------------------------- motions
// While a canned-cycle group is open its expanded hole chain (rapid,
// plunge, feed, retract) is the cycle block's business only - the
// chain is the generator's truth, not a second copy of output.
function onRapid() {
  startPreamble();
  var tw = tilt(properties);
  if (tw === null || inCycle) return;
  block(["G00"].concat(xyz(properties), tw));
}
function onLinear() {
  startPreamble();
  var tw = tilt(properties);
  if (tw === null || inCycle) return;
  block(["G01"].concat(xyz(properties), tw, [fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  var tw = tilt(properties);
  if (tw === null || inCycle) return;
  var p = properties;
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G02" : "G03",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat(tw, [fw(p.FeedRate)]));
}

// ------------------------------------------------------- canned cycles
// The cycle record replaces its whole move group: one block per hole,
// holes of an unchanged cycle continuing modally with bare X Y lines.
// Feed word comes from the cycle (the generator synthesizes it from
// the group's chain, so a tap's F is rpm x pitch by the agreement
// law); a legacy marker carries none, and the modal feed stands.
function onCycle() {
  startPreamble();
  restorePlane();            // M11.2 A: a cycle drills along G17's Z
  var p = properties;
  var ms = has(p.P) ? Math.round(p.P * 1000) : 0; // P is seconds
  var g = "G81";
  if (p.CycleType === "SPOT" || p.CycleType === "DRILL") g = ms > 0 ? "G82" : "G81";
  else if (p.CycleType === "PECK") g = "G73";
  else if (p.CycleType === "DEEP") g = "G83";
  else if (p.CycleType === "TAP") g = "G84";
  else if (p.CycleType === "BORE") g = "G86";
  else if (p.CycleType === "BACKBORE") g = "G89";
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
    if (ms > 0 && g !== "G73" && g !== "G84") words.push("P" + ms);
    words.push(fw(p.FeedRate));
    block(words);
    cycleSig = sig;
  }
  inCycle = true;
}

// ------------------------------------------------------------ footer
function onClose() {
  endPreamble();
  block(["G91", "G28", "Z0"]);
  block(["G28", "X0", "Y0"]);
  block(["M30"]);
  writeln("%");
}
