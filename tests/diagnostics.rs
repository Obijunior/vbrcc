use vbrcc::lexer::Lexer;
use vbrcc::parser::Parser;
use vbrcc::codegen::Codegen;
use vbrcc::diagnostic::{render, CompileError};

fn lex_err(src: &str) -> CompileError {
    Lexer::new(src).tokenize().unwrap_err()
}

fn parse_err(src: &str) -> CompileError {
    let toks = Lexer::new(src).tokenize().unwrap();
    Parser::new(toks).parse_program().unwrap_err()
}

fn codegen_err(src: &str) -> CompileError {
    let toks = Lexer::new(src).tokenize().unwrap();
    let program = Parser::new(toks).parse_program().unwrap();
    Codegen::new().generate(&program).unwrap_err()
}

#[test]
fn lexer_error_renders_frame() {
    let src = "int main() { int x = @; }";
    let err = lex_err(src);
    let out = render("prog.c", src, &err, false);
    assert!(out.contains("error: unexpected character `@`"), "got:\n{out}");
    assert!(out.contains("--> prog.c:1:22"), "got:\n{out}");
}

#[test]
fn parser_error_renders_frame() {
    let src = "int main() { return 42 }";
    let err = parse_err(src);
    let out = render("prog.c", src, &err, false);
    assert!(out.contains("error: expected `;`, found `}`"), "got:\n{out}");
    assert!(out.contains("^ expected `;` here"), "got:\n{out}");
}

#[test]
fn codegen_error_renders_frame() {
    let src = "int main() {\n    return x;\n}";
    let err = codegen_err(src);
    let out = render("prog.c", src, &err, false);
    assert!(out.contains("error: undefined variable `x`"), "got:\n{out}");
    assert!(out.contains("--> prog.c:2:12"), "got:\n{out}");
    assert!(out.contains("^ not found in this scope"), "got:\n{out}");
}