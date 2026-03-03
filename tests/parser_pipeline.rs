use rust_c_compiler::lexer::Lexer;
use rust_c_compiler::parser::{BinaryOp, Expr, Function, Parser, Program, Stmt, UnaryOp};

fn parse(source: &str) -> Result<Program, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

#[test]
fn parse_return_literal_program() {
    let program = parse("int main() { return 42; }").unwrap();
    assert_eq!(
        program,
        Program {
            functions: vec![Function {
                name: "main".to_string(),
                body: vec![Stmt::Return(Expr::IntLiteral(42))],
            }]
        }
    );
}

#[test]
fn parse_unary_negate_in_return_statement() {
    let program = parse("int main() { return -42; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(
        body[0],
        Stmt::Return(Expr::UnaryOp(
            UnaryOp::Negate,
            Box::new(Expr::IntLiteral(42))
        ))
    );
}

#[test]
fn parse_binary_addition_expression() {
    let program = parse("int main() { return 1 + 2; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(
        body[0],
        Stmt::Return(Expr::BinaryOp(
            BinaryOp::Add,
            Box::new(Expr::IntLiteral(1)),
            Box::new(Expr::IntLiteral(2)),
        ))
    );
}

#[test]
fn parse_operator_precedence_multiplication_before_addition() {
    let program = parse("int main() { return 1 + 2 * 3; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(
        body[0],
        Stmt::Return(Expr::BinaryOp(
            BinaryOp::Add,
            Box::new(Expr::IntLiteral(1)),
            Box::new(Expr::BinaryOp(
                BinaryOp::Mul,
                Box::new(Expr::IntLiteral(2)),
                Box::new(Expr::IntLiteral(3)),
            )),
        ))
    );
}

#[test]
fn parse_missing_semicolon_returns_error() {
    assert!(parse("int main() { return 42 }").is_err());
}