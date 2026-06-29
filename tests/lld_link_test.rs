// tests/lld_link_test.rs
//
// Task 6 — end-to-end integration tests for the `--lld-link` pipeline:
//   C source -> codegen -> custom assembler emits COFF .obj -> lld-link -> .exe -> run it.
//
// Unlike the assembler-level tests, these RUN the produced binary and assert on its
// behaviour (exit code / stdout), which is the only way to prove relocations and
// external-symbol resolution (printf) actually linked correctly.
//
// Every test skips cleanly (returns Ok with an eprintln) when the host can't run the
// step in question, so the suite stays green on a Linux box without lld-link/wine and
// does real work on a Windows box (or Linux + wine).

use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// ---- capability probes ------------------------------------------------------

/// Is `lld-link` on PATH? The `--lld-link` path shells out to it; skip if absent.
fn lld_link_available() -> bool {
    // lld-link is MSVC-style; `/?` prints help and exits 0. We only care that it launches.
    Command::new("lld-link").arg("/?").output().is_ok()
}

/// Returns a Command primed to execute a produced Windows PE, or None if this host can't.
/// Native on Windows; via `wine` on other OSes when available; otherwise None (skip the run).
fn runner_for(exe: &Path) -> Option<Command> {
    if cfg!(target_os = "windows") {
        Some(Command::new(exe))
    } else if Command::new("wine").arg("--version").output().is_ok() {
        let mut c = Command::new("wine");
        c.arg(exe);
        Some(c)
    } else {
        None
    }
}

// ---- compile helper ---------------------------------------------------------

/// Compile `c_src` through the main crate with the given extra flags, returning the
/// path to the produced binary. `base` is a unique stem for temp files.
fn compile(c_src: &str, base: &str, extra_flags: &[&str]) -> Result<PathBuf, Box<dyn Error>> {
    let mut c_path = std::env::temp_dir();
    c_path.push(format!("{base}.c"));
    let mut out_base = std::env::temp_dir();
    out_base.push(base);

    File::create(&c_path)?.write_all(c_src.as_bytes())?;

    let mut args: Vec<&str> = vec!["run", "--", c_path.to_str().unwrap(), "-o", out_base.to_str().unwrap()];
    args.extend_from_slice(extra_flags);

    let status = Command::new("cargo").args(&args).status()?;
    if !status.success() {
        return Err(format!("compiler (cargo run {:?}) failed: {}", extra_flags, status).into());
    }

    // No-extension output gets `.exe` in the lld-link/PE path; fall back to bare name.
    let mut exe = out_base.clone();
    exe.set_extension("exe");
    Ok(if exe.exists() { exe } else { out_base })
}

/// Run a produced binary and return its Output, or None if the host can't execute it.
fn run(exe: &Path) -> Result<Option<Output>, Box<dyn Error>> {
    match runner_for(exe) {
        Some(mut cmd) => Ok(Some(cmd.output()?)),
        None => Ok(None),
    }
}

// ---- tests ------------------------------------------------------------------

/// 1. The pipeline produces a non-empty binary via `--lld-link`.
///    (Build-only — runs even on a host that can't execute PEs.)
#[test]
fn lld_link_produces_binary() -> Result<(), Box<dyn Error>> {
    if !lld_link_available() {
        eprintln!("skipping lld_link_produces_binary: lld-link not found");
        return Ok(());
    }

    let exe = compile("int main() { return 0; }", "lld_produces", &["--lld-link"])?;
    let meta = fs::metadata(&exe)?;
    assert!(meta.len() > 0, "lld-linked binary should be non-empty");
    Ok(())
}

/// 2. `return 42` round-trips to a process exit code of 42 through the lld-link path.
///    This proves the .text bytes, entry point, and exit plumbing all survived linking.
#[test]
fn lld_link_return_value_is_exit_code() -> Result<(), Box<dyn Error>> {
    if !lld_link_available() {
        eprintln!("skipping lld_link_return_value_is_exit_code: lld-link not found");
        return Ok(());
    }

    let exe = compile("int main() { return 42; }", "lld_ret42", &["--lld-link"])?;
    let Some(output) = run(&exe)? else {
        eprintln!("skipping run assertion: no PE runner (not Windows, no wine)");
        return Ok(());
    };

    assert_eq!(output.status.code(), Some(42), "exit code should equal the C return value");
    Ok(())
}

