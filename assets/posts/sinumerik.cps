// Siemens 840D 3X milling post. Provenance: the G-dialect from public
// DIN 66025 / ISO 6983 documentation and the control family's
// published operator conventions (behavior only, per this repo's
// Clean-Room Protocol); the hook contract from Autodesk's published
// post docs. No vendor script text; original work. Where a public
// convention leaves a hole, the house style (see fanuc.cps) decides,
// and the conformance goldens lock that decision, not any vendor's.
description = "Siemens 840D (3-axis mill, DIN/ISO cycles G73-G89, G2/G3)";
vendor = "CamCraft (independent, unaffiliated)";
legal = "Original clean-room work; public documentation only";
certificationLevel = 1;
minimumRevision = 1;
extension = "nc";
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
var inCycle = false;
var coolant = "";
var coolantAsk = "flood";  // the section's PARAM Coolant (#112), flood when unstated
var afterAsk = "";         // the section's PARAM AfterOperation (#110), none when unstated
var homedAt = -1;          // linesWritten() just after a trip home (#504): onClose skips its own home when nothing followed
var coolantOp = "";        // the section's OperationName, for refusals (#150)
var sectionOpen = false;
var compG = "";            // G41/G42/G40 pending for the next straight block (#39)
var compD = "";            // the D word riding the engage block ("" when the tool call selects it)
var compOn = false;        // compensation is engaged (G41/G42 written, no G40 yet)
var compNoted = false;     // the section's operator note was written
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

// -------------------------------------------------------------- header
// The DIN family comments with ';' and needs no fence; identity is one
// comment line, and the first real block carries the plane.
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
// #391: a control offsets the cutter in the ACTIVE plane, and every mill
// generator compensates in XY - so the block that writes G41/G42/G40
// states G17 when an XZ/YZ arc left G18/G19 modal, and no arc leaves
// G17 while compensation is engaged (refused by name, see onCircular).
function compPlaneWord(cg) { return cg ? planeWord(17) : ""; }
function compPlaneRefuse(g) {
  if (!compOn || g === 17) return false;
  msg("an " + (g === 18 ? "XZ (G18)" : "YZ (G19)") + " arc while cutter compensation is engaged - the control would compensate in that plane; cancel compensation (G40) first");
  return true;
}
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

// ------------------------------------------------------------ comments
// Program commentary (#257): the setup, the operation and the tool, for
// the operator reading the program - never a word the control moves on.
// The stream's PARAMs OperationName / OperationKind / SetupName /
// ToolDiameter / ToolKind / ToolCornerRadius and the TOOL record's name
// feed it; a stream without them (a legacy .cl) writes none. User text
// is cleaned for this dialect: a semicolon opens the comment, so a
// second one, and % (the tape's start mark) and parentheses, become
// spaces; the line rides unnumbered, as the machine-name line does;
// printable ASCII only, capped at 40 characters.
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
function note(s) { writeln("; " + s); }
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

function onOpen() {
  var nm = (typeof machine !== "undefined" && machine.name) || "CAMCRAFT";
  // #594: the name is user text from the machine file - cleaned like
  // every other comment text, one rule for every comment line.
  writeln("; " + (clean(nm) || "CAMCRAFT"));
  var z = safeZ();
  if (z !== null && z !== 0) note("SAFE RETRACT G53 Z" + fu(z) + " - STATED BY THE MACHINE");
}

// #503 (#495's rule): where "straight up" is, in machine coordinates.
// G53 Z0 is the top of Z travel only where the builder put the machine
// zero there; the family's public documentation leaves the reference
// point and machine zero to the machine builder's machine data, so the
// post cannot assume it. The machine file states the height (its Z
// part's `safe_retract`, `machine.safeRetractZ`, mm - this dialect
// writes mm); a document with no machine keeps today's Z0; any machine
// stating none is refused by name and no retract is guessed. Judged once.
var safeZDone = false;
var safeZValue = null;     // null = refused, no retract is written
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
  if (nm === "") {
    safeZValue = 0;
  } else {
    msg("machine " + nm + " does not state its safe Z retract - this post lifts Z with G53 for a home move and at the end, and where machine Z0 sits is the machine builder's choice on this control; state the Z axis's safe_retract (machine coordinates) in the machine file");
  }
  return safeZValue;
}

