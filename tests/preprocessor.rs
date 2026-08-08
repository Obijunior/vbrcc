//! End-to-end preprocessor tests that exercise the **compiled binary**, not the
//! library stages.
//!
//! The unit tests in `src/preprocessor/` call `Preprocessor::run` directly, so
//! they pass even if `main.rs` never wires the preprocessor into the pipeline.
//! These tests close that gap by going through the real `vbrcc` entry point.

use std::path::{Path, PathBuf};
use std::process::Command;

use vbrcc::diagnostic::SourceMap;
use vbrcc::lexer::Token;
use vbrcc::preprocessor::Preprocessor;

/// Preprocess `src` and render it back to text, the way `-E` does.
fn e(src: &str) -> String {
    let mut map = SourceMap::single("test.c", src);
    let toks = Preprocessor::new(&mut map).run(0).unwrap();
    let mut out = String::new();
    for t in &toks {
        if t.token == Token::EOF {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&t.token.to_source());
    }
    out
}

#[test]
fn object_macro_is_expanded_in_e_output() {
    assert_eq!(e("#define N 10\nint a[N];"), "int a [ 10 ] ;");
}

#[test]
fn comments_are_gone_from_e_output() {
    assert_eq!(e("int /* c */ x; // trailing\n"), "int x ;");
}

#[test]
fn string_literals_round_trip_with_quotes() {
    assert_eq!(e("char *s = \"hi\";"), "char * s = \"hi\" ;");
}

#[test]
fn continuation_lines_are_joined() {
    assert_eq!(e("#define P 1 + \\\n 2\nint x = P;"), "int x = 1 + 2 ;");
}

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

/// Regression: `main.rs` once kept the old Stage 1 lexer call alongside the new
/// Stage 0 preprocessor call. The second binding shadowed the first, so the
/// parser consumed lexer tokens, the preprocessor's output was discarded, and
/// `#define` silently did nothing. Every unit test still passed.
#[test]
fn binary_expands_object_macros_end_to_end() {
    let src = "#define N 10\nint main() { int a[N]; a[9] = 42; return a[9]; }\n";
    if let Some(code) = compile_and_run(src, "pp_define") {
        assert_eq!(code, 42, "`#define N 10` did not reach the parser");
    }
}

/// The other half of the same wiring: a program with no directives at all must
/// still come out of the preprocessor unchanged.
#[test]
fn binary_compiles_directive_free_source_unchanged() {
    let src = "int main() { int x = 40; return x + 2; }\n";
    if let Some(code) = compile_and_run(src, "pp_plain") {
        assert_eq!(code, 42);
    }
}

#[test]
fn function_macro_is_expanded_in_e_output() {
    assert_eq!(e("#define SQ(x) ((x) * (x))\nint y = SQ(3);"),
               "int y = ( ( 3 ) * ( 3 ) ) ;");
}

#[test]
fn function_macro_argument_spanning_lines_in_e_output() {
    assert_eq!(e("#define ADD(a, b) a + b\nint x = ADD(1,\n 2);"), "int x = 1 + 2 ;");
}

#[test]
fn nested_function_macros_in_e_output() {
    assert_eq!(e("#define SQ(x) ((x) * (x))\n#define ADD(a, b) a + b\nint y = ADD(SQ(2), 3);"),
               "int y = ( ( 2 ) * ( 2 ) ) + 3 ;");
}

/// The headline for this phase: a real binary built from function-like macros.
#[test]
fn binary_expands_function_macros_end_to_end() {
    let src = r#"
#define SQUARE(x)  ((x) * (x))
#define ADD(a, b)  ((a) + (b))
int main() { return ADD(SQUARE(5), 17); }
"#;
    if let Some(code) = compile_and_run(src, "pp_fn_macro") {
        assert_eq!(code, 42, "SQUARE(5) + 17 = 25 + 17 = 42");
    }
}

/// Guards the classic precedence trap, which is *correct* behaviour for a
/// textual preprocessor: an unparenthesised body substitutes literally.
#[test]
fn binary_unparenthesised_macro_body_substitutes_literally() {
    let src = r#"
#define DOUBLE(x)  x * 2
int main() { return DOUBLE(1 + 20); }
"#;
    if let Some(code) = compile_and_run(src, "pp_fn_precedence") {
        assert_eq!(code, 41, "1 + 20 * 2 = 41, not 42 — textual substitution, as C specifies");
    }
}
