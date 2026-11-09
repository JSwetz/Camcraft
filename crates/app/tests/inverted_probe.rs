//! Probing on a machine with an inverted X / Y (#392): the posted Fanuc
//! probing blocks, run by a macro B controller (`macro_b`) on a
//! simulated machine whose inverted axis moves the other way, against a
//! part sitting off its model position, write the PHYSICALLY correct
//! work offset - the part datum in the controller's coordinates - for
//! face, edge, corner, bore and boss probing. The same reader on a
//! standard machine finds the same physical answer (the interpreter is
//! checked too); a part out of reach alarms; and the standard-sign
//! program, or a translation missing any one of its probing words, does
//! NOT find it on the inverted machine (the reader is no tautology).
//!
//! Provenance: Fanuc's published operator documentation (G31, custom
//! macro B, G10 L2); touch-probe practice. Fixture values are our own.
//!
//! Gate map (M4.4): covers post::invert (probing), post::js (manifest
//! flags), assets/posts fanuc.cps (probing hooks), motion::probing.

mod inverted_support;
mod macro_b;

use camcraft_app::geometry::Point3;
use camcraft_app::post::cnc::{self, Record};
use camcraft_app::post::{self, engine, invert, probe_rec};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};
use inverted_support::{cps, machine};
use macro_b::{Pt, Run, Stop};

/// The setup's work-offset origin in the model (CAM world) frame.
const ORIGIN: Pt = [12.5, -7.25, 3.0];
/// The machine's physical position of that origin as the model places
/// it: G54 holds it (negated on an inverted axis) before probing.
const DATUM: Pt = [-312.5, -201.25, -405.0];
/// Where the real part sits relative to the model: what probing finds.
const SHIFT: Pt = [0.37, -0.21, -0.13];
/// Stylus ball radius.
const R: f64 = 2.0;
/// G55 before the program writes it: an unwritten axis stays this.
const UNSET: f64 = 999.0;
/// Program words carry three decimals.
const TOL: f64 = 0.002;

const MACHINES: &[&[&str]] = &[&[], &["X"], &["Y"], &["X", "Y"]];

/// The part, in model coordinates of the stylus ball CENTRE.
#[derive(Clone, Debug)]
enum Shape {
    /// The intersection of half-spaces: (surface point, outward normal).
    Solid(Vec<(Pt, Pt)>),
    /// Material outside a vertical bore (centre, radius) below `top`.
    Bore(Pt, f64),
    /// Material inside a vertical boss (centre, radius) below its top.
    Boss(Pt, f64),
}

impl Shape {
    fn holds(&self, c: Pt) -> bool {
        let rho = |o: Pt| ((c[0] - o[0]).powi(2) + (c[1] - o[1]).powi(2)).sqrt();
        match self {
            Shape::Solid(faces) => faces
                .iter()
                .all(|(q, n)| (c[0] - q[0]) * n[0] + (c[1] - q[1]) * n[1] + (c[2] - q[2]) * n[2] < R),
            Shape::Bore(o, rad) => rho(*o) > rad - R && c[2] - o[2] < R,
            Shape::Boss(o, rad) => rho(*o) < rad + R && c[2] - o[2] < R,
        }
    }
}

struct Case {
    name: &'static str,
    params: ProbeParams,
    shape: Shape,
    /// The axes the probing finds (0 X, 1 Y, 2 Z).
    found: &'static [usize],
}

fn params(mode: ProbeMode, dir: ProbeDir, dir_y: ProbeDir, t: Pt, radius: f64) -> ProbeParams {
    ProbeParams {
        mode,
        dir,
        dir_y,
        target: Point3::new(t[0], t[1], t[2]),
        radius,
        ball_diameter: 2.0 * R,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 50.0,
    }
}

fn wall(t: Pt, d: ProbeDir) -> (Pt, Pt) {
    let u = d.unit();
    (t, [-u.x, -u.y, -u.z])
}

