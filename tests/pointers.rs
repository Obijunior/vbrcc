mod common;
use common::compile_and_run;

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