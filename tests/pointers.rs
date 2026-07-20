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
fn pointer_roundtrip_returns_42() {
    let src = "int main() { int x = 0; int *p = &x; *p = 42; return x; }";
    if let Some(code) = compile_and_run(src, "ptr_roundtrip") {
        assert_eq!(code, 42);
    }
}

#[test]
fn array_index_returns_stored_value() {
    let src = "int main() { int a[3]; a[0] = 10; a[1] = 20; a[2] = 12; return a[0] + a[1] + a[2]; }";
    if let Some(code) = compile_and_run(src, "array_index") {
        assert_eq!(code, 42);
    }
}