use rust_c_compiler::codegen::Codegen;

fn compile(source: &str) -> String {
    let mut lexer = rust_c_compiler::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = rust_c_compiler::parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let mut codegen = Codegen::new();
    codegen.generate(&program).unwrap()
}

#[test]
fn test_full_compilation() {
    let asm = compile("int main() { return 42; }");
    assert!(asm.contains("mov rax, 42"));
    assert!(asm.contains("ret"));
}

#[test]
fn test_full_compilation_variable_roundtrip() {
    let asm = compile("int main() { int x = 7; return x; }");
    assert!(asm.contains("mov rax, 7"));
    assert!(asm.contains("mov [rbp - 8], rax"));
    assert!(asm.contains("mov rax, [rbp - 8]"));
    assert!(asm.contains("ret"));
}

#[test]
fn test_full_compilation_for_loop() {
    let asm = compile(
        "int main() { int sum = 0; for (int i = 0; i < 10; i++) { sum += i; } return sum; }"
    );
    assert!(asm.contains("loop_0_start:"));
    assert!(asm.contains("loop_0_end:"));
    assert!(asm.contains("setl al"));
    assert!(asm.contains("jmp loop_0_start"));
    assert!(asm.contains("je loop_0_end"));
    assert!(asm.contains("ret"));
}

#[test]
fn test_full_compilation_while_loop() {
    let asm = compile(
        "int main() { int i = 10; while (i > 0) { i--; } return i; }"
    );
    assert!(asm.contains("loop_0_start:"));
    assert!(asm.contains("loop_0_end:"));
    assert!(asm.contains("setg al"));
}

#[test]
fn test_full_compilation_if_else() {
    let asm = compile(
        "int main() { int x = 5; if (x < 10) { x = 1; } else { x = 0; } return x; }"
    );
    assert!(asm.contains("if_0_else:"));
    assert!(asm.contains("if_0_end:"));
}

#[test]
fn test_full_compilation_nested_loops() {
    let asm = compile(
        "int main() { int s = 0; for (int i = 0; i < 3; i++) { for (int j = 0; j < 3; j++) { s += 1; } } return s; }"
    );
    assert!(asm.contains("loop_0_start:"));
    assert!(asm.contains("loop_0_end:"));
    assert!(asm.contains("loop_1_start:"));
    assert!(asm.contains("loop_1_end:"));
}