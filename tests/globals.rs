//! End-to-end tests for file-scope variables and static initializers.
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

fn compile_and_capture(src: &str, base: &str) -> Option<String> {
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
fn initialized_global_scalar_is_returned() {
    if let Some(code) = compile_and_run("int counter = 42; int main() { return counter; }", "glob_scalar_init") {
        assert_eq!(code, 42);
    }
}

#[test]
fn uninitialized_global_is_zero() {
    if let Some(code) = compile_and_run("int g; int main() { return g; }", "glob_zero") {
        assert_eq!(code, 0);
    }
}

#[test]
fn a_global_can_be_written_then_read() {
    if let Some(code) = compile_and_run("int g; int main() { g = 7; return g; }", "glob_write") {
        assert_eq!(code, 7);
    }
}

#[test]
fn a_char_global_holds_its_value() {
    if let Some(code) = compile_and_run("char c = 'A'; int main() { return c; }", "glob_char") {
        assert_eq!(code, 65);
    }
}

#[test]
fn a_long_global_holds_its_value() {
    if let Some(code) = compile_and_run("long n = 100; int main() { return (int)n; }", "glob_long") {
        assert_eq!(code, 100);
    }
}

#[test]
fn a_constant_expression_initializer_is_folded() {
    if let Some(code) = compile_and_run("int x = 2 + 3 * 4; int main() { return x; }", "glob_constexpr") {
        assert_eq!(code, 14);
    }
}

#[test]
fn a_nonzero_bool_global_normalizes_to_one() {
    if let Some(code) = compile_and_run("_Bool b = 5; int main() { return b; }", "glob_bool") {
        assert_eq!(code, 1);
    }
}

#[test]
fn a_string_array_global_copies_the_literal() {
    // 'h' (104) + 'i' (105) = 209
    if let Some(code) = compile_and_run("char s[] = \"hi\"; int main() { return s[0] + s[1]; }", "glob_strarr") {
        assert_eq!(code, 209);
    }
}

#[test]
fn an_uninitialized_global_array_is_usable() {
    if let Some(code) = compile_and_run("int a[4]; int main() { a[2] = 9; return a[2]; }", "glob_arr_zero") {
        assert_eq!(code, 9);
    }
}

#[test]
fn two_globals_keep_separate_storage() {
    if let Some(code) = compile_and_run("int a = 10; int b = 20; int main() { return a + b; }", "glob_two") {
        assert_eq!(code, 30);
    }
}

#[test]
fn a_string_array_global_prints() {
    let src = "int printf(const char *f, ...); char s[] = \"hello\"; \
               int main() { printf(\"%s\\n\", s); return 0; }";
    if let Some(out) = compile_and_capture(src, "glob_printf") {
        assert_eq!(out.trim_end(), "hello");
    }
}
