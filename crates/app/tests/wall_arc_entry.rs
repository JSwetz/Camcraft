//! The wall-side arc entry and the level-to-level transition of an
//! outside contour (M11.1 follow-up to a hand-test report): the user's
//! own part (`testdata/models/Milling-test.step`, a D-shaped plate with
//! a blind bore) contoured by its whole section with a tangential lead
//! and Entry = Arc, regenerated through the commands; and a single-ring
//! wall to pin the stay-down transition. Every path must verify silent.
//!
//! Gate map (M11.1): covers app::commands, app::machining2d (contour
//! wall arc, seam leads), app::operation, app::verify, app::cl.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::machining2d::{contour, ContourParams, Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, verify};
use camcraft_app::world::new_world;

/// The reported op: Contour, EM-1000-4FL, clearance stock top +20.5,
/// retract stock top +13.5, top = model top, bottom = Z -7 (the report's
/// work offset; here the stock bottom, which resolves to the same Z),
/// stepdown 5, leave 0, blends on, tangential lead r5, Entry = Arc,
/// no tabs, feed 1200 / plunge 300, climb.
fn user_op() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 20.5),
        retract_z: Height::of(HeightReference::TopOfStock, 13.5),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfStock, 0.0),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Tangential { radius: 5.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Arc,
        helix_radius: 1.0,
        blend_turns: true,
        tilt: None,
    }
}

/// Import the part, a 70 x 70 block from Z -7 to the part's top (Z 20),
/// the tool, the op - and generate it, all through commands.
fn user_job() -> (Toolpath, usize) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let loc = Location { pos: Point3::new(0.0, 0.0, -7.0), ..Default::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz: 27.0 }, loc);
    h.execute(&world, stock.expect("block stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", tool).expect("tool"));
    let op = CreateOperation::new(&world, "C1", "EM-1000-4FL", SrcRef::Entity(name), user_op());
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    let e = model::find_by_name(&world, "C1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.len())
}

/// An arc entry's chord: a feed that descends while it moves in XY.
fn is_chord(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.to.z < m.from.z && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-9
}

/// The arc entries: (index of the first chord, index past the last).
fn fans(t: &Toolpath) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.moves.len() {
        if is_chord(&t.moves[i]) {
            let s = i;
            while i < t.moves.len() && is_chord(&t.moves[i]) {
                i += 1;
            }
            out.push((s, i));
        } else {
            i += 1;
        }
    }
    out
}

/// A readable token per move: kind and what it does to Z.
fn token(m: &Move) -> &'static str {
    let xy = (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-9;
    match (m.kind, m.to.z.total_cmp(&m.from.z), xy) {
        (MoveKind::Rapid, std::cmp::Ordering::Greater, _) => "rapid-up",
        (MoveKind::Rapid, std::cmp::Ordering::Less, false) => "rapid-down",
        (MoveKind::Rapid, _, _) => "rapid-across",
        (MoveKind::Plunge, _, _) => "plunge",
        (MoveKind::Feed, std::cmp::Ordering::Less, true) => "arc-chord",
        (MoveKind::Feed, std::cmp::Ordering::Equal, _) => "feed",
        (MoveKind::Feed, _, _) => "feed-z",
    }
}

/// The moves that carry the tool from the last planar cut before `at`
/// to the first chord at `at`, as tokens.
fn approach(t: &Toolpath, at: usize) -> Vec<&'static str> {
    let mut s = at;
    while s > 0 && !(t.moves[s - 1].kind == MoveKind::Feed && t.moves[s - 1].from.z == t.moves[s - 1].to.z) {
        s -= 1;
    }
    t.moves[s..at].iter().map(token).collect()
}

fn unit(x: f64, y: f64) -> (f64, f64) {
    let n = x.hypot(y);
    (x / n, y / n)
}

