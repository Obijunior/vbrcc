//! Execution tests for statement forms and control flow: blocks, the empty
//! statement, optional `for` clauses, `return;`, `break`, `continue`, and
//! `do`-`while`.
//!
//! A control-flow bug often hangs instead of failing, so each run has a time limit.

mod common;
use common::compile_and_run;

fn expect(src: &str, base: &str, want: i32) {
    if let Some(code) = compile_and_run(src, base) {
        assert_eq!(code, want, "{base}");
    }
}

// ---- Phase 0: statement forms -------------------------------------------------

#[test]
fn a_bare_block_runs_in_place() {
    expect("int main() { int x = 1; { x = x + 1; { x = x * 10; } } return x; }", "cf_block", 20);
}

#[test]
fn empty_statements_do_nothing() {
    expect("int main() { int x = 4; ; ; if (x) ; return x; }", "cf_empty", 4);
}

#[test]
fn a_while_with_an_empty_body() {
    expect("int main() { int i = 0; while (i++ < 5) ; return i; }", "cf_while_empty", 6);
}

#[test]
fn a_for_with_no_clauses_leaves_by_return() {
    expect("int main() { int i = 0; for (;;) { i++; if (i == 5) return i; } return 0; }", "cf_for_none", 5);
}

#[test]
fn a_for_with_no_condition() {
    expect("int main() { for (int i = 0; ; i++) { if (i == 3) return i; } return 0; }", "cf_for_nocond", 3);
}

#[test]
fn a_for_with_no_update() {
    expect("int main() { int i = 0; for (; i < 7;) { i = i + 2; } return i; }", "cf_for_noupdate", 8);
}

#[test]
fn a_bare_return_leaves_a_void_function() {
    expect(
        "void set(int *p) { *p = 7; return; *p = 9; } int main() { int x = 0; set(&x); return x; }",
        "cf_return_void",
        7,
    );
}

// ---- Phase 1: break and continue ----------------------------------------------

#[test]
fn break_leaves_a_while_loop() {
    expect("int main() { int i = 0; while (1) { i++; if (i == 9) break; } return i; }", "cf_break_while", 9);
}

#[test]
fn break_leaves_a_for_with_no_clauses() {
    expect("int main() { int i = 0; for (;;) { if (i == 4) break; i++; } return i; }", "cf_break_forever", 4);
}

#[test]
fn break_leaves_only_the_inner_loop() {
    expect(
        "int main() { int n = 0; for (int i = 0; i < 3; i++) { \
         for (int j = 0; j < 10; j++) { if (j == 2) break; n++; } } return n; }",
        "cf_break_inner",
        6,
    );
}

/// `continue` in a `for` must run the update. A jump to the loop start skips
/// `i++` and never ends.
#[test]
fn continue_in_a_for_runs_the_update() {
    expect(
        "int main() { int s = 0; for (int i = 0; i < 10; i++) { if (i % 2 == 0) continue; s += i; } return s; }",
        "cf_continue_for",
        25,
    );
}

#[test]
fn continue_in_a_for_with_no_update() {
    expect("int main() { int i = 0; for (; i < 5;) { i++; continue; } return i; }", "cf_continue_noupdate", 5);
}

#[test]
fn continue_in_a_while_tests_the_condition_again() {
    expect(
        "int main() { int i = 0; int s = 0; while (i < 10) { i++; if (i % 2) continue; s += i; } return s; }",
        "cf_continue_while",
        30,
    );
}

#[test]
fn continue_goes_to_the_inner_loop() {
    expect(
        "int main() { int n = 0; for (int i = 0; i < 3; i++) { \
         for (int j = 0; j < 4; j++) { if (j == 1) continue; n++; } } return n; }",
        "cf_continue_inner",
        9,
    );
}

// ---- Phase 2: do-while --------------------------------------------------------

#[test]
fn a_do_while_body_runs_once_when_the_condition_is_false() {
    expect("int main() { int n = 0; do { n++; } while (0); return n; }", "cf_do_once", 1);
}

#[test]
fn a_do_while_with_a_single_statement_body() {
    expect("int main() { int i = 0; do i++; while (i < 5); return i; }", "cf_do_single", 5);
}

#[test]
fn break_leaves_a_do_while() {
    expect("int main() { int i = 0; do { i++; if (i == 3) break; } while (1); return i; }", "cf_do_break", 3);
}

/// `continue` in a `do`-`while` goes to the condition, not the top of the body.
#[test]
fn continue_in_a_do_while_tests_the_condition() {
    expect(
        "int main() { int i = 0; int s = 0; do { i++; if (i % 2) continue; s += i; } while (i < 6); return s; }",
        "cf_do_continue",
        12,
    );
}
