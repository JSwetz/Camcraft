// Siemens 840D 3X milling post. Provenance: the G-dialect from public
// DIN 66025 / ISO 6983 documentation and the control family's
// published operator conventions (behavior only, per this repo's
// Clean-Room Protocol); the hook contract from Autodesk's published
// post docs. No vendor script text; original work. Where a public
// convention leaves a hole, the house style (see fanuc.cps) decides,
// and the conformance goldens lock that decision, not any vendor's.
description = "Siemens 840D (3-axis mill, DIN/ISO cycles G73-G89, G2/G3)";
vendor = "HSMWorks (independent, unaffiliated)";
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
var nLine = 0;             // the numbered-block dialect: every written
                           // block advances N by 10 (gap left for hand edits)

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
function block(words) {  // one numbered line per block - the DIN law
  var a = [];
  for (var i = 0; i < words.length; i++) {
    if (words[i] !== "" && words[i] !== null && words[i] !== undefined) a.push(words[i]);
  }
  if (a.length > 0) {
    nLine += 10;
    writeln("N" + nLine + " " + a.join(" "));
  }
}
function xyz(p) { return [wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ)]; }
function wo(letter, v, o) { return has(v) ? letter + fu(v - o) : ""; }
function msg(s) {  // counted warning where the host offers message()
  if (typeof message === "function") message(s); else writeln("; " + s);
}
var tiltRefused = false;  // stage D: one named refusal per program, not per line
function tiltOk(p) {  // RTCP words are Fanuc-family; this dialect refuses them
  if (!has(p.A)) return true;
  if (!tiltRefused) { tiltRefused = true; msg("tilted move - Sinumerik post has no RTCP output"); }
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
// The DIN family comments with ';' and needs no fence; identity is one
// comment line, and the first real block carries the plane.
function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "HSMWORKS";
  writeln("; " + nm);
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
// legal program: plane, offsets, tool, spindle, coolant. The tool call
// selects tool and geometry together (T n D n) - no G43 word exists
// in this family.
function startPreamble() {
  if (sectionOpen) return;
  sectionOpen = true;
  block(["G17", "G90", wcsWord, "G40", "G80"]);
  if (toolWait) {
    block(["T" + ni(tNo), "D" + ni(tNo), "M6"]);
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
function onCircular() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  var p = properties;
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
// The cycle record replaces its whole move group: one block per hole,
// holes of an unchanged cycle continuing modally with bare X Y lines.
// The DIN cycles retract to the R plane by the documented default of
// this family's plain G81-G89 forms - there is no G98 word to write,
// so none is invented; every cycle block restates the hole position.
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
// Homing rides machine coordinates (G53), not a stored home point.
function onClose() {
  endPreamble();
  block(["G53", "Z0"]);
  block(["M2"]);
}
