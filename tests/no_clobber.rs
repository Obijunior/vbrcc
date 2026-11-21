//! The driver must not delete or overwrite files it did not create.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SENTINEL: &str = "user data";

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vbrcc_no_clobber_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("clob.c"), "int main() { return 0; }\n").unwrap();
    dir
}

fn run(dir: &PathBuf, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_vbrcc"))
        .current_dir(dir)
        .arg("clob.c")
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn default_mode_keeps_user_files() {
    let dir = scratch_dir("default");
    for ext in ["s", "obj", "def", "lib"] {
        fs::write(dir.join(format!("clob.{ext}")), SENTINEL).unwrap();
    }
    let out = run(&dir, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for ext in ["s", "obj", "def", "lib"] {
        let text = fs::read_to_string(dir.join(format!("clob.{ext}"))).unwrap();
        assert_eq!(text, SENTINEL, "clob.{ext} changed");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn default_mode_writes_no_intermediate_file() {
    let dir = scratch_dir("none");
    let out = run(&dir, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!dir.join("clob.s").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn gcc_mode_refuses_to_overwrite_and_keeps_file() {
    if !common_gcc_available() {
        return;
    }
    let dir = scratch_dir("gcc");
    fs::write(dir.join("clob.s"), SENTINEL).unwrap();
    let out = run(&dir, &["--gcc"]);
    assert!(!out.status.success());
    assert_eq!(fs::read_to_string(dir.join("clob.s")).unwrap(), SENTINEL);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn gcc_mode_removes_its_own_assembly_file() {
    if !common_gcc_available() {
        return;
    }
    let dir = scratch_dir("gcc_own");
    let out = run(&dir, &["--gcc"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!dir.join("clob.s").exists());
    let _ = fs::remove_dir_all(&dir);
}

fn common_gcc_available() -> bool {
    Command::new("gcc").arg("--version").output().is_ok()
}
