use vbrcc::lexer::Lexer;
use vbrcc::parser::Parser;
use vbrcc::ast::*;

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
                params: vec![],
                return_type: "int".to_string(),
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

#[test]
fn parse_var_decl_and_return() {
    let program = parse("int main() { int x = 5; return x; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], Stmt::VarDecl { name: "x".into(), init: Some(Expr::IntLiteral(5)) });
    assert_eq!(body[1], Stmt::Return(Expr::Var("x".into())));
}

#[test]
fn parse_for_loop() {
    let program = parse("int main() { for (int i = 0; i < 10; i++) { } return 0; }").unwrap();
    let body = &program.functions[0].body;
    match &body[0] {
        Stmt::For { init, cond, update, body } => {
            assert_eq!(**init, Stmt::VarDecl { name: "i".into(), init: Some(Expr::IntLiteral(0)) });
            assert_eq!(*cond, Expr::BinaryOp(
                BinaryOp::Lt,
                Box::new(Expr::Var("i".into())),
                Box::new(Expr::IntLiteral(10)),
            ));
            match &**update {
                Stmt::Expr(Expr::Assign(name, _)) => assert_eq!(name, "i"),
                other => panic!("expected assignment update, got {:?}", other),
            }
            assert!(body.is_empty());
        }
        other => panic!("expected For statement, got {:?}", other),
    }
}

#[test]
fn parse_while_loop() {
    let program = parse("int main() { int i = 0; while (i < 5) { i++; } return i; }").unwrap();
    let body = &program.functions[0].body;
    match &body[1] {
        Stmt::While { cond, body } => {
            assert_eq!(*cond, Expr::BinaryOp(
                BinaryOp::Lt,
                Box::new(Expr::Var("i".into())),
                Box::new(Expr::IntLiteral(5)),
            ));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected While statement, got {:?}", other),
    }
}

#[test]
fn parse_if_else() {
    let program = parse("int main() { int x = 0; if (x < 1) { x = 1; } else { x = 2; } return x; }").unwrap();
    let body = &program.functions[0].body;
    match &body[1] {
        Stmt::If { cond: _, then_branch, else_branch } => {
            assert_eq!(then_branch.len(), 1);
            assert_eq!(else_branch.len(), 1);
        }
        other => panic!("expected If statement, got {:?}", other),
    }
}

#[test]
fn parse_compound_assignment_in_program() {
    let program = parse("int main() { int x = 0; x += 5; return x; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[1], Stmt::Expr(Expr::Assign(
        "x".into(),
        Box::new(Expr::BinaryOp(
            BinaryOp::Add,
            Box::new(Expr::Var("x".into())),
            Box::new(Expr::IntLiteral(5)),
        )),
    )));
}

#[test]
fn parse_logical_and() {
    let program = parse("int main() { return 1 && 2; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(
        body[0],
        Stmt::Return(Expr::BinaryOp(
            BinaryOp::LogicalAnd,
            Box::new(Expr::IntLiteral(1)),
            Box::new(Expr::IntLiteral(2)),
        ))
    );
}

#[test]
fn parse_logical_or() {
    let program = parse("int main() { return 0 || 1; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(
        body[0],
        Stmt::Return(Expr::BinaryOp(
            BinaryOp::LogicalOr,
            Box::new(Expr::IntLiteral(0)),
            Box::new(Expr::IntLiteral(1)),
        ))
    );
}

#[test]
fn parse_logical_and_binds_tighter_than_or() {
    // a || b && c  should parse as  a || (b && c)
    let program = parse("int main() { return 0 || 1 && 2; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(
        body[0],
        Stmt::Return(Expr::BinaryOp(
            BinaryOp::LogicalOr,
            Box::new(Expr::IntLiteral(0)),
            Box::new(Expr::BinaryOp(
                BinaryOp::LogicalAnd,
                Box::new(Expr::IntLiteral(1)),
                Box::new(Expr::IntLiteral(2)),
            )),
        ))
    );
}

#[test]
fn parse_logical_ops_bind_looser_than_comparison() {
    // a < 5 && b > 3  should parse as  (a < 5) && (b > 3)
    let program = parse("int main() { int a = 1; int b = 4; return a < 5 && b > 3; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(
        body[2],
        Stmt::Return(Expr::BinaryOp(
            BinaryOp::LogicalAnd,
            Box::new(Expr::BinaryOp(
                BinaryOp::Lt,
                Box::new(Expr::Var("a".into())),
                Box::new(Expr::IntLiteral(5)),
            )),
            Box::new(Expr::BinaryOp(
                BinaryOp::Gt,
                Box::new(Expr::Var("b".into())),
                Box::new(Expr::IntLiteral(3)),
            )),
        ))
    );
}