/// 3. `printf("hello\n")` prints to stdout and exits 0 — the real payoff of the COFF
///    work: a cross-section relocation (str_0 in .data) AND an external symbol (printf
///    from the CRT) both have to resolve for this to pass.
///    NOTE: assumes the front-end accepts a bare printf call. If yours needs a
///    declaration/#include, prepend it to `c_src`.
#[test]
fn lld_link_printf_writes_stdout() -> Result<(), Box<dyn Error>> {
    if !lld_link_available() {
        eprintln!("skipping lld_link_printf_writes_stdout: lld-link not found");
        return Ok(());
    }

    let c_src = "int main() { printf(\"hello\\n\"); return 0; }";
    let exe = compile(c_src, "lld_printf", &["--lld-link"])?;
    let Some(output) = run(&exe)? else {
        eprintln!("skipping run assertion: no PE runner (not Windows, no wine)");
        return Ok(());
    };

    assert!(output.status.success(), "printf program should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Compare on trimmed content to stay agnostic to \n vs \r\n on the CRT side.
    assert_eq!(stdout.trim_end(), "hello", "stdout should be the printed string");
    Ok(())
}

/// 4. Parity across several instruction paths: each source compiled via the default
///    (custom PE) path and via `--lld-link` must behave identically — same exit code,
///    same stdout. This pins the new path to the proven one without hard-coding
///    correctness, while the trailing `expected` check also catches the case where
///    *both* paths are wrong in the same way.
///
///    Cases mirror the instruction coverage of the assembler-level tests (arithmetic,
///    idiv/cqo, branching, loop with setcc/movzx/jmp), but end-to-end through lld-link.
///    Sources assume the same C subset your codegen already emits (see assembler tests
///    3 and 4): locals, `if`, `while`, and the standard operator set/precedence.
#[test]
fn lld_link_matches_default_path() -> Result<(), Box<dyn Error>> {
    if !lld_link_available() {
        eprintln!("skipping lld_link_matches_default_path: lld-link not found");
        return Ok(());
    }

    // (stem, C source, expected exit code)
    let cases: &[(&str, &str, i32)] = &[
        ("ret",    "int main() { return 7; }",                                              7),
        // imul / sub / add — leftmost op is `*`, so this is precedence-agnostic (always 33)
        ("arith",  "int main() { return 10 * 3 - 2 + 5; }",                                 33),
        ("div",    "int main() { return 20 / 3; }",                                         6),  // idiv/cqo
        ("mod",    "int main() { return 17 % 5; }",                                         2),  // idiv -> rdx
        ("branch", "int main() { int x = 7; if (x > 5) { return 1; } return 0; }",          1),  // cmp/setcc/je
        ("loop",   "int main() { int sum = 0; int i = 0; \
                    while (i < 10) { sum = sum + i; i = i + 1; } return sum; }",            45), // loop labels/jmp
    ];

    let mut ran_any = false;
    for &(stem, src, expected) in cases {
        let default_exe = compile(src, &format!("parity_{stem}_default"), &[])?;
        let lld_exe = compile(src, &format!("parity_{stem}_lld"), &["--lld-link"])?;

        // Runner availability is the same for both binaries; skip this case's run if absent.
        let (Some(d), Some(l)) = (run(&default_exe)?, run(&lld_exe)?) else {
            eprintln!("skipping run for case '{stem}': no PE runner (not Windows, no wine)");
            continue;
        };
        ran_any = true;

        assert_eq!(
            d.status.code(), l.status.code(),
            "case '{stem}': lld-link exit code should match the default path"
        );
        assert_eq!(
            d.stdout, l.stdout,
            "case '{stem}': lld-link stdout should match the default path"
        );
        assert_eq!(
            l.status.code(), Some(expected),
            "case '{stem}': exit code should be {expected}"
        );
    }

    if !ran_any {
        eprintln!("parity: built all cases but executed none (no PE runner available)");
    }
    Ok(())
}