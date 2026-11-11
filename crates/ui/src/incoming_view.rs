//! The incoming stock in the viewport (#406): while a setup's settings
//! page is open and its stock source is "What <previous setup> leaves
//! behind", the body the setups before it leave stands in the viewport
//! as a teal translucent sheet, apart from the setup's own Stock node.
//!
//! Provenance: our own display design over #317's carry and #320's
//! stock source; no vendor material consulted.
//!
//! ## The laws here
//!
//! - **Measured once per document version, never per frame.** [`refresh`]
//!   runs at the end of every `State::drain_actions`; it measures only
//!   when the key (setup, active setup, `doc_version`) moves, through
//!   `camcraft_app::setup_stock_view::incoming_shape` - one program sim
//!   for the sheet and for the page's box and volume.
//! - **Shown only while wanted, dropped when not.** A closed page, another
//!   page, or a setup taking its own stock clears the cache at once - the
//!   mesh is never held for a page nobody looks at.
//! - **A refusal draws nothing and says so once.** The reason is logged
//!   when measured (not per frame) and the page prints it; the scene
//!   gets no part.
//! - **View state, never a command.** Opening a page is not undoable; the
//!   sheet follows the page, the history is untouched.

use camcraft_app::geometry::Triangle;
use camcraft_app::setup_stock::{self, Incoming, StockSource};
use camcraft_app::setup_stock_view::{self, IncomingShape};

use crate::state::{SetupPage, State, UiTarget};

/// The scene's deviation-class channel value that `fs_stock` paints as
/// the incoming stock's teal (the verdicts own 0..3).
pub const INCOMING_CLASS: f32 = 4.0;

/// What one measurement answers for: the setup, the active setup (the
/// viewport's frame) and the document version.
type Key = (String, Option<String>, u64);

/// The cache, held in `State::incoming_view`.
#[derive(Default)]
pub struct IncomingView {
    key: Option<Key>,
    shape: Option<Result<IncomingShape, String>>,
    /// Measurements taken so far - the tests' proof that a frame without
    /// a document change measures nothing.
    pub(crate) measured: u64,
}

impl IncomingView {
    fn clear(&mut self) {
        self.key = None;
        self.shape = None;
    }
}

/// The setup whose incoming stock the viewport should show this frame:
/// its settings page open and its source "What <previous> leaves behind".
#[must_use]
pub fn wanted(state: &State) -> Option<String> {
    let UiTarget::SetupNode(name, SetupPage::Settings) = state.pane_target() else {
        return None;
    };
    (setup_stock::effective(&state.world, &name) == Some(StockSource::FromPrevious)).then_some(name)
}

fn key_of(state: &State, name: String) -> Key {
    (name, camcraft_app::setup::active(&state.world), state.doc_version)
}

/// Measure when the wanted key moved; clear when nothing is wanted.
pub fn refresh(state: &mut State) {
    let Some(name) = wanted(state) else {
        state.incoming_view.clear();
        return;
    };
    let key = key_of(state, name);
    if state.incoming_view.key.as_ref() == Some(&key) {
        return;
    }
    let got = setup_stock_view::incoming_shape(&state.world, &key.0);
    if let Err(why) = &got {
        eprintln!("incoming stock of {}: not drawn - {why}", key.0);
    }
    let view = &mut state.incoming_view;
    view.measured = view.measured.saturating_add(1);
    view.shape = Some(got);
    view.key = Some(key);
}

/// The cached measurement for `name` at this document version, if one
/// stands (the page's box and volume read it rather than re-run the sim).
#[must_use]
pub fn measured(state: &State, name: &str) -> Option<Result<Incoming, String>> {
    let key = state.incoming_view.key.as_ref()?;
    if key.0 != name || key.2 != state.doc_version {
        return None;
    }
    match state.incoming_view.shape.as_ref()? {
        Ok(s) => Some(Ok(s.incoming.clone())),
        Err(why) => Some(Err(why.clone())),
    }
}

/// The sheet to draw this frame: only while still wanted and measured for
/// exactly this key - a stale cache is never drawn.
#[must_use]
pub fn mesh(state: &State) -> Option<&[Triangle]> {
    let name = wanted(state)?;
    if state.incoming_view.key.as_ref() != Some(&key_of(state, name)) {
        return None;
    }
    match state.incoming_view.shape.as_ref()? {
        Ok(s) => Some(&s.mesh),
        Err(_) => None,
    }
}
