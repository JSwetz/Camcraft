//! Split from `types.rs` by M5.0.1; structure only.


use super::sel::WCS_SLOTS;
/// What a height is measured from. Every source but [`HeightReference::Fixed`]
/// is a world truth re-read at resolve time - the stock and model bounding
/// boxes and the work-offset slot placements - so a height that follows the
/// moved setup is expressed, never stale. Rotations are not part of a Z
/// reference: only the source's height feeds an offset (a full WCS frame
/// transform is out of 2D scope, like 3D itself).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeightReference {
    /// The offset is the Z itself, in world (model) coordinates. What
    /// pre-reference files and hand-typed absolute values carry.
    Fixed,
    TopOfStock,
    BottomOfStock,
    TopOfModel,
    BottomOfModel,
    /// The Z=0 plane of a work-offset slot: `WCS_SLOTS[i]`'s origin. An
    /// existing-but-unplaced slot reads as the world origin, exactly as
    /// the rest of the world treats an entity nobody has moved.
    WorkOffset(usize),
}

impl HeightReference {
    /// Panel combo entries, in this order; WorkOffset gets its slot name
    /// appended by the panel.
    pub const KIND_LABELS: [&'static str; 6] = [
        "Fixed value",
        "Top of stock",
        "Bottom of stock",
        "Top of model",
        "Bottom of model",
        "Work offset",
    ];

    /// Which entry of [`Self::KIND_LABELS`] this reference's kind is.
    #[must_use]
    pub fn kind_index(&self) -> usize {
        match self {
            HeightReference::Fixed => 0,
            HeightReference::TopOfStock => 1,
            HeightReference::BottomOfStock => 2,
            HeightReference::TopOfModel => 3,
            HeightReference::BottomOfModel => 4,
            HeightReference::WorkOffset(_) => 5,
        }
    }

    /// The slot index for WorkOffset references, 0 for everything else.
    #[must_use]
    pub fn wcs_slot(&self) -> usize {
        match self {
            HeightReference::WorkOffset(i) => *i,
            _ => 0,
        }
    }

    /// Replace the slot part, keeping the kind - only meaningful on a
    /// WorkOffset reference.
    #[must_use]
    pub fn with_wcs_slot(&self, i: usize) -> Self {
        match self {
            HeightReference::WorkOffset(_) => HeightReference::WorkOffset(i),
            other => *other,
        }
    }
}

impl core::fmt::Display for HeightReference {
    /// Chip text for a plane label: short, and the G-code word for a
    /// work offset so it names what the user selected in Setup.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HeightReference::Fixed => f.write_str("fixed"),
            HeightReference::TopOfStock => f.write_str("stock top"),
            HeightReference::BottomOfStock => f.write_str("stock bottom"),
            HeightReference::TopOfModel => f.write_str("model top"),
            HeightReference::BottomOfModel => f.write_str("model bottom"),
            HeightReference::WorkOffset(i) => match WCS_SLOTS.get(*i) {
                Some(name) => f.write_str(name),
                None => write!(f, "work offset #{i}"),
            },
        }
    }
}

/// One height: From-reference plus offset in mm (HSMWorks-style "From:
/// model top, Offset: 0.5"). The stored truth is the expression, never a
/// resolved Z - so a height cannot disagree with the setup it references.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Height {
    pub reference: HeightReference,
    pub offset: f64,
}

impl Height {
    /// A height that stands on its own: the value is the world Z.
    #[must_use]
    pub fn fixed(z: f64) -> Self {
        Height { reference: HeightReference::Fixed, offset: z }
    }

    #[must_use]
    pub fn of(reference: HeightReference, offset: f64) -> Self {
        Height { reference, offset }
    }

    /// Structural sanity, no world: a Fixed height must be finite, and
    /// any offset must be finite. Ordering rules run on resolved values
    /// ([`crate::operation::depth_sane`]).
    /// Structural sanity: a finite offset, and a WorkOffset reference
    /// naming a slot that exists. Ordering lives on resolved values
    /// ([`crate::operation::depth_sane`]).
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.offset.is_finite() && self.reference.wcs_slot() < WCS_SLOTS.len()
    }

    #[must_use]
    pub fn is_fixed(&self) -> bool {
        matches!(self.reference, HeightReference::Fixed)
    }
}
