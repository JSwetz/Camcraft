//! Cutting tools: the parameters that define a cutter and the starter
//! catalog the UI lists.
//!
//! Provenance: end mill / ball nose / bull nose geometry and the
//! diameter-flute-shank-holder stack are standard machine-tool handbook
//! descriptions. The catalog's sizes are ordinary metric stock; the
//! speeds and feeds are our own numbers, set for aluminum with carbide
//! tooling at typical chip loads - general published practice, no vendor
//! library copied.
//!
//! Pure module: no flecs, no I/O. Commands persist a `Tool` on a `ToolDef`
//! component; the operation layer reads radii and lengths out of it for
//! generation and the shank-reach law in `verify`.

use crate::verify::ToolGeom;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    EndMill,
    BallNose,
    BullNose,
}

/// One rotating cutter, coaxial with the spindle: a cutting body of
/// `diameter`/`flute_len` (corner rounded by `corner_r`), a plain shank,
/// and whatever holder the setup assumes above it. Radii live as
/// diameters here; `verify::ToolGeom` takes the halved coaxial stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub kind: ToolKind,
    pub diameter: f64,
    /// Corner radius: 0 for a square end mill, d/2 for a ball, between
    /// for a bull.
    pub corner_r: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    pub holder_d: f64,
    pub holder_len: f64,
    /// Free-text material the numbers were picked for ("aluminum").
    pub material_group: String,
    /// Suggested starting spindle speed (rpm) and feed (mm/min) for that
    /// material - the machine operator's starting point, not a limit.
    pub rpm: f64,
    pub feed: f64,
}

impl Tool {
    /// Finite, positive where it must be, and geometrically consistent:
    /// a corner radius cannot exceed half the diameter, lengths cannot be
    /// negative. The gate every tool command runs before writing.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let fin = [
            self.diameter,
            self.corner_r,
            self.flute_len,
            self.shank_d,
            self.shank_len,
            self.holder_d,
            self.holder_len,
            self.rpm,
            self.feed,
        ]
        .into_iter()
        .all(|v| v.is_finite());
        fin && self.diameter > 0.0
            && self.diameter <= 500.0
            && self.corner_r >= 0.0
            && self.corner_r <= self.diameter * 0.5
            && self.flute_len > 0.0
            && self.shank_d >= 0.0
            && self.shank_len >= 0.0
            && self.holder_d >= 0.0
            && self.holder_len >= 0.0
            && self.rpm > 0.0
            && self.feed > 0.0
    }

    /// The same tool as the verifier sees it: coaxial radii upward from
    /// the cutting edge.
    #[must_use]
    pub fn geom(&self) -> ToolGeom {
        ToolGeom {
            radius: self.diameter * 0.5,
            flute_len: self.flute_len,
            shank_radius: self.shank_d * 0.5,
            shank_len: self.shank_len,
            holder_radius: self.holder_d * 0.5,
            holder_len: self.holder_len,
        }
    }
}

/// The starter catalog: name, then tool. Ordinary metric sizes in a
/// 2-flute/4-flute ladder; shanks step up to the next common gauge so
/// the shank-reach law has something wider than the flute to collide
/// with. Display order - the tool library pane lists these top to bottom.
#[must_use]
pub fn catalog() -> Vec<(&'static str, Tool)> {
    let em = |diameter: f64, corner_r: f64, kind: ToolKind, flute_len: f64, shank_d: f64, rpm: f64, feed: f64| Tool {
        kind,
        diameter,
        corner_r,
        flute_len,
        shank_d,
        shank_len: 40.0,
        holder_d: 44.0,
        holder_len: 35.0,
        material_group: "aluminum".into(),
        rpm,
        feed,
    };
    vec![
        ("EM-0200-2FL", em(2.0, 0.0, ToolKind::EndMill, 6.0, 4.0, 18000.0, 600.0)),
        ("EM-0300-2FL", em(3.0, 0.0, ToolKind::EndMill, 9.0, 4.0, 15000.0, 900.0)),
        ("EM-0400-2FL", em(4.0, 0.0, ToolKind::EndMill, 12.0, 6.0, 12000.0, 1200.0)),
        ("EM-0600-4FL", em(6.0, 0.0, ToolKind::EndMill, 15.0, 8.0, 9500.0, 1800.0)),
        ("EM-0800-4FL", em(8.0, 0.0, ToolKind::EndMill, 20.0, 10.0, 7200.0, 2400.0)),
        ("EM-1000-4FL", em(10.0, 0.0, ToolKind::EndMill, 22.0, 10.0, 5800.0, 3000.0)),
        ("EM-1200-2FL", em(12.0, 0.0, ToolKind::EndMill, 25.0, 12.0, 4800.0, 3600.0)),
        ("BN-0600", em(6.0, 3.0, ToolKind::BallNose, 15.0, 8.0, 9000.0, 1500.0)),
        ("BN-1000", em(10.0, 5.0, ToolKind::BallNose, 22.0, 10.0, 6000.0, 2000.0)),
        ("BB-1000-R1", em(10.0, 1.0, ToolKind::BullNose, 20.0, 10.0, 6500.0, 2200.0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sane_and_geom_matches() {
        for (name, t) in catalog() {
            assert!(t.is_sane(), "{name}");
            let g = t.geom();
            assert_eq!(g.radius, t.diameter * 0.5);
            assert_eq!(g.shank_radius, t.shank_d * 0.5);
        }
    }

    #[test]
    fn degenerate_tools_are_refused() {
        let base = catalog().remove(0).1;
        assert!(!Tool { diameter: 0.0, ..base.clone() }.is_sane());
        assert!(!Tool { diameter: f64::NAN, ..base.clone() }.is_sane());
        assert!(!Tool { corner_r: base.diameter, ..base.clone() }.is_sane());
        assert!(!Tool { flute_len: -1.0, ..base.clone() }.is_sane());
        assert!(!Tool { rpm: 0.0, ..base }.is_sane());
    }
}
