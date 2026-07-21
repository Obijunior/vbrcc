use vbrcc::lexer::Lexer;
use vbrcc::parser::Parser;
use vbrcc::ast::*;
use vbrcc::diagnostic::{Span, Spanned};

fn parse(source: &str) -> Result<Program, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().map_err(|e| e.message)?;
    let mut parser = Parser::new(tokens);
    parser.parse_program().map_err(|e| e.message)
}

fn e(x: Expr) -> TypedExpr { TypedExpr::new(x, Span::dummy()) }
fn s(x: Stmt) -> Spanned<Stmt> { Spanned::new(x, Span::dummy()) }

#[test]
fn parse_return_literal_program() {
    let program = parse("int main() { return 42; }").unwrap();
    assert_eq!(program.functions.len(), 1);
    let f = &program.functions[0];
    assert_eq!(f.name, "main");
    assert_eq!(f.params, Vec::<(Type, String)>::new());
    assert_eq!(f.return_type, Type::Int);
    assert_eq!(f.body, vec![s(Stmt::Return(e(Expr::IntLiteral(42))))]);
}

#[test]
fn parse_multi_parameter_function() {
    let program = parse("int add(int a, int b) { return a + b; }").unwrap();
    let f = &program.functions[0];
    assert_eq!(f.params, vec![
        (Type::Int, "a".to_string()),
        (Type::Int, "b".to_string()),
    ]);
}

#[test]
fn parse_unary_negate_in_return_statement() {
    let program = parse("int main() { return -42; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], s(Stmt::Return(e(Expr::UnaryOp(
        UnaryOp::Negate,
        Box::new(e(Expr::IntLiteral(42))),
    )))));
}

#[test]
fn parse_binary_addition_expression() {
    let program = parse("int main() { return 1 + 2; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], s(Stmt::Return(e(Expr::BinaryOp(
        BinaryOp::Add,
        Box::new(e(Expr::IntLiteral(1))),
        Box::new(e(Expr::IntLiteral(2))),
    )))));
}

#[test]
fn parse_operator_precedence_multiplication_before_addition() {
    let program = parse("int main() { return 1 + 2 * 3; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], s(Stmt::Return(e(Expr::BinaryOp(
        BinaryOp::Add,
        Box::new(e(Expr::IntLiteral(1))),
        Box::new(e(Expr::BinaryOp(
            BinaryOp::Mul,
            Box::new(e(Expr::IntLiteral(2))),
            Box::new(e(Expr::IntLiteral(3))),
        ))),
    )))));
}

#[test]
fn parse_missing_semicolon_returns_error() {
    assert!(parse("int main() { return 42 }").is_err());
}

#[test]
fn parse_var_decl_and_return() {
    let program = parse("int main() { int x = 5; return x; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], s(Stmt::VarDecl { ty: Type::Int, name: "x".into(), init: Some(e(Expr::IntLiteral(5))) }));
    assert_eq!(body[1], s(Stmt::Return(e(Expr::Var("x".into())))));
}

#[test]
fn parse_for_loop() {
    let program = parse("int main() { for (int i = 0; i < 10; i++) { } return 0; }").unwrap();
    let body = &program.functions[0].body;
    match &body[0].node {
        Stmt::For { init, cond, update, body } => {
            assert_eq!(**init, s(Stmt::VarDecl { ty: Type::Int, name: "i".into(), init: Some(e(Expr::IntLiteral(0))) }));
            assert_eq!(*cond, e(Expr::BinaryOp(
                BinaryOp::Lt,
                Box::new(e(Expr::Var("i".into()))),
                Box::new(e(Expr::IntLiteral(10))),
            )));
            match &update.node {
                Stmt::Expr(inner) => match &inner.node {
                    Expr::Assign(lval, _) => match &lval.node {
                        Expr::Var(name) => assert_eq!(name, "i"),
                        other => panic!("expected var lvalue, got {:?}", other),
                    },
                    other => panic!("expected assignment update, got {:?}", other),
                },
                other => panic!("expected expr update, got {:?}", other),
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
    match &body[1].node {
        Stmt::While { cond, body } => {
            assert_eq!(*cond, e(Expr::BinaryOp(
                BinaryOp::Lt,
                Box::new(e(Expr::Var("i".into()))),
                Box::new(e(Expr::IntLiteral(5))),
            )));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected While statement, got {:?}", other),
    }
}

#[test]
fn parse_if_else() {
    let program = parse("int main() { int x = 0; if (x < 1) { x = 1; } else { x = 2; } return x; }").unwrap();
    let body = &program.functions[0].body;
    match &body[1].node {
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
    assert_eq!(body[1], s(Stmt::Expr(e(Expr::Assign(
        Box::new(e(Expr::Var("x".into()))),
        Box::new(e(Expr::BinaryOp(
            BinaryOp::Add,
            Box::new(e(Expr::Var("x".into()))),
            Box::new(e(Expr::IntLiteral(5))),
        ))),
    )))));
}

#[test]
fn parse_logical_and() {
    let program = parse("int main() { return 1 && 2; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], s(Stmt::Return(e(Expr::BinaryOp(
        BinaryOp::LogicalAnd,
        Box::new(e(Expr::IntLiteral(1))),
        Box::new(e(Expr::IntLiteral(2))),
    )))));
}

#[test]
fn parse_logical_or() {
    let program = parse("int main() { return 0 || 1; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], s(Stmt::Return(e(Expr::BinaryOp(
        BinaryOp::LogicalOr,
        Box::new(e(Expr::IntLiteral(0))),
        Box::new(e(Expr::IntLiteral(1))),
    )))));
}

#[test]
fn parse_logical_and_binds_tighter_than_or() {
    let program = parse("int main() { return 0 || 1 && 2; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[0], s(Stmt::Return(e(Expr::BinaryOp(
        BinaryOp::LogicalOr,
        Box::new(e(Expr::IntLiteral(0))),
        Box::new(e(Expr::BinaryOp(
            BinaryOp::LogicalAnd,
            Box::new(e(Expr::IntLiteral(1))),
            Box::new(e(Expr::IntLiteral(2))),
        ))),
    )))));
}

#[test]
fn parse_logical_ops_bind_looser_than_comparison() {
    let program = parse("int main() { int a = 1; int b = 4; return a < 5 && b > 3; }").unwrap();
    let body = &program.functions[0].body;
    assert_eq!(body[2], s(Stmt::Return(e(Expr::BinaryOp(
        BinaryOp::LogicalAnd,
        Box::new(e(Expr::BinaryOp(
            BinaryOp::Lt,
            Box::new(e(Expr::Var("a".into()))),
            Box::new(e(Expr::IntLiteral(5))),
        ))),
        Box::new(e(Expr::BinaryOp(
            BinaryOp::Gt,
            Box::new(e(Expr::Var("b".into()))),
            Box::new(e(Expr::IntLiteral(3))),
        ))),
    )))));
}