//! Regression tests for argument evaluation at call sites.
//!
//! Codegen used to materialise each argument directly into its Win64 register
//! (`rcx`, `rdx`, `r8`, `r9`) as it walked the argument list. That is wrong twice
//! over:
//!
//! 1. Binary-op codegen uses `rcx` as a scratch register, so evaluating a later
//!    argument that contains an expression destroyed argument 0. `printf("%d\n",
//!    a + b)` dereferenced an integer as a `char *` and segfaulted.
//! 2. A nested call clobbers every volatile register *and* writes its own shadow
//!    space, so `add(5, g(1))` silently returned the wrong number rather than
//!    crashing.
//!
//! Both are the same root cause: a register was loaded, then something else ran
//! before the `call`. These tests execute real binaries because the failure only
//! appears at run time — the generated assembly is perfectly well-formed.

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
fn expression_as_a_later_argument_does_not_clobber_the_first() {
    // Was: segfault. `a + b` used rcx as scratch, destroying the format pointer.
    let src = r#"
int main() {
    int a = 40;
    int b = 2;
    printf("%d\n", a + b);
    return 0;
}
"#;
    if let Some(code) = compile_and_run(src, "callarg_expr_printf") {
        assert_eq!(code, 0, "printf with an expression argument crashed");
    }
}

#[test]
fn expression_as_a_later_argument_to_a_user_function() {
    let src = r#"
int add(int a, int b) { return a + b; }
int main() { return add(1, 20 * 2 + 1); }
"#;
    if let Some(code) = compile_and_run(src, "callarg_expr_user") {
        assert_eq!(code, 42);
    }
}

#[test]
fn nested_call_as_a_later_argument_keeps_earlier_arguments() {
    // Was: returned 3. The inner call set rcx for its own argument, and that
    // value survived into the outer call.
    let src = r#"
int g(int x) { return x + 1; }
int add(int a, int b) { return a + b; }
int main() { return add(5, g(1)); }
"#;
    if let Some(code) = compile_and_run(src, "callarg_nested_second") {
        assert_eq!(code, 7, "nested call as second argument corrupted the first");
    }
}

#[test]
fn nested_call_as_the_first_argument_still_works() {
    let src = r#"
int g(int x) { return x + 1; }
int add(int a, int b) { return a + b; }
int main() { return add(g(1), 5); }
"#;
    if let Some(code) = compile_and_run(src, "callarg_nested_first") {
        assert_eq!(code, 7);
    }
}

#[test]
fn every_argument_position_survives_expression_evaluation() {
    // Four expression arguments, exercising rcx, rdx, r8 and r9 together.
    let src = r#"
int four(int a, int b, int c, int d) { return a + b + c + d; }
int main() { return four(1 + 1, 2 * 3, 10 - 3, 5 + 22); }
"#;
    if let Some(code) = compile_and_run(src, "callarg_all_four") {
        assert_eq!(code, 42, "2 + 6 + 7 + 27 = 42");
    }
}

#[test]
fn nested_calls_in_several_argument_positions() {
    let src = r#"
int g(int x) { return x + 1; }
int add(int a, int b) { return a + b; }
int main() { return add(g(20), g(20)); }
"#;
    if let Some(code) = compile_and_run(src, "callarg_nested_both") {
        assert_eq!(code, 42, "g(20) + g(20) = 21 + 21 = 42");
    }
}
