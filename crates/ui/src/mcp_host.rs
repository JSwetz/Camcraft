//! The GUI's live MCP host (#183): each frame, the agent's waiting calls
//! are answered on the UI thread against the window's own document and
//! History through `camcraft_app::mcp::answer_live`, so the browser
//! shows an agent's step and Ctrl+Z undoes it like any user step.
//! Provenance: our own code over our own app seam; no HSMWorks material.

use crate::state::State;

/// Answer every waiting request. Safe to call every frame; never blocks
/// (each agent waits for its answer, so there is rarely more than one).
pub fn pump(state: &mut State) {
    let Some(inbox) = state.mcp.as_ref() else {
        return;
    };
    let waiting: Vec<_> = std::iter::from_fn(|| inbox.next()).collect();
    for request in waiting {
        let answer = camcraft_app::mcp::answer_live(
            &state.world,
            &mut state.history,
            state.project_path.as_deref(),
            request.line(),
        );
        if let Some(tool) = &answer.tool {
            // A tool may have changed the document: every view keyed on
            // the document version refetches, the same as after an undo.
            state.doc_version += 1;
            state.status = format!("agent: {tool}");
        }
        request.answer(answer.reply);
    }
}

/// The status strip's note while the server listens, so the user can
/// see the window is open to an agent.
#[must_use]
pub fn badge(state: &State) -> Option<String> {
    state.mcp.as_ref()?.addr().map(|addr| format!("MCP listening on {addr}"))
}
