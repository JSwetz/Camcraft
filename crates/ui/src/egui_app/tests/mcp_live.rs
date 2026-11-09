//! #183 in the window: an agent's `add_operation`, queued on the live MCP
//! inbox, is answered by the shell's own frame loop against the window's
//! document - the browser shows the operation - and the user's Ctrl+Z
//! removes it, because the agent's step sits on the window's History.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use super::super::browser_tree::Leaf;
use camcraft_app::mcp::{Inbox, Request};
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;

fn call(id: u64, name: &str, args: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } })
        .to_string()
}

/// The browser pane paints exactly these rows (they are not egui labels).
fn has_op_row(state: &State, name: &str) -> bool {
    let (_, leaves) = panes::browser_rows(state);
    leaves.iter().any(|l| *l == Leaf::Op(name.to_owned()))
}

/// The answer a frame gave, as JSON; it must be a tool success.
fn answered(rx: &Receiver<Option<String>>) -> Value {
    let line = rx.try_recv().expect("the frame answered").expect("a reply line");
    let v: Value = serde_json::from_str(&line).expect("json");
    assert_eq!(v["result"]["isError"], json!(false), "{v}");
    v
}

fn ctrl_z(harness: &mut Harness<'static, ShellUi>) {
    harness.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
}

#[test]
fn an_agents_operation_shows_in_the_browser_and_ctrl_z_removes_it() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    let (tx, inbox) = Inbox::channel();
    harness.state_mut().state.mcp = Some(inbox);

    let mut answers = Vec::new();
    for line in [
        call(1, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } })),
        call(2, "add_tool", json!({ "catalog": "EM-0400-2FL" })),
        call(3, "add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" })),
    ] {
        let (req, rx) = Request::pair(line);
        tx.send(req).expect("inbox open");
        answers.push(rx);
    }
    // Nothing runs off the frame loop: the queue waits for a frame.
    assert!(!has_op_row(&harness.state().state, "Face1"));
    harness.run();
    for rx in &answers {
        answered(rx);
    }
    let st = &harness.state().state;
    assert!(has_op_row(st, "Face1"), "the browser lists the agent's operation");
    assert_eq!(st.status, "agent: add_operation");

    // The user's own Ctrl+Z undoes the agent's last step.
    ctrl_z(&mut harness);
    let st = &harness.state().state;
    assert!(!has_op_row(st, "Face1"), "Ctrl+Z removed the agent's operation");
    assert!(st.history.can_redo());

    // And the agent sees the user's undo: its own redo brings it back.
    let (req, rx) = Request::pair(call(4, "redo", json!({})));
    tx.send(req).expect("inbox open");
    harness.run();
    answered(&rx);
    assert!(has_op_row(&harness.state().state, "Face1"));
}

#[test]
fn the_status_strip_says_the_server_is_listening() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    assert!(harness.query_by_label_contains("MCP listening").is_none(), "off by default");
    let inbox = Inbox::listen(0, Box::new(|| {})).expect("listen on loopback");
    let addr = inbox.addr().expect("addr");
    harness.state_mut().state.mcp = Some(inbox);
    harness.run();
    assert!(harness.query_by_label(&format!("MCP listening on {addr}")).is_some());
}
