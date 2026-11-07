// Windowed GPU bring-up (#74): enumerate every adapter, log each, and try
// them adapter-by-adapter (gpu_select::order) inside the backend fallback
// chain (gpu::attempts) until one gives a device AND a configured surface.
// Provenance: our own code, specced from the wgpu 30 public API docs
// (Instance::enumerate_adapters, RequestAdapterOptions::power_preference,
// Adapter::request_device, Surface::get_capabilities/configure,
// Device::push_error_scope) - no vendor reference consulted.

use std::collections::HashSet;
use std::sync::Arc;

use egui_wgpu::wgpu;
use winit::window::Window;

use super::gpu;
use super::gpu_select::{self, AdapterDesc};

/// What a successful bring-up hands the shell.
pub struct Bring {
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

/// Bring the window's GPU up, or refuse naming every adapter tried and
/// why each failed. `want` is the `--gpu` / `CAMCRAFT_GPU` substring.
pub fn bring_up(window: &Arc<Window>, pick: &gpu::GpuPick, want: Option<&str>) -> Result<Bring, String> {
    let size = window.inner_size();
    let mut seen: HashSet<(String, u32, u32, &'static str)> = HashSet::new();
    let mut tried: HashSet<(String, u32, u32, &'static str)> = HashSet::new();
    let mut failures: Vec<String> = Vec::new();
    for (label, backends) in gpu::attempts(pick) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        // SAFETY: the shell keeps `window` alive in its own field and drops
        // the surface first (its field-order note); the handle stays valid.
        let target = unsafe { wgpu::SurfaceTargetUnsafe::from_display_and_window(window, window) }
            .map_err(|e| format!("surface handle: {e}"))?;
        // SAFETY: same argument - the window outlives the surface.
        let surface = match unsafe { instance.create_surface_unsafe(target) } {
            Ok(s) => s,
            Err(err) => {
                eprintln!("gpu: {label}: surface creation failed ({err}) - falling back");
                failures.push(format!("{label}: surface creation failed ({err})"));
                continue;
            }
        };
        let adapters = pollster::block_on(instance.enumerate_adapters(backends));
        let descs: Vec<AdapterDesc> = adapters
            .iter()
            .map(|a| AdapterDesc::from_info(&a.get_info(), a.is_surface_supported(&surface)))
            .collect();
        for d in &descs {
            if seen.insert(d.key()) {
                println!("{}", d.log_line());
            }
        }
        if descs.is_empty() {
            eprintln!("gpu: {label}: no adapters enumerated - falling back");
        }
        let hp = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .ok()
        .map(|a| AdapterDesc::from_info(&a.get_info(), true));
        let (order, notes) = gpu_select::order(&descs, want, hp.as_ref());
        for n in notes {
            println!("gpu: {label}: {n}");
        }
        for i in order {
            let (Some(adapter), Some(desc)) = (adapters.get(i), descs.get(i)) else { continue };
            if !tried.insert(desc.key()) {
                continue; // already failed under an earlier mask
            }
            let who = format!("\"{}\" via {}", desc.name, gpu::backend_name(desc.backend));
            match try_adapter(&surface, adapter, size.width, size.height) {
                Ok((device, queue, config)) => {
                    println!(
                        "gpu: using {who} ({}; {:?} {:?} {:?})",
                        gpu_select::type_name(desc.device_type),
                        config.format,
                        config.present_mode,
                        config.alpha_mode
                    );
                    return Ok(Bring { surface, adapter: adapter.clone(), device, queue, config });
                }
                Err(why) => {
                    eprintln!("gpu: {who} failed: {why} - trying the next adapter");
                    failures.push(format!("{who}: {why}"));
                }
            }
        }
    }
    if failures.is_empty() {
        failures.push("no GPU adapter was found on any backend".into());
    }
    Err(format!("no GPU adapter could render to this window; tried: {}", failures.join("; ")))
}

/// One candidate: device, capability-driven config, and a configure that
/// is checked (validation scope + unwind guard) instead of assumed.
fn try_adapter(
    surface: &wgpu::Surface<'static>,
    adapter: &wgpu::Adapter,
    w: u32,
    h: u32,
) -> Result<(wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration), String> {
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|e| format!("device request refused ({e})"))?;
    let config = gpu_select::surface_config(&surface.get_capabilities(adapter), w, h)?;
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let configured = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| surface.configure(&device, &config)));
    let err = pollster::block_on(scope.pop());
    if configured.is_err() {
        return Err(format!("surface configure aborted ({:?}, {:?})", config.format, config.present_mode));
    }
    if let Some(e) = err {
        return Err(format!("surface configure refused ({e})"));
    }
    Ok((device, queue, config))
}
