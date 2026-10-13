//! GPU smoke tests for the viewport pipelines.
//!
//! Provenance: our own test rig; nothing here runs in the app path.
//!
//! Headless UI tests build frames with no GPU, so buffer/pipeline/draw code
//! is otherwise unverified. These tests instantiate a real `View3D`, render
//! known geometry, and assert it paints pixels. On a machine with no usable
//! adapter they skip (green with a note) rather than fail.

use std::collections::HashSet;

use dear_imgui_wgpu::wgpu;
use hsmworks_app::stock::{StockShape, stock_mesh};
use hsmworks_ui::viewport::{Camera, GpuPart, View3D};

const SIDE: u32 = 256;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn device_and_queue() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let options = wgpu::RequestAdapterOptions {
        force_fallback_adapter: true,
        compatible_surface: None,
        ..Default::default()
    };
    let adapter = pollster::block_on(instance.request_adapter(&options)).ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some((device, queue))
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
    }
}

fn model_part(shape_d: f64) -> GpuPart {
    GpuPart {
        verts: soup_of(&StockShape::Box {
            dx: shape_d,
            dy: shape_d,
            dz: shape_d,
        }),
        selected: false,
        stock: false,
    }
}

fn render(parts: &[GpuPart], cam: &Camera) -> Option<(Vec<u8>, u32, u32)> {
    let (device, queue) = device_and_queue()?;
    let mut view = View3D::new(&device, FORMAT, SIDE, SIDE);
    view.set_document(&device, &queue, parts);
    let mvp = cam.mvp(1.0);
    let eye = cam.eye_f32();
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gpu-test-frame"),
    });
    view.render(&queue, &mut enc, &mvp, eye);
    queue.submit([enc.finish()]);
    view.snapshot_rgba(&device, &queue)
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

#[test]
fn empty_document_clears_to_background_only() {
    let Some((img, w, h)) = render(&[], &test_cam()) else {
        eprintln!("gpu test skipped: no usable adapter");
        return;
    };
    let changed = changed_pixels(&img);
    assert_eq!(changed, 0, "empty document on {w}x{h} should be uniform");
}
