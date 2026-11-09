//! The locate-in-fixture pick draft (#561): while a Fixtures page row or
//! the Model page collects mates, a viewport Face or Vertex click lands
//! here instead of in the open operation's geometry - first the part's
//! face or point, then the fixture's face or point it sits on, one mate
//! per pair, up to three (floor, jaw, stop). Ephemeral view state like
//! the lay-on-a-plane draft (#330) - nothing here writes the world;
//! Apply queues the undoable `Locate` through `CommandKind::Locate`.
//!
//! Provenance: our own UI over 3-2-1 fixturing practice; no vendor
//! material consulted.

use camcraft_app::fixture_locate::{LocateOffset, LocateSpec, MAX_MATES, Mate, Mover};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{GeoPick, State};

/// The mates being collected for one fixture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocateDraft {
    pub fixture: String,
    /// The part; empty until the first part pick names it (or the page
    /// that armed the draft did).
    pub part: String,
    pub mover: Mover,
    pub mates: Vec<Mate>,
    /// The part half of the mate being picked, waiting for its fixture half.
    pub half: Option<GeoSel>,
    pub offset: LocateOffset,
}

impl LocateDraft {
    /// The command's input as drafted.
    #[must_use]
    pub fn spec(&self) -> LocateSpec {
        LocateSpec {
            part: self.part.clone(),
            fixture: self.fixture.clone(),
            mates: self.mates.clone(),
            offset: self.offset,
        }
    }
}

/// The document's parts (models that are not fixtures), name order.
#[must_use]
pub fn parts(state: &State) -> Vec<String> {
    model::model_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| camcraft_app::setup_align::is_part(&state.world, e))
        .map(|(n, _)| n)
        .collect()
}

/// Start collecting mates for `fixture` (and `part`, when the page knows
/// it - else the only part, else the first part pick). Face mode: a click
/// on a face takes the face; switch to Vertex for points.
pub fn arm(state: &mut State, fixture: &str, part: Option<&str>) {
    let part = part.map(str::to_owned).or_else(|| match parts(state).as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    });
    state.locate_pick = Some(LocateDraft { fixture: fixture.to_owned(), part: part.unwrap_or_default(), ..LocateDraft::default() });
    if !matches!(state.geo_pick, GeoPick::Face | GeoPick::Vertex) {
        state.geo_pick = GeoPick::Face;
    }
    state.status = format!("{fixture}: pick a part face or point, then the {fixture} face or point it sits on");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.locate_pick = None;
}

/// Drop the waiting half pick, else the last mate.
pub fn remove_last(state: &mut State) {
    let Some(d) = state.locate_pick.as_mut() else { return };
    if d.half.take().is_none() {
        d.mates.pop();
    }
}

/// One viewport pick into the draft: a part pick opens a mate, the next
/// fixture pick closes it. Every pick it cannot take is said, by name.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(d) = state.locate_pick.as_ref() else { return };
    let words = format!("{} of '{}'", pick.feature, pick.entity);
    if matches!(pick.feature, FeatureSel::Chain(_) | FeatureSel::Patch(_)) {
        state.status = format!("{words} is not a mate - pick a flat face or a vertex");
        return;
    }
    if d.mates.len() >= MAX_MATES && d.half.is_none() {
        state.status = "three mates fully locate the part - Remove last to change one".into();
        return;
    }
    let is_part = model::find_by_name(&state.world, &pick.entity)
        .is_some_and(|e| camcraft_app::setup_align::is_part(&state.world, &e));
    let fixture = d.fixture.clone();
    let Some(d) = state.locate_pick.as_mut() else { return };
    state.status = if pick.entity == fixture {
        match d.half.take() {
            Some(part) => {
                d.mates.push(Mate { part, fixture: pick });
                format!("mate {} picked - {}", d.mates.len(), next_hint(d))
            }
            None => format!("pick the part's face or point first, then {words}"),
        }
    } else if !is_part {
        format!("'{}' is neither the part nor the fixture '{fixture}'", pick.entity)
    } else if !d.part.is_empty() && d.part != pick.entity && !d.mates.is_empty() {
        format!("the mates so far are on '{}' - finish or clear them before picking '{}'", d.part, pick.entity)
    } else {
        d.part.clone_from(&pick.entity);
        d.half = Some(pick);
        format!("{words} - now pick the {fixture} face or point it sits on")
    };
}

/// What the draft wants next, in plain words.
#[must_use]
pub fn next_hint(d: &LocateDraft) -> String {
    match (d.half.as_ref(), d.mates.len()) {
        (Some(_), _) => format!("pick the {} face or point it sits on", d.fixture),
        (None, n) if n >= MAX_MATES => "the part is fully located - Apply".into(),
        (None, 0) => "pick a part face or point (the floor mate first)".into(),
        (None, 1) => "Apply, or pick the next mate (the fixed jaw)".into(),
        (None, _) => "Apply, or pick the last mate (the stop)".into(),
    }
}
