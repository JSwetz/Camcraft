//! #265 in the shell: a refused post's reason stays on screen in its own
//! window after the status line moves on. A viewport click writes the
//! status line, never the window. The window goes when the user closes
//! it, a later post succeeds, or the document is switched; an edit only
//! adds a note that the document has changed since.
//! Provenance: our own tests over our own shell; no HSMWorks material
//! involved.

use super::*;

/// The refusal window's title.
const TITLE: &str = "Post refused";
/// The note the window adds once the document has changed after the
/// refusal (an edit, a regenerate).
const CHANGED_NOTE: &str =
    "The document has changed since this post was refused - post again to check it.";

fn program_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-refusal-{tag}-{}.nc", std::process::id()))
}

/// Post the seed as shipped. Its contour takes the full block height,
/// and plunging that rim beside 30 mm of uncut wall is a shank hit, so
/// the gate refuses it (the M1.7b test at the head of `tests.rs`).
/// Returns the reason the status line gave.
fn post_the_seed(h: &mut Harness<'static, ShellUi>, path: &std::path::Path) -> String {
    h.state_mut()
        .state
        .pending
        .push_back(CommandKind::PostProgramFile(path.to_path_buf()));
    h.run();
    let status = h.state().state.status.clone();
    status
        .strip_prefix("post refused: ")
        .unwrap_or_else(|| panic!("the seed's post is refused (status: {status})"))
        .to_owned()
}

/// Re-cut the seed's contour as a 0.5 mm top skim and regenerate it:
/// legal geometry the gate posts. Both go through the command queue, so
/// the document version moves the way a user's edit moves it.
fn make_the_seed_postable(h: &mut Harness<'static, ShellUi>) {
    let mut skim = {
        let st = &h.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        model::operation_of(&st.world, &e)
            .expect("op")
            .params
            .clone()
    };
    if let OpParams::Contour { bottom_z, .. } = &mut skim {
        *bottom_z = Height::fixed(29.5);
    }
    let pending = &mut h.state_mut().state.pending;
    pending.push_back(CommandKind::EditOperationParams {
        name: "Contour1".into(),
        params: skim,
    });
    pending.push_back(CommandKind::GenerateOperation("Contour1".into()));
    h.run();
}

#[test]
fn a_refused_posts_reason_survives_a_viewport_click() {
    let mut harness = shell_harness();
    let path = program_path("click");
    let _ = std::fs::remove_file(&path);
    let reason = post_the_seed(&mut harness, &path);
    harness.run();
    assert!(
        harness.query_by_label(TITLE).is_some(),
        "a refused post opens its window"
    );
    assert!(
        harness.query_by_label(&reason).is_some(),
        "the window shows the gate's reason: {reason}"
    );

    // The issue's gesture: a click in the viewport. It picks (or misses)
    // a model and writes that to the status line, as it always has.
    let c = vp_center(harness.state());
    click_viewport(&mut harness, c);
    let status = harness.state().state.status.clone();
    assert!(
        !status.contains("post refused"),
        "the click reached the viewport and wrote the status line (status: {status})"
    );
    assert!(
        harness.query_by_label(&reason).is_some(),
        "the reason outlives the status line: {reason}"
    );
    assert!(
        harness.query_by_label(CHANGED_NOTE).is_none(),
        "a pick is not an edit"
    );
    assert!(!path.exists(), "a refused post writes nothing");
}

#[test]
fn an_edit_after_the_refusal_notes_it_and_a_successful_post_retires_it() {
    let mut harness = shell_harness();
    let path = program_path("retire");
    let _ = std::fs::remove_file(&path);
    let reason = post_the_seed(&mut harness, &path);

    make_the_seed_postable(&mut harness);
    harness.run();
    assert!(
        harness.query_by_label(&reason).is_some(),
        "an edit keeps the reason on screen"
    );
    assert!(
        harness.query_by_label(CHANGED_NOTE).is_some(),
        "but says it may no longer hold"
    );

    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    let st = &harness.state().state;
    assert!(
        st.status.starts_with("posted"),
        "the skim posts (status: {})",
        st.status
    );
    assert!(
        st.post_refusal.is_none(),
        "a program went out: the refusal is retired"
    );
    assert!(
        harness.query_by_label(TITLE).is_none(),
        "and its window is gone"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn closing_the_refusal_window_retires_it() {
    let mut harness = shell_harness();
    let path = program_path("close");
    post_the_seed(&mut harness, &path);
    harness.run();
    // egui's title-bar ✖; the refusal is the only window a refused post
    // opens on the seed.
    harness.get_by_label("Close window").click();
    harness.run();
    assert!(
        harness.state().state.post_refusal.is_none(),
        "the ✖ drops the refusal"
    );
    assert!(harness.query_by_label(TITLE).is_none(), "and its window");
}

#[test]
fn a_new_document_retires_the_refusal() {
    let mut harness = shell_harness();
    let path = program_path("new");
    post_the_seed(&mut harness, &path);
    assert!(
        harness.state().state.post_refusal.is_some(),
        "the refusal is kept"
    );
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::NewDocument);
    // The seed is unsaved: New waits for the prompt's answer (#107).
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::ResolveUnsaved(
            crate::unsaved::UnsavedChoice::DontSave,
        ));
    harness.run();
    assert!(
        harness.state().state.post_refusal.is_none(),
        "a refusal is about the old document's program, never the new one's"
    );
    assert!(
        harness.query_by_label(TITLE).is_none(),
        "and its window is gone"
    );
}
