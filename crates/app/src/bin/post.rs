//! `post` - our command-line post processor (our own post.exe).
//!
//! Provenance: the CLI shape (`post [OPTIONS] <config> <intermediate>
//! <output>`, plus machine and log paths, with counted warnings
//! reported rather than fatal) follows the published Fusion 360 post
//! engine's documented behavior; this Rust, our flag spellings, and
//! our output text are ours. Development tool and CI check runner:
//! the in-app validation gate (M1.7b) is what refuses a machine.

use hsmworks_app::post::{self, engine};
use serde_json::Value;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
post - HSMWorks post processor (clean-room)

USAGE:
    post [OPTIONS] <config.cps> <intermediate.cnc|.cl> <output.nc>

ARGS:
    <config>          post script (.cps) to run
    <intermediate>    input stream (.cnc, or any golden .cl dump)
    <output>          program file to write

OPTIONS:
    --machine <mch>   machine definition (.mch JSON)
    --log <file>      write the warning list here instead of stderr
    -h, --help        print this text
    --version         print the version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut machine: Option<String> = None;
    let mut log: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--version" => {
                println!("post {} (boa_engine)", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--machine" | "--log" => {
                let key = if a == "--machine" { "machine" } else { "log" };
                match it.next() {
                    Some(v) => {
                        if key == "machine" {
                            machine = Some(v.clone());
                        } else {
                            log = Some(v.clone());
                        }
                    }
                    None => {
                        eprintln!("post: {a} needs a value");
                        return ExitCode::FAILURE;
                    }
                }
            }
            other if other.starts_with("--") => {
                eprintln!("post: unknown option {other:?} (try --help)");
                return ExitCode::FAILURE;
            }
            other => positional.push(other.to_string()),
        }
    }
    if positional.len() != 3 {
        eprintln!("post: expected exactly three paths (try --help)");
        return ExitCode::FAILURE;
    }
    run(&positional[0], &positional[1], &positional[2], machine.as_deref(), log.as_deref())
}

fn die(msg: String) -> ExitCode {
    eprintln!("post: {msg}");
    ExitCode::FAILURE
}

fn run(
    config: &str,
    input: &str,
    output: &str,
    machine: Option<&str>,
    log: Option<&str>,
) -> ExitCode {
    let cps = match std::fs::read_to_string(config) {
        Ok(s) => s,
        Err(e) => return die(format!("cannot read {config}: {e}")),
    };
    let stream = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => return die(format!("cannot read {input}: {e}")),
    };
    let mch: Value = match machine {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => return die(format!("cannot read {path}: {e}")),
            };
            match post::machine::from_json(&text) {
                Ok(v) => v,
                Err(e) => return die(format!("cannot parse {path}: {e}")),
            }
        }
        None => Value::Null,
    };
    match engine::run(&cps, &mch, &stream) {
        Ok(out) => {
            if let Err(e) = std::fs::write(Path::new(output), &out.program) {
                return die(format!("cannot write {output}: {e}"));
            }
            let lines = out.program.matches('\n').count();
            if !out.warnings.is_empty() {
                let text: String = out
                    .warnings
                    .iter()
                    .map(|w| format!("warning: {w}\n"))
                    .collect();
                match log {
                    Some(path) => {
                        if let Err(e) = std::fs::write(Path::new(path), &text) {
                            return die(format!("cannot write {path}: {e}"));
                        }
                        eprintln!("post: {} warning(s) logged to {path}", out.warnings.len());
                    }
                    None => {
                        eprint!("{text}post: {} warning(s)", out.warnings.len());
                        eprintln!();
                    }
                }
            }
            println!(
                "post: {input} -> {output}: {} line(s), {} mapped record(s), {} swallowed",
                lines,
                out.line_map.iter().filter(|l| l.record.is_some()).count(),
                out.swallowed,
            );
            ExitCode::SUCCESS
        }
        Err(e) => die(e.to_string()),
    }
}
