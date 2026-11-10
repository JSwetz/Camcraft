//! GPU smoke tests for the viewport pipelines.
//!
//! Provenance: our own test rig; nothing here runs in the app path.
//!
//! Headless UI tests build frames with no GPU, so buffer/pipeline/draw code
//! is otherwise unverified. These tests instantiate a real `View3D`, render
//! known geometry, and assert it paints pixels. The device comes from
//! `headless_gpu`: the software adapter, else the hardware one (#615). On
//! a machine with no adapter at all they print a loud SKIP (not a pass) -
//! `CAMCRAFT_REQUIRE_GPU=1` makes that a failure.
//!
//! Gate map (M4.4): covers ui::state, ui::viewport, app::stock.

use std::collections::HashSet;
use std::sync::Mutex;

use egui_wgpu::wgpu;
use camcraft_app::stock::{StockShape, stock_mesh};
use camcraft_ui::state::DisplayMode;
use camcraft_ui::viewport::{Camera, GpuPart, ProjectionMode, View3D};

const SIDE: u32 = 256;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
// Concurrent fallback-device creation and teardown can crash in software
// Vulkan drivers; keep the entire render/readback lifecycle on one test at
// a time while leaving the rest of the test suite parallel.
static GPU_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Software adapter first, else the hardware one (#615); no adapter at
/// all prints a loud SKIP, and `CAMCRAFT_REQUIRE_GPU=1` fails instead.
fn device_and_queue() -> Option<(wgpu::Device, wgpu::Queue)> {
    match camcraft_ui::headless_gpu::device_for_test("gpu_render") {
        Ok(gpu) => gpu.map(|g| (g.device, g.queue)),
        Err(err) => panic!("{err}"),
    }
}

/// Triangle soup of a stock shape, world-identity placed.
fn soup_of(shape: &StockShape) -> Vec<[f32; 3]> {
    let tris = stock_mesh(shape).expect("sane shape meshes");
    let mut v = Vec::with_capacity(tris.len() * 3);
    for t in &tris {
        for p in [t.a, t.b, t.c] {
            v.push([p.x as f32, p.y as f32, p.z as f32]);
        }
    }
    v
}

fn stock_part(shape: &StockShape) -> GpuPart {
    GpuPart {
        verts: soup_of(shape),
        selected: false,
        stock: true,
        fill: false,
        classes: Vec::new(),
    }
}

fn model_part(shape_d: f64) -> GpuPart {
    model_part_at(shape_d, [0.0, 0.0, 0.0])
}

fn model_part_at(shape_d: f64, off: [f32; 3]) -> GpuPart {
    let mut verts = soup_of(&StockShape::Box {
        dx: shape_d,
        dy: shape_d,
        dz: shape_d,
    });
    for v in &mut verts {
        for k in 0..3 {
            v[k] += off[k];
        }
    }
    GpuPart {
        verts,
        selected: false,
        stock: false,
        fill: false,
        classes: Vec::new(),
    }
}

/// A flat CCW square sheet at height `z`, half-width `h`: the shape a
/// region fill part carries (a triangle list, no lighting, no wire).
fn fill_square(h: f32, z: f32) -> GpuPart {
    GpuPart {
        verts: vec![
            [-h, -h, z],
            [h, -h, z],
            [h, h, z],
            [-h, -h, z],
            [h, h, z],
            [-h, h, z],
        ],
        selected: false,
        stock: false,
        fill: true,
        classes: Vec::new(),
    }
}

fn render(parts: &[GpuPart], cam: &Camera) -> Option<(Vec<u8>, u32, u32)> {
    render_mode(parts, cam, DisplayMode::ShadedEdges)
}

/// One frame of `parts` under an explicit display mode.
fn render_mode(
    parts: &[GpuPart],
    cam: &Camera,
    display: DisplayMode,
) -> Option<(Vec<u8>, u32, u32)> {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (device, queue) = device_and_queue()?;
    let mut view = View3D::new(&device, FORMAT, SIDE, SIDE);
    view.set_document(&device, &queue, parts);
    let mvp = cam.mvp(1.0);
    let eye = cam.eye_f32();
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gpu-test-frame"),
    });
    view.render(&queue, &mut enc, &mvp, eye, display);
    queue.submit([enc.finish()]);
    view.snapshot_rgba(&device, &queue)
}

/// True when the pixel differs from the frame's corner (background).
fn off_background(img: &[u8], ix: usize) -> bool {
    let bg: [u8; 4] = img[..4].try_into().expect("frame wider than 4 px");
    let p = &img[ix * 4..ix * 4 + 4];
    p.iter()
        .zip(bg)
        .any(|(a, b)| i32::from(*a) - i32::from(b) > 6 || i32::from(b) - i32::from(*a) > 6)
}

/// Pixels differing from the corner (background) color by a small epsilon.
fn changed_pixels(img: &[u8]) -> usize {
    let bg: [u8; 4] = img[..4].try_into().expect("frame wider than 4 px");
    img.chunks(4)
        .filter(|p| {
            p.iter().zip(bg).any(|(a, b)| i32::from(*a) - i32::from(b) > 6 || i32::from(b) - i32::from(*a) > 6)
        })
        .count()
}

