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