/// XY travel direction of `m` at its start (a line's chord, an arc's
/// tangent) and at its end.
fn dir_at(m: &Move, start: bool) -> (f64, f64) {
    match m.arc {
        None => unit(m.to.x - m.from.x, m.to.y - m.from.y),
        Some(a) => {
            let p = if start { m.from } else { m.to };
            let (rx, ry) = (p.x - a.center.x, p.y - a.center.y);
            if a.ccw { unit(-ry, rx) } else { unit(ry, -rx) }
        }
    }
}

fn dot(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

#[test]
fn the_reported_outside_contour_enters_every_level_on_one_arc() {
    let (t, warnings) = user_job();
    assert!(t.is_chained() && t.all_finite());
    // The verifier speaks silent: no gouge, no crossing - the 35 the
    // report carried were 14 lead gouges in the bore ring and 21
    // lead-in/lead-out crossings at the two seams.
    assert_eq!(warnings, 0, "{:?}", t.notes);
    let summary = t.status_summary("C1", warnings, Default::default());
    assert!(summary.contains("7 level(s)"), "{summary}");
    let notes = t.notes.join(" | ");
    assert!(!notes.contains("needs bounded air"), "the arc is no longer refused on the wall: {notes}");
    assert!(notes.contains("closes on the lead-in's start"), "{notes}");
    // The bore ring of the section has no room for the arc beside its
    // wall: refused by name, its graze stands.
    assert!(notes.contains("no arc fits beside the wall at the lead-in"), "{notes}");

    let levels = [20.0, 15.0, 10.0, 5.0, 0.0, -5.0, -7.0];
    // The outer ring's entries stand beyond the part's R30 arc; the bore
    // ring's (inside R12.5) are counted apart.
    let outer = |m: &Move| m.to.x.hypot(m.to.y) > 30.0;
    let all = fans(&t);
    let f: Vec<_> = all.iter().copied().filter(|&(s, _)| outer(&t.moves[s])).collect();
    assert_eq!(f.len(), levels.len(), "one arc entry per level on the outer ring");
    // The bore ring: only the last, 2 mm drop leaves its arc room in
    // the bore's air; the other six levels graze by name (noted).
    let bore: Vec<_> = all.iter().filter(|&&(s, _)| !outer(&t.moves[s])).collect();
    assert_eq!(bore.len(), 1, "{bore:?}");
    assert_eq!(t.moves[bore[0].1 - 1].to.z, -7.0);
    for (k, &(s, e)) in f.iter().enumerate() {
        let last = &t.moves[e - 1];
        assert_eq!(last.to.z, levels[k], "entry {k} lands on its level");
        assert!(e - s >= 4, "entry {k} is a chord fan, not one diagonal: {} chords", e - s);
        // The fan is tangent to the lead-in at the bottom: its last
        // chord heads the way the lead-in arc starts, and nearly level.
        let lead_in = &t.moves[e];
        assert!(lead_in.arc.is_some() && lead_in.to.z == levels[k], "the lead-in follows the arc");
        assert!(dot(dir_at(last, false), dir_at(lead_in, true)) > 0.999, "entry {k} meets the lead-in tangent");
        let drop = last.from.z - last.to.z;
        assert!(drop < 0.1, "entry {k}'s last chord is near level: {drop}");
        // The tangential lead lands on the wall tangent to the walk.
        let walk = &t.moves[e + 1];
        assert!(dot(dir_at(lead_in, false), dir_at(walk, true)) > 0.999, "lead-in {k} joins the wall tangent");
        // The transition reads as the standard sequence: first entry
        // from clearance; the second level drops onto the level above
        // (the cut top); deeper levels feed the rest at plunge feed.
        let seq = approach(&t, s);
        let want: &[&str] = match k {
            0 => &["rapid-across", "rapid-down"],
            1 => &["rapid-up", "rapid-across", "rapid-down"],
            _ => &["rapid-up", "rapid-across", "rapid-down", "plunge"],
        };
        assert_eq!(seq, want, "entry {k}'s approach");
    }
    // Plunges left: the outer ring's deeper approaches to the arc top
    // (5), the bore ring's six named grazes and its one arc approach.
    let plunges = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert_eq!(plunges, 5 + 6 + 1);
}

fn disc() -> Region {
    let pts = (0..128)
        .map(|k| {
            let a = std::f64::consts::TAU * f64::from(k) / 128.0;
            Point2::new(30.0 * a.cos(), 30.0 * a.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).expect("ring")]).expect("disc")
}

fn wall_params(entry: Entry3D, tabs: u32) -> ContourParams {
    ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 40.0,
        retract_z: 30.0,
        top_z: 20.0,
        bottom_z: 5.0,
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::Tangential { radius: 5.0 },
        tolerance: 0.01,
        tab_count: tabs,
        // Wider than the two lead arcs' reach over a tab (2 x 5): the
        // tab-gap lead crossing is a separate, pre-existing law.
        tab_width: 12.0,
        tab_height: 2.0,
        entry,
        helix_radius: 1.0,
        blend_turns: false,
    }
}

