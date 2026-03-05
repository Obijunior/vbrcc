use rust_c_compiler::codegen::Codegen;

#[test]
fn test_full_compilation() {
    let source = "int main() { return 42; }";
    let mut lexer = rust_c_compiler::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = rust_c_compiler::parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let mut codegen = Codegen::new();
    let asm = codegen.generate(&program).unwrap();

    // Just check that the generated assembly contains the expected instructions
    assert!(asm.contains("mov rax, 42"));
    assert!(asm.contains("ret"));
}