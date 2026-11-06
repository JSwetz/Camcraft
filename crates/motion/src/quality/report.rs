//! The quality report's types (provenance in `quality/mod.rs`).

use crate::geometry::Point3;

/// What to measure with. Every field has a stated default
/// ([`QualitySpec::new`]); the thresholds are the report's law, the
/// cell is its resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QualitySpec {
    pub tool_diameter: f64,
    /// Grid cell of the measuring stock (mm).
    pub cell: f64,
    /// A tangent change larger than this between consecutive in-cut
    /// moves is a curvature break (degrees).
    pub break_deg: f64,
    /// A sample whose contact stands for more than this load (ae/D) is
    /// "in cut"; a zero-depth wall touch reads as none.
    pub in_cut_load: f64,
    /// Rapid traverse rate for the air-time estimate (mm/min).
    pub rapid_rate: f64,
    /// Columns standing below this height are the part's walls
    /// ([`super::stock_for`] puts them a millimeter under the floor):
    /// read as the material a strip on the wall continues into, never
    /// as stock. `None`: every column is stock or open.
    pub wall_below: Option<f64>,
}

impl QualitySpec {
    /// Defaults: a D/80 grid, 10-degree breaks, half a percent of D is
    /// cutting, a 10 m/min rapid.
    #[must_use]
    pub fn new(tool_diameter: f64) -> Self {
        QualitySpec {
            tool_diameter,
            cell: tool_diameter / 80.0,
            break_deg: 10.0,
            in_cut_load: 0.005,
            rapid_rate: 10_000.0,
            wall_below: None,
        }
    }
}

/// The measured verdicts of one path.
#[derive(Clone, Debug, PartialEq)]
pub struct PathQuality {
    /// Largest contact angle on a planar cut (degrees) and where.
    pub max_deg: f64,
    pub max_at: Point3,
    /// Largest contact angle during a descent or inside a named entry
    /// bore (plunge, ramp, helix and the bore's own orbit).
    pub entry_max_deg: f64,
    /// Planar cut samples in 10-degree bins; the last bin is 180+.
    pub hist: [u32; 19],
    /// Planar feed length engaged at 170 degrees or more (slotting).
    pub slot_len: f64,
    /// Curvature breaks in cut, the worst one's angle and place, and
    /// every one of them.
    pub breaks: usize,
    pub worst_break_deg: f64,
    pub worst_break_at: Point3,
    pub break_list: Vec<(Point3, f64)>,
    /// Tightest in-cut curvature (1/mm).
    pub max_curvature: f64,
    /// Planar feed length in material / in air, rapid length (mm).
    pub cut_len: f64,
    pub air_len: f64,
    pub rapid_len: f64,
    /// Air feed + rapid time (s) at the path's own feeds.
    pub air_time_s: f64,
    /// Rapid climbs (lifts).
    pub retracts: usize,
    /// Descents (plunge, ramp, helix runs) that bit stock: one per cut
    /// level is the entry law - a second is a retract-and-replunge.
    pub descents: usize,
    /// Per move (index-aligned with the path): its largest contact
    /// angle (degrees; 0 for rapids) - the plot the report colors by.
    pub move_max: Vec<f64>,
    /// Per move: its planar feed length in air (mm) - where the report's
    /// air total comes from.
    pub move_air: Vec<f64>,
    /// Every air sample's position (the plot of where the air is).
    pub air_at: Vec<Point3>,
    /// The rim the angles were read on and the tool they were read for.
    pub rim: f64,
    pub diameter: f64,
}

impl Default for PathQuality {
    fn default() -> Self {
        PathQuality {
            max_deg: 0.0,
            max_at: Point3::ZERO,
            entry_max_deg: 0.0,
            hist: [0; 19],
            slot_len: 0.0,
            breaks: 0,
            worst_break_deg: 0.0,
            worst_break_at: Point3::ZERO,
            break_list: Vec::new(),
            max_curvature: 0.0,
            cut_len: 0.0,
            air_len: 0.0,
            rapid_len: 0.0,
            air_time_s: 0.0,
            retracts: 0,
            descents: 0,
            move_max: Vec::new(),
            move_air: Vec::new(),
            air_at: Vec::new(),
            rim: 0.0,
            diameter: 0.0,
        }
    }
}

impl PathQuality {
    /// The worst contact angle as the straight-wall load (ae/D) it
    /// stands for - the quantity the optimal load names: the wall the
    /// reading rim meets at that angle (`crate::load::load_at_rim`).
    #[must_use]
    pub fn max_load(&self) -> f64 {
        if self.diameter > 0.0 {
            crate::load::load_at_rim(self.max_deg, self.diameter, self.rim)
        } else {
            0.0
        }
    }

    /// The `q`-quantile of the planar contact angle, read off the bins
    /// (upper bin edge, degrees).
    #[must_use]
    pub fn quantile_deg(&self, q: f64) -> f64 {
        let total: u64 = self.hist.iter().map(|&n| u64::from(n)).sum();
        if total == 0 {
            return 0.0;
        }
        let want = (q.clamp(0.0, 1.0) * total as f64).ceil() as u64;
        let mut acc = 0u64;
        for (i, &n) in self.hist.iter().enumerate() {
            acc += u64::from(n);
            if acc >= want {
                return ((i + 1) * 10) as f64;
            }
        }
        190.0
    }

    /// One report line, the heavy lane's table row.
    #[must_use]
    pub fn report_line(&self, name: &str) -> String {
        format!(
            "{name:<24} max {:5.1} deg = load {:5.1}% D at ({:.2},{:.2}) p50 {:3.0} p95 {:3.0} | slot {:5.1} mm \
             | breaks {:3} (worst {:5.1} deg) kmax {:5.3}/mm | cut {:7.1} air {:7.1} rapid {:6.1} mm, air {:5.1} s \
             | retracts {} descents {}",
            self.max_deg,
            self.max_load() * 100.0,
            self.max_at.x,
            self.max_at.y,
            self.quantile_deg(0.5),
            self.quantile_deg(0.95),
            self.slot_len,
            self.breaks,
            self.worst_break_deg,
            self.max_curvature,
            self.cut_len,
            self.air_len,
            self.rapid_len,
            self.air_time_s,
            self.retracts,
            self.descents,
        )
    }
}
