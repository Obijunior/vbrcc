use vbrcc::codegen::Codegen;

fn compile(source: &str) -> String {
    let mut lexer = vbrcc::lexer::Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = vbrcc::parser::Parser::new(tokens);
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

#[test]
fn test_full_compilation_logical_and() {
    let asm = compile("int main() { return 1 && 2; }");
    assert!(asm.contains("je and_0_false"));
    assert!(asm.contains("and_0_false:"));
    assert!(asm.contains("and_0_end:"));
}

#[test]
fn test_full_compilation_logical_or() {
    let asm = compile("int main() { return 0 || 1; }");
    assert!(asm.contains("jne or_0_true"));
    assert!(asm.contains("or_0_true:"));
    assert!(asm.contains("or_0_end:"));
}

#[test]
fn test_full_compilation_logical_and_in_if_condition() {
    let asm = compile(
        "int main() { int x = 5; if (x < 10 && x > 3) { return 1; } return 0; }"
    );
    // if grabs label 0 first, then && inside the condition gets label 1
    assert!(asm.contains("and_1_false:"));
    assert!(asm.contains("and_1_end:"));
    assert!(asm.contains("if_0_end:"));
}

#[test]
fn test_full_compilation_chained_logical_or() {
    let asm = compile("int main() { return 0 || 0 || 1; }");
    assert!(asm.contains("or_0_true:"));
    assert!(asm.contains("or_1_true:"));
}