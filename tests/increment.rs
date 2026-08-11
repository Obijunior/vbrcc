//! Regression tests for the value of `x++` and `x--`.
//!
//! The parser used to rewrite `i++` as `i = i + 1`. An assignment evaluates to the value
//! it stored, so that form gave the value *after* the update. A postfix operator must
//! give the value before it. In statement position, such as `i++;` or a `for` update,
//! the program discards the value and the bug does not show. This is why every test
//! passed. `int x = i++;` showed the bug: it returned 6 for `i = 5`.
//!
//! These tests run real binaries. The assembly was correct in shape either way, so only
//! the value proves the fix.

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
fn post_increment_yields_the_value_before_the_update() {
    // Was 6.
    let src = "int main() { int i = 5; int x = i++; return x; }";
    if let Some(code) = compile_and_run(src, "postinc_value") {
        assert_eq!(code, 5, "`i++` must evaluate to the old value");
    }
}

#[test]
fn post_increment_still_updates_the_variable() {
    let src = "int main() { int i = 5; i++; return i; }";
    if let Some(code) = compile_and_run(src, "postinc_effect") {
        assert_eq!(code, 6);
    }
}

#[test]
fn post_decrement_yields_the_value_before_the_update() {
    let src = "int main() { int i = 43; int x = i--; return x; }";
    if let Some(code) = compile_and_run(src, "postdec_value") {
        assert_eq!(code, 43);
    }
}

#[test]
fn post_decrement_still_updates_the_variable() {
    let src = "int main() { int i = 43; i--; return i; }";
    if let Some(code) = compile_and_run(src, "postdec_effect") {
        assert_eq!(code, 42);
    }
}

/// The `for`-loop shape that made the bug invisible: the update is a statement,
/// so its value is discarded and the loop always behaved correctly.
#[test]
fn a_for_loop_counter_still_runs_the_right_number_of_times() {
    let src = "int main() { int n = 0; for (int i = 0; i < 10; i++) { n++; } return n; }";
    if let Some(code) = compile_and_run(src, "postinc_for") {
        assert_eq!(code, 10);
    }
}

/// `p++` steps by one element, not one byte.
#[test]
fn post_increment_on_a_pointer_steps_by_the_pointee_size() {
    let src = "int main() { int a[3]; a[0] = 1; a[1] = 42; int *p = a; p++; return *p; }";
    if let Some(code) = compile_and_run(src, "postinc_pointer") {
        assert_eq!(code, 42, "`p++` must advance one int, not one byte");
    }
}

/// A post-increment used for its value inside an index expression — the classic
/// idiom the desugaring got wrong.
#[test]
fn post_increment_as_an_index_reads_the_old_slot() {
    let src = "int main() { int a[3]; a[0] = 42; a[1] = 7; int i = 0; return a[i++]; }";
    if let Some(code) = compile_and_run(src, "postinc_index") {
        assert_eq!(code, 42, "`a[i++]` must read a[0], not a[1]");
    }
}
