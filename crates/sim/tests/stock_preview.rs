//! M7.2 A: the preview march sees what the verdicts say. The eye's
//! sphere trace runs over the same field the column march answers
//! heights from, so a top-down pixel's hit must agree with the
//! verdict's column top, the class map must agree with
//! `color_classes`' vocabulary, and every no-verdict pixel must
//! carry NaN and unjudged - never a clean-looking zero. Refusals
//! name their defect (the Prime Directive's malformed-input law,
//! now covering cameras and frames).
//! Provenance: Hart's sphere tracing (SIGGRAPH 1996) over this
//! codebase's own field; the class law is `deviate`'s, relocated to
//! the hit point per the ROADMAP. Nothing from vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::preview and the
//! Stock::preview seam, end to end.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3};
use camcraft_sim::stock_sim::{
    preview, Backend, Deviation, PreviewCamera, SimOp, Stock, DEV_TOL, FIELD_BAND,
};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

const CELL: f64 = 1.0;

/// The 10 x 10 x 5 bounded box on unit cells - a stock with a floor
/// of its own, which is what the field arm always had.
fn box_stock() -> Stock {
    let f = cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    Stock::from_facets(Backend::Field { band: FIELD_BAND }, &f.facets, CELL).unwrap()
}

/// A camera looking straight down the box's axis from z = 40.
fn top_down() -> PreviewCamera {
    PreviewCamera::look_at(p(5.0, 5.0, 40.0), p(5.0, 5.0, 0.0), p(0.0, 1.0, 0.0), 0.5).unwrap()
}

/// The center pixel of a `w` frame: the ray straight down the axis.
fn center_hit(pv: &preview::Preview, w: u32) -> (f64, f32, f32) {
    let i = ((w / 2) * w + w / 2) as usize;
    (pv.depth[i], pv.class[i], pv.shade[i])
}

/// The plunge that floors a pocket at `floor_z`: a flat tool of r 2
/// driven down the box's axis (a Feed move - rapids touch nothing).
fn pocket(floor_z: f64) -> Vec<Move> {
    vec![Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: p(5.0, 5.0, 6.0),
        to: p(5.0, 5.0, floor_z),
        feed: Some(500.0),
        arc: None,
        axis: None,
    }]
}

fn cut(stock: &mut Stock, moves: &[Move]) {
    let ops = [SimOp {
        moves,
        radius: 2.0,
        ball: false,
        cone: None,
        corner: None,
    }];
    stock.simulate(&ops).unwrap();
}

/// The model the deviation reads against: a plate whose top is
/// `part_z`, solid below it.
fn plate(part_z: f64) -> Stock {
    let f = cube(p(0.0, 0.0, part_z - 10.0), p(10.0, 10.0, part_z));
    Stock::from_facets(Backend::Field { band: FIELD_BAND }, &f.facets, CELL).unwrap()
}

#[test]
fn top_down_finds_the_stock_top() {
    let stock = box_stock();
    let pv = stock.preview(None, &top_down(), 64, 64, DEV_TOL).unwrap();
    let (depth, class, shade) = center_hit(&pv, 64);
    // Eye at 40, top at 5: a vertical ray's distance is the one the
    // lattice reads EXACTLY along a column (a straight BFS stair).
    assert!((depth - 35.0).abs() < 0.1, "depth {depth}");
    assert_eq!(class, 3.0, "no target judged this pixel");
    assert!(shade > 0.2, "the lit top cannot shade black: {shade}");
    assert_eq!(pv.unresolved, 0, "a whole box in plain view resolves");
}

#[test]
fn the_hit_agrees_with_the_verdict_height() {
    // The law of the seam: what the eye lands on is the height the
    // march answers, not a second opinion of the surface.
    let mut stock = box_stock();
    cut(&mut stock, &pocket(2.0));
    let pv = stock.preview(None, &top_down(), 64, 64, DEV_TOL).unwrap();
    let (depth, _, _) = center_hit(&pv, 64);
    let eye_z = 40.0;
    let hit_z = eye_z - depth;
    let top = stock.top(5.5, 5.5);
    assert!(
        (hit_z - top).abs() < 0.25,
        "the eye saw {hit_z}, the verdict says {top}"
    );
}

