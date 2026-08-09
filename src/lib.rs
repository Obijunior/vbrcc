// vbrcc - Very Basic Rust C Compiler
// Copyright (C) 2026 Henry Nwagwu
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! A hobby C compiler and x86-64 assembler, written from scratch in Rust.
//!
//! `vbrcc` (Very Basic Rust C Compiler) takes a single C source file and emits a
//! Windows executable. It uses no external compiler libraries: the lexer, parser,
//! type checker, code generator, and assembler are all hand-written. In its default
//! mode it needs no external toolchain at all: it encodes x86-64 machine bytes and
//! writes the PE container itself.
//!
//! This crate is **binary-first**. The primary interface is the `vbrcc` command-line
//! tool; the modules below are published so the pipeline can be inspected and reused,
//! but they are not a stability-guaranteed API. Expect breaking changes between
//! minor versions before 1.0.
//!
//! # Platform support
//!
//! The compiler *runs* anywhere Rust does, but it only *emits* Windows PE/COFF
//! binaries for x86-64. There is no ELF or Mach-O backend yet, so output produced on
//! Linux or macOS will not run natively on the host.
//!
//! # Installation
//!
//! ```console
//! $ cargo install vbrcc
//! ```
//!
//! # Usage
//!
//! ```console
//! $ vbrcc input.c -o program
//! [ SUCCESS ] :: Wrote assembly to "program.s"
//! [ SUCCESS ] :: Created Windows Executable: "program.exe"
//!   - .text size: 34 bytes
//!   - .data size: 0 bytes
//!   - .idata size: 0 bytes
//! [ SUCCESS ] :: Compiled binary to "program.exe"
//! ```
//!
//! | Flag | Pipeline | External dependencies |
//! |---|---|---|
//! | *(none)* | Built-in assembler emits a complete PE executable | none |
//! | `--lld-link` | Built-in assembler emits a COFF object, then `lld-link` links it | LLVM + Windows SDK |
//! | `--gcc` | System `gcc` assembles and links the `.s` file | MinGW-w64 GCC |
//! | `-o <path>` | Set the output path (default: input with no extension) | |
//! | `--keep-artifacts` | Keep intermediate `.s` / `.obj` files | |
//! | `-E` | Preprocess only; print the expanded source and exit | none |
//! | `-I <dir>` | Add a directory to the `#include` search path | |
//!
//! Use `--lld-link` for a program that imports from **more than one DLL**. The default
//! backend writes a working import table, but it currently targets a single DLL
//! (`msvcrt.dll`), so a call into `kernel32`, `user32`, or the UCRT will not resolve.
//! Single-DLL C runtime calls such as `printf` and `puts` work on the default path with
//! no external linker.
//!
//! # The pipeline
//!
//! A source file moves through five stages. Each stage hands its output to the next,
//! and any stage may stop compilation by returning a [`diagnostic::CompileError`].
//!
//! ```text
//! C source
//!    │
//!    ▼
//!  Preprocessor   →  resolves directives, expands macros; drives the lexer
//!    │                (reads other files into a SourceMap)
//!    ▼
//!  tokens, each carrying a span tagged with the file it came from
//!    │
//!    ▼
//!  Parser         →  AST (a Program of functions)
//!    │
//!    ▼
//!  Type checker   →  the same AST, with a type on every expression
//!    │
//!    ▼
//!  Code generator →  Intel-syntax x86-64 assembly text
//!    │
//!    ▼
//!  Assembler      →  machine bytes, then a PE executable or COFF object
//! ```
//!
//! Compilation stops at the first error; only one diagnostic is ever reported.
//!
//! | Module | Stage |
//! |---|---|
//! | [`preprocessor`] | Directives and macro expansion; produces the token stream |
//! | [`lexer`] | Turns source text into tokens with source spans |
//! | [`parser`] | Recursive-descent parser building the AST |
//! | [`ast`] | AST node definitions and the [`ast::Type`] enum |
//! | [`typeck`] | Assigns a type to every expression; reports type errors |
//! | [`codegen`] | Walks the typed AST, emitting assembly text |
//! | [`assembler`] | Parses and encodes assembly into PE or COFF output |
//! | [`assembler_driver`] | Selects the output mode and invokes any external linker |
//! | [`diagnostic`] | [`diagnostic::CompileError`], [`diagnostic::Span`], and rustc-style rendering |
//!
//! # Supported C subset
//!
//! The compiler handles integer types (`int`, `char`, `long`), `void`,
//! pointers, arrays, casts, address-of and dereference, pointer arithmetic,
//! arithmetic and bitwise operators, comparisons, compound assignment,
//! post-increment/decrement, function definitions and calls, function prototypes
//! (including variadic ones such as `printf`), `const` as an accepted and dropped
//! qualifier, `if`/`else`, `while`, and `for`.
//!
//! Not yet implemented: `struct`, `union`, `enum`, `typedef`, `unsigned`, `float`,
//! `double`, `switch`, `do`/`while`, `break`, `continue`, block-level scope
//! (all variables share one flat scope)
//!
//! Two behaviours are worth calling out explicitly, because they fail quietly rather
//! than loudly:
//!
//! - **The preprocessor is nearly complete.** `#include`, `#define` for object-like
//!   and function-like macros, `#undef`, the whole `#if` family with a
//!   constant-expression evaluator, `#error`, `#warning`, and `#pragma once` all
//!   work. Missing: `#` stringizing, `##` pasting, `__VA_ARGS__`, and `#line`.
//!   Each reports an explicit error rather than failing silently.
//!   A small header set ships inside the binary (`limits.h`, `stddef.h`,
//!   `stdbool.h`, `stdint.h`, `stdio.h`, `string.h`, `stdlib.h`); `-I <dir>` adds
//!   a search directory, and a directory on that path shadows a bundled header.
//! - **`-E` prints the preprocessed source** and exits, which is the fastest way to
//!   see what macro expansion actually produced.
//! - **`long` is the wrong width for the target.** Scalars otherwise carry true C
//!   widths (`char` = 1, `int` = 4), and codegen sizes every load, store, and stack
//!   slot from them. But `long` is 8 bytes here, which is the Linux LP64 width; the
//!   Windows target this compiler actually emits is LLP64, where `long` is 4. Making
//!   the width target-dependent is a planned phase. [`ast::Type::size`] and
//!   [`ast::Type::align`] are the single place that controls this.
//! - **Calls take at most four arguments.** Codegen reports an error past
//!   `rcx`/`rdx`/`r8`/`r9`; stack arguments are not implemented. `printf` therefore
//!   accepts a format string plus three values.
//!
//! # Debugging
//!
//! Setting any of these environment variables dumps the corresponding intermediate
//! representation to stderr:
//!
//! | Variable | Dumps |
//! |---|---|
//! | `DUMP_TOKENS` | The token stream from the lexer |
//! | `DUMP_AST` | The parsed AST, pretty-printed |
//! | `DUMP_ASM` | The generated assembly text |
//!
//! ```console
//! $ DUMP_AST=1 vbrcc input.c
//! ```
//!
//! # Further reading
//!
//! `docs/architecture.md` in the repository covers each stage in depth, including the
//! calling convention, the register discipline used by the code generator, and
//! step-by-step recipes for adding an instruction, an operator, or a statement.
//!
//! # License
//!
//! GPL-3.0-or-later. See `COPYING` for the full text.

pub mod lexer;
pub mod parser;
pub mod preprocessor;
pub mod ast;
pub mod codegen;
pub mod assembler;
pub mod assembler_driver;
pub mod diagnostic;
pub mod typeck;