mod lexer;

use std::fs;
use std::path::PathBuf;
use std::process;

use crate::lexer::Lexer;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <source-file.c> [-o <output>]", args[0]);
        process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);

    // parse -o flag, defaults to "a.out"
    let output_path = args
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("a.out"));

    // read source file
    let source = fs::read_to_string(&input_path).unwrap_or_else(|e| {
        eprintln!("Error reading {:?}: {}", input_path, e);
        process::exit(1);
    });

    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize();

    println!("Lexed {} tokens", tokens.len());
    println!("Output would go to: {:?}", output_path);
}