#[test]
fn pocket_floor_is_clean_and_the_unmachined_top_is_rest() {
    let mut stock = box_stock();
    cut(&mut stock, &pocket(2.0));
    let target = plate(2.0);
    let pv = stock
        .preview(Some(&target), &top_down(), 64, 64, DEV_TOL)
        .unwrap();
    let (_, floor_class, _) = center_hit(&pv, 64);
    assert_eq!(floor_class, 0.0, "the floored pocket sits ON the part");
    // A pixel off the pocket's axis but inside the box at these
    // settings (row 38 of 64 lands ~3.5 mm off-axis, clear of the
    // r-2 pocket and of the walls): an uncut column, standing 3 mm
    // of rest over the plate.
    let i = (38 * 64 + 32) as usize;
    assert!(pv.depth[i].is_finite(), "the box covers that pixel");
    assert_eq!(pv.class[i], 1.0, "uncut stock over the part reads rest");
}

#[test]
fn cutting_under_the_part_reads_a_gouge() {
    let mut stock = box_stock();
    cut(&mut stock, &pocket(1.0));
    let target = plate(2.0);
    let pv = stock
        .preview(Some(&target), &top_down(), 64, 64, DEV_TOL)
        .unwrap();
    let (depth, class, _) = center_hit(&pv, 64);
    assert!(depth.is_finite(), "the gouged floor stands at 1");
    assert_eq!(class, 2.0, "one mm under the part reads gouge, not rest");
}

#[test]
fn a_lateral_ray_hits_the_wall_it_flyswats_into() {
    // The march trusts the gap-to-AABB jump outside the solid; the
    // jump is exact, so the first thing the eye meets is the box's
    // own face at x = 0.
    let stock = box_stock();
    let cam = PreviewCamera::look_at(p(-30.0, 5.0, 2.5), p(5.0, 5.0, 2.5), p(0.0, 0.0, 1.0), 0.2).unwrap();
    let pv = stock.preview(None, &cam, 32, 32, DEV_TOL).unwrap();
    let i = (16 * 32 + 16) as usize;
    assert!((pv.depth[i] - 30.0).abs() < 0.1, "depth {}", pv.depth[i]);
}

#[test]
fn empty_sky_carries_no_verdict() {
    // A camera aimed at the sky beside the world, every ray climbing
    // away from it: no material, no depth, class 3 - the no-verdict
    // value, never a clean zero. (Aim matters: a frame this wide at
    // this distance, pointed ACROSS the world, legitimately sees the
    // box's bottom face.)
    let stock = box_stock();
    let cam = PreviewCamera::look_at(p(-50.0, 50.0, 45.0), p(-50.0, 50.0, 60.0), p(0.0, 1.0, 0.0), 0.5).unwrap();
    let pv = stock.preview(None, &cam, 16, 16, DEV_TOL).unwrap();
    assert!(pv.depth.iter().all(|d| d.is_nan()), "sky answers no depth");
    assert!(pv.class.iter().all(|&c| c == 3.0), "sky judges nothing");
    assert_eq!(pv.unresolved, 0, "missing the world is an answer, not a bail");
}

#[test]
fn the_column_arm_refuses_the_march_by_name() {
    // The field-only law, said out loud: a height field's distance is
    // vertical and a lateral ray would thread its walls.
    let f = cube(p(0.0, 0.0, 0.0), p(10.0, 10.0, 5.0));
    let stock = Stock::from_facets(Backend::Column, &f.facets, CELL).unwrap();
    let e = stock.preview(None, &top_down(), 16, 16, DEV_TOL).unwrap_err();
    assert!(format!("{e}").contains("field arm"), "{e}");
}

#[test]
fn cameras_and_frames_refuse_their_own_bad_numbers() {
    // Degenerate frames and nonsense bases: named, never guessed at.
    let e = PreviewCamera::look_at(p(1.0, 1.0, 1.0), p(1.0, 1.0, 1.0), p(0.0, 0.0, 1.0), 0.5).unwrap_err();
    assert!(format!("{e}").contains("coincide"), "{e}");
    let e = PreviewCamera::look_at(p(0.0, 0.0, 5.0), p(0.0, 0.0, 0.0), p(0.0, 0.0, 1.0), 0.5).unwrap_err();
    assert!(format!("{e}").contains("parallel"), "{e}");
    let e = PreviewCamera::look_at(p(0.0, 0.0, 5.0), p(0.0, 0.0, 0.0), p(0.0, 1.0, 0.0), f64::NAN).unwrap_err();
    assert!(format!("{e}").contains("field of view"), "{e}");
    let e = PreviewCamera::look_at(p(f64::NAN, 0.0, 5.0), p(0.0, 0.0, 0.0), p(0.0, 1.0, 0.0), 0.5).unwrap_err();
    assert!(format!("{e}").contains("finite"), "{e}");

    let stock = box_stock();
    let cam = top_down();
    assert!(matches!(
        stock.preview(None, &cam, 0, 8, DEV_TOL),
        Err(camcraft_sim::stock_sim::StockError::Invalid(s)) if s.contains("frame size")
    ));
    assert!(matches!(
        stock.preview(None, &cam, 4096, 4096, DEV_TOL),
        Err(camcraft_sim::stock_sim::StockError::Invalid(s)) if s.contains("pixels")
    ));
}

