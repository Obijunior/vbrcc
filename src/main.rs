use std::fs;
use std::path::PathBuf;
use std::process;

use rust_c_compiler::lexer::Lexer;
use rust_c_compiler::parser::Parser;

fn parse_output_path(args: &[String]) -> Result<PathBuf, String> {
    args.iter()
        .position(|a| a == "-o")
        .map(|i| {
            args.get(i + 1)
                .map(|p| PathBuf::from(p))
                .ok_or_else(|| "-o flag provided but no output filename specified".to_string())
        })
        .unwrap_or(Ok(PathBuf::from("a.out")))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <source-file.c> [-o <output>]", args[0]);
        process::exit(1);
    }

    let input_path = PathBuf::from(&args[1]);

    // parse -o flag, defaults to "a.out"
    let output_path = match parse_output_path(&args) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("[ ERROR ] :: {}", e);
            process::exit(1);
        }
    };

    // read source file
    let source = fs::read_to_string(&input_path).unwrap_or_else(|e| {
        eprintln!("Error reading {:?}: {}", input_path, e);
        process::exit(1);
    });

    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize();

    let mut parser = Parser::new(tokens.clone());
    let program = parser.parse_program();

    println!("Lexed {} tokens", tokens.len());
    println!("Output would go to: {:?}", output_path);
    println!("Parsed program: {:?}", program);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_path() {
        // Test 1: No -o flag, should default to "a.out"
        let args = vec!["prog".to_string(), "file.c".to_string()];
        let result = parse_output_path(&args);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("a.out"));

        // Test 2: -o flag with filename specified
        let args = vec!["prog".to_string(), "file.c".to_string(), "-o".to_string(), "output.o".to_string()];
        let result = parse_output_path(&args);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("output.o"));

        // Test 3: -o flag without filename, should error
        let args = vec!["prog".to_string(), "file.c".to_string(), "-o".to_string()];
        let result = parse_output_path(&args);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "-o flag provided but no output filename specified");
    }
}
