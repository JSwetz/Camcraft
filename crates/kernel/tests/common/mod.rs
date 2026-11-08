//! Shared verdict helpers of the kernel benches: a closed-form answer and the
//! check that locates a measured solid's departure from it to one engine.
//! Provenance: our own bench design (M12.1 retirement gate).

use camcraft_kernel::{Measure, P3};

/// Relative volume floor every engine is held to (solidcore integrates at
/// 1e-7 relative; the floor leaves one decade for the booleans' own error).
pub const REL_FLOOR: f64 = 1e-6;

/// The closed-form answer for one fixture.
pub struct Want {
    pub name: &'static str,
    pub volume: f64,
    pub bbox: (P3, P3),
}

impl Want {
    pub fn measure(&self) -> Measure {
        Measure {
            volume: self.volume,
            min: self.bbox.0,
            max: self.bbox.1,
        }
    }
}

/// One engine's verdict on one fixture, `Err` = a located finding.
pub fn verdict(
    engine: &str,
    m: &Measure,
    w: &Want,
    vol_tol: f64,
    box_tol: f64,
) -> Result<(), String> {
    let wm = w.measure();
    let dv = (m.volume - wm.volume).abs();
    let db = m.bbox_deviation(&wm);
    println!(
        "{:<26} {:<9} V={:.9} (analytic {:.9}, |dV|={:.3e} tol {:.3e}) bbox dev {:.3e} tol {:.3e}",
        w.name, engine, m.volume, wm.volume, dv, vol_tol, db, box_tol
    );
    if dv > vol_tol || db > box_tol {
        Err(format!(
            "{}: {engine} left the closed form (|dV|={dv:.3e} > {vol_tol:.3e} or bbox {db:.3e} > {box_tol:.3e}); measured {m:?}",
            w.name
        ))
    } else {
        Ok(())
    }
}