fn verify_wall(t: &Toolpath, region: &Region) -> Vec<camcraft_app::verify::Warning> {
    let tool = ToolGeom {
        radius: 5.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 5.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: None,
        tool: &tool,
        mode: Mode::Boundary,
        stock_top_z: 20.0,
        clearance_z: 40.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    };
    verify(t, &cx)
}

#[test]
fn a_single_wall_ring_stays_down_between_levels() {
    let r = disc();
    let t = contour(&r, &wall_params(Entry3D::Arc, 0)).expect("contour");
    assert_eq!(verify_wall(&t, &r), Vec::new(), "{:?}", t.notes);
    assert!(!t.moves.iter().any(|m| m.kind == MoveKind::Plunge), "no plunge anywhere: {:?}", t.notes);
    let f = fans(&t);
    assert_eq!(f.len(), 4, "levels 20, 15, 10, 5 each enter on one arc");
    // Level 0 comes down from clearance onto the arc top; every level
    // after links at feed along the plane above, off its own lead-out,
    // and arcs down - no lift, no rapid, no loop.
    assert_eq!(approach(&t, f[0].0), ["rapid-across", "rapid-down"]);
    for &(s, _) in &f[1..] {
        assert_eq!(approach(&t, s), Vec::<&str>::new(), "the stay-down link is a planar feed");
        let link = &t.moves[s - 1];
        assert_eq!(link.kind, MoveKind::Feed);
        assert!(link.arc.is_none() && link.from.z == link.to.z);
    }
    // Every cutter center of the whole path stays out of the part's
    // tool-radius offset (the verifier's Boundary law above), and the
    // descents stand off the wall: the fan never comes nearer the axis
    // than the walked ring.
    for &(s, e) in &f {
        for m in &t.moves[s..e] {
            assert!(m.to.x.hypot(m.to.y) >= 35.0 - 0.01, "fan point inside the offset: {m:?}");
        }
    }
}

#[test]
fn the_wall_arc_stands_down_by_name_on_tabbed_levels() {
    let r = disc();
    let t = contour(&r, &wall_params(Entry3D::Arc, 3)).expect("contour");
    assert_eq!(verify_wall(&t, &r), Vec::new(), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("the arc stands down on tabbed levels")), "{:?}", t.notes);
    // Untabbed levels (20, 15, 10) still arc; the tabbed bottom grazes.
    assert_eq!(fans(&t).len(), 3);
}

#[test]
fn plunge_entry_on_the_wall_is_unchanged_by_the_arc_law() {
    // Opt-in: the same ring under Plunge walks the same planar cut, and
    // nothing about it descends on a fan.
    let r = disc();
    let plunge = contour(&r, &wall_params(Entry3D::Plunge, 0)).expect("contour");
    assert!(fans(&plunge).is_empty());
    let planar = |t: &Toolpath| -> Vec<_> {
        t.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && m.arc.is_some()).map(|m| (m.from, m.to)).collect()
    };
    let arc = contour(&r, &wall_params(Entry3D::Arc, 0)).expect("contour");
    assert_eq!(planar(&plunge), planar(&arc), "the walked rings and leads are the same");
}
