//! Runs the vendored c-testsuite (`tests/c-testsuite/single-exec/`) with the default
//! backend.
//!
//! A test passes when it exits with `0` and prints its `.expected` file.
//! `tests/c-testsuite/passing.txt` lists the tests that pass. This test fails when a
//! listed test fails (a regression), or when a test that is not listed passes (update
//! the list). `C_TESTSUITE_REPORT=1` prints why each failing test fails.

mod common;
use common::{run, try_compile};

use std::path::{Path, PathBuf};

/// `None` when the host cannot run a PE. Otherwise the reason the test fails, or `Ok`.
fn check(path: &Path) -> Option<Result<(), String>> {
    let src = std::fs::read_to_string(path).unwrap();
    let expected = std::fs::read_to_string(path.with_extension("c.expected"))
        .unwrap()
        .replace("\r\n", "\n");
    let base = format!("ctest_{}", path.file_stem().unwrap().to_str().unwrap());

    let exe = match try_compile(&src, &base) {
        Ok(exe) => exe,
        Err(stderr) => {
            let first = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            return Some(Err(format!("compile failed: {first}")));
        }
    };
    // `run` panics when the program does not finish in time. Count that as a failure.
    let r = match std::thread::spawn(move || run(&exe, &base)).join() {
        Ok(r) => r?,
        Err(_) => return Some(Err("timed out".into())),
    };
    if r.code != 0 {
        return Some(Err(format!("exit code {}", r.code)));
    }
    if r.stdout != expected {
        return Some(Err("stdout differs from .expected".into()));
    }
    Some(Ok(()))
}

#[test]
fn c_testsuite_matches_passing_list() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("c-testsuite");
    if !root.exists() {
        // The published crate leaves the suite out.
        eprintln!("skipping: tests/c-testsuite is not present");
        return;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("single-exec"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "c"))
        .collect();
    files.sort();

    // Bound the number of programs that run at once, so a loaded machine does not
    // push a slow program past the time limit.
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut results = Vec::new();
    for chunk in files.chunks(workers) {
        std::thread::scope(|s| {
            let handles: Vec<_> = chunk.iter().map(|p| s.spawn(move || check(p))).collect();
            for (p, h) in chunk.iter().zip(handles) {
                let name = p.file_stem().unwrap().to_string_lossy().into_owned();
                results.push((name, h.join().unwrap()));
            }
        });
    }
    if results.iter().any(|(_, r)| r.is_none()) {
        return;
    }

    let listed = std::fs::read_to_string(root.join("passing.txt")).unwrap_or_default();
    let listed: Vec<&str> = listed.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut regressed = Vec::new();
    let mut newly_passing = Vec::new();
    let mut passed = 0;
    for (name, r) in &results {
        let ok = matches!(r, Some(Ok(())));
        passed += ok as usize;
        match (ok, listed.contains(&name.as_str())) {
            (false, true) => regressed.push(format!("  {name}: {}", r.clone().unwrap().unwrap_err())),
            (true, false) => newly_passing.push(format!("  {name}")),
            _ => {}
        }
        if !ok && std::env::var_os("C_TESTSUITE_REPORT").is_some() {
            eprintln!("{name}: {}", r.clone().unwrap().unwrap_err());
        }
    }
    eprintln!("c-testsuite: {passed} of {} pass", results.len());

    let mut msg = String::new();
    if !regressed.is_empty() {
        msg += &format!("listed in passing.txt, but now fail:\n{}\n", regressed.join("\n"));
    }
    if !newly_passing.is_empty() {
        msg += &format!("these now pass. Add each one to passing.txt:\n{}\n", newly_passing.join("\n"));
    }
    assert!(msg.is_empty(), "{msg}");
}
