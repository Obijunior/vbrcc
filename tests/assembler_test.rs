//! Assembler tests that need the public API rather than the unit-test module.
//!
//! Instruction bytes are checked against GNU `as` in `encoder_vs_gas.rs`.

use vbrcc::assembler;

#[test]
fn a_data_label_defined_with_long_is_reachable_by_rip() {
    let asm = "\
.intel_syntax noprefix
.section .data
counter:
  .long 42
.section .text
.globl main
main:
  lea rax, [rip + counter]
  ret
";
    let (text, data, _idata, _entry) = assembler::assemble(asm).unwrap();
    assert_eq!(data, vec![42, 0, 0, 0], "counter is four little-endian bytes");
    assert!(!text.is_empty(), "main assembled to bytes");
}

/// The entry point is the start stub when there is one. Returning from `main`
/// as the entry point ends only the main thread, and a loader worker thread can
/// keep the process alive for 30 s and give it exit code 0.
#[test]
fn the_entry_point_is_the_start_stub() {
    let asm = "\
.intel_syntax noprefix
.section .text
main:
  mov rax, 7
  ret
__vbrcc_start:
  sub rsp, 40
  call main
  mov rcx, rax
  call exit
";
    let (_text, _data, _idata, entry) = assembler::assemble(asm).unwrap();
    // main is `mov rax, imm64` (10 bytes) + `ret` (1), so the stub starts at 11.
    assert_eq!(entry, 11, "entry must be __vbrcc_start, not main");
}

/// Hand-written assembly with no stub still starts at `main`.
#[test]
fn without_a_stub_the_entry_point_is_main() {
    let asm = ".intel_syntax noprefix\nhelper:\n  ret\nmain:\n  mov rax, 42\n  ret\n";
    let (_text, _data, _idata, entry) = assembler::assemble(asm).unwrap();
    assert_eq!(entry, 1, "helper is one byte, so main starts at 1");
}
