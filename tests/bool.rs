//! Regression tests for `_Bool` and `<stdbool.h>`.
//!
//! C99 6.3.1.2: any nonzero value stored through a `_Bool` lvalue becomes exactly `1`.
//! A store that only truncated to the low byte would still pass a value like `5`
//! (`5 & 0xFF == 5`), so these tests use `256` to catch that specific failure mode:
//! its low byte is `0`, so a naive truncating store reads back as false.

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
fn a_small_nonzero_initializer_normalizes_to_one() {
    let src = "int main() { _Bool b = 5; return b; }";
    if let Some(code) = compile_and_run(src, "bool_init_small") {
        assert_eq!(code, 1);
    }
}

#[test]
fn an_initializer_whose_low_byte_is_zero_still_normalizes_to_one() {
    // A byte-truncating store would read this back as 0. It must read back as 1.
    let src = "int main() { _Bool b = 256; return b; }";
    if let Some(code) = compile_and_run(src, "bool_init_low_byte_zero") {
        assert_eq!(code, 1, "256 is nonzero and must normalize to 1, not truncate to 0");
    }
}

#[test]
fn a_zero_initializer_stays_zero() {
    let src = "int main() { _Bool b = 0; return b; }";
    if let Some(code) = compile_and_run(src, "bool_init_zero") {
        assert_eq!(code, 0);
    }
}

#[test]
fn assignment_normalizes_the_same_as_initialization() {
    let src = "int main() { _Bool b = 0; b = 42; return b; }";
    if let Some(code) = compile_and_run(src, "bool_assign") {
        assert_eq!(code, 1);
    }
}

#[test]
fn assignment_of_a_value_with_a_zero_low_byte_still_normalizes_to_one() {
    let src = "int main() { _Bool b = 0; b = 256; return b; }";
    if let Some(code) = compile_and_run(src, "bool_assign_low_byte_zero") {
        assert_eq!(code, 1);
    }
}

#[test]
fn plain_int_assignment_is_unaffected_by_bool_normalization() {
    // Regression guard: normalize_bool() must be gated on Type::Bool. An earlier
    // version applied it to every assignment, which collapsed every int to 0/1.
    let src = "int main() { int x = 42; return x; }";
    if let Some(code) = compile_and_run(src, "bool_does_not_leak_into_int") {
        assert_eq!(code, 42);
    }
}

#[test]
fn stdbool_header_bool_true_false_compile_and_run() {
    let src = r#"
        #include <stdbool.h>
        int main() {
            bool ok = true;
            if (ok) {
                return 1;
            }
            return 0;
        }
    "#;
    if let Some(code) = compile_and_run(src, "bool_stdbool_true") {
        assert_eq!(code, 1);
    }
}

#[test]
fn stdbool_header_false_is_zero() {
    let src = r#"
        #include <stdbool.h>
        int main() {
            bool ok = false;
            return ok;
        }
    "#;
    if let Some(code) = compile_and_run(src, "bool_stdbool_false") {
        assert_eq!(code, 0);
    }
}