/// Coarse color clusters among the changed pixels: how many distinct
/// surfaces the frame distinguishes.
fn distinct_colors(img: &[u8]) -> usize {
    let bg: [u8; 4] = img[..4].try_into().expect("frame wider than 4 px");
    let mut hues = HashSet::new();
    for p in img.chunks(4) {
        if p.iter().zip(bg).any(|(a, b)| (i32::from(*a) - i32::from(b)).abs() > 6) {
            hues.insert((p[0] >> 4, p[1] >> 4, p[2] >> 4));
        }
    }
    hues.len()
}

/// A camera that frames the stock shapes below (centered-ish on z∈[0,h]).
fn test_cam() -> Camera {
    Camera {
        target: [0.0, 0.0, 20.0],
        yaw: 0.9,
        pitch: 0.5,
        dist: 160.0,
        ..Camera::default()
    }
}

#[test]
fn box_stock_paints_steel_over_background() {
    let Some((img, w, h)) = render(
        &[stock_part(&StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 40.0,
        })],
        &test_cam(),
    ) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    let total = (w * h) as usize;
    assert!(
        changed > total / 20 && changed < total - total / 20,
        "box stock should cover a solid but partial band: {changed}/{total}"
    );
}

#[test]
fn cylinder_stock_paints_steel_over_background() {
    // The feedback-4 regression: the parametric cylinder must render just
    // like the box does.
    let Some((img, w, h)) = render(
        &[stock_part(&StockShape::Cylinder {
            diameter: 60.0,
            height: 40.0,
        })],
        &test_cam(),
    ) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    let total = (w * h) as usize;
    assert!(
        changed > total / 20 && changed < total - total / 20,
        "cylinder stock should cover a solid but partial band: {changed}/{total}"
    );
}

#[test]
fn model_reads_through_translucent_stock() {
    // Opaque cube first, translucent cylinder stock around it: the frame
    // must distinguish background, bare model, and stock-blended regions.
    let parts = [
        model_part(20.0),
        stock_part(&StockShape::Cylinder {
            diameter: 60.0,
            height: 40.0,
        }),
    ];
    let Some((img, _w, _h)) = render(&parts, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let hues = distinct_colors(&img);
    assert!(hues >= 3, "want >=3 color clusters, got {hues}");
}

/// M13.1 owed: the machined stock's extracted mesh (the STL export's
/// triangles) draws through the stock pipeline like the box it stands
/// in for - a 12x12x20 box faced to 18, at the app suite's coarse cell.
#[test]
fn machined_mesh_paints_as_stock() {
    use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
    use camcraft_app::geometry::{cube, IDENTITY_Q, Point3};
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::schema::Location;
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let shape = StockShape::Box { dx: 12.0, dy: 12.0, dz: 20.0 };
    history.execute(&world, SetStock::new(&world, shape, loc).expect("box stock"));
    let tool = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t);
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool.expect("catalog entry")).expect("tool"));
    let part = cube(Point3::new(-6.0, -6.0, 0.0), Point3::new(6.0, 6.0, 18.0));
    history.execute(&world, ImportModel::from_mesh("part", part));
    let params = OpParams::Face { holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let fv = camcraft_app::stock_view::field_view_with(&world, 12.0 * 12.0 * 20.0 / 3.6).expect("field view");
    let tris = camcraft_app::stock_export::machined_mesh(&fv).expect("a closed shell");
    let verts = tris.iter().flat_map(|t| [t.a, t.b, t.c].map(|p| [p.x as f32, p.y as f32, p.z as f32])).collect();
    let part = GpuPart { verts, selected: false, stock: true, fill: false, classes: Vec::new() };
    let cam = Camera { target: [0.0, 0.0, 10.0], yaw: 0.9, pitch: 0.5, dist: 50.0, ..Camera::default() };
    let Some((img, w, h)) = render(&[part], &cam) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    let total = (w * h) as usize;
    assert!(changed > total / 20 && changed < total - total / 20, "the machined shell paints a band: {changed}/{total}");
}

