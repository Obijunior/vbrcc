//! End-to-end tests for multi-dimensional arrays.
//!
//! Each test compiles a C string through the default PE backend and runs
//! the executable. On a host that cannot run a PE (not Windows, no wine)
//! the run is skipped and the test passes.

mod common;
use common::compile_and_run;

#[test]
fn two_d_array_index_and_address() {
    let src = r#"
int main() {
    int a[2][3];
    a[0][0] = 1; a[0][1] = 2; a[0][2] = 3;
    a[1][0] = 4; a[1][1] = 5; a[1][2] = 6;
    int *p = &a[0][0];
    int sum = 0;
    for (int i = 0; i < 6; i++) sum = sum + *(p + i);
    return sum;
}
"#;
    if let Some(code) = compile_and_run(src, "nd_two_d_index") {
        assert_eq!(code, 21);
    }
}

#[test]
fn three_d_array_row_stride() {
    let src = r#"
int main() {
    int a[2][2][2];
    a[1][1][1] = 9;
    a[0][0][0] = 1;
    return a[1][1][1] + a[0][0][0];
}
"#;
    if let Some(code) = compile_and_run(src, "nd_three_d") {
        assert_eq!(code, 10);
    }
}

/// A row is a whole `int[3]`. Writing through a flat pointer at index 3 must
/// land on `a[1][0]`, which proves the outer index scales by the row size.
#[test]
fn row_stride_is_the_inner_array_size() {
    let src = r#"
int main() {
    int a[2][3];
    int *p = &a[0][0];
    p[3] = 55;
    a[0][0] = 0;
    return a[1][0];
}
"#;
    if let Some(code) = compile_and_run(src, "nd_row_stride") {
        assert_eq!(code, 55);
    }
}

/// `char` rows are 1 byte wide, so the stride must come from `Type::size`,
/// not from a hardcoded 4.
#[test]
fn two_d_char_array_uses_a_one_byte_stride() {
    let src = r#"
int main() {
    char a[2][3];
    a[0][0] = 1;
    a[1][0] = 2;
    char *p = &a[0][0];
    return p[0] + p[3];
}
"#;
    if let Some(code) = compile_and_run(src, "nd_char_stride") {
        assert_eq!(code, 3);
    }
}
