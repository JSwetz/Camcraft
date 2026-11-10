//! The axes-in-the-plane pick draft (#557): while a Setups page row
//! collects picks for +Y or +X, a viewport Edge or Vertex click lands
//! here instead of in the open operation's geometry. Each axis takes one
//! straight edge (Edge mode) or two points (Vertex mode); a face sets no
//! direction and is refused by name in the status line. Ephemeral view
//! state like the lay-on-a-plane draft - nothing here writes the world;
//! Apply queues the undoable `AlignPartToSetup` through
//! `CommandKind::TurnPartInPlane`.
//!
//! Provenance: our own UI over plain fixture practice (indicate an edge
//! along the table's travel); no vendor material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{AxisPick, InPlaneAxes, InPlaneAxis};

use crate::state::{GeoPick, State};

/// What one axis is picked by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AxisBy {
    /// One straight edge, start to end.
    #[default]
    Edge,
    /// Two points (feature corners), first to second.
    Points,
}

impl AxisBy {
    /// What a click takes in this mode, in plain words.
    #[must_use]
    pub fn noun(self) -> &'static str {
        match self {
            AxisBy::Edge => "a straight edge",
            AxisBy::Points => "two points",
        }
    }
}

/// One setup's in-plane axis picks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisDraft {
    /// The setup the picks will turn.
    pub setup: String,
    /// The axis viewport clicks land in; `None` is not collecting.
    pub picking: Option<InPlaneAxis>,
    pub y_by: AxisBy,
    pub x_by: AxisBy,
    /// The picks and flips, per axis (an empty pick list is "not given").
    pub axes: InPlaneAxes,
}

impl AxisDraft {
    fn new(setup: &str) -> Self {
        AxisDraft {
            setup: setup.to_owned(),
            axes: InPlaneAxes { y: Some(AxisPick::default()), x: Some(AxisPick::default()), primary: InPlaneAxis::Y },
            ..AxisDraft::default()
        }
    }

    #[must_use]
    pub fn by(&self, axis: InPlaneAxis) -> AxisBy {
        match axis {
            InPlaneAxis::Y => self.y_by,
            InPlaneAxis::X => self.x_by,
        }
    }

    /// One axis's picks and flip (always present in a live draft).
    pub fn slot(&mut self, axis: InPlaneAxis) -> &mut AxisPick {
        let s = match axis {
            InPlaneAxis::Y => &mut self.axes.y,
            InPlaneAxis::X => &mut self.axes.x,
        };
        s.get_or_insert_with(AxisPick::default)
    }

    /// What Apply sends: an axis with no picks is not given.
    #[must_use]
    pub fn wanted(&self) -> InPlaneAxes {
        let given = |p: &Option<AxisPick>| p.clone().filter(|p| !p.picks.is_empty());
        InPlaneAxes { y: given(&self.axes.y), x: given(&self.axes.x), primary: self.axes.primary }
    }
}

/// The draft of `setup`, created when there is none (or it is another
/// setup's).
pub fn draft_for<'a>(state: &'a mut State, setup: &str) -> &'a mut AxisDraft {
    if state.axis_pick.as_ref().is_none_or(|d| d.setup != setup) {
        state.axis_pick = Some(AxisDraft::new(setup));
    }
    state.axis_pick.get_or_insert_with(|| AxisDraft::new(setup))
}

fn mode_of(by: AxisBy) -> GeoPick {
    match by {
        AxisBy::Edge => GeoPick::Edge,
        AxisBy::Points => GeoPick::Vertex,
    }
}

/// Start collecting picks for `axis` of `setup`: Edge mode for an edge,
/// Vertex mode for two points. The lay-on-a-plane draft stops.
pub fn arm(state: &mut State, setup: &str, axis: InPlaneAxis) {
    state.plane_pick = None;
    state.points_pick = None; // #555: one draft collects at a time
    let d = draft_for(state, setup);
    d.picking = Some(axis);
    let by = d.by(axis);
    state.geo_pick = mode_of(by);
    state.status = format!("{setup}: pick {} for {}", by.noun(), axis.label());
}

/// Stop collecting; the picks stay for Apply.
pub fn disarm(state: &mut State) {
    if let Some(d) = state.axis_pick.as_mut() {
        d.picking = None;
    }
}

/// Is a Setups page row collecting axis picks?
#[must_use]
pub fn collecting(state: &State) -> bool {
    state.axis_pick.as_ref().is_some_and(|d| d.picking.is_some())
}

/// Switch what `axis` is picked by; its picks are dropped (an edge and
/// two points do not mix), and a live pick follows the new mode.
pub fn set_by(state: &mut State, setup: &str, axis: InPlaneAxis, by: AxisBy) {
    let d = draft_for(state, setup);
    match axis {
        InPlaneAxis::Y => d.y_by = by,
        InPlaneAxis::X => d.x_by = by,
    }
    d.slot(axis).picks.clear();
    if d.picking == Some(axis) {
        state.geo_pick = mode_of(by);
    }
}

/// One viewport pick into the axis being collected. An edge replaces
/// the axis's edge; a point joins, the last two kept. A pick of the
/// wrong kind is refused by name and changes nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(d) = state.axis_pick.as_mut() else { return };
    let Some(axis) = d.picking else { return };
    let by = d.by(axis);
    let setup = d.setup.clone();
    let words = format!("{} of '{}'", pick.feature, pick.entity);
    let refuse = |what: &str| format!("{setup}: {words} is {what} - {} takes {}", axis.label(), by.noun());
    let status = match (by, pick.feature) {
        (AxisBy::Edge, FeatureSel::Chain(_)) => {
            d.slot(axis).picks = vec![pick];
            format!("{setup}: edge picked for {}", axis.label())
        }
        (AxisBy::Points, FeatureSel::Vertex(_)) => {
            let picks = &mut d.slot(axis).picks;
            picks.push(pick);
            let extra = picks.len().saturating_sub(2);
            picks.drain(..extra);
            format!("{setup}: {} point{} for {}", picks.len(), if picks.len() == 1 { "" } else { "s" }, axis.label())
        }
        (_, FeatureSel::Face(_) | FeatureSel::Patch(_)) => refuse("a face (a face sets no direction)"),
        (AxisBy::Edge, FeatureSel::Vertex(_)) => refuse("a point"),
        (AxisBy::Points, FeatureSel::Chain(_)) => refuse("an edge"),
    };
    state.status = status;
}
