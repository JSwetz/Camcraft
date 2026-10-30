// CamCraft job script - the batch lane's shape (M5.3 F).
// Open a document, bring it current, post it, and print the report.
// Every verb answers JSON; every world write rides the command
// history; refusals arrive as catchable Errors and still land in the
// run's error record. The `cam` global is typed by cam.d.ts.

var job = cam.load("pocket-4x.camp");
cam.log("loaded", job.operations.length, "operations");

// Generate what is missing or stale, then judge before committing.
cam.generate();
var verdict = cam.verify();
if (!verdict.ok) {
    cam.log("verify says:", JSON.stringify(verdict));
}

// Post to the run's output - the same gate the app's Post button
// enforces; a refusal names the operation and the reason.
try {
    var out = cam.post({ to: "out/program.nc" });
    cam.log("posted", out.path, out.lines, "lines");
} catch (e) {
    cam.log("post refused:", e.message);
}

// A trailing expression is the run's answer: the batch CLI prints this.
cam.report();
