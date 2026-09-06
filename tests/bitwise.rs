//! End-to-end tests for bitwise operators, shifts, compound bitwise
//! assignment, and the ternary conditional.
//!
//! Each test compiles a C string through the default PE backend and runs
//! the executable. On a host that cannot run a PE (not Windows, no wine)
//! the run is skipped and the test passes.

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

fn expect(body: &str, want: i32, base: &str) {
    let src = format!("int main() {{ {body} }}");
    match compile_and_run(&src, base) {
        Some(code) => assert_eq!(code, want, "for `{body}`"),
        None => {}
    }
}

#[test]
fn bitwise_operators() {
    expect("return 6 & 3;", 2, "bw_and");
    expect("return 5 | 2;", 7, "bw_or");
    expect("return 5 ^ 1;", 4, "bw_xor");
    expect("return 1 << 4;", 16, "bw_shl");
    expect("return -8 >> 1;", -4, "bw_sar"); // arithmetic shift
    expect("return ~0;", -1, "bw_not");      // unary ~ (already supported; regression)
}

#[test]
fn bitwise_precedence() {
    expect("return 1 | 2 & 3;", 3, "bw_prec1");      // 1 | (2 & 3)
    expect("return (1 + 2) << 1;", 6, "bw_prec2");
    expect("return 1 + 2 << 1;", 6, "bw_prec3");     // (1 + 2) << 1
    expect("return 4 & 6 == 6;", 0, "bw_prec4");     // 4 & (6 == 6) == 4 & 1
}

#[test]
fn compound_bitwise_assignment() {
    expect("int x = 12; x &= 10; return x;", 8, "bw_ce_and");
    expect("int x = 12; x |= 1; return x;", 13, "bw_ce_or");
    expect("int x = 12; x ^= 5; return x;", 9, "bw_ce_xor");
    expect("int x = 3; x <<= 3; return x;", 24, "bw_ce_shl");
    expect("int x = 40; x >>= 2; return x;", 10, "bw_ce_shr");
}

#[test]
fn ternary_selects_a_branch() {
    expect("return 1 ? 42 : 99;", 42, "tern_true");
    expect("return 0 ? 42 : 99;", 99, "tern_false");
    expect("return 0 ? 1 : 1 ? 2 : 3;", 2, "tern_chain"); // right-assoc
}

#[test]
fn ternary_does_not_evaluate_the_untaken_branch() {
    // Only the taken branch runs its side effect.
    expect("int a = 0; int b = 1 ? (a = 5) : (a = 9); return a + b;", 10, "tern_side");
}

#[test]
fn ternary_and_bitwise_in_a_global_initializer() {
    let src = "int pick = 1 ? 7 : 8;\nint flags = (1 << 0) | (1 << 2);\n\
               int main() { return pick + flags; }"; // 7 + 5
    match compile_and_run(src, "tern_global") {
        Some(code) => assert_eq!(code, 12),
        None => {}
    }
}
