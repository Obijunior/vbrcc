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
