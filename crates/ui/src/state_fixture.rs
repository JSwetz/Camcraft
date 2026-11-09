//! The fixture views a frame reads (#339 slice 4): each fixture's mesh
//! state for the browser rows and the Fixtures page, and the fixtures
//! the selected op's `FixtureHit` warnings strike, for the viewport tint
//! and the op banner. Both answers are the app's
//! (`camcraft_app::fixture_view`), cached per document version; this
//! file only picks the key. Display only - nothing here is a command.
//! Provenance: our own view law; no vendor material consulted.

use std::collections::HashMap;
use std::sync::Arc;

use camcraft_app::fixture::MACHINE_TABLE;
use camcraft_app::fixture_struck::Struck;
use camcraft_app::fixture_view::{self, MeshState};

use super::State;

/// At most this many unresolved moves are listed by number.
const LISTED_MOVES: usize = 5;

impl State {
    /// Every fixture's mesh state, keyed by name (this document version).
    #[must_use]
    pub fn fixture_mesh_states(&self) -> Arc<HashMap<String, MeshState>> {
        fixture_view::mesh_states_cached(&self.fixture_mesh_cache, &self.world, self.doc_version)
    }

    /// One fixture's mesh state, if `name` is a fixture.
    #[must_use]
    pub fn fixture_mesh_state(&self, name: &str) -> Option<MeshState> {
        self.fixture_mesh_states().get(name).copied()
    }

    /// Who `op`'s fixture warnings strike (`None`: no fixture warning).
    #[must_use]
    pub fn struck_by(&self, op: &str) -> Option<Struck> {
        Option::clone(&fixture_view::struck_cached(&self.struck_cache, &self.world, self.doc_version, op))
    }

    /// The fixtures the viewport tints: those the SELECTED op's warnings
    /// strike (the derived table is not an entity, so it is never drawn).
    #[must_use]
    pub fn fixture_highlight(&self) -> Vec<String> {
        self.selected_operation()
            .and_then(|op| self.struck_by(&op))
            .map(|s| s.fixtures.into_iter().filter(|f| f != MACHINE_TABLE).collect())
            .unwrap_or_default()
    }

    /// The op banner's fixture lines: who is struck, and - said, never
    /// guessed - which warned moves no fixture reproduces today.
    #[must_use]
    pub fn struck_lines(&self, op: &str) -> Vec<String> {
        let Some(s) = self.struck_by(op) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if !s.fixtures.is_empty() {
            out.push(format!("Strikes fixture: {} - tinted in the viewport while selected", s.fixtures.join(", ")));
        }
        if !s.unresolved.is_empty() {
            let mut moves: Vec<String> = s.unresolved.iter().take(LISTED_MOVES).map(usize::to_string).collect();
            if s.unresolved.len() > LISTED_MOVES {
                moves.push(format!("and {} more", s.unresolved.len() - LISTED_MOVES));
            }
            out.push(format!(
                "Fixture not resolved for move {}: no single fixture reproduces the hit as the steel stands now - regenerate to re-check",
                moves.join(", ")
            ));
        }
        out
    }
}
