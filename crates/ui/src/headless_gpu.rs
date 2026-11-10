//! Windowless wgpu device for the headless run and the GPU test suites:
//! the software adapter first (deterministic on CI: lavapipe / WARP),
//! then the platform's hardware adapter (Metal has no software fallback,
//! so on macOS this is the only way to render at all - #615).
//! Provenance: our own adapter policy over wgpu's public
//! `RequestAdapterOptions` API.

use egui_wgpu::wgpu;

/// Which adapter the device came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    /// wgpu's fallback (software rasterizer) adapter.
    Software,
    /// The platform default hardware adapter.
    Hardware,
}

impl AdapterKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Software => "software",
            Self::Hardware => "hardware",
        }
    }
}

/// A device with no surface, and where it came from.
pub struct HeadlessGpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub kind: AdapterKind,
    /// `"<adapter name> via <backend>"`, for logs.
    pub name: String,
}

/// The env var that turns a test's "no adapter" skip into a failure.
pub const REQUIRE_GPU_ENV: &str = "CAMCRAFT_REQUIRE_GPU";

fn try_kind(instance: &wgpu::Instance, kind: AdapterKind) -> Result<HeadlessGpu, String> {
    let options = wgpu::RequestAdapterOptions {
        force_fallback_adapter: kind == AdapterKind::Software,
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        ..Default::default()
    };
    let adapter = pollster::block_on(instance.request_adapter(&options))
        .map_err(|err| format!("{} adapter: {err}", kind.label()))?;
    let info = adapter.get_info();
    let name = format!("\"{}\" via {:?}", info.name, info.backend);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|err| format!("{} device {name}: {err}", kind.label()))?;
    Ok(HeadlessGpu { device, queue, kind, name })
}

/// Software adapter first, then hardware. The error names why each failed.
pub fn device() -> Result<HeadlessGpu, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut why = Vec::new();
    for kind in [AdapterKind::Software, AdapterKind::Hardware] {
        match try_kind(&instance, kind) {
            Ok(gpu) => return Ok(gpu),
            Err(err) => why.push(err),
        }
    }
    Err(format!("no graphics adapter: {}", why.join("; ")))
}

/// For GPU tests: the device, or `Ok(None)` after printing a loud SKIP
/// line to stderr when the machine has no adapter at all. With
/// `CAMCRAFT_REQUIRE_GPU=1` a missing adapter is `Err` instead, so a lane
/// that must render can't pass by skipping.
pub fn device_for_test(suite: &str) -> Result<Option<HeadlessGpu>, String> {
    match device() {
        Ok(gpu) => {
            eprintln!("{suite}: rendering on the {} adapter {}", gpu.kind.label(), gpu.name);
            Ok(Some(gpu))
        }
        Err(err) => {
            let required = std::env::var(REQUIRE_GPU_ENV).is_ok_and(|v| v.trim() == "1");
            if required {
                Err(format!("{suite}: {REQUIRE_GPU_ENV}=1 but {err}"))
            } else {
                eprintln!("*** {suite}: SKIPPED (no adapter) - NOT a pass: {err} ***");
                Ok(None)
            }
        }
    }
}
