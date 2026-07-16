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

use std::fs;
use std::path::PathBuf;
use std::process;

mod ast;
mod lexer;
mod parser;
mod codegen;
mod assembler;
mod assembler_driver;
mod diagnostic;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.c> [-o <output>] [-gcc] [-lld-link] [--keep-artifacts]", args[0]);
        process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);
    let use_gcc = args.iter().any(|a| a == "-gcc" || a == "--gcc");
    let use_lld = args.iter().any(|a| a == "-lld-link" || a == "--lld-link");
    let keep_artifacts = args.iter().any(|a| a == "-keep" || a == "--keep-artifacts");
    let output_path = args
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| input_path.with_extension(""));

    // Read source file
    let source = fs::read_to_string(&input_path).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: Error reading {:?}: {}", input_path, e);
        process::exit(1);
    });

    // --- Stage 1: Lex ---
    let mut lexer = lexer::Lexer::new(&source);
    let tokens = lexer.tokenize();

    if std::env::var("DUMP_TOKENS").is_ok() {
        eprintln!("=== TOKENS ===");
        for tok in &tokens {
            eprintln!("{:?}", tok);
        }
    }

    // --- Stage 2: Parse ---
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: Parse error: {}", e);
        process::exit(1);
    });

    if std::env::var("DUMP_AST").is_ok() {
        eprintln!("=== AST ===");
        eprintln!("{:#?}", program);
    }

    // --- Stage 3: Codegen ---
    let mut codegen = codegen::Codegen::new();
    let asm = codegen.generate(&program).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: Codegen error: {}", e);
        process::exit(1);
    });

    if std::env::var("DUMP_ASM").is_ok() {
        eprintln!("=== ASSEMBLY ===");
        eprintln!("{}", asm);
    }

    // --- Write .s file ---
    let asm_path = output_path.with_extension("s");
    fs::write(&asm_path, &asm).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: Error writing {:?}: {}", asm_path, e);
        process::exit(1);
    });

    println!("[ SUCCESS ] :: Wrote assembly to {:?}", asm_path);

    // --- Assemble and link ---
    let bin_path = if use_gcc || use_lld {
        output_path.with_extension("exe")
    } else if output_path.extension().is_none() {
        output_path.with_extension("exe")
    } else {
        output_path.clone()
    };

    // will likely need to add better arg parsing later, but for now this is fine
    let linker = if use_gcc {
        assembler_driver::LinkerMode::Gcc
    } else if use_lld {
        assembler_driver::LinkerMode::LldLink
    } else {
        assembler_driver::LinkerMode::CustomPe
    };

    assembler_driver::assemble_and_link(&asm_path, &bin_path, linker).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: {}", e);
        process::exit(1);
    });

    println!("[ SUCCESS ] :: Compiled binary to {:?}", bin_path);

    // Clean up intermediate artifacts unless --keep-artifacts is passed
    if !keep_artifacts {
        let artifacts = [
            output_path.with_extension("s"),
            output_path.with_extension("obj"),
            output_path.with_extension("def"),
            output_path.with_extension("lib"),
        ];
        for path in &artifacts {
            let _ = fs::remove_file(path);
        }
    }
}