#[test]
fn empty_document_clears_to_background_only() {
    let Some((img, w, h)) = render(&[], &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    assert_eq!(changed, 0, "empty document on {w}x{h} should be uniform");
}

#[test]
fn wireframe_paints_wire_not_faces() {
    // The M1.4 display-mode law, pixel half: the SAME buffers render two
    // ways. Shaded floods the silhouette with painted faces; wireframe
    // leaves the face interiors as background and keeps only the (bright,
    // over the dark background - `u.wire` lifts the wire color) wire.
    let cube = [model_part(60.0)];
    let Some((shaded, w, h)) = render(&cube, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((wire, _, _)) = render_mode(&cube, &test_cam(), DisplayMode::Wireframe) else {
        return; // the shaded render found an adapter; this one cannot differ
    };
    let total = (w * h) as usize;
    let s = changed_pixels(&shaded);
    let t = changed_pixels(&wire);
    assert!(s > total / 16, "shaded should flood the silhouette: {s}/{total}");
    assert!(t > 100, "wireframe must still draw its (brightened) wire: {t} px");
    assert!(t * 3 < s, "wireframe paints lines, not faces: {t} vs {s}");
}

#[test]
fn wireframe_hides_wire_behind_solids() {
    // The hidden-line half of the law: the wireframe faces write depth, so
    // a bigger cube farther down the view line may add wire OUTSIDE the
    // front cube's silhouette but never pierce through it. The front
    // cube's shaded render is the mask - faces are exactly its footprint.
    let cam = test_cam();
    let e = cam.eye_f32();
    let t = [0.0f32, 0.0, 20.0]; // the same target test_cam orbits
    let k = 140.0 / 160.0; // push the back cube 140 units past the target
    let back = [t[0] + (t[0] - e[0]) * k, t[1] + (t[1] - e[1]) * k, t[2] + (t[2] - e[2]) * k];
    let front = [model_part(60.0)];
    let both = [model_part(60.0), model_part_at(90.0, back)];
    let Some((mask, w, h)) = render(&front, &cam) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((a, _, _)) = render_mode(&front, &cam, DisplayMode::Wireframe) else {
        return;
    };
    let Some((b, _, _)) = render_mode(&both, &cam, DisplayMode::Wireframe) else {
        return;
    };
    let total = (w * h) as usize;
    let mut inside = 0usize;
    let mut pierced = 0usize;
    let mut outside_added = 0usize;
    for i in 0..total {
        let in_face = off_background(&mask, i);
        if in_face {
            inside += 1;
            if off_background(&a, i) != off_background(&b, i) {
                pierced += 1;
            }
        } else if !off_background(&a, i) && off_background(&b, i) {
            outside_added += 1;
        }
    }
    assert!(inside > total / 16, "the front silhouette should be a real mask");
    assert!(pierced * 200 < inside, "wire must not bleed through solids: {pierced}/{inside}");
    assert!(outside_added > 100, "the far cube must still draw around the near one: {outside_added}");
}

#[test]
fn fill_only_document_uses_no_empty_edge_buffer_slice() {
    let Some((image, _, _)) = render(&[fill_square(12.0, 0.0)], &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    assert!(changed_pixels(&image) > 0, "the fill-only scene should paint");
}

#[test]
fn cut_region_fill_is_translucent() {
    // The same model twice, once with a fill sheet riding just above its
    // top face: the sheet must change pixels (it paints) and every pixel
    // it paints must keep most of what was under it (it blends, never
    // replaces) - the plan's "translucent, not a paint-over".
    let Some((a_img, _, _)) = render(&[model_part(40.0)], &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let Some((b_img, _, _)) = render(&[model_part(40.0), fill_square(12.0, 41.0)], &test_cam())
    else {
        return; // first render found an adapter, this one cannot fail differently
    };
    let mut moved = 0usize;
    let mut see_through = 0usize;
    for (pa, pb) in a_img.chunks(4).zip(b_img.chunks(4)) {
        let d = (pb[0] as i32 - pa[0] as i32).abs()
            + (pb[1] as i32 - pa[1] as i32).abs()
            + (pb[2] as i32 - pa[2] as i32).abs();
        if d > 12 {
            moved += 1;
            // alpha 0.22: a channel can shift at most ~0.22*255 across a
            // full-range boundary, and 40 keeps the test honest against
            // an opaque plate, which would repaint channels far harder.
            if (pb[0] as i32 - pa[0] as i32).abs() < 40
                && (pb[1] as i32 - pa[1] as i32).abs() < 40
                && (pb[2] as i32 - pa[2] as i32).abs() < 40
            {
                see_through += 1;
            }
        }
    }
    assert!(moved > 200, "the fill sheet painted {moved} pixels; want a visible footprint");
    assert!(
        see_through * 2 > moved,
        "fill must blend over what it covers, not replace it: {see_through}/{moved} gentle"
    );
}

#[test]
fn orthographic_paints_the_band_perspective_does() {
    // #600: the ortho matrix mapped depth OpenGL-style, into negative
    // clip z, and WebGPU's 0 <= z <= w rule discarded every triangle -
    // the viewport went blank the moment Orthographic was clicked.
    // The same part and camera must paint a comparable band in both
    // modes; the framing is built to match at the target plane.
    let parts = [stock_part(&StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 40.0,
    })];
    let Some((persp_img, w, h)) = render(&parts, &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let ortho_cam = Camera {
        projection: ProjectionMode::Orthographic,
        ..test_cam()
    };
    let Some((ortho_img, _, _)) = render(&parts, &ortho_cam) else {
        return; // the perspective render found an adapter; this one cannot differ
    };
    let total = (w * h) as usize;
    let p = changed_pixels(&persp_img);
    let o = changed_pixels(&ortho_img);
    assert!(o > total / 20 && o < total - total / 20, "ortho should paint a solid band, not {o}/{total}");
    assert!(o * 2 < p * 3 && p * 2 < o * 3, "the modes frame alike at dist: {o} ortho vs {p} perspective");
}
