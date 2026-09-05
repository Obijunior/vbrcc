use std::path::{Path, PathBuf};
use std::process::Command;

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

#[test]
fn struct_member_write_then_read() {
    let src = r#"
struct Point { int x; int y; };
int main() {
    struct Point p;
    p.x = 11;
    p.y = 31;
    return p.x + p.y; /* 42 */
}
"#;
    match compile_and_run(src, "struct_member_rw") {
        Some(code) => assert_eq!(code, 42),
        None => {}
    }
}

#[test]
fn struct_with_char_then_int_offsets() {
    let src = r#"
struct S { char c; int n; };
int main() {
    struct S s;
    s.c = 7;
    s.n = 100;
    return s.c + s.n; /* 107, proves n is at offset 4 not 1 */
}
"#;
    match compile_and_run(src, "struct_char_int") {
        Some(code) => assert_eq!(code, 107),
        None => {}
    }
}

#[test]
fn nested_struct_and_array_member() {
    let src = r#"
struct Inner { int a; int b; };
struct Outer { struct Inner in; int tail[3]; };
int main() {
    struct Outer o;
    o.in.a = 1; o.in.b = 2;
    o.tail[0] = 10; o.tail[2] = 30;
    return o.in.a + o.in.b + o.tail[0] + o.tail[2]; /* 43 */
}
"#;
    match compile_and_run(src, "struct_nested") {
        Some(code) => assert_eq!(code, 43),
        None => {}
    }
}

#[test]
fn whole_struct_assignment_copies_all_fields() {
    let src = r#"
struct P { int x; int y; int z; };
int main() {
    struct P a;
    a.x = 1; a.y = 2; a.z = 3;
    struct P b;
    b = a;
    a.x = 99;                 /* must not affect b */
    return b.x + b.y + b.z;   /* 6 */
}
"#;
    match compile_and_run(src, "struct_copy_assign") {
        Some(code) => assert_eq!(code, 6),
        None => {}
    }
}

#[test]
fn struct_copy_on_initialization() {
    let src = r#"
struct P { int x; int y; };
int main() {
    struct P a;
    a.x = 4; a.y = 5;
    struct P b = a;
    return b.x + b.y; /* 9 */
}
"#;
    match compile_and_run(src, "struct_copy_init") {
        Some(code) => assert_eq!(code, 9),
        None => {}
    }
}

// ---- Task 7: pass by value ------------------------------------------------

#[test]
fn pass_small_struct_by_value() {
    let src = r#"
struct Pair { int a; int b; };   /* 8 bytes -> register */
int sum(struct Pair p) { return p.a + p.b; }
int main() {
    struct Pair p;
    p.a = 20; p.b = 22;
    return sum(p); /* 42 */
}
"#;
    match compile_and_run(src, "struct_pass_small") {
        Some(code) => assert_eq!(code, 42),
        None => {}
    }
}

#[test]
fn pass_large_struct_by_value_is_a_caller_copy() {
    let src = r#"
struct Big { int a; int b; int c; int d; };  /* 16 bytes -> memory */
int total(struct Big b) {
    b.a = 0;                 /* mutate the callee copy */
    return b.b + b.c + b.d;
}
int main() {
    struct Big x;
    x.a = 1; x.b = 2; x.c = 3; x.d = 4;
    int t = total(x);
    return t + x.a;          /* 9 + 1 = 10; x.a unchanged proves a copy */
}
"#;
    match compile_and_run(src, "struct_pass_big") {
        Some(code) => assert_eq!(code, 10),
        None => {}
    }
}

// ---- Task 8: return by value --------------------------------------------

#[test]
fn return_small_struct_by_value() {
    let src = r#"
struct Pair { int a; int b; };
struct Pair make(int x) { struct Pair p; p.a = x; p.b = x + 1; return p; }
int main() {
    struct Pair p = make(20);
    return p.a + p.b; /* 41 */
}
"#;
    match compile_and_run(src, "struct_ret_small") {
        Some(code) => assert_eq!(code, 41),
        None => {}
    }
}

#[test]
fn return_large_struct_by_value() {
    let src = r#"
struct Quad { int a; int b; int c; int d; };
struct Quad make(int base) {
    struct Quad q;
    q.a = base; q.b = base + 1; q.c = base + 2; q.d = base + 3;
    return q;
}
int main() {
    struct Quad q = make(10);
    return q.a + q.b + q.c + q.d; /* 46 */
}
"#;
    match compile_and_run(src, "struct_ret_large") {
        Some(code) => assert_eq!(code, 46),
        None => {}
    }
}

#[test]
fn return_large_struct_then_pass_an_argument() {
    let src = r#"
struct Quad { int a; int b; int c; int d; };
struct Quad shift(struct Quad q, int by) {
    q.a = q.a + by; q.b = q.b + by; q.c = q.c + by; q.d = q.d + by;
    return q;
}
int main() {
    struct Quad q; q.a = 1; q.b = 2; q.c = 3; q.d = 4;
    struct Quad r = shift(q, 10);
    return r.a + r.b + r.c + r.d; /* 50 */
}
"#;
    match compile_and_run(src, "struct_ret_large_arg") {
        Some(code) => assert_eq!(code, 50),
        None => {}
    }
}

// ---- Task 9: struct globals ------------------------------------------------

#[test]
fn zero_initialized_struct_global() {
    let src = r#"
struct P { int x; int y; };
struct P g;
int main() {
    g.x = 5; g.y = 9;
    return g.x + g.y; /* 14 */
}
"#;
    match compile_and_run(src, "struct_global_zero") {
        Some(code) => assert_eq!(code, 14),
        None => {}
    }
}
