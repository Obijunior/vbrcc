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
//! `vbrcc` (Very Basic Rust C Compiler) reads one C source file and writes a Windows
//! executable. It uses no external compiler library. The lexer, the parser, the type
//! checker, the code generator, and the assembler are all hand-written. In its default
//! mode it needs no external toolchain: it encodes the x86-64 bytes and writes the PE
//! container itself.
//!
//! This crate is **binary-first**. The `vbrcc` command-line tool is the interface. The
//! modules below are public so that you can inspect and reuse the pipeline, but they
//! are not a stable API. Expect breaking changes between minor versions before 1.0.
//!
//! # Platform support
//!
//! The compiler *runs* anywhere Rust runs. It *emits* only Windows PE/COFF binaries for
//! x86-64. There is no ELF or Mach-O backend yet, so output built on Linux or macOS
//! does not run on the host.
//!
//! Install the tool with `cargo install vbrcc`. The README documents the command line:
//! the flags, the three backends, and the environment variables that dump the token
//! stream, the AST, and the assembly.
//!
//! One backend limit matters here, because it is easy to meet by accident. The default
//! backend writes a working import table, but that table covers one DLL, `msvcrt.dll`.
//! A C runtime call such as `printf` or `puts` therefore runs with no external linker.
//! A call into `kernel32`, `user32`, or the UCRT does not resolve. Such a program needs
//! `--lld-link`.
//!
//! # The pipeline
//!
//! A source file moves through five stages. Each stage gives its output to the next
//! stage. Any stage can stop the compilation with a [`diagnostic::CompileError`].
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
//! The compiler stops at the first error. It reports one diagnostic.
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
//! The compiler handles the integer types `int`, `char`, and `long`, plus `void`. It
//! also handles pointers, arrays, casts, address-of, dereference, pointer arithmetic,
//! the arithmetic operators, `~` and `!`, the comparisons, `&&` and `||`, compound
//! assignment, post-increment and post-decrement, function definitions, function
//! calls, function prototypes (including variadic ones such as `printf`), `if` and
//! `else`, `while`, and `for`. It accepts `const` and then discards it.
//!
//! These do not exist yet: `struct`, `union`, `enum`, `typedef`, `unsigned`, `float`,
//! `double`, `switch`, `do`/`while`, `break`, `continue`, the bitwise operators `&`,
//! `|`, `^`, `<<`, and `>>`, and block-level scope. Every variable in a function
//! shares one flat scope.
//!
//! # Behaviour to know about
//!
//! These four points are easy to miss, because they do not fail loudly.
//!
//! - **The preprocessor is nearly complete.** `#include`, `#define` for object-like
//!   and function-like macros, `#undef`, the full `#if` family with a
//!   constant-expression evaluator, `#error`, `#warning`, and `#pragma once` all work.
//!   Four features are missing: `#` stringizing, `##` pasting, `__VA_ARGS__`, and
//!   `#line`. Each one reports an error instead of failing in silence. A small header
//!   set ships inside the binary: `limits.h`, `stddef.h`, `stdbool.h`, `stdint.h`,
//!   `stdio.h`, `string.h`, and `stdlib.h`. `-I <dir>` adds a search directory, and a
//!   header there replaces a bundled header of the same name.
//! - **`-E` prints the preprocessed source and exits.** This is the fastest way to see
//!   what macro expansion produced.
//! - **`long` has the wrong width for the target.** The other scalars use true C
//!   widths, so `char` is 1 byte and `int` is 4, and the code generator sizes every
//!   load, store, and stack slot from them. But `long` is 8 bytes here, which is the
//!   Linux LP64 width. This compiler emits for Windows, which is LLP64, where `long`
//!   is 4 bytes. A later phase makes the width depend on the target.
//!   [`ast::Type::size`] and [`ast::Type::align`] are the one place that decides this.
//! - **A call takes at most four arguments.** The code generator reports an error past
//!   `rcx`, `rdx`, `r8`, and `r9`, because stack arguments do not exist yet. `printf`
//!   therefore accepts a format string and three values.
//!
//! # Further reading
//!
//! `docs/architecture.md` in the repository describes each stage in more depth. It
//! covers the calling convention, the rules the code generator follows, and recipes for
//! adding an instruction, an operator, or a statement. `docs/CONTRIBUTING.md` covers the
//! build, the tests, and how to debug the compiler.

pub mod lexer;
pub mod parser;
pub mod preprocessor;
pub mod ast;
pub mod codegen;
pub mod assembler;
pub mod assembler_driver;
pub mod diagnostic;
pub mod typeck;
pub mod constfold;