// Cutter radius compensation in the control (#39; DIN 66025 / ISO 6983
// G41 left / G42 right / G40 cancel, and the family's published
// programming fundamentals: the radius is the ACTIVE cutting edge's -
// the D the tool call already selected (T n D n) - so no D word rides
// the engage block; the approach NORM runs straight to the first point
// and stands the tool perpendicular to the path tangent at that start
// point, the start-up the generator's lead law assumes - stated on the
// engage block, never left to a modal KONT). The stream's COMP
// record says which side; the NEXT motion record is the engage (or
// cancel) block, a straight lead at least a radius long. A comp word
// that would land on a rapid or an arc is refused by name (counted,
// gate-blocked) - never written where the control would misread it.
// Under wear (CompensationType "wear", the centre path) the edge must
// hold only the wear delta: the section says so once, in a comment.
function onRadiusCompensation(side) {
  var s = String(side || properties.Compensation || "off");
  if (s === "off") {
    compG = compOn ? "G40" : "";
    compD = "";
    compOn = false;
    return;
  }
  if (inCycle) { msg("cutter compensation inside a canned-cycle group"); return; }
  compG = ((s === "left") ? "G41" : "G42") + " NORM";  // NORM stated, never a stale KONT
  compD = "";
  compOn = true;
  if (properties.CompensationType === "wear" && !compNoted) {
    block(["; WEAR COMP - D" + ni(tNo) + " HOLDS THE WEAR DELTA ONLY, NOT THE TOOL RADIUS"]);
    compNoted = true;
  }
}
function compRefuse(what) {
  if (compG) {
    msg("cutter compensation must engage or cancel on a straight G1 move, not " + what);
    compG = ""; compD = ""; compOn = false;
  }
}

function onParameter(name, value) {
  commentParam(name, value);
  if (name === "OperationName") coolantOp = String(value || "");
  if (name === "Coolant") { coolantAsk = String(value || "flood"); return; }
  if (name === "AfterOperation") { afterAsk = String(value || ""); return; }
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
  toolName = clean(name);
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
  opNotes();
  block(["G17", "G90", wcsWord, "G40", "G80"]);
  planeG = 17;              // the preamble states the plane
  if (toolWait) {
    toolNote();
    block(["T" + ni(tNo), "D" + ni(tNo), "M6"]);
    toolWait = false;
  }
  if (spindleWait && sRpm > 0) {   // a touch probe (#173) states no spindle
    block(["S" + ff(sRpm), "M3"]);
    spindleWait = false;
  }
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
    var z = safeZ();            // refused (named once): no Z lift, so no XY move either
    if (z === null) return;
    block(["G0", "G53", "Z" + fu(z)]);   // machine coordinates, Z first (onClose's law)
    block(["G0", "G53", "X0", "Y0"]);
    homedAt = linesWritten();
    return;
  }
  msg(coolantWho() + "unknown after-operation ask " + ask);
}

function endPreamble() {
  if (!sectionOpen) return;
  restorePlane();            // M11.2 A: the plane never leaks past its section
  sectionOpen = false;
  if (inCycle) block(["G80"]);
  inCycle = false;
  if (compOn || compG) {
    msg("cutter compensation still engaged at section end - no cancel block");
    compG = ""; compD = ""; compOn = false;
  }
  compNoted = false;
  if (coolant) block(["M9"]);
  coolant = "";
  coolantAsk = "flood";      // the next section states its own ask
  block(["M5"]);
  afterOp();
}

function onSection() { startPreamble(); }
function onSectionEnd() { endPreamble(); }

