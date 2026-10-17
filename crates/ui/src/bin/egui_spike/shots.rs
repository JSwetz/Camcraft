//! S2: the text-crispness dumps.
//!
//! The spike's claim is that egui renders at Fusion-caliber sharpness on
//! our stack, and the S4 report has to show it, not assert it. So on the
//! first windowed frame we run the identical shell layout twice - once at
//! 1.0 and once at 1.5 pixels per point - through a private Context and a
//! private Renderer onto offscreen Rgba8Unorm targets, read them back and
//! write `spike-shots/egui_100.png` / `egui_150.png` for the user's eyes.
//! (The imgui shell cannot produce these: its renderer is welded to the
//! live surface, and the harness has no offscreen pass - the kittest
//! snapshots in S3 are the version of this that becomes a regression gate.)
//!
//! No unwrap anywhere: every wgpu/fs step returns through `Result`, and a
//! failed dump only prints - the spike window must survive its own proof.

use egui_wgpu::wgpu;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};

use hsmworks_ui::state::{CommandKind, State};

use super::{build_dock, drain_textures, shell_layout, theme, world, VIEW3D_FORMAT};

/// Dump both scales. `vp_rgba`/`vp_w`/`vp_h` are this frame's View3D
/// readback, pasted into the shot as the viewport tab's image so the shot
/// shows real geometry behind real text.
pub fn dump(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vp_rgba: Vec<u8>,
    vp_w: u32,
    vp_h: u32,
) -> Result<(), String> {
    if let Err(err) = std::fs::create_dir_all("spike-shots") {
        return Err(format!("cannot create spike-shots: {err}"));
    }
    for (name, ppp) in [("egui_100.png", 1.0f32), ("egui_150.png", 1.5f32)] {
        shot(device, queue, &vp_rgba, vp_w, vp_h, name, ppp)
            .map_err(|err| format!("{name}: {err}"))?;
        println!("egui spike: wrote spike-shots/{name}");
    }
    Ok(())
}

/// One offscreen frame of the shell at `ppp`, saved as a PNG. The document
/// is re-seeded per shot (fresh `State`, same `world::seed` the live shell
/// and the headless gate run): the dump must not mutate - or be mutated
/// by - the window the user is looking at.
fn shot(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vp_rgba: &[u8],
    vp_w: u32,
    vp_h: u32,
    name: &str,
    ppp: f32,
) -> Result<(), String> {
    let points = egui::Vec2::new(1280.0, 800.0);
    let size_px = [
        (points.x * ppp).max(1.0) as u32,
        (points.y * ppp).max(1.0) as u32,
    ];

    // A private Context per scale: Visuals, the font atlas and the texture
    // manager are all per-context, and the Renderer's texture table must
    // match the context that produced the deltas - so a private Renderer
    // too. The live window is untouched by any of this.
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut renderer = Renderer::new(device, VIEW3D_FORMAT, RendererOptions::default());

    let image = egui::ColorImage::from_rgba_unmultiplied([vp_w as usize, vp_h as usize], vp_rgba);
    let tex = ctx.load_texture("shot-viewport", image, egui::TextureOptions::LINEAR);

    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, points)),
        ..Default::default()
    };
    // 0.36 derives ppp as zoom * native from viewport info - carry the
    // scale there, not the deleted RawInput field, so the deferred-zoom
    // pass (which rewrites screen_rect) never fires.
    raw.viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .native_pixels_per_point = Some(ppp);
    let mut dock = build_dock();
    let mut viewport_points = [700.0f32, 640.0];
    let mut state = seeded();
    let mut out = ctx.run_ui(raw, |ui| {
        shell_layout(ui, &mut state, &mut dock, tex.id(), &mut viewport_points);
    });
    drain_textures(&mut renderer, device, queue, &mut out);
    let clipped = ctx.tessellate(out.shapes, ppp);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("egui-spike-shot"),
        size: wgpu::Extent3d {
            width: size_px[0],
            height: size_px[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: VIEW3D_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let sd = ScreenDescriptor { size_in_pixels: size_px, pixels_per_point: ppp };
    let mut buffer_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui-spike-shot-buffers"),
    });
    let extra = renderer.update_buffers(device, queue, &mut buffer_encoder, &clipped, &sd);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui-spike-shot"),
    });
    // Same dance as the live pass in egui_spike.rs: Renderer::render wants
    // a RenderPass<'static>, so the descriptor's borrows (target view,
    // egui textures) are erased and the pass is dropped before the encoder
    // closes. Everything it encodes against outlives this block.
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut pass = encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui-spike-shot-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // a: 1 - a PNG of transparent black is not a screenshot.
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
        .forget_lifetime();
    renderer.render(&mut pass, &clipped, &sd);
    drop(pass);
    let mut submits = extra;
    submits.push(buffer_encoder.finish());
    submits.push(encoder.finish());
    queue.submit(submits);

    let row_bytes = (u64::from(size_px[0]) * 4).next_multiple_of(256);
    let buf_size = row_bytes * u64::from(size_px[1]);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("egui-spike-shot-readback"),
        size: buf_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui-spike-shot-copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(u32::try_from(row_bytes).unwrap_or(u32::MAX)),
                rows_per_image: Some(size_px[1]),
            },
        },
        wgpu::Extent3d {
            width: size_px[0],
            height: size_px[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let (tx, rx) = std::sync::mpsc::channel();
    buf.slice(..).map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    if device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
        return Err("device poll failed".to_string());
    }
    if !matches!(rx.recv(), Ok(Ok(()))) {
        return Err("buffer map failed".to_string());
    }
    let raw = buf
        .slice(..)
        .get_mapped_range()
        .map_err(|err| format!("mapped range: {err}"))?;
    let w4 = usize::try_from(size_px[0] * 4).map_err(|err| err.to_string())?;
    let mut rgba = Vec::with_capacity(usize::try_from(buf_size).unwrap_or(usize::MAX).min(w4 * size_px[1] as usize));
    for row in 0..size_px[1] {
        let off = usize::try_from(row_bytes * u64::from(row)).map_err(|err| err.to_string())?;
        let end = (off + w4).min(raw.len());
        if off >= raw.len() {
            return Err("readback buffer shorter than the texture rows".to_string());
        }
        rgba.extend_from_slice(&raw[off..end]);
        rgba.extend(std::iter::repeat_n(0u8, off + w4 - end));
    }
    drop(raw);

    let path = std::path::Path::new("spike-shots").join(name);
    image::save_buffer(&path, &rgba, size_px[0], size_px[1], image::ColorType::Rgba8)
        .map_err(|err| format!("{}: {err}", path.display()))
}

/// A throwaway document for one dump: `State::new` + the same seed the
/// live shell and the headless gate run, framed. Two extra worlds per
/// process (one per scale) is cheap - the seed is a handful of commands -
/// and it keeps the window's own State untouched by the proof pass.
fn seeded() -> State {
    let mut state = State::new();
    world::seed(&mut state);
    state.pending.push_back(CommandKind::FrameAll);
    state.drain_actions();
    state
}
