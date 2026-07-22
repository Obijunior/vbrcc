  .intel_syntax noprefix
  .globl main
main:
  push rbp
  mov rbp, rsp
  mov rax, 1
  push rax
  mov rax, 2
  mov rcx, rax
  pop rax
  add rax, rcx
  pop rbp
  ret
  