#[test]
fn the_free_function_matches_the_seam() {
    // Stock::preview is a thin arm: the field's own march answers the
    // same frame byte for byte.
    let stock = box_stock();
    let field = stock.field().unwrap();
    let via_seam = stock.preview(None, &top_down(), 32, 32, DEV_TOL).unwrap();
    let direct = preview::preview(field, None, &top_down(), 32, 32, DEV_TOL).unwrap();
    // Depth compares as BITS: the sky pixels are NaN on both sides,
    // and NaN != NaN would fail the very honesty this test proves.
    let diff = (0..via_seam.depth.len()).find(|&i| {
        via_seam.depth[i].to_bits() != direct.depth[i].to_bits()
            || via_seam.class[i] != direct.class[i]
            || via_seam.shade[i] != direct.shade[i]
    });
    assert!(diff.is_none(), "seam and free function differ at pixel {diff:?}");
    assert_eq!(via_seam.unresolved, direct.unresolved);
}

#[test]
fn the_classes_the_eye_names_are_the_classes_the_verdict_counts() {
    // A program, judged twice: `deviate`'s column census and the eye
    // marching the same two fields. The vocabulary (0 clean, 1 rest,
    // 2 gouge) must mean the same pixels it means the same columns -
    // the located-not-just-counted law, continuous.
    let mut stock = box_stock();
    cut(&mut stock, &pocket(2.0));
    let target = plate(2.0);
    let facets = target.field().unwrap().facets().to_vec();
    let dev: Deviation = stock.deviate(&facets, DEV_TOL).unwrap();
    let pv = stock
        .preview(Some(&target), &top_down(), 64, 64, DEV_TOL)
        .unwrap();
    // The census says there is rest AND a clean floor; the eye must
    // see both classes in the same frame.
    let (rest_n, clean_n) = pv.class.iter().fold((0, 0), |a, &c| {
        (a.0 + (c == 1.0) as i32, a.1 + (c == 0.0) as i32)
    });
    assert!(dev.rest_cells > 0, "the plate leaves rest by census");
    assert!(rest_n > 0, "and the eye sees it: {rest_n} rest pixels");
    assert!(clean_n > 0, "the floor is clean to both: {clean_n} pixels");
}

#[test]
fn an_orthographic_eye_casts_parallel_rays_from_its_view_plane() {
    // #609: an orthographic camera traces the box as the parallel box
    // draws it - every hit lands under its own pixel's plane point, the
    // flat top reads one depth edge to edge, and a pixel off the box's
    // footprint misses even where a perspective cone would have hit.
    let stock = box_stock();
    let cam = top_down().orthographic(10.0).unwrap(); // 20 x 20 world, 1 px = 1 unit
    let (w, h) = (20u32, 20u32);
    let pv = stock.preview(None, &cam, w, h, DEV_TOL).unwrap();
    for py in 0..h {
        for px in 0..w {
            let (o, d) = cam.ray(px, py, w, h);
            assert_eq!((d.x, d.y, d.z), (cam.fwd.x, cam.fwd.y, cam.fwd.z), "every ray is the view axis");
            assert!((o.z - 40.0).abs() < 1e-9, "every origin sits on the eye plane");
            // Pixel centre in world XY: 1 unit pixels about (5, 5), row 0 at +Y.
            let (x, y) = (o.x, o.y);
            let depth = pv.depth[(py * w + px) as usize];
            let inside = (1.0..=9.0).contains(&x) && (1.0..=9.0).contains(&y);
            let outside = !(-2.0..=12.0).contains(&x) || !(-2.0..=12.0).contains(&y);
            if inside {
                assert!((depth - 35.0).abs() < 0.1, "pixel ({px},{py}) over ({x},{y}) reads the top at {depth}");
            }
            if outside {
                assert!(depth.is_nan(), "pixel ({px},{py}) over ({x},{y}) is off the box, yet hit at {depth}");
            }
        }
    }
    assert_eq!(pv.unresolved, 0);
    let flat = PreviewCamera { ortho_half_height: None, ..cam };
    assert_eq!(flat.ray(3, 7, w, h), (flat.eye, flat.ray_dir(3, 7, w, h)), "perspective rays still leave the eye");
    assert!(top_down().orthographic(0.0).is_err() && top_down().orthographic(f64::NAN).is_err());
}
