//! Colour-vision-deficiency arithmetic (M13.4): how a dichromat sees an
//! sRGB colour, and how far apart two colours look. Pure maths, no
//! chrome - `colorvision`'s gate is the consumer.
//! Provenance: the simulation matrices are the severity-1.0 rows of
//! Machado, Oliveira & Fernandes, "A Physiologically-based Model for
//! Simulation of Color Vision Deficiency", IEEE TVCG 15(6), 2009,
//! applied in linear light; the colour difference is CIEDE2000 (CIE
//! 142-2001) over CIELAB D65, written from Sharma, Wu & Dalal, "The
//! CIEDE2000 Color-Difference Formula: Implementation Notes...", Color
//! Res. Appl. 30(1), 2005 (whose test pairs pin it below); sRGB per
//! IEC 61966-2-1. Our own code.

use camcraft_app::schema::ColorVision;

/// Machado 2009, severity 1.0, linear-RGB rows: protanopia.
const PROTAN: [[f64; 3]; 3] = [
    [0.152_286, 1.052_583, -0.204_868],
    [0.114_503, 0.786_281, 0.099_216],
    [-0.003_882, -0.048_116, 1.051_998],
];
/// Machado 2009, severity 1.0: deuteranopia.
const DEUTAN: [[f64; 3]; 3] = [
    [0.367_322, 0.860_646, -0.227_968],
    [0.280_085, 0.672_501, 0.047_413],
    [-0.011_820, 0.042_940, 0.968_881],
];
/// Machado 2009, severity 1.0: tritanopia.
const TRITAN: [[f64; 3]; 3] = [
    [1.255_528, -0.076_749, -0.178_779],
    [-0.078_411, 0.930_809, 0.147_602],
    [0.004_733, 0.691_367, 0.303_900],
];

