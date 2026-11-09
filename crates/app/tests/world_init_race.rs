//! The first-world race (#379): flecs keeps its OS API (malloc, free,
//! abort, the clock) in one process-global struct, and the first world
//! a process creates fills it in without a lock. Two threads creating
//! their first worlds at the same moment could see it half-written:
//! `ecs_mini` aborted with ECS_MISSING_OS_API (SIGABRT) or called a NULL
//! hook (SIGSEGV). Under load, the scripting suite crashed that way in
//! about 1 run out of 170. project_roundtrip's one SIGABRT looks the
//! same. `world::new_world` now finishes that
//! initialisation once, behind a `Once`, before any world exists.
//!
//! The race only happens on a process's FIRST worlds, so one in-process
//! test proves nothing once any world exists. This binary therefore
//! re-runs itself as child processes. Each child releases a crowd of
//! threads at one barrier, and every thread builds its first world at
//! once. Every child must exit cleanly.
//!
//! Gate map (M4.4): covers app::world.

use camcraft_app::world::new_world;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};

const CHILD_ENV: &str = "CAMCRAFT_WORLD_RACE_CHILD";
/// Before the fix, about 3 children in 1000 crashed under this load
/// (6 of 1920). With 720 children, a run took about 4 s and failed in 4
/// of 6 runs. After the fix, 3840 of 3840 children passed. It is a
/// stress test, so a pass is evidence, not proof. The scripting
/// suite's stress loop in docs/decisions/379-scripting-sigsegv.md is
/// the larger check.
const CHILDREN: usize = 720;
/// How many children run at once.
const WAVE: usize = 24;
const THREADS: usize = 32;

/// One child's whole job: a crowd of threads released at one barrier,
/// each building the first world it sees.
fn crowd() {
    let gate = Arc::new(Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || {
                gate.wait();
                drop(new_world());
            })
        })
        .collect();
    for h in handles {
        h.join().expect("a world-building thread panicked");
    }
}

#[test]
fn first_worlds_on_many_threads_never_crash() {
    if std::env::var_os(CHILD_ENV).is_some() {
        crowd();
        return;
    }
    let exe = std::env::current_exe().expect("own test binary");
    // Spinners keep every core busy while the children run, so a thread
    // can be preempted in the middle of the first initialisation. That
    // is how the gate's concurrent suites hit the race.
    let stop = Arc::new(AtomicBool::new(false));
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let spinners: Vec<_> = (0..cores)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::hint::spin_loop();
                }
            })
        })
        .collect();
    let mut crashed = Vec::new();
    for wave in 0..CHILDREN / WAVE {
        let children: Vec<_> = (0..WAVE)
            .map(|_| {
                Command::new(&exe)
                    .args(["--exact", "first_worlds_on_many_threads_never_crash", "--test-threads=1", "-q"])
                    .env(CHILD_ENV, "1")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .expect("spawn child")
            })
            .collect();
        for (i, child) in children.into_iter().enumerate() {
            let out = child.wait_with_output().expect("child output");
            if !out.status.success() {
                crashed.push(format!(
                    "wave {wave} child {i}: {:?}\n{}{}",
                    out.status,
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    for s in spinners {
        s.join().expect("spinner thread panicked");
    }
    assert!(
        crashed.is_empty(),
        "{} of {CHILDREN} children crashed building their first worlds:\n{}",
        crashed.len(),
        crashed.join("\n")
    );
}
