use crate::ast::*;
use crate::diagnostic::{CompileError, Spanned};
use std::collections::HashSet;

pub fn check(program: &mut Program) -> Result<(), CompileError> {
    for func in &mut program.functions {
        let mut scope: HashSet<String> = HashSet::new();
        for (_ty, name) in &func.params {
            scope.insert(name.clone());
        }
        check_block(&mut func.body, &mut scope)?;
    }
    Ok(())
}

fn check_block(stmts: &mut [Spanned<Stmt>], scope: &mut HashSet<String>) -> Result<(), CompileError> {
    for stmt in stmts {
        check_stmt(&mut stmt.node, scope)?;
    }
    Ok(())
}

fn check_stmt(stmt: &mut Stmt, scope: &mut HashSet<String>) -> Result<(), CompileError> {
    match stmt {
        Stmt::Return(e) | Stmt::Expr(e) => check_expr(e, scope)?,
        Stmt::VarDecl { name, init, .. } => {
            if let Some(e) = init {
                check_expr(e, scope)?;
            }
            scope.insert(name.clone());
        }
        Stmt::If { cond, then_branch, else_branch } => {
            check_expr(cond, scope)?;
            check_block(then_branch, scope)?;
            check_block(else_branch, scope)?;
        }
        Stmt::While { cond, body } => {
            check_expr(cond, scope)?;
            check_block(body, scope)?;
        }
        Stmt::For { init, cond, update, body } => {
            check_stmt(&mut init.node, scope)?;
            check_expr(cond, scope)?;
            check_stmt(&mut update.node, scope)?;
            check_block(body, scope)?;
        }
    }
    Ok(())
}

fn check_expr(expr: &mut TypedExpr, scope: &mut HashSet<String>) -> Result<(), CompileError> {
    let span = expr.span;
    match &mut expr.node {
        Expr::IntLiteral(_) | Expr::StringLiteral(_) => {}
        Expr::Var(name) => {
            if !scope.contains(name) {
                return Err(CompileError::new(format!("undefined variable `{name}`"), span)
                    .with_label("not found in this scope"));
            }
        }
        Expr::UnaryOp(_, inner) => check_expr(inner, scope)?,
        Expr::BinaryOp(_, l, r) => {
            check_expr(l, scope)?;
            check_expr(r, scope)?;
        }
        Expr::Assign(name, value) => {
            if !scope.contains(name) {
                return Err(CompileError::new(format!("undefined variable `{name}`"), span)
                    .with_label("not found in this scope"));
            }
            check_expr(value, scope)?;
        }
        Expr::FunctionCall { args, .. } => {
            for a in args {
                check_expr(a, scope)?;
            }
        }
    }
    expr.ty = Type::Int;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn typecheck(src: &str) -> Result<Program, CompileError> {
        let tokens = Lexer::new(src).tokenize().unwrap();
        let mut program = Parser::new(tokens).parse_program().unwrap();
        check(&mut program)?;
        Ok(program)
    }

    #[test]
    fn well_typed_program_annotates_int() {
        let program = typecheck("int main() { int x = 5; return x; }").unwrap();
        let body = &program.functions[0].body;
        match &body[1].node {
            Stmt::Return(e) => assert_eq!(e.ty, Type::Int),
            other => panic!("expected return, got {:?}", other),
        }
    }

    #[test]
    fn undefined_variable_is_located_error() {
        let src = "int main() { return y; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains('y'), "message: {}", err.message);
        assert_eq!(err.span.start, src.find('y').unwrap());
    }
}