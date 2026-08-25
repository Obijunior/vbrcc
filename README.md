# VBRCC: Very Basic Rust C Compiler

[![Crates.io](https://img.shields.io/crates/v/vbrcc.svg)](https://crates.io/crates/vbrcc)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Downloads](https://img.shields.io/crates/d/vbrcc.svg)](https://crates.io/crates/vbrcc)
<!-- [![Docs.rs](https://docs.rs/vbrcc/badge.svg)](https://docs.rs/vbrcc) -->
<!-- [![CI](https://github.com/obijunior/vbrcc/actions/workflows/ci.yml/badge.svg)](https://github.com/obijunior/vbrcc/actions/workflows/ci.yml) -->

A hobby C compiler and x86-64 assembler, written from scratch in Rust.

VBRCC uses no external compiler library. The lexer, the parser, the type checker, the
code generator, and the assembler are all hand-written. So are the instruction encoder
and the PE executable writer. In its default mode it needs no assembler, no linker, and
no toolchain. It emits the machine bytes and builds the Windows executable itself.

## Platform support

VBRCC **runs** anywhere Rust runs. It **emits** only Windows PE/COFF binaries for
x86-64. There is no ELF or Mach-O backend yet, so a binary built on Linux or macOS does
not run on the host. An ELF backend is on the roadmap.

## Installation

```sh
cargo install vbrcc
```

Or build from source:

```sh
git clone https://github.com/obijunior/vbrcc
cd vbrcc
cargo build --release
```

## Usage

```sh
vbrcc <input.c> [-o <output>] [--lld-link | --gcc] [--keep-artifacts]
vbrcc --version    # or -v
vbrcc --help       # or -h
```

VBRCC compiles one C file. It writes an assembly file and an executable.


```console
$ vbrcc examples/return42.c -o program
[ SUCCESS ] :: Wrote assembly to "program.s"
[ SUCCESS ] :: Created Windows Executable: "program.exe"
  - .text size: 34 bytes
  - .data size: 0 bytes
  - .idata size: 0 bytes
[ SUCCESS ] :: Compiled binary to "program.exe"
```

### Options

| Flag | Effect |
|---|---|
| `-o <output>` | Set the output path (default: input with no extension) |
| `--gcc` | Assemble and link with the system `gcc` instead |
| `--lld-link` | Emit a COFF object and link it with `lld-link` |
| `--keep-artifacts` | Keep intermediate `.s` / `.obj` files |
| `-h`, `--help` | Print the option list |
| `-v`, `--version` | Print version information |

### Backends

| Flag | Pipeline | External dependencies |
|---|---|---|
| *(none)* | Built-in assembler emits a complete PE executable directly | **none** |
| `--lld-link` | Built-in assembler emits a COFF `.obj`, then `lld-link` links it | LLVM (`lld-link`, `llvm-dlltool`) + Windows SDK |
| `--gcc` | System `gcc` assembles and links | MinGW-w64 GCC |

The default path needs no external toolchain. It builds a runnable PE with a working
import table. A program that calls a standard-library function from `msvcrt.dll`, such
as `printf`, therefore runs directly:

```console
$ vbrcc examples/input.c -o input
$ ./input.exe
hello world - sum: 52
```

> **Use `--lld-link` for a program that imports from more than one DLL.** The import
> table of the default backend covers one DLL, `msvcrt.dll`. A program that also calls
> into `kernel32`, `user32`, or the UCRT does not resolve those symbols yet. Use
> `--lld-link` for such a program. A C runtime call such as `printf` works with no extra
> setup.

### Debugging output

Set any of these environment variables to write the matching intermediate form to
stderr:

| Variable | Dumps |
|---|---|
| `DUMP_TOKENS` | The token stream from the lexer |
| `DUMP_AST` | The parsed AST, pretty-printed |
| `DUMP_ASM` | The generated assembly text |

```sh
DUMP_AST=1 vbrcc input.c
```

## Example

```c
int main() {
    int total = 0;
    for (int i = 1; i <= 10; i++) {
        total += i;
    }
    return total;
}
```

```console
$ vbrcc sum.c -o sum
$ ./sum.exe; echo $?
55
```

More sample programs live in [`examples/`](https://github.com/obijunior/vbrcc/tree/main/examples).

## Compiler: supported C

### Expressions

| Feature | Example |
| --- | --- |
| Integer literals | `42` |
| String literals | `"hello\n"` |
| Character literals | `'a'`, `'\0'`, `'\n'` |
| Variables | `x`, `sum` |
| Arithmetic | `a + b`, `a - b`, `a * b`, `a / b`, `a % b`, `-a` |
| Bitwise NOT | `~a` |
| Logical NOT | `!a` |
| Comparison | `<`, `<=`, `>`, `>=`, `==`, `!=` |
| Assignment | `x = 5` |
| Compound assignment | `+=`, `-=`, `*=`, `/=`, `%=` |
| Post-increment/decrement | `i++`, `i--` |
| Function calls | `printf("hello")` |
| Address-of / dereference | `&x`, `*p` |
| Array index | `a[i]` |
| Cast | `(char)x`, `(int *)p` |

### Types

VBRCC has a type checker. It runs after the parser and before the code generator. It
gives a type to every expression. It reports a type error with a source location, for
example a dereference of a value that is not a pointer.

| Feature | Example |
| --- | --- |
| Integer types | `int`, `char`, `long` |
| Boolean type | `_Bool`, `bool` (via `<stdbool.h>`) |
| Void type | `void`, `void *` |
| Pointers | `int *p`, `int **pp` |
| Arrays | `int a[10]` |
| Type aliases | `typedef long size_t;`, `typedef char *cstring;` |

> **Type sizes are the real C widths.** `char` is 1 byte, `int` is 4, and `long`, a
> pointer, and `void *` are 8. A local occupies its true size on the stack, aligned to
> its type. Array elements pack at element size. A narrow load sign-extends with `movsx`
> or `movsxd`, and a store writes exactly the width of the value. `Type::size` and
> `Type::align` in `src/ast.rs` are the one place that decides this.

> **`_Bool` is its own type, 1 byte wide, not an alias for `int` or `char`.** A store
> through a `_Bool` lvalue normalizes the value first: any nonzero value becomes exactly
> `1`, per C99 6.3.1.2. Integer promotion and the usual arithmetic conversions are not
> implemented yet for any type, `_Bool` included — see the roadmap.

### Statements and control flow

| Feature | Example |
| :--- | :--- |
| Return | `return expr;` |
| Variable declaration | `int x = 0;`, `char c;`, `int *p;`, `int a[10];` |
| For loops | `for (int i = 0; i < 10; i++) { ... }` |
| While loops | `while (cond) { ... }` |
| If / else | `if (cond) { ... } else { ... }` |
| Single-statement bodies | `while (c) x++;`, `if (c) return 1;` |
| Logical AND / OR | `&&`, `\|\|` |
| Line comments | `// single-line comment` |

### Not yet supported

* `struct`, `union`, and `enum`
* `unsigned`, `float`, and `double`
* `switch`, `do-while`, `break`, and `continue`
* The bitwise operators `&`, `|`, `^`, `<<`, and `>>`
* Block-level scope. Every variable shares one flat scope for each function
* `#` stringizing, `##` pasting, `__VA_ARGS__`, and `#line`

## Preprocessor

| Feature | Notes |
|---|---|
| `#include <name>` / `#include "name"` | Quoted form searches the includer's directory first |
| `-I <dir>` | Adds a search directory; a match there shadows a bundled header |
| `#define` | Object-like and function-like macros, with argument pre-expansion |
| `#undef` | |
| `#if`, `#elif`, `#else`, `#endif` | Full constant-expression evaluator with `defined` |
| `#ifdef`, `#ifndef` | |
| `#error`, `#warning` | |
| `#pragma once` | Every other pragma is ignored |
| Predefined | `__FILE__`, `__LINE__`, `__STDC__`, `__STDC_VERSION__`, `_WIN32`, `_WIN64` |

A small header set ships inside the binary, so an install needs no data files:
`limits.h`, `stddef.h`, `stdbool.h`, `stdint.h`, `stdio.h`, `string.h`, and `stdlib.h`.
They are small on purpose. Each one uses `typedef` and a macro where a language feature
is still missing. For example, `size_t` is `typedef`'d to `long` in `stddef.h`, but
`bool` is still a macro for `_Bool` since `stdbool.h`'s job is only to spell the keyword
the way C99 expects.

`-E` prints the preprocessed source and exits. This is the fastest way to see what
expansion produced.

```c
#include <stdio.h>

int main() {
    printf("hello from vbrcc\n");
    return 0;
}
```

> **Note:** the compiler checks the argument count of a call when the function has a
> prototype. A call to a function with no declaration is still legal, so a program
> written before `#include` worked still compiles.

## Assembler

The built-in assembler (`src/assembler/`) accepts a small subset of Intel-syntax x86-64.

- **Syntax:** Intel (`.intel_syntax noprefix` is accepted).
- **Registers:** all 64-bit general-purpose registers (RAX–R15) and the 8-bit
  sub-registers AL, BL, CL, DL.
- **Instructions:**
  - `ret`, `syscall`, `cqo`
  - `push <reg>`, `pop <reg>`
  - `neg <reg>`, `not <reg>`, `idiv <reg>`
  - `mov <reg>, <reg>` / `mov <reg>, <imm64>` / `mov <reg>, [reg +/- disp]` / `mov [reg +/- disp], <reg>`
  - `movzx <reg64>, <reg8>`
  - `add <reg>, <reg|imm32>`, `sub <reg>, <reg|imm32>`
  - `imul <reg>, <reg|imm32>`
  - `and <reg>, <reg|imm32>`, `cmp <reg>, <reg|imm32>`
  - `xor <reg>, <reg|imm32>`
  - `sete`, `setne`, `setl`, `setle`, `setg`, `setge` (8-bit register operand)
  - `jmp`, `je`, `jne`, `jl`, `jle`, `jg`, `jge` (label operand)
  - `lea <reg>, [rip + label]` / `lea <reg>, [reg +/- disp]`
  - `call <label>`

### Output formats

- **PE executable** (default). The assembler encodes the instructions and writes a
  complete Windows PE32+ image. The image has a DOS header, a COFF header, a section
  table, and a working import table. A call into `msvcrt.dll`, such as `printf`,
  resolves through that table and runs. The table covers one DLL, so a program that also
  imports from `kernel32`, `user32`, or the UCRT needs `--lld-link`.
- **COFF object** (used by `--lld-link`). The assembler writes a relocatable object file
  with a symbol table and `IMAGE_REL_AMD64_REL32` relocations, for `lld-link` to resolve.

  > There is no separate flag that stops at a `.obj`. The `--lld-link` pipeline produces
  > the COFF output. Pass `--keep-artifacts` to keep the intermediate file.

### How to inspect the output

- `objdump -d -M intel <exe>` disassembles the output. To find a miscompilation, compare
  that output against the same program built with `--gcc`.
- `hexdump -v -e '1/1 "%02x "' <exe>` shows the raw image bytes.
- `gcc -S -masm=intel input.c` shows how GCC compiles the same source.
- `gcc -S -masm=intel -O0 -fno-asynchronous-unwind-tables -fno-ident input.c` does the
  same with no optimisation and no `.seh_*` directives. The result is closer in shape to
  VBRCC's output, which makes a diff practical.

## Tests

```sh
cargo test
```

## Documentation

- [API documentation on docs.rs](https://docs.rs/vbrcc)
- [Architecture guide](https://github.com/obijunior/vbrcc/blob/main/docs/architecture.md):
  how the five stages fit together, the code generator's register discipline, and
  recipes for adding an instruction, an operator, or a statement.
- [Contributing & bug reports](https://github.com/obijunior/vbrcc/blob/main/docs/CONTRIBUTING.md):
  building from source, backend prerequisites, running tests, debugging the compiler.
- [Example programs](https://github.com/obijunior/vbrcc/tree/main/examples): what each
  sample demonstrates and which backend it needs.

## Roadmap to C99

**Done**

- Multiple integer types (`int`, `char`, `long`) and `void`
- `_Bool` (C99 6.3.1.2 store normalization)
- A type checker with source-located type errors
- Pointers, address-of, dereference, and pointer arithmetic
- Arrays and array indexing
- Cast expressions
- A built-in PE import table: single-DLL libc calls (`printf` and friends via `msvcrt.dll`)
  run through the default backend with no `--lld-link`
- `typedef`

**Next**

- Extend the built-in import table to multiple DLLs (`kernel32`, `user32`, the UCRT)
- `struct`, `union`, and `enum`
- More control flow: `switch`, `do-while`, `break`, `continue`
- Preprocessor: `#` stringizing, `##` pasting, `__VA_ARGS__`
- More than four call arguments
- Block-level scope

**Later**

- ELF64 output (currently Windows PE/COFF only)
- A custom linker, to replace the `lld-link` dependency

## Contributing

VBRCC is a personal learning project. The design changes too fast for an outside patch
to be practical. This stays true until the compiler reaches C99 compliance at v1.0.0.

**Bug reports, questions, and ideas are welcome.** Please
[open an issue](https://github.com/obijunior/vbrcc/issues). A miscompilation helps most.
The ideal report is a minimal C file and the wrong output.

**Pull requests are not accepted at this time.** This changes at v1.0.0.

## License

VBRCC is free software. You can redistribute it and change it under the terms of the
GNU General Public License, as published by the Free Software Foundation. Use either
version 3 of the License, or any later version. See
[COPYING](https://github.com/obijunior/vbrcc/blob/main/COPYING) for the full text.
