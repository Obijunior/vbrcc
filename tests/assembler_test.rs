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

// Test 3: Assembler handles arithmetic and comparison instructions
#[test]
fn test_assembler_arithmetic_instructions() -> Result<(), Box<dyn std::error::Error>> {
    let asm = r#".intel_syntax noprefix
.globl main
main:
  push rbp
  mov rbp, rsp
  mov rax, 10
  mov rcx, 3
  cmp rax, rcx
  cmp rax, 0
  neg rax
  not rcx
  cqo
  idiv rcx
  imul rax, rcx
  add rax, 5
  sub rax, 2
  pop rbp
  ret
"#;
    let mut asm_path = std::env::temp_dir();
    asm_path.push("test_asm_arithmetic.s");
    let mut out_obj = std::env::temp_dir();
    out_obj.push("test_asm_arithmetic.o");

    File::create(&asm_path)?.write_all(asm.as_bytes())?;

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
        return Err(format!("assembler failed on arithmetic instructions: {}", status).into());
    }

    let meta = fs::metadata(&out_obj)?;
    assert!(meta.len() > 0, "object file should be non-empty");
    Ok(())
}

// Test 4: Assembler handles control flow instructions (setcc, movzx, jumps, labels)
#[test]
fn test_assembler_control_flow_instructions() -> Result<(), Box<dyn std::error::Error>> {
    let asm = r#".intel_syntax noprefix
.globl main
main:
  push rbp
  mov rbp, rsp
  sub rsp, 32
  mov rax, 0
  mov [rbp - 8], rax
  mov rax, 0
  mov [rbp - 16], rax
loop_0_start:
  mov rax, [rbp - 16]
  push rax
  mov rax, 10
  mov rcx, rax
  pop rax
  cmp rax, rcx
  setl al
  movzx rax, al
  cmp rax, 0
  je loop_0_end
  mov rax, [rbp - 16]
  push rax
  mov rax, 2
  mov rcx, rax
  pop rax
  cqo
  idiv rcx
  mov rax, rdx
  push rax
  mov rax, 0
  mov rcx, rax
  pop rax
  cmp rax, rcx
  sete al
  movzx rax, al
  cmp rax, 0
  je if_1_end
  mov rax, [rbp - 8]
  push rax
  mov rax, 2
  mov rcx, rax
  pop rax
  imul rax, rcx
  mov [rbp - 8], rax
if_1_end:
  mov rax, [rbp - 8]
  push rax
  mov rax, 1
  mov rcx, rax
  pop rax
  add rax, rcx
  mov [rbp - 8], rax
  mov rax, [rbp - 16]
  push rax
  mov rax, 1
  mov rcx, rax
  pop rax
  add rax, rcx
  mov [rbp - 16], rax
  jmp loop_0_start
loop_0_end:
"#;
    let mut asm_path = std::env::temp_dir();
    asm_path.push("test_asm_control_flow.s");
    let mut out_obj = std::env::temp_dir();
    out_obj.push("test_asm_control_flow.o");

    File::create(&asm_path)?.write_all(asm.as_bytes())?;

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
        return Err(format!("assembler failed on control flow instructions: {}", status).into());
    }

    let meta = fs::metadata(&out_obj)?;
    assert!(meta.len() > 0, "object file should be non-empty");
    Ok(())
}

// Test 5: Full pipeline: run the main crate to compile a tiny C program to a binary.
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