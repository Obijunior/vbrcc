# VBRCC: Very Basic Rust C Compiler

[![Crates.io](https://img.shields.io/crates/v/vbrcc.svg)](https://crates.io/crates/vbrcc)
[![Docs.rs](https://docs.rs/vbrcc/badge.svg)](https://docs.rs/vbrcc)
[![CI](https://github.com/obijunior/vbrcc/actions/workflows/ci.yml/badge.svg)](https://github.com/obijunior/vbrcc/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
<!-- [![Downloads](https://img.shields.io/crates/d/vbrcc.svg)](https://crates.io/crates/vbrcc) -->

A hobby C compiler and x86-64 assembler, written from scratch in Rust.

VBRCC uses no external compiler libraries. The lexer, parser, type checker, code
generator, and assembler are all hand-written, including the instruction encoder and
the PE executable writer. In its default mode it needs no assembler, no linker, and no
toolchain: it emits machine bytes and builds the Windows executable itself.

## Platform support

VBRCC **runs** anywhere Rust does, but it only **emits** Windows PE/COFF binaries for
x86-64. There is no ELF or Mach-O backend yet, so binaries produced on Linux or macOS
will not run natively on the host. An ELF backend is on the roadmap.

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
$ vbrcc examples/test_return.c -o program
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

The default path is fully self-contained and is the right choice for programs that call
nothing external.

> **Use `--lld-link` for anything touching the C standard library.** The default backend
> *builds* a program that calls `printf` without complaint: it emits an import table and
> reports success. The resulting executable then fails to start, with exit code `127` and
> no output. Import-table generation is still being developed. Until it lands,
> `--lld-link` is the working path for `printf` and friends.

### Debugging output

Set any of these environment variables to dump the corresponding intermediate
representation to stderr:

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

VBRCC has a type checker. It runs after the parser and before the code generator,
assigning a type to every expression and reporting type errors with a source location,
such as a dereference of a non-pointer value.

| Feature | Example |
| --- | --- |
| Integer types | `int`, `char`, `long` |
| Void type | `void`, `void *` |
| Pointers | `int *p`, `int **pp` |
| Arrays | `int a[10]` |

> **Note on loose type sizes.** Every scalar and every pointer is currently 8 bytes, and
> an array reserves 8 bytes per element. True widths (`char` = 1, `int` = 4) are a
> planned phase. `Type::size` and `Type::align` in `src/ast.rs` are the single place
> this is decided.

### Statements and control flow

| Feature | Example |
| :--- | :--- |
| Return | `return expr;` |
| Variable declaration | `int x = 0;`, `char c;`, `int *p;`, `int a[10];` |
| For loops | `for (int i = 0; i < 10; i++) { ... }` |
| While loops | `while (cond) { ... }` |
| If / else | `if (cond) { ... } else { ... }` |
| Logical AND / OR | `&&`, `\|\|` |
| Line comments | `// single-line comment` |

### Not yet supported

* `struct`, `union`, `enum`, and `typedef`
* `unsigned`, `float`, and `double`
* `switch`, `do-while`, `break`, and `continue`
* Block-level scope. All variables share one flat scope per function
* Block comments (`/* */`). Only `//` line comments are recognised
* Preprocessor directives

> **Note: preprocessor directives are skipped, not rejected.** Any line beginning with
> `#` is discarded by the lexer, so `#include <stdio.h>` compiles without error *and
> without effect*. Nothing the header would have declared exists. Calls to `printf` link
> anyway under `--lld-link` because the symbol is resolved from `msvcrt.dll`.

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

- **PE executable** (default). Encodes instructions into machine bytes and produces a
  complete Windows PE32+ image with DOS header, COFF header, section table, and import
  table. Self-contained programs work today. The import-table path for external calls
  such as `printf` is written but does not yet produce a loadable image, so use
  `--lld-link` for those.
- **COFF object** (used by `--lld-link`). Emits a relocatable object file with a symbol
  table and `IMAGE_REL_AMD64_REL32` relocations, for `lld-link` to resolve.

  > There is no standalone flag to stop at a `.obj`. COFF output is produced as part of
  > the `--lld-link` pipeline; pass `--keep-artifacts` to retain the intermediate file.

### Inspecting the output

- `objdump -d -M intel <exe>` disassembles the output. Comparing VBRCC's output against
  the same program built with `--gcc` is the fastest way to spot a miscompilation.
- `hexdump -v -e '1/1 "%02x "' <exe>` dumps the raw image bytes.
- `gcc -S -masm=intel input.c` shows how GCC compiles the same source.
- `gcc -S -masm=intel -O0 -fno-asynchronous-unwind-tables -fno-ident input.c` does the
  same without optimisations or `.seh_*` directives, which lands closer to what VBRCC
  emits.

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
- A type checker with source-located type errors
- Pointers, address-of, dereference, and pointer arithmetic
- Arrays and array indexing
- Cast expressions

**Next**

- Fix the built-in PE import table, so `printf` works without `--lld-link`
- True type widths (`char` = 1, `int` = 4)
- `struct`, `union`, `enum`, and `typedef`
- More control flow: `switch`, `do-while`, `break`, `continue`
- Preprocessor: `#include`, `#define`
- Block-level scope

**Later**

- ELF64 output (currently Windows PE/COFF only)
- A custom linker, to replace the `lld-link` dependency

## Contributing

VBRCC is a personal learning project. Until it reaches C99 compliance at v1.0.0, the
design moves too fast for outside patches to be practical.

**Bug reports, questions, and ideas are welcome.** Please
[open an issue](https://github.com/obijunior/vbrcc/issues). Miscompilations help most;
a minimal C file plus the wrong output makes the ideal report.

**Pull requests are not being accepted at this time.** This will change at v1.0.0.

## License

VBRCC is free software: you can redistribute it and/or modify it under the terms of the
GNU General Public License as published by the Free Software Foundation, either version
3 of the License, or (at your option) any later version. See
[COPYING](https://github.com/obijunior/vbrcc/blob/main/COPYING) for the full text.
