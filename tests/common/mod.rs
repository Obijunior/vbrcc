//! Helpers that the integration tests share. A test file uses them with `mod common;`.
//!
//! Each test binary compiles this module separately and uses only part of it.
#![allow(dead_code)]

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A control-flow bug often hangs instead of failing, so each run has a time limit.
const RUN_LIMIT: Duration = Duration::from_secs(10);

/// What a program did when it ran.
pub struct Run {
    pub code: i32,
    pub stdout: String,
}

/// Write `src` to a temp file and compile it with `vbrcc`. Returns the executable,
/// or the compiler's stderr.
pub fn try_compile(src: &str, base: &str) -> Result<PathBuf, String> {
    try_compile_with(src, base, &[])
}

/// Like [`try_compile`], with extra `vbrcc` flags such as `--gcc`.
pub fn try_compile_with(src: &str, base: &str, flags: &[&str]) -> Result<PathBuf, String> {
    let c_path = std::env::temp_dir().join(format!("{base}.c"));
    let out_base = std::env::temp_dir().join(base);
    std::fs::write(&c_path, src).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_vbrcc"))
        .args([c_path.to_str().unwrap(), "-o", out_base.to_str().unwrap()])
        .args(flags)
        .output()
        .unwrap();
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }

    let exe = out_base.with_extension("exe");
    Ok(if exe.exists() { exe } else { out_base })
}

/// Like [`try_compile`], but panics with the compiler's stderr on failure.
pub fn compile(src: &str, base: &str) -> PathBuf {
    try_compile(src, base).unwrap_or_else(|e| panic!("compile failed for {base}:\n{e}"))
}

/// True when native `gcc` can serve as the reference compiler. Only on Windows:
/// there `gcc` targets the same LLP64 model as vbrcc, so `long` has the same width.
pub fn gcc_available() -> bool {
    cfg!(target_os = "windows") && Command::new("gcc").arg("--version").output().is_ok()
}

/// Compile `src` with native `gcc`. Panics if `gcc` rejects it.
pub fn compile_with_gcc(src: &str, base: &str) -> PathBuf {
    let c_path = std::env::temp_dir().join(format!("{base}.c"));
    let exe = std::env::temp_dir().join(format!("{base}.exe"));
    std::fs::write(&c_path, src).unwrap();
    let out = Command::new("gcc")
        .args(["-w", c_path.to_str().unwrap(), "-o", exe.to_str().unwrap()])
        .output()
        .unwrap();
    if !out.status.success() {
        panic!("gcc rejected {base}:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    exe
}

/// Compile `src`, which must fail. Returns the compiler's stderr.
pub fn compile_error(src: &str, base: &str) -> String {
    let c_path = std::env::temp_dir().join(format!("{base}.c"));
    let out_base = std::env::temp_dir().join(base);
    std::fs::write(&c_path, src).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_vbrcc"))
        .args([c_path.to_str().unwrap(), "-o", out_base.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{base}: expected a compile error");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Run an executable. `None` means the host cannot run a PE (not Windows, no wine).
pub fn run(exe: &PathBuf, base: &str) -> Option<Run> {
    let mut cmd = if cfg!(target_os = "windows") {
        Command::new(exe)
    } else if Command::new("wine").arg("--version").output().is_ok() {
        let mut c = Command::new("wine");
        c.arg(exe);
        c
    } else {
        eprintln!("skipping run: no PE runner (not Windows, no wine)");
        return None;
    };

    let mut child = cmd.stdout(Stdio::piped()).spawn().unwrap();
    // Read stdout on a thread, so a full pipe cannot block the child.
    let mut pipe = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = pipe.read_to_string(&mut s);
        s
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            panic!("{base} did not finish in {RUN_LIMIT:?}: probably an endless loop");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // The C runtime writes `\r\n` in text mode on Windows. Compare with `\n`.
    let stdout = reader.join().unwrap().replace("\r\n", "\n");
    Some(Run { code: status.code().unwrap(), stdout })
}

/// Compile and run `src`. Returns the exit code, or `None` when the host cannot run it.
pub fn compile_and_run(src: &str, base: &str) -> Option<i32> {
    run(&compile(src, base), base).map(|r| r.code)
}

/// Compile and run `src`. Returns what it printed, or `None` when the host cannot run it.
pub fn compile_and_capture(src: &str, base: &str) -> Option<String> {
    run(&compile(src, base), base).map(|r| r.stdout)
}
