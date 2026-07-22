//! Stage 3: assigning a type to every expression, and rejecting the ones that don't work.
//!
//! [`check`] walks the AST produced by [`crate::parser`] and mutates it in place,
//! writing a resolved [`Type`] into the `ty` field of every `TypedExpr`. It runs before
//! code generation, so the code generator can assume every expression is typed.
//!
//! # Errors reported here
//!
//! - Use of an undeclared variable.
//! - Dereferencing a value that is not a pointer.
//! - Indexing a value that is not a pointer or an array.
//! - Assigning to something that is not an lvalue. An lvalue is a variable, a
//!   dereference, or an index. The parser accepts any expression on the left of `=`,
//!   and this is where that gets caught.
//!
//! # Scoping
//!
//! The scope is a single flat `HashMap<String, Type>` for the whole function. Block-level
//! scope is not implemented, so a variable declared inside an `if` or a loop body remains
//! visible after that block ends, and an inner declaration shadowing an outer one will
//! overwrite it instead. Adding real scoping means replacing this map with a stack of
//! maps and pushing a frame per block.

use crate::ast::*;
use crate::diagnostic::{CompileError, Spanned};
use std::collections::HashMap;

pub fn check(program: &mut Program) -> Result<(), CompileError> {
    for func in &mut program.functions {
        let mut scope: HashMap<String, Type> = HashMap::new();
        for (ty, name) in &func.params {
            scope.insert(name.clone(), ty.clone());
        }
        check_block(&mut func.body, &mut scope)?;
    }
    Ok(())
}

fn check_block(stmts: &mut [Spanned<Stmt>], scope: &mut HashMap<String, Type>) -> Result<(), CompileError> {
    for stmt in stmts {
        check_stmt(&mut stmt.node, scope)?;
    }
    Ok(())
}

fn check_stmt(stmt: &mut Stmt, scope: &mut HashMap<String, Type>) -> Result<(), CompileError> {
    match stmt {
        Stmt::Return(e) | Stmt::Expr(e) => check_expr(e, scope)?,
        Stmt::VarDecl { ty, name, init } => {
            if let Some(e) = init {
                check_expr(e, scope)?;
            }
            scope.insert(name.clone(), ty.clone());
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

fn is_lvalue(e: &Expr) -> bool {
    matches!(e, Expr::Var(_) | Expr::Deref(_) | Expr::Index(_, _))
}

fn check_expr(expr: &mut TypedExpr, scope: &mut HashMap<String, Type>) -> Result<(), CompileError> {
    let span = expr.span;
    let ty: Type = match &mut expr.node {
        Expr::IntLiteral(_) => Type::Int,
        Expr::StringLiteral(_) => Type::Pointer(Box::new(Type::Char)),
        Expr::Var(name) => scope.get(name).cloned().ok_or_else(|| {
            CompileError::new(format!("undefined variable `{name}`"), span)
                .with_label("not found in this scope")
        })?,
        Expr::UnaryOp(_, inner) => {
            check_expr(inner, scope)?;
            Type::Int
        }
        Expr::BinaryOp(op, l, r) => {
            check_expr(l, scope)?;
            check_expr(r, scope)?;
            let lt = l.ty.decay();
            if matches!(lt, Type::Pointer(_)) && matches!(op, BinaryOp::Add | BinaryOp::Sub) {
                lt
            } else {
                Type::Int
            }
        }
        Expr::AddressOf(inner) => {
            check_expr(inner, scope)?;
            if !is_lvalue(&inner.node) {
                return Err(CompileError::new("cannot take the address of this expression", span)
                    .with_label("not an lvalue"));
            }
            Type::Pointer(Box::new(inner.ty.clone()))
        }
        Expr::Deref(inner) => {
            check_expr(inner, scope)?;
            match inner.ty.pointee() {
                Some(t) => t,
                None => {
                    return Err(CompileError::new(
                        format!("cannot dereference value of type `{}`", inner.ty.describe()),
                        span,
                    )
                    .with_label("expected a pointer"));
                }
            }
        }
        Expr::Index(base, idx) => {
            check_expr(base, scope)?;
            check_expr(idx, scope)?;
            match base.ty.pointee() {
                Some(t) => t,
                None => {
                    return Err(CompileError::new(
                        format!("cannot index value of type `{}`", base.ty.describe()),
                        span,
                    )
                    .with_label("expected a pointer or array"));
                }
            }
        }
        Expr::Cast(t, inner) => {
            check_expr(inner, scope)?;
            t.clone()
        }
        Expr::Assign(lval, rhs) => {
            check_expr(lval, scope)?;
            check_expr(rhs, scope)?;
            if !is_lvalue(&lval.node) {
                return Err(CompileError::new("cannot assign to this expression", span)
                    .with_label("not an lvalue"));
            }
            lval.ty.clone()
        }
        Expr::FunctionCall { args, .. } => {
            for a in args {
                check_expr(a, scope)?;
            }
            Type::Int
        }
    };
    expr.ty = ty;
    Ok(())
}


/* ===================================== */
//                                       //
//        Unit tests for type checker    //
//                                       // 
/* ===================================== */

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

        #[test]
    fn address_of_yields_pointer() {
        let program = typecheck("int main() { int x = 1; int *p = &x; return 0; }").unwrap();
        // the initializer `&x` on body[1] is a pointer-to-int
        if let Stmt::VarDecl { init: Some(e), .. } = &program.functions[0].body[1].node {
            assert_eq!(e.ty, Type::Pointer(Box::new(Type::Int)));
        } else { panic!("expected var decl with init"); }
    }

    #[test]
    fn deref_of_non_pointer_is_error() {
        let src = "int main() { int x = 1; return *x; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains("dereference"), "message: {}", err.message);
    }

    #[test]
    fn index_of_non_pointer_is_error() {
        let src = "int main() { int x = 1; return x[0]; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains("index"), "message: {}", err.message);
    }

    #[test]
    fn assign_to_non_lvalue_is_error() {
        let src = "int main() { 1 = 2; return 0; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains("assign"), "message: {}", err.message);
    }
}