//! Regression tests for how a call site evaluates its arguments.
//!
//! The code generator used to load each argument straight into its Win64 register
//! (`rcx`, `rdx`, `r8`, `r9`) as it walked the argument list. That is wrong in two ways:
//!
//! 1. Binary-operation code uses `rcx` as a scratch register. A later argument that
//!    held an expression therefore destroyed argument 0. `printf("%d\n", a + b)` read an
//!    integer as a `char *` and crashed.
//! 2. A nested call overwrites every volatile register, and it writes its own shadow
//!    space. `add(5, g(1))` returned the wrong number in silence.
//!
//! Both have one cause: the generator loaded a register, and then more code ran before
//! the `call`. These tests run real binaries, because the failure appears only at run
//! time. The generated assembly is correct in shape.

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

// The fix above covered arguments, which live in frame slots. A binary *operand* still
// used `push rax`, so the same hazard stayed one level out. A nested call writes its
// shadow space with `mov [rsp + n], reg`, and that store landed on the pushed operand.
// The odd push also left rsp misaligned at the call. These are the cases that fix
// missed.

#[test]
fn a_call_in_the_right_operand_does_not_clobber_the_left() {
    // Was 4. The shadow-space store in g overwrote the pushed `f(1)`, so the reload
    // gave 2 and the sum was 2 + 2.
    let src = r#"
int f(int x) { return x; }
int g(int x) { return x; }
int main() { return f(1) + g(2); }
"#;
    if let Some(code) = compile_and_run(src, "operand_call_right") {
        assert_eq!(code, 3, "a call in the right operand corrupted the left");
    }
}

#[test]
fn calls_on_both_sides_of_an_operator() {
    let src = r#"
int f(int x) { return x * 2; }
int main() { return f(15) + f(6); }
"#;
    if let Some(code) = compile_and_run(src, "operand_call_both") {
        assert_eq!(code, 42, "30 + 12 = 42");
    }
}

#[test]
fn a_call_inside_a_comparison_operand() {
    let src = r#"
int f(int x) { return x; }
int main() { if (f(1) < f(2)) { return 42; } return 0; }
"#;
    if let Some(code) = compile_and_run(src, "operand_call_cmp") {
        assert_eq!(code, 42);
    }
}

#[test]
fn a_call_in_the_value_of_an_assignment_keeps_the_destination() {
    // The assignment path spilled the rhs with `push rax` too.
    let src = r#"
int f(int x) { return x; }
int main() { int a[2]; a[0] = 0; a[1] = 0; a[1] = f(42); return a[1]; }
"#;
    if let Some(code) = compile_and_run(src, "operand_call_assign") {
        assert_eq!(code, 42);
    }
}

#[test]
fn a_call_in_an_index_expression() {
    // gen_lvalue_addr's Index arm pushed the base pointer across the index.
    let src = r#"
int idx(int x) { return x; }
int main() { int a[3]; a[0] = 1; a[2] = 42; return a[idx(2)]; }
"#;
    if let Some(code) = compile_and_run(src, "operand_call_index") {
        assert_eq!(code, 42);
    }
}

/// Deep nesting: several outstanding spills at once, each with a call after it.
#[test]
fn several_nested_calls_across_one_expression() {
    let src = r#"
int f(int x) { return x; }
int main() { return f(1) + f(2) + f(3) + f(36); }
"#;
    if let Some(code) = compile_and_run(src, "operand_call_deep") {
        assert_eq!(code, 42, "1 + 2 + 3 + 36 = 42");
    }
}