/// sRGB (0..1, gamma-encoded) -> linear light. Non-finite input reads
/// as black: the gate must never be fed a NaN it then calls "distinct".
fn lin(s: f64) -> f64 {
    let s = if s.is_finite() { s.clamp(0.0, 1.0) } else { 0.0 };
    if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

/// linear light -> sRGB, clamped to the gamut (a simulated colour can
/// leave it by a hair; the screen cannot).
fn gamma(v: f64) -> f64 {
    let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// The colour as the given eye sees it, back in sRGB. `Off` is the
/// identity (normal trichromacy) on the gamut; every mode returns
/// finite channels in 0..1 whatever it was fed.
#[must_use]
pub fn simulate(c: [f64; 3], mode: ColorVision) -> [f64; 3] {
    let clean = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let m = match mode {
        ColorVision::Off => return [clean(c[0]), clean(c[1]), clean(c[2])],
        ColorVision::Protan => &PROTAN,
        ColorVision::Deutan => &DEUTAN,
        ColorVision::Tritan => &TRITAN,
    };
    let l = [lin(c[0]), lin(c[1]), lin(c[2])];
    let row = |r: &[f64; 3]| gamma(r[0] * l[0] + r[1] * l[1] + r[2] * l[2]);
    [row(&m[0]), row(&m[1]), row(&m[2])]
}

/// sRGB -> CIELAB (D65 white, the sRGB matrix of IEC 61966-2-1).
#[must_use]
pub fn lab(c: [f64; 3]) -> [f64; 3] {
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b;
    let z = 0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b;
    let d = 6.0 / 29.0;
    let f = |t: f64| if t > d * d * d { t.cbrt() } else { t / (3.0 * d * d) + 4.0 / 29.0 };
    let (fx, fy, fz) = (f(x / 0.950_47), f(y), f(z / 1.088_83));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 between two CIELAB colours (kL = kC = kH = 1).
#[must_use]
pub fn de2000(p: [f64; 3], q: [f64; 3]) -> f64 {
    let [l1, a1, b1] = p;
    let [l2, a2, b2] = q;
    let c7 = |c: f64| {
        let c7 = c.powi(7);
        (c7 / (c7 + 25f64.powi(7))).sqrt()
    };
    let cbar = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
    let g = 0.5 * (1.0 - c7(cbar));
    let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1p, c2p) = (a1p.hypot(b1), a2p.hypot(b2));
    let hue = |b: f64, a: f64| if b == 0.0 && a == 0.0 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
    let (h1p, h2p) = (hue(b1, a1p), hue(b2, a2p));
    let chroma0 = c1p * c2p == 0.0;
    let dlp = l2 - l1;
    let dcp = c2p - c1p;
    let dhp = if chroma0 {
        0.0
    } else {
        let d = h2p - h1p;
        if d > 180.0 { d - 360.0 } else if d < -180.0 { d + 360.0 } else { d }
    };
    let d_hp = 2.0 * (c1p * c2p).sqrt() * (dhp.to_radians() / 2.0).sin();
    let lbar = (l1 + l2) / 2.0;
    let cbarp = (c1p + c2p) / 2.0;
    let hbarp = if chroma0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };
    let cos = |deg: f64| deg.to_radians().cos();
    let t = 1.0 - 0.17 * cos(hbarp - 30.0) + 0.24 * cos(2.0 * hbarp) + 0.32 * cos(3.0 * hbarp + 6.0)
        - 0.20 * cos(4.0 * hbarp - 63.0);
    let dtheta = 30.0 * (-((hbarp - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * c7(cbarp);
    let l50 = (lbar - 50.0).powi(2);
    let sl = 1.0 + 0.015 * l50 / (20.0 + l50).sqrt();
    let sc = 1.0 + 0.045 * cbarp;
    let sh = 1.0 + 0.015 * cbarp * t;
    let rt = -(2.0 * dtheta).to_radians().sin() * rc;
    let (x, y, z) = (dlp / sl, dcp / sc, d_hp / sh);
    (x * x + y * y + z * z + rt * y * z).max(0.0).sqrt()
}

/// How far apart two sRGB colours look to the given eye: CIEDE2000 of
/// their simulated appearances.
#[must_use]
pub fn seen_apart(a: [f64; 3], b: [f64; 3], mode: ColorVision) -> f64 {
    de2000(lab(simulate(a, mode)), lab(simulate(b, mode)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sharma, Wu & Dalal 2005, Table 1: pairs 1, 7, 17 and 25 pin the
    /// hue-wrap, the neutral-axis and the large-difference branches.
    #[test]
    fn ciede2000_matches_the_published_test_pairs() {
        for (p, q, want) in [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, 2.5, 0.0], [73.0, 25.0, -18.0], 27.1492),
            ([60.2574, -34.0099, 36.2677], [60.4626, -34.1751, 39.4387], 1.2644),
        ] {
            let got = de2000(p, q);
            assert!((got - want).abs() < 1e-4, "{p:?} {q:?}: {got} != {want}");
            assert!((de2000(q, p) - want).abs() < 1e-4, "symmetric");
        }
    }

    /// The simulation fixes the neutrals (Machado's rows each sum to
    /// ~1, so white stays white and grey stays grey) and Off is the
    /// identity; non-finite input never escapes as NaN.
    #[test]
    fn simulation_keeps_neutrals_and_refuses_nan() {
        for mode in ColorVision::ALL {
            for g in [0.0, 0.5, 1.0] {
                let s = simulate([g, g, g], mode);
                assert!(s.iter().all(|v| (v - g).abs() < 2e-3), "{mode:?} grey {g}: {s:?}");
            }
            let s = simulate([f64::NAN, 0.5, f64::INFINITY], mode);
            assert!(s.iter().all(|v| v.is_finite()), "{mode:?}: {s:?}");
        }
        assert_eq!(simulate([0.9, 0.2, 0.1], ColorVision::Off), [0.9, 0.2, 0.1]);
        // The textbook confusion: pure red and a matched green collapse
        // for a deutan far more than for the normal eye.
        let (r, g) = ([0.8, 0.3, 0.1], [0.45, 0.45, 0.1]);
        assert!(seen_apart(r, g, ColorVision::Deutan) < seen_apart(r, g, ColorVision::Off) / 2.0);
    }
}