// ------------------------------------------------------------- motions
// While a canned-cycle group is open its expanded hole chain is the
// cycle block's business only - the chain is the generator's truth,
// not a second copy of output.
function onRapid() {
  startPreamble();
  compRefuse("a rapid");
  if (!tiltOk(properties) || inCycle) return;
  block(["G0"].concat(xyz(properties)));
}
function onLinear() {
  startPreamble();
  if (!tiltOk(properties) || inCycle) return;
  // The pending comp word rides this straight block (#39).
  var cg = compG, cd = compD;
  compG = ""; compD = "";
  block([compPlaneWord(cg), "G1", cg].concat(xyz(properties), [cd, fw(properties.FeedRate)]));
}
function onCircular() {
  startPreamble();
  compRefuse("an arc");
  if (!tiltOk(properties) || inCycle) return;
  var p = properties;
  if (compPlaneRefuse(planeOf(p))) return;
  block([
    planeWord(planeOf(p)),
    p.CircularDirection === "CW" ? "G2" : "G3",
    wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ),
  ].concat(centerWords(p)).concat([fw(p.FeedRate)]));
}

// ------------------------------------------------------- canned cycles
// The cycle record replaces its whole move group: one block per hole,
// holes of an unchanged cycle continuing modally with bare X Y lines.
// The DIN cycles retract to the R plane by the documented default of
// this family's plain G81-G89 forms - there is no G98 word to write,
// so none is invented; every cycle block restates the hole position.
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
  var words = [g, wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), wo("R", p.R, orgZ)];
  if ((p.CycleType === "PECK" || p.CycleType === "DEEP") && has(p.Q) && p.Q > 0) {
    words.push(w("Q", p.Q)); // an increment (peck depth): no origin shift
  }
  if (ms > 0 && g !== "G73" && g !== "G84") words.push("P" + ms);
  words.push(fw(p.FeedRate));
  block(words);
  inCycle = true;
}

