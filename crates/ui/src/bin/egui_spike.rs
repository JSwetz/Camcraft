//! M1.4.1 spike binary (branch ui-egui-spike only; see ROADMAP.md).
//!
//! S0 stub: proves the egui stack links and its types unify with the
//! existing wgpu/winit pins. No window, no world - the winit loop and the
//! docked View3D texture land in S1, replacing this main entirely.
//! Provenance: our own spike harness, specced from the egui docs
//! (egui-rs.github.io); no HSMWorks material involved.

fn main() {
    // Pure-CPU construction only: exercises egui + egui_dock type-checked
    // against the pinned egui, without touching a GPU or a window.
    let _ctx = egui::Context::default();
    let _dock = egui_dock::DockState::new(vec!["viewport"]);
    println!(
        "hsmworks-ui-egui spike S0: egui stack + dock state construct cleanly \
         (wgpu 30 / winit 0.30 resolution proved by `cargo tree -d` \
         and the compile-time test in this file)"
    );
}

#[cfg(test)]
mod resolution {
    /// Compile-time proof of the S0 gate: exactly ONE wgpu in the tree.
    /// If egui-wgpu resolved a different wgpu major than the imgui
    /// renderer's re-export, the reference below would not typecheck and
    /// the spike's registered-texture plan would be dead on arrival.
    #[test]
    fn wgpu_types_are_unified() {
        // Elided lifetimes bind output to the single input lifetime, so the
        // coercion is only accepted when both paths name the SAME type.
        fn coerce(d: &dear_imgui_wgpu::wgpu::Device) -> &egui_wgpu::wgpu::Device {
            d
        }
        // The coercion itself is the assertion; the pointer bind is the use.
        let _f: fn(&dear_imgui_wgpu::wgpu::Device) -> &egui_wgpu::wgpu::Device = coerce;
    }
}
