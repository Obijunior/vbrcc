//! Runs every C program in `tests/c/` and checks it against its expected result.
//!
//! Each program starts with `// expect: N`, the exit code. An optional
//! `// expect-stdout: text` gives the output, with `\n` for a newline. When native
//! `gcc` is available, the program also compiles with `gcc`, and both compilers must
//! agree with the expectation. So a wrong expectation fails too. The program also
//! builds with `vbrcc --gcc`, which checks that the generated assembly is valid for
//! GNU `as`.
//!
//! To add coverage, add a `.c` file. No Rust changes.
//!
//! `tests/c/known_bugs/` holds one program for each open miscompile. Those must still
//! fail. When a fix makes one pass, move its file up to `tests/c/`.

mod common;
use common::{compile_with_gcc, gcc_available, run, try_compile_with, Run};

use std::path::{Path, PathBuf};

struct Expect {
    code: i32,
    stdout: Option<String>,
}

fn parse_expect(src: &str, path: &Path) -> Expect {
    let mut code = None;
    let mut stdout = None;
    for line in src.lines().take_while(|l| l.starts_with("//")) {
        if let Some(v) = line.strip_prefix("// expect: ") {
            code = Some(v.trim().parse().unwrap());
        } else if let Some(v) = line.strip_prefix("// expect-stdout: ") {
            stdout = Some(v.replace("\\n", "\n"));
        }
    }
    let code = code.unwrap_or_else(|| panic!("{} has no `// expect: N` line", path.display()));
    Expect { code, stdout }
}

/// `None` when the host cannot run a PE. Otherwise the reason vbrcc got it wrong,
/// or `Ok`. Panics when `gcc` disagrees with the expectation: that is a bad test.
fn check(path: &Path) -> Option<Result<(), String>> {
    let src = std::fs::read_to_string(path).unwrap();
    let want = parse_expect(&src, path);
    let base = format!("cprog_{}", path.file_stem().unwrap().to_str().unwrap());

    if gcc_available() {
        let r = run(&compile_with_gcc(&src, &format!("{base}_gcc")), &base).unwrap();
        assert_eq!(r.code, want.code, "{}: gcc disagrees with `// expect:`", path.display());
        if let Some(out) = &want.stdout {
            assert_eq!(&r.stdout, out, "{}: gcc disagrees with `// expect-stdout:`", path.display());
        }
    }

    let mut backends: Vec<&[&str]> = vec![&[]];
    if gcc_available() {
        backends.push(&["--gcc"]);
    }
    for flags in backends {
        let label = if flags.is_empty() { "default backend" } else { "--gcc backend" };
        let tag = if flags.is_empty() { base.clone() } else { format!("{base}_gccmode") };
        let exe = match try_compile_with(&src, &tag, flags) {
            Ok(exe) => exe,
            Err(stderr) => {
                let first = stderr.lines().next().unwrap_or("");
                return Some(Err(format!("{label}: compile failed: {first}")));
            }
        };
        if let Err(why) = compare(&run(&exe, &tag)?, &want) {
            return Some(Err(format!("{label}: {why}")));
        }
    }
    Some(Ok(()))
}

fn compare(r: &Run, want: &Expect) -> Result<(), String> {
    if r.code != want.code {
        return Err(format!("exit code {}, expected {}", r.code, want.code));
    }
    match &want.stdout {
        Some(out) if &r.stdout != out => Err(format!("stdout {:?}, expected {:?}", r.stdout, out)),
        _ => Ok(()),
    }
}

fn programs(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "c"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no programs found");
    files
}

/// Check every program on its own thread. Returns (file name, result) pairs.
fn check_all(dir: &str) -> Vec<(String, Option<Result<(), String>>)> {
    let files = programs(dir);
    std::thread::scope(|s| {
        let handles: Vec<_> = files.iter().map(|p| s.spawn(move || check(p))).collect();
        files
            .iter()
            .zip(handles)
            .map(|(p, h)| (p.file_name().unwrap().to_string_lossy().into_owned(), h.join().unwrap()))
            .collect()
    })
}

#[test]
fn c_programs_match_their_expectations() {
    let failures: Vec<String> = check_all("c")
        .into_iter()
        .filter_map(|(name, r)| match r {
            Some(Err(why)) => Some(format!("  {name}: {why}")),
            _ => None,
        })
        .collect();
    assert!(failures.is_empty(), "miscompiled:\n{}", failures.join("\n"));
}

#[test]
fn known_bugs_still_fail() {
    let fixed: Vec<String> = check_all("c/known_bugs")
        .into_iter()
        .filter_map(|(name, r)| matches!(r, Some(Ok(()))).then_some(format!("  {name}")))
        .collect();
    assert!(
        fixed.is_empty(),
        "these now pass. Move each one to tests/c/:\n{}",
        fixed.join("\n")
    );
}