fn cases() -> Vec<Case> {
    use ProbeDir::{XNeg, XPos, YNeg, YPos, ZNeg};
    use ProbeMode::{Boss, Bore, Corner, Edge, Face};
    let up = |t: Pt| (t, [0.0, 0.0, 1.0]);
    let (tz, tx, te) = ([10.0, 5.0, 20.0], [40.0, 5.0, 15.0], [20.0, 40.0, 20.0]);
    let (tc, tn, tb) = ([0.0, 0.0, 20.0], [80.0, 60.0, 20.0], [30.0, 30.0, 20.0]);
    vec![
        Case {
            name: "face -Z",
            params: params(Face, ZNeg, YPos, tz, 0.0),
            shape: Shape::Solid(vec![up(tz)]),
            found: &[2],
        },
        Case {
            name: "face -X",
            params: params(Face, XNeg, YPos, tx, 0.0),
            shape: Shape::Solid(vec![wall(tx, XNeg)]),
            found: &[0],
        },
        Case {
            name: "edge -Y",
            params: params(Edge, YNeg, YPos, te, 0.0),
            shape: Shape::Solid(vec![wall(te, YNeg), up(te)]),
            found: &[1],
        },
        Case {
            name: "corner +X+Y",
            params: params(Corner, XPos, YPos, tc, 0.0),
            shape: Shape::Solid(vec![wall(tc, XPos), wall(tc, YPos), up(tc)]),
            found: &[0, 1],
        },
        Case {
            name: "corner -X-Y",
            params: params(Corner, XNeg, YNeg, tn, 0.0),
            shape: Shape::Solid(vec![wall(tn, XNeg), wall(tn, YNeg), up(tn)]),
            found: &[0, 1],
        },
        Case { name: "bore", params: params(Bore, ZNeg, YPos, tb, 10.0), shape: Shape::Bore(tb, 10.0), found: &[0, 1] },
        Case { name: "boss", params: params(Boss, ZNeg, YPos, tb, 8.0), shape: Shape::Boss(tb, 8.0), found: &[0, 1] },
    ]
}

/// The case's probing records: set G55 from the trips, tolerance 0.5.
fn records(c: &Case) -> Vec<Record> {
    let t = probe_cycle(&c.params).unwrap_or_else(|e| panic!("{}: {e:?}", c.name));
    let d = post::Dump {
        op_name: "Probe1",
        tool_number: 7,
        tool_name: "PROBE-4",
        tool_diameter: 2.0 * R,
        clearance_z: 50.0,
        retract_z: 50.0,
        wcs: "G54",
        origin: ORIGIN,
        probe_result: Some(probe_rec::ProbeResult { set_wcs: Some(1), tol_mm: 0.5 }),
        ..post::Dump::default()
    };
    post::dump(&t, &d)
}

fn post_for(records: &[Record], letters: &[&str]) -> String {
    let out = engine::run(&cps("fanuc"), &machine(letters), &cnc::serialize(records))
        .unwrap_or_else(|e| panic!("inverted {letters:?}: {e}"));
    assert!(out.warnings.is_empty(), "inverted {letters:?}: {:?}", out.warnings);
    out.program
}

fn sign(flip: [bool; 3]) -> Pt {
    flip.map(|f| if f { -1.0 } else { 1.0 })
}

fn flip_of(letters: &[&str]) -> [bool; 3] {
    [letters.contains(&"X"), letters.contains(&"Y"), false]
}

/// Run `program` on the machine with `flip` inverted, the part shifted
/// by `shift` off its model position.
fn simulate(program: &str, flip: [bool; 3], shape: &Shape, shift: Pt) -> Run {
    let s = sign(flip);
    let mut offsets = [[UNSET; 3]; 6];
    offsets[0] = [0, 1, 2].map(|a| s[a] * DATUM[a]);
    // Physical tip -> model coordinates of the ball centre, with the
    // part moved by `shift` (so the part's model point m sits at m+shift).
    let material = |p: Pt| {
        let c = [0, 1, 2].map(|a| p[a] - DATUM[a] + ORIGIN[a] - shift[a] + if a == 2 { R } else { 0.0 });
        shape.holds(c)
    };
    macro_b::run(program, flip, offsets, &material).unwrap_or_else(|e| panic!("{e}\n{program}"))
}

/// The register the probing must write: the real part's datum in the
/// controller's coordinates (negated on an inverted axis).
fn want(flip: [bool; 3], shift: Pt) -> Pt {
    let s = sign(flip);
    [0, 1, 2].map(|a| s[a] * (DATUM[a] + shift[a]))
}

