//! The headless door (M5.3 G): `camcraft-cli`'s contract is the
//! process contract - exit codes, and who prints where. What must
//! stay true: the CLI is thin over the script host (every run here
//! is a `script::run` someone could have typed), usage refuses with
//! 2, a job that refused exits 1, and a job that stood exits 0 and
//! leaves its answer on stdout. The spawns are real processes: CI
//! runs this door, not a Rust call.
//!
//! Gate map (M4.4): covers app::cli, app::script.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, History, SetStock};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

use std::path::PathBuf;
use std::process::{Command, Output};

fn tmp(tag: &str, ext: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-cli-{tag}-{}.{}", std::process::id(), ext));
    let _ = std::fs::remove_file(&p);
    p
}

/// The fixture both CLI lanes share: the scripting suite's document -
/// box stock and one clean facing op - saved so `process` has a
/// project to bring current. If this refuses, the door is fine and
/// the fixture is wrong.
fn fixture_proj() -> World {
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let params = OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create"));
    world
}

fn cli_str(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_camcraft-cli"));
    cmd.args(args);
    cmd.output().expect("the CLI binary runs")
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

#[test]
fn help_and_version_stand_up() {
    let o = cli_str(&["help"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("USAGE"), "help shows the shape");
    let o = cli_str(&["version"]);
    assert_eq!(code(&o), 0);
    assert!(String::from_utf8_lossy(&o.stdout).contains("camcraft-cli"), "version names itself");
}

#[test]
fn no_command_and_strangers_refuse_with_a_two() {
    // 2 is "the command line was wrong", never "the job ran bad".
    let o = cli_str(&[]);
    assert_eq!(code(&o), 2);
    let o = cli_str(&["frobnicate"]);
    assert_eq!(code(&o), 2);
    assert!(String::from_utf8_lossy(&o.stderr).contains("frobnicate"), "refusals name what they met");
    let o = cli_str(&["script", "job.camproj"]);
    assert_eq!(code(&o), 2);
    let o = cli_str(&["process", "project.js"]);
    assert_eq!(code(&o), 2);
    let o = cli_str(&["script", "/no/such/job.js"]);
    assert_eq!(code(&o), 2, "an unreadable file is a usage-side failure");
}

#[test]
fn a_script_lane_prints_logs_then_the_answer() {
    let job = tmp("job", "js");
    std::fs::write(&job, "cam.log(\"one\"); 1 + 2").expect("write job");
    let o = cli_str(&["script", job.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{:?}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(out.trim().lines().collect::<Vec<_>>(), ["one", "3"], "logs, then the parked answer");
}

#[test]
fn a_refused_script_exits_one_with_the_line_on_stderr() {
    let job = tmp("bad", "js");
    std::fs::write(&job, "cam.operations(\"extra\")").expect("write job");
    let o = cli_str(&["script", job.to_str().unwrap()]);
    assert_eq!(code(&o), 1);
    assert!(String::from_utf8_lossy(&o.stderr).contains("operations:"), "the refusal reaches stderr by verb");
}

#[test]
fn process_lane_loads_generates_and_posts_the_project() {
    let proj = tmp("proc", "camproj");
    let world = fixture_proj();
    project::save(&world, &proj).expect("save fixture");
    let nc = tmp("proc", "nc");
    let o = cli_str(&["process", proj.to_str().unwrap(), "-o", nc.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{:?}", String::from_utf8_lossy(&o.stderr));
    assert!(nc.exists(), "the -o lane posts");
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("\"ok\":true"), "verify's answer is on stdout: {text}");
    assert!(text.contains("\"status\":\"ok\""), "the report speaks a fresh op: {text}");
}

#[test]
fn process_lane_without_o_judges_rather_than_writes() {
    let proj = tmp("judge", "camproj");
    let world = fixture_proj();
    project::save(&world, &proj).expect("save fixture");
    let o = cli_str(&["process", proj.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "a fresh-after-generate job stands");
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("\"gate\""), "the answer is the verify report: {text}");
    // A named post rides the same door: --cps is a post option, not a CLI flag.
    let nc = tmp("judge-haas", "nc");
    let o = cli_str(&["process", proj.to_str().unwrap(), "-o", nc.to_str().unwrap(), "--cps", "Haas"]);
    assert_eq!(code(&o), 0, "{:?}", String::from_utf8_lossy(&o.stderr));
    assert!(nc.exists());
}

#[test]
fn a_post_that_refuses_exits_one_and_says_why() {
    let proj = tmp("refuse", "camproj");
    let world = fixture_proj();
    project::save(&world, &proj).expect("save fixture");
    // A directory that is not there: the post verb refuses, the
    // script stops on the Error, and no verify answer prints.
    let nc = tmp("refuse", "no-such-dir").join("out.nc");
    let o = cli_str(&["process", proj.to_str().unwrap(), "-o", nc.to_str().unwrap()]);
    assert_eq!(code(&o), 1);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("post:"), "the verb names itself: {err}");
    assert!(!o.stdout.contains(&b'{'), "a stopped script parks no answer: {}", String::from_utf8_lossy(&o.stdout));
}
