//! Regression test for the PE entry-point bug.
//!
//! The custom PE writer once hardcoded the entry point to the start of `.text` — i.e.
//! whichever function the code generator emitted first — instead of `main`. Every
//! example and unit test happened to define `main` first, so the bug stayed invisible
//! while 151 tests passed. This test defines a helper *before* `main`: if the entry
//! point regresses, the process runs `helper` (returning 7) instead of `main`
//! (returning 42), and the exit code catches it.
//!
//! The bug is only observable at runtime — the assembly for both functions is
//! identical, so only running the binary and checking its exit code detects it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn compile_and_run(src: &str, base: &str) -> Option<i32> {
    let mut c_path = std::env::temp_dir();
    c_path.push(format!("{base}.c"));
    let mut out_base = std::env::temp_dir();
    out_base.push(base);
    std::fs::write(&c_path, src).unwrap();

    let status = Command::new("cargo")
        .args(["run", "--quiet", "--", c_path.to_str().unwrap(), "-o", out_base.to_str().unwrap()])
        .status()
        .unwrap();
    if !status.success() {
        panic!("compile failed for {base}");
    }

    let mut exe = out_base.clone();
    exe.set_extension("exe");
    let exe: PathBuf = if exe.exists() { exe } else { out_base };
    run_exit_code(&exe)
}

fn run_exit_code(exe: &Path) -> Option<i32> {
    if cfg!(target_os = "windows") {
        Some(Command::new(exe).status().unwrap().code().unwrap())
    } else if Command::new("wine").arg("--version").output().is_ok() {
        Some(Command::new("wine").arg(exe).status().unwrap().code().unwrap())
    } else {
        eprintln!("skipping run: no PE runner (not Windows, no wine)");
        None
    }
}

#[test]
fn entry_point_is_main_not_first_function() {
    // `helper` is defined first; a correct entry point still runs `main`.
    let src = "int helper() { return 7; } int main() { return 42; }";
    if let Some(code) = compile_and_run(src, "entry_point_regression") {
        assert_eq!(code, 42, "entry point ran the wrong function (got {code}, want 42)");
    }
}
