# VBRCC - Very Basic Rust C Compiler

[![Crates.io](https://img.shields.io/crates/v/vbrcc.svg)](https://crates.io/crates/vbrcc)
[![CI](https://github.com/obijunior/vbrcc/actions/workflows/ci.yml/badge.svg)](https://github.com/obijunior/vbrcc/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
<!-- [![Downloads](https://img.shields.io/crates/d/vbrcc.svg)](https://crates.io/crates/vbrcc) -->

A hobby C compiler and assembler written in Rust targeting x86-64 (Intel syntax)

## Usage

```sh
vbrcc <input.c> [-o <output_file>] [--lld-link/--gcc] [--keep-artifacts]
vbrcc --version    # or -v
vbrcc --help       # or -h
```

The compiler reads one C file. It writes an assembly file and an executable. Use
`-o` to set the output path. Use `--version` to print the version. Use `--help`
to print the option list.

| Flag | Pipeline | External dependencies |
|---|---|---|
| *(none)* | Custom assembler emits a complete PE executable directly | None |
| `--lld-link` | Custom assembler emits COFF `.obj`, then `lld-link` links it | LLVM (`lld-link`, `llvm-dlltool`) + Windows SDK |
| `--gcc` | System `gcc` assembles and links | MinGW-w64 GCC |

The default path is fully self-contained — no external tools needed. Use `--lld-link` when you need C standard library functions (e.g., `printf`) resolved via `msvcrt.dll`.

## Tests

Run the test suite with:

```sh
cargo test
```

## Compiler: C Features

### Expressions

| Feature | Example |
| --- | --- |
| Integer literals | `42` |
| String literals | `"hello\n"` |
| Variables | `x`, `sum` |
| Arithmetic | `a + b`, `a - b`, `a * b`, `a / v`, `a % b`, `-a` |
| Bitwise NOT | `~a` |
| Logical NOT | `!a` |
| Comparison | `<`, `<=`, `>`, `>=` |
| Assignment | `x = 5` |
| Compound assignment | `+=`, `-=`, `*=`, `/=`, `%=` |
| Post-increment/decrement | `i++`, `i--` |
| Function calls | `printf("hello")` |
| Address-of / dereference | `&x`, `*p` |
| Array index | `a[i]` |
| Cast | `(char)x`, `(int *)p` |

### Types

The compiler has a type checker. The type checker runs after the parser and
before the code generator. It gives a type to each expression. It reports a type
error with a source location, for example a dereference of a non-pointer value.

| Feature | Example |
| --- | --- |
| Integer types | `int`, `char`, `long` |
| Void type | `void`, `void *` |
| Pointers | `int *p`, `int **pp` |
| Arrays | `int a[10]` |

Note: the compiler uses loose sizing at present. Every scalar and every pointer is
8 bytes. True widths (`char` = 1, `int` = 4) are a planned phase.

### Statements and Control Flow

| Feature | Example |
| :--- | :--- |
| Return | `return expr;` |
| Variable declaration | `int x = 0;`, `char c;`, `int *p;`, `int a[10];` |
| For loops | `for (int i = 0; i < 10; i++) { ... }` |
| While loops | `while (cond) { ... }` |
| If/Else | `if (cond) { ... } else { ... }` |
| Logical AND / OR | `&&`, `\|\|` |
| Comments | `// single-line comment` |

### Not yet supported

* `struct`, `union`, `enum`, and `typedef`
* `unsigned`, `float`, and `double`
* `switch`, `do-while`, `break`, and `continue`
* Block-level scope (variables use one flat scope)
* Preprocessor directives (`#include`, `#define`)

## Assembler: currently supported features

The custom assembler (`src/assembler/` module) supports a small subset of Intel x86-64 instructions and registers:

- Syntax: Intel syntax (the assembler accepts `.intel_syntax noprefix`).
- Registers: all 64-bit general-purpose registers (RAX–R15) and 8-bit sub-registers (AL, BL, CL, DL).
- Supported instructions (textual forms accepted by the assembler):
  - `ret`, `syscall`, `cqo`
  - `push <reg>`, `pop <reg>`
  - `neg <reg>`, `not <reg>`, `idiv <reg>`
  - `mov <reg>, <reg>` / `mov <reg>, <imm64>` / `mov <reg>, [reg +/- disp]` / `mov [reg +/- disp], <reg>`
  - `movzx <reg64>, <reg8>`
  - `add <reg>, <reg|imm32>`, `sub <reg>, <reg|imm32>`
  - `imul <reg>, <reg|imm32>`
  - `and <reg>, <reg|imm32>`, `cmp <reg>, <reg|imm32>`
  - `sete`, `setne`, `setl`, `setle`, `setg`, `setge` (8-bit register operand)
  - `jmp <label>`, `je <label>`, `jne <label>`, `jl <label>`, `jle <label>`, `jg <label>`, `jge <label>`
  - `lea <reg>, [rip + label]` / `lea <reg>, [reg +/- disp]`
  - `call <label>`
  - `xor <reg>, <reg|imm32>`

### Fun tests you can run

- if you run `objdump -d -M intel <executable file>` you can see the disassembled assembly contained in the executable. Doing a comparison between the output from my assembler and from when the --gcc flag is passed produces some pretty interesting results
- running `hexdump -v -e '1/1 "%02x "' <executable file>` will give the raw hex from the executable in a big chunk
- running `gcc -S -masm=intel <c code>` will show you how `gcc` compiles the inputted c code 
- Similar: `gcc -S -masm=intel -O0 -fno-asynchronous-unwind-tables -fno-ident input.c` but without optimizations or the `.seh_*` directives

  
### Output formats

The assembler supports two output modes:

- **PE executable** (default): Encodes instructions into raw machine bytes and produces a complete Windows PE32+ executable with DOS header, COFF header, section table, and import table. External calls (e.g., `printf`) are resolved via IAT.
- **COFF `.obj`** (`--coff` flag): Emits a relocatable COFF object file with symbol table and `IMAGE_REL_AMD64_REL32` relocations for cross-section and external references. Designed to be linked by `lld-link`.

## Architecture

Read [docs/architecture.md](docs/architecture.md) to learn how the stages fit
together.

## Contributing / next steps
- C99 compliance
- Emit ELF64 output (currently Windows PE/COFF only).
- Write a custom linker to replace `lld-link` dependency.

## Roadmap to C99

Done:
- Multiple integer types (`int`, `char`, `long`) and `void`
- A type checker with source-located type errors
- Pointers, address-of, dereference, and pointer arithmetic
- Arrays and array indexing
- Cast expressions

Next:
- True type widths (`char` = 1, `int` = 4)
- `struct`, `union`, `enum`, and `typedef`
- More control flow: `switch`, `do-while`, `break`, `continue`
- Preprocessor: `#include`, `#define`

## License

VBRCC is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See the [COPYING](COPYING) file for the full license text.
