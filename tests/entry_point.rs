//! Regression test for the PE entry-point bug.
//!
//! The custom PE writer once hardcoded the entry point to the start of `.text` — i.e.
//! whichever function the code generator emitted first — instead of `main`. Every
//! example and unit test happened to define `main` first, so the bug stayed invisible
//! while 151 tests passed. This test defines a helper *before* `main`: if the entry
//! point regresses, the process runs `helper` (returning 7) instead of `main`
//! (returning 42), and the exit code catches it.
//!
//! The bug is only observable at runtime — the assembly for both functions is
//! identical, so only running the binary and checking its exit code detects it.

mod common;
use common::compile_and_run;

#[test]
fn entry_point_is_main_not_first_function() {
    // `helper` is defined first; a correct entry point still runs `main`.
    let src = "int helper() { return 7; } int main() { return 42; }";
    if let Some(code) = compile_and_run(src, "entry_point_regression") {
        assert_eq!(code, 42, "entry point ran the wrong function (got {code}, want 42)");
    }
}