/// True when the run ends cleanly with G55 right on every found axis
/// and G54 untouched.
fn correct(run: &Run, c: &Case, flip: [bool; 3], shift: Pt) -> bool {
    let (w, g54) = (want(flip, shift), want(flip, [0.0; 3]));
    run.stop == Stop::End
        && c.found.iter().all(|&a| (run.offsets[1][a] - w[a]).abs() <= TOL)
        && (0..3).all(|a| c.found.contains(&a) || run.offsets[1][a] == UNSET)
        && (0..3).all(|a| (run.offsets[0][a] - g54[a]).abs() < 1e-12)
}

#[test]
fn every_mode_sets_the_physical_offset_on_every_machine() {
    for c in cases() {
        let recs = records(&c);
        let touches = recs.iter().filter(|r| matches!(r, Record::Touch(_))).count();
        for letters in MACHINES {
            let flip = flip_of(letters);
            let program = post_for(&recs, letters);
            let run = simulate(&program, flip, &c.shape, SHIFT);
            let what = format!("{} on inverted {letters:?}", c.name);
            assert_eq!(run.stop, Stop::End, "{what}\n{program}");
            assert_eq!(run.trips.len(), touches, "{what}: every touch trips once\n{program}");
            assert!(
                correct(&run, &c, flip, SHIFT),
                "{what}: G55 {:?}, want {:?} on axes {:?}\n{program}",
                run.offsets[1],
                want(flip, SHIFT),
                c.found
            );
        }
    }
}

#[test]
fn a_part_where_the_model_says_sets_the_unchanged_datum() {
    for c in cases() {
        for letters in MACHINES {
            let flip = flip_of(letters);
            let run = simulate(&post_for(&records(&c), letters), flip, &c.shape, [0.0; 3]);
            assert!(correct(&run, &c, flip, [0.0; 3]), "{} on {letters:?}: {:?}", c.name, run);
        }
    }
}

#[test]
fn a_part_out_of_reach_alarms_no_contact_and_writes_nothing() {
    let c = cases().into_iter().find(|c| c.name == "face -X").expect("face -X");
    // Travelling -X onto a part 7 mm further -X: past the 5 mm overtravel.
    for letters in MACHINES {
        let run = simulate(&post_for(&records(&c), letters), flip_of(letters), &c.shape, [-7.0, 0.0, 0.0]);
        assert_eq!(run.stop, Stop::Alarm(2, "PROBE NO CONTACT".into()), "{letters:?}");
        assert_eq!(run.offsets[1], [UNSET; 3], "{letters:?}: no offset is written");
    }
}

#[test]
fn a_part_off_by_more_than_the_tolerance_alarms() {
    let c = cases().into_iter().find(|c| c.name == "bore").expect("bore");
    for letters in MACHINES {
        let run = simulate(&post_for(&records(&c), letters), flip_of(letters), &c.shape, [0.8, 0.0, 0.0]);
        assert_eq!(run.stop, Stop::Alarm(1, "PROBE X OUT OF TOLERANCE".into()), "{letters:?}");
        assert_eq!(run.offsets[1], [UNSET; 3], "{letters:?}");
    }
}

#[test]
fn the_standard_program_on_an_inverted_machine_does_not_find_the_part() {
    for c in cases().into_iter().filter(|c| c.found.iter().any(|&a| a < 2)) {
        let program = post_for(&records(&c), &[]);
        for letters in &MACHINES[1..] {
            let flip = flip_of(letters);
            if !c.found.iter().any(|&a| flip[a]) {
                continue; // nothing measured on the inverted axis
            }
            let run = simulate(&program, flip, &c.shape, SHIFT);
            assert!(!correct(&run, &c, flip, SHIFT), "{} untranslated on {letters:?} passed: {run:?}", c.name);
        }
    }
}

/// What a probing program must do on the machine `flip`: set the right
/// offset for a part within reach, and alarm "no contact" (writing
/// nothing) for one 7 mm past the first touch's overtravel.
fn behaves(program: &str, c: &Case, flip: [bool; 3]) -> bool {
    let u = c.params.dir.unit();
    let away = [7.0 * u.x, 7.0 * u.y, 7.0 * u.z];
    let far = simulate(program, flip, &c.shape, away);
    correct(&simulate(program, flip, &c.shape, SHIFT), c, flip, SHIFT)
        && far.stop == Stop::Alarm(2, "PROBE NO CONTACT".into())
        && far.offsets[1] == [UNSET; 3]
}

