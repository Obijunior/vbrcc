use std::fs;
use std::path::PathBuf;
use std::process;

mod ast;
mod lexer;
mod parser;
mod codegen;
mod assembler_driver;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.c> [-o <output>] [-gcc]", args[0]);
        process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);
    let use_gcc = args.iter().any(|a| a == "-gcc");
    let output_path = args
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| input_path.with_extension("s"));

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

    println!("Wrote assembly to {:?}", asm_path);

    // --- Assemble and link ---
    let bin_path = output_path.with_extension("");
    assembler_driver::assemble_and_link(&asm_path, &bin_path, use_gcc).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: {}", e);
        process::exit(1);
    });

    println!("Compiled binary to {:?}", bin_path);
}
