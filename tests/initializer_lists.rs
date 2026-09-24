//! End-to-end tests for brace initializers (`int a[3] = {1, 2, 3};`).
//!
//! Each test compiles a C string through the default PE backend and runs
//! the executable. On a host that cannot run a PE (not Windows, no wine)
//! the run is skipped and the test passes.

mod common;
use common::{compile_and_run, compile_and_capture, compile_error};

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
    let stderr = compile_error(src, "init_global_nonconst");
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
