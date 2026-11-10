//! `gate test <filter>`: exactly the `#[test]` fns whose name matches, in the crate and suite that
//! hold them (ignored ones in release with --ignored).
//! Provenance: our own build law (AGENTS.md "Test the change"), ported from scripts/gate.ps1 -Test (#748).

use std::path::Path;

use regex::escape;

use crate::plan::{ALL_CRATES, J, Plan, Step, T, pkg};
use crate::scan::{self, re};

pub struct Filter<'a> {
    pub name: &'a str,
    pub krate: Option<&'a str>,
    pub binary: Option<&'a str>,
    pub exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Hit {
    dir: String,
    /// true: an integration suite in `crates/<dir>/tests/`; false: a unit test in the lib.
    suite: bool,
    bin: String,
    func: String,
    ignored: bool,
}

/// (crate dir, is a suite, suite name or empty for the lib, ignored).
type Group = (String, bool, String, bool);

/// The plan; `Err` names why nothing can run (no fn matches the filter).
pub fn plan(root: &Path, f: &Filter) -> Result<Plan, String> {
    let dirs: Vec<String> = match f.krate {
        Some(c) => vec![c.strip_prefix("camcraft-").unwrap_or(c).to_string()],
        None => ALL_CRATES.iter().map(|d| (*d).to_string()).collect(),
    };
    let t = escape(f.name);
    let pat = if f.exact {
        re(&format!(r"(?i)fn\s+{t}\s*[(<]"))?
    } else {
        re(&format!(r"(?i)fn\s+\w*{t}\w*\s*[(<]"))?
    };
    let mut hits: Vec<Hit> = Vec::new();
    for d in &dirs {
        let crate_dir = root.join("crates").join(d);
        let tests_dir = crate_dir.join("tests");
        let files = match f.binary {
            Some(b) => {
                let file = tests_dir.join(format!("{b}.rs"));
                if file.is_file() { vec![file] } else { Vec::new() }
            }
            None => {
                let mut v = scan::rs_files(&tests_dir);
                v.extend(scan::rs_files_recursive(&crate_dir.join("src")));
                v
            }
        };
        for file in files {
            let text = scan::read(&file);
            if f.binary.is_none() && !pat.is_match(&text) {
                continue;
            }
            // Only a file directly in crates/<dir>/tests is its own suite; a `tests/` module under
            // src/ is part of the lib.
            let suite = file.parent() == Some(tests_dir.as_path());
            let bin = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            for tf in scan::test_fns(&text)? {
                let ok = if f.exact {
                    tf.name.eq_ignore_ascii_case(f.name)
                } else {
                    tf.name.contains(f.name)
                };
                if ok {
                    hits.push(Hit {
                        dir: d.clone(),
                        suite,
                        bin: bin.clone(),
                        func: tf.name,
                        ignored: tf.ignored,
                    });
                }
            }
        }
    }
    if hits.is_empty() {
        return Err(format!("no #[test] fn matches '{}'", f.name));
    }
    let mut plan = Plan::default();
    plan.notes.push("matched:".into());
    for h in &hits {
        plan.notes.push(format!(
            "  {} {} {}::{}{}",
            h.dir,
            if h.suite { "test" } else { "lib" },
            h.bin,
            h.func,
            if h.ignored { "  [ignored -> release]" } else { "" }
        ));
    }
    // One step per (crate, suite or lib, ignored), in the order they were found. The lib is one
    // target however many src files matched, so its hits share a group.
    let group = |h: &Hit| (h.dir.clone(), h.suite, if h.suite { h.bin.clone() } else { String::new() }, h.ignored);
    let mut groups: Vec<(Group, Vec<String>)> = Vec::new();
    for h in &hits {
        let g = group(h);
        match groups.iter_mut().find(|(k, _)| *k == g) {
            Some((_, fns)) => fns.push(h.func.clone()),
            None => groups.push((g, vec![h.func.clone()])),
        }
    }
    for ((dir, suite, bin, ignored), fns) in groups {
        let pk = pkg(&dir);
        let mut args: Vec<String> = vec!["test".into()];
        if ignored {
            args.push("--release".into());
        }
        for a in ["-j", J, "-p", &pk] {
            args.push(a.into());
        }
        if suite {
            args.extend(["--test".to_string(), bin.clone()]);
        } else {
            args.push("--lib".into());
        }
        args.extend(["--".to_string(), T.to_string()]);
        if ignored {
            args.push("--ignored".into());
        }
        // --exact needs the full test path: top-level fns in suites; lib unit tests filter by substring.
        if f.exact && suite {
            args.push("--exact".into());
            args.extend(fns);
        } else {
            args.push(f.name.to_string());
        }
        let name = format!(
            "{dir} {}{}",
            if suite { bin.as_str() } else { "lib" },
            if ignored { " (ignored, release)" } else { "" }
        );
        plan.steps.push(Step::new(name, &[], &dir).with(&args));
    }
    Ok(plan)
}