// ------------------------------------------------------------ probing
// #173. Words from Siemens' published SINUMERIK 840D sl programming
// manuals (Fundamentals; Job Planning): MEAS=1 (measure with probe 1,
// rising edge, deleting distance-to-go) on a G1 block, $AC_MEA[1] (the
// probe responded, 1, or not, 0), $AA_MW[axis] (the trip position in
// workpiece coordinates), IF..ENDIF, MSG, SETAL (user cycle alarms from
// 65000), the settable frames $P_UIFR[n,axis,TR|FI] (G54..G57 = 1..4)
// whose new value takes effect when the G5x is programmed again.
// Reading a main-run variable stops preprocessing implicitly, so the
// trip is read after the move ends. A measuring block that never trips
// alarms at once - a probe that never touched must not back off and
// measure air. A measured trip k is kept in R(50+3k)..R(52+3k) and the
// deviations in R90.., inside R0..R99 (the R parameters every 840D
// carries; more are a machine-data option). The back-off is incremental
// (G91). G58/G59 are axial programmable offsets
// on this control, not settable frames: setting one is refused by name.
// Arming the probe is the machine's own.
var probeBase = 50;
function axisIx(a) { return a === "X" ? 0 : (a === "Y" ? 1 : 2); }
function travelAxis(p) {  // where the target and the nominal trip differ most
  var d = [Math.abs(p.X - p.ExpectX), Math.abs(p.Y - p.ExpectY), Math.abs(p.Z - p.ExpectZ)];
  return d[0] >= d[1] && d[0] >= d[2] ? 0 : (d[1] >= d[2] ? 1 : 2);
}
function stopAlarm(cond, n, text) {  // message, alarm and stop when cond holds
  block(["IF " + cond]);
  block(["MSG(\"" + text + "\")"]);
  block(["SETAL(" + n + ")"]);
  block(["M0"]);
  block(["ENDIF"]);
}
function onProbeMove() {
  startPreamble();
  compRefuse("a probing move");
  var p = properties;
  block(["MEAS=1", "G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  stopAlarm("$AC_MEA[1]==0", 65000, "PROBE NO CONTACT");
  if (has(p.Measure) && p.Measure >= 0) {
    if (p.Measure > 12) { msg("probing: measurement " + p.Measure + " - only 13 trips fit R50..R88"); return; }
    var b = probeBase + 3 * p.Measure;
    block(["R" + b + "=$AA_MW[X]"]);
    block(["R" + (b + 1) + "=$AA_MW[Y]"]);
    block(["R" + (b + 2) + "=$AA_MW[Z]"]);
  }
}
function onProbeRetreat() {
  var p = properties;
  block(["G91", "G1", w("X", p.DeltaX), w("Y", p.DeltaY), w("Z", p.DeltaZ), fw(p.Feed)]);
  block(["G90"]);
}
// #174. A protected positioning move (the descent to a touch's start,
// the travel between touches, the rise) is the same MEAS=1 G1 block,
// read the other way round (840D sl Job Planning, measurement as
// above): MEAS deletes the distance-to-go on a trip and sets
// $AC_MEA[1] to 1, so a guard block that ends with the probe responded
// was stopped short by contact - it alarms (SETAL 65002, distinct from
// no-contact's 65000) and stops, so an obstacle where the model has
// none stops the program instead of the stylus being driven into it.
// $AC_MEA[1] is the test, not $AA_MW: the measured value is only
// written on a trip, so it is meaningless when the move ran clear.
function onProbeGuard() {
  startPreamble();
  compRefuse("a protected probe move");
  var p = properties;
  block(["MEAS=1", "G1", wo("X", p.X, orgX), wo("Y", p.Y, orgY), wo("Z", p.Z, orgZ), fw(p.Feed)]);
  stopAlarm("$AC_MEA[1]==1", 65002, "PROBE HIT ON POSITIONING");
}
function frameOf(word) {  // "G54".."G57" -> 1..4, G505..G599 -> 5..99; 0 none
  var n = Number(String(word).replace(/[^0-9]/g, ""));
  if (n >= 54 && n <= 57) return n - 53;
  if (n >= 505 && n <= 599) return n - 500;
  return 0;
}
function onProbeResult() {
  var p = properties;
  var org = [orgX, orgY, orgZ];
  var axes = p.Axes || [];
  var dev = 90;   // R90.. hold each axis's deviation, found - expected
  for (var i = 0; i < axes.length; i++) {
    var a = axes[i];
    var ai = axisIx(a.Axis);
    var sum = [];
    for (var k = 0; k < a.Measures.length; k++) sum.push("R" + (probeBase + 3 * a.Measures[k] + ai));
    var mean = a.Measures.length > 1 ? "(" + sum.join("+") + ")/" + a.Measures.length : sum[0];
    // The sign is folded into the operator so a negative expected value
    // (edge, outside corner) never writes a doubled sign like `--`.
    var ex = a.Expected - org[ai];
    var op = ex < 0 ? "+" : "-";
    block(["R" + (dev + i) + "=" + mean + op + fu(Math.abs(ex))]);
    stopAlarm("ABS(R" + (dev + i) + ")>" + fu(p.Tolerance), 65001, "PROBE " + a.Axis + " OUT OF TOLERANCE");
  }
  if (has(p.SetWorkOffset) && p.SetWorkOffset >= 0) {
    var want = "G" + (54 + Number(p.SetWorkOffset));
    var to = frameOf(want), from = frameOf(wcsWord);
    if (!to || !from) {
      msg("probing: setting " + want + " from " + wcsWord + " - only the settable frames G54..G57 / G505..G599 can be set on this control");
      return;
    }
    for (var j = 0; j < axes.length; j++) {
      var ax = axes[j].Axis;
      block(["$P_UIFR[" + to + "," + ax + ",TR]=$P_UIFR[" + from + "," + ax + ",TR]+R" + (dev + j)]);
      if (to !== from) block(["$P_UIFR[" + to + "," + ax + ",FI]=$P_UIFR[" + from + "," + ax + ",FI]"]);
    }
    block([wcsWord]);  // re-select: a written frame takes effect on its G5x
  } else {
    block(["MSG(\"CHECK PROBE RESULT R" + dev + "..\")"]);
    block(["M0"]);
  }
}

// ------------------------------------------------------------ footer
// Homing rides machine coordinates (G53), not a stored home point; the
// height is the machine's stated safe Z (#503, safeZ).
function onClose() {
  endPreamble();
  // #504: a trip home with nothing written after it (a trailing Go
  // home) already is the program-end home - written once, not twice.
  if (linesWritten() !== homedAt) {
    var z = safeZ();
    if (z !== null) block(["G53", "Z" + fu(z)]);
  }
  block(["M2"]);
}
