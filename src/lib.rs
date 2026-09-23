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
//! executable. It uses no external compiler library. In its default mode it needs no
//! external toolchain: it encodes the x86-64 bytes and writes the PE container itself.
//!
//! This crate is **binary-first**. The `vbrcc` command-line tool is the interface. The
//! modules are public so that you can inspect the pipeline, but they are not a stable
//! API before 1.0.
//!
//! The compiler runs anywhere Rust runs. It emits only Windows PE/COFF binaries for
//! x86-64.
//!
//! The README lists the supported C subset, the command-line flags, and the backends.
//! `docs/architecture.md` describes each stage and the rules the code generator follows.
//!
//! # The pipeline
//!
//! Each stage gives its output to the next stage. The compiler stops at the first
//! [`diagnostic::CompileError`] and reports only that one.
//!
//! | Module | Stage |
//! |---|---|
//! | [`preprocessor`] | Directives and macro expansion. Calls the lexer one line at a time |
//! | [`lexer`] | Turns source text into tokens with source spans |
//! | [`parser`] | Recursive-descent parser. Builds the AST |
//! | [`ast`] | AST node definitions and the [`ast::Type`] enum |
//! | [`typeck`] | Assigns a type to every expression. Reports type errors |
//! | [`constfold`] | Folds constant expressions for global initializers and enumerators |
//! | [`codegen`] | Walks the typed AST and emits Intel-syntax assembly text |
//! | [`assembler`] | Encodes the assembly into a PE executable or a COFF object |
//! | [`assembler_driver`] | Selects the output mode and calls any external linker |
//! | [`diagnostic`] | [`diagnostic::CompileError`], [`diagnostic::Span`], and rustc-style rendering |
//!
//! # Limits to know about
//!
//! - The default backend imports only from `msvcrt.dll`. `--lld-link` adds `kernel32`.
//!   A call into any other DLL builds and then fails at load time.
//! - A call takes at most four arguments, because stack arguments do not exist yet. The
//!   code generator reports an error past `rcx`, `rdx`, `r8`, and `r9`.
//! - A cast changes the type of a value, not its bits. `(char)300` stays 300.

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