/// Undo one word of the translation in a record.
type Undo = fn(&mut Record);

/// Each probing word the translation negates is load-bearing: undo one
/// (after the engine's own translation) and the inverted machine
/// crashes, alarms or sets a wrong offset - or, for the touch target,
/// probes past its overtravel instead of alarming "no contact" (the
/// skip runs on toward the wrong end, so a part out of reach is driven
/// into rather than reported).
#[test]
fn every_translated_probing_word_is_needed() {
    let (x, flip) = (invert::Flip { x: true, y: false, z: false }, [true, false, false]);
    for c in cases().into_iter().filter(|c| c.name.starts_with("corner")) {
        let mirrored = invert::translate_with(records(&c), x, true).expect("translates");
        // The pre-translated stream on a standard post is the engine's
        // own (less the header line naming the machine's inverted axis).
        let body = |p: String| p.lines().filter(|l| !l.starts_with("(MACHINE ")).collect::<Vec<_>>().join("\n");
        assert_eq!(body(post_for(&mirrored, &[])), body(post_for(&records(&c), &["X"])), "{}", c.name);
        assert!(behaves(&post_for(&mirrored, &[]), &c, flip), "{}: the full translation", c.name);
        let undo: [(&str, Undo); 3] = [
            ("touch target", |r| {
                if let Record::Touch(t) = r {
                    t.target[0] = -t.target[0];
                }
            }),
            ("back-off delta", |r| {
                if let Record::Retreat(t) = r {
                    t.delta[0] = -t.delta[0];
                }
            }),
            ("found expected", |r| {
                if let Record::Found(f) = r {
                    f.axes.iter_mut().filter(|a| a.axis == 0).for_each(|a| a.expected = -a.expected);
                }
            }),
        ];
        for (what, f) in undo {
            let mut broken = mirrored.clone();
            broken.iter_mut().for_each(f);
            assert!(!behaves(&post_for(&broken, &[]), &c, flip), "{}: passes without the {what}", c.name);
        }
    }
}

#[test]
fn probing_through_an_unverified_script_still_refuses_by_name() {
    let c = cases().into_iter().find(|c| c.name == "bore").expect("bore");
    let stream = cnc::serialize(&records(&c));
    let e = engine::run(&cps("haas"), &machine(&["X"]), &stream).map(|o| o.program).expect_err("refused").to_string();
    assert_eq!(
        e,
        "machine VMC 3-AXIS has an inverted X axis: operation Probe1: probing with a post script that does not \
         declare probeAxisInversion = true (Fanuc does) is not supported for an inverted axis yet"
    );
    let probing = invert::translate(records(&c), invert::Flip { x: false, y: true, z: false });
    assert!(probing.is_err_and(|e| e.iter().any(|l| l.contains("probeAxisInversion"))), "plain translate refuses");
}

/// The reader itself: both deviation forms (main's `a--b` and #377's
/// folded `a+b`) read alike, brackets and ABS nest, IF..GOTO jumps, and
/// an unmodelled word is an error rather than a pass.
#[test]
fn the_macro_reader_reads_both_sign_forms() {
    let program = "#100=5\n#101=#100--2.5\n#102=#100+2.5\n#103=[[#101+#102]/2]-ABS[-1]\n\
                   IF[#101EQ#102]GOTO7\n#3000=9(NOT EQUAL)\nN7G10L2P3X[#101]Y[#102]Z#103\nM30";
    let run = macro_b::run(program, [false; 3], [[0.0; 3]; 6], &|_| false).expect("reads");
    assert_eq!(run.stop, Stop::End);
    assert_eq!(run.offsets[2], [7.5, 7.5, 6.5]);
    let bad = macro_b::run("G90 G02 X1 Y1 I1", [false; 3], [[0.0; 3]; 6], &|_| false);
    assert!(bad.is_err_and(|e| e.contains("G2 is not modelled")), "an arc is not a probing block");
}
