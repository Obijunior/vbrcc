//! End-to-end tests for brace initializers (`int a[3] = {1, 2, 3};`).
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

    let status = Command::new(env!("CARGO_BIN_EXE_vbrcc"))
        .args([c_path.to_str().unwrap(), "-o", out_base.to_str().unwrap()])
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

fn compile_and_capture(src: &str, base: &str) -> Option<String> {
    let mut c_path = std::env::temp_dir();
    c_path.push(format!("{base}.c"));
    let mut out_base = std::env::temp_dir();
    out_base.push(base);
    std::fs::write(&c_path, src).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_vbrcc"))
        .args([c_path.to_str().unwrap(), "-o", out_base.to_str().unwrap()])
        .status()
        .unwrap();
    if !status.success() {
        panic!("compile failed for {base}");
    }

    let mut exe = out_base.clone();
    exe.set_extension("exe");
    let exe: PathBuf = if exe.exists() { exe } else { out_base };

    if cfg!(target_os = "windows") {
        let out = Command::new(&exe).output().unwrap();
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else if Command::new("wine").arg("--version").output().is_ok() {
        let out = Command::new("wine").arg(&exe).output().unwrap();
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        eprintln!("skipping run: no PE runner (not Windows, no wine)");
        None
    }
}

#[test]
fn local_flat_initializer_sums() {
    let src = r#"
int main() {
    int a[3] = {10, 20, 30};
    return a[0] + a[1] + a[2];
}
"#;
    if let Some(code) = compile_and_run(src, "init_local_flat") {
        assert_eq!(code, 60);
    }
}

/// C zero-fills every element the list leaves out.
#[test]
fn local_partial_initializer_zero_fills() {
    let src = r#"
int main() {
    int a[4] = {7};
    return a[0] + a[1] + a[2] + a[3];
}
"#;
    if let Some(code) = compile_and_run(src, "init_local_partial") {
        assert_eq!(code, 7);
    }
}

#[test]
fn local_nested_initializer_is_row_major() {
    let src = r#"
int main() {
    int a[2][3] = {{1, 2, 3}, {4, 5, 6}};
    return a[0][0] + a[1][2];
}
"#;
    if let Some(code) = compile_and_run(src, "init_local_nested") {
        assert_eq!(code, 7);
    }
}

/// A short inner list zero-fills its own row, not the tail of the array.
#[test]
fn local_nested_partial_rows_zero_fill_per_row() {
    let src = r#"
int main() {
    int a[3][3] = {{1}, {2, 2}};
    int sum = 0;
    int *p = &a[0][0];
    for (int i = 0; i < 9; i++) sum = sum + p[i];
    return sum;
}
"#;
    if let Some(code) = compile_and_run(src, "init_local_nested_partial") {
        assert_eq!(code, 5);
    }
}

/// A `char` array zero-fills a tail that is not a multiple of 8 bytes, which
/// is the case the store-width choice has to get right.
#[test]
fn local_char_array_partial_initializer_zero_fills() {
    let src = r#"
int main() {
    char a[7] = {1, 2, 3};
    int sum = 0;
    for (int i = 0; i < 7; i++) sum = sum + a[i];
    return sum;
}
"#;
    if let Some(code) = compile_and_run(src, "init_local_char_partial") {
        assert_eq!(code, 6);
    }
}

/// The initializer must not disturb a neighbouring frame slot.
#[test]
fn zero_fill_stays_inside_the_array() {
    let src = r#"
int main() {
    int guard = 42;
    int a[3] = {1};
    return guard + a[0];
}
"#;
    if let Some(code) = compile_and_run(src, "init_local_guard") {
        assert_eq!(code, 43);
    }
}

/// An element may be any expression, not only a literal.
#[test]
fn local_initializer_elements_may_be_expressions() {
    let src = r#"
int main() {
    int n = 5;
    int a[3] = {n, n * 2, n + 1};
    return a[0] + a[1] + a[2];
}
"#;
    if let Some(code) = compile_and_run(src, "init_local_expr") {
        assert_eq!(code, 21);
    }
}

#[test]
fn global_flat_initializer_sums() {
    let src = r#"
int g[3] = {7, 8, 9};
int main() { return g[0] + g[1] + g[2]; }
"#;
    if let Some(code) = compile_and_run(src, "init_global_flat") {
        assert_eq!(code, 24);
    }
}

#[test]
fn global_nested_initializer_indexes() {
    let src = r#"
int g[2][2] = {{1, 2}, {3, 4}};
int main() { return g[0][1] + g[1][0] + g[1][1]; }
"#;
    if let Some(code) = compile_and_run(src, "init_global_nested") {
        assert_eq!(code, 9);
    }
}

#[test]
fn global_partial_initializer_zero_fills() {
    let src = r#"
int g[5] = {4};
int main() {
    int sum = 0;
    for (int i = 0; i < 5; i++) sum = sum + g[i];
    return sum;
}
"#;
    if let Some(code) = compile_and_run(src, "init_global_partial") {
        assert_eq!(code, 4);
    }
}

/// An unsized global takes its length from the list, so the loop bound and
/// the data must agree.
#[test]
fn global_unsized_array_length_comes_from_the_list() {
    let src = r#"
int g[] = {1, 2, 3, 4, 5};
int after = 99;
int main() {
    int sum = 0;
    for (int i = 0; i < 5; i++) sum = sum + g[i];
    return sum + after;
}
"#;
    if let Some(code) = compile_and_run(src, "init_global_unsized") {
        assert_eq!(code, 114);
    }
}

/// Every leaf of a global list must fold to a constant.
#[test]
fn global_initializer_elements_must_be_constant() {
    let src = "int n; int g[2] = {n, 1}; int main() { return 0; }";
    let mut c_path = std::env::temp_dir();
    c_path.push("init_global_nonconst.c");
    std::fs::write(&c_path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_vbrcc"))
        .args([c_path.to_str().unwrap(), "-o", "init_global_nonconst"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected a compile error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a constant"), "got: {stderr}");
}

/// The worked example from the C99 roadmap: a 2D array with a nested
/// initializer, walked through a decayed pointer.
#[test]
fn matrix_test_example_runs() {
    let src = std::fs::read_to_string("examples/matrix_test.c").unwrap();
    if let Some(out) = compile_and_capture(&src, "init_matrix_example") {
        assert_eq!(out.trim_end(), "1 2 3 4 5 6 7 8 9");
    }
}
