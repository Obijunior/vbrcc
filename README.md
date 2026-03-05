# VBRCC - Very Basic Rust C Compiler

This is a little hobby project of mine I made to learn more about Rust and how code works at a low level. Working on building an assembler to go with it, but you can still use gcc to assemble by passing `-gcc`. This is built for my machine, so it uses the Intel x86-64 assembly syntax and it's only been tested on a Windows machine.



Current usage "cargo run -- <input.c> [-o <output_path>] [-gcc]"

No makefile yet, so for now you'll need to run it with cargo, but no other external dependencies

## C Functionality supported by compiler (what I have ast nodes for)
| ast | symbol |
| --- | --- |
| Add | + |
| Sub | - |
| Mul | * |
| Div | / |
| Negate | - |
| BitNot | ~ |
| LogNot | ! |

## Instruction Set (currently) supported by assembler

- mov
- ret
