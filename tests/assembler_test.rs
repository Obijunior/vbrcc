use std::fs::{self, File};
use std::io::Write;
use std::process::Command;

// Test 1: Basic assembler invocation (run the assembler subcrate via cargo)
#[test]
fn test_assembler_basic_invocation() -> Result<(), Box<dyn std::error::Error>> {
    let asm = ".intel_syntax noprefix\n.globl main\nmain:\n  mov rax, 42\n  ret\n";
    let mut asm_path = std::env::temp_dir();
    asm_path.push("test_asm_basic.s");
    let mut out_obj = std::env::temp_dir();
    out_obj.push("test_asm_basic.o");

    File::create(&asm_path)?.write_all(asm.as_bytes())?;

    // Invoke the assembler subcrate the same way the project does.
    let status = Command::new("cargo")
        .args(&[
            "run",
            "--manifest-path",
            "src/assembler/Cargo.toml",
            "--",
            asm_path.to_str().unwrap(),
            out_obj.to_str().unwrap(),
        ])
        .status()?;

    if !status.success() {
        return Err(format!("assembler process failed with status: {}", status).into());
    }

    let meta = fs::metadata(&out_obj)?;
    assert!(meta.len() > 0, "object file should be non-empty");
    Ok(())
}

// Test 2: Use system `gcc` to assemble the same intel-syntax file (skip if gcc missing)
#[test]
fn test_assemble_with_gcc() -> Result<(), Box<dyn std::error::Error>> {
    // check gcc availability
    if Command::new("gcc").arg("--version").output().is_err() {
        eprintln!("skipping test_assemble_with_gcc: gcc not found");
        return Ok(());
    }

    let asm = ".intel_syntax noprefix\n.globl main\nmain:\n  mov rax, 42\n  ret\n";
    let mut asm_path = std::env::temp_dir();
    asm_path.push("test_gcc_asm.s");
    let mut out_obj = std::env::temp_dir();
    out_obj.push("test_gcc_asm.o");

    File::create(&asm_path)?.write_all(asm.as_bytes())?;

    let status = Command::new("gcc")
        .args(&["-c", asm_path.to_str().unwrap(), "-o", out_obj.to_str().unwrap()])
        .status()?;

    if !status.success() {
        return Err(format!("gcc failed to assemble: {}", status).into());
    }

    let meta = fs::metadata(&out_obj)?;
    assert!(meta.len() > 0, "gcc produced object file should be non-empty");
    Ok(())
}

// Test 3: Full pipeline: run the main crate to compile a tiny C program to a binary.
#[test]
fn test_full_pipeline_c_to_executable() -> Result<(), Box<dyn std::error::Error>> {
    let c_src = r#"int main() { return 42; }"#;
    let mut c_path = std::env::temp_dir();
    c_path.push("test_pipeline_main.c");
    let mut out_base = std::env::temp_dir();
    out_base.push("test_pipeline_output");

    File::create(&c_path)?.write_all(c_src.as_bytes())?;

    let status = Command::new("cargo")
        .args(&["run", "--", c_path.to_str().unwrap(), "-o", out_base.to_str().unwrap()])
        .status()?;

    if !status.success() {
        return Err(format!("full pipeline (cargo run) failed: {}", status).into());
    }

    // On Windows the produced binary may have .exe
    let mut exe_path = out_base.clone();
    exe_path.set_extension("exe");
    let candidate = if exe_path.exists() { exe_path } else { out_base };

    let meta = fs::metadata(&candidate)?;
    assert!(meta.len() > 0, "final binary should be non-empty");

    Ok(())
}