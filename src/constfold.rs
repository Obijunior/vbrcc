//! compile-time evaluation of constant expressions.

use crate::ast::{BinaryOp, Expr, TypedExpr, UnaryOp};
use crate::diagnostic::CompileError;

/// The folded value of a constant expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Int(i64),
    Bytes(Vec<u8>),
}

/// Fold `e` to a [`ConstValue`], or report why it is not a constant.
pub fn eval_const(e: &TypedExpr) -> Result<ConstValue, CompileError> {
    match &e.node {
        Expr::IntLiteral(n) => Ok(ConstValue::Int(*n)),
        Expr::StringLiteral(s) => {
            let mut bytes = s.clone().into_bytes();
            bytes.push(0);
            Ok(ConstValue::Bytes(bytes))
        }
        Expr::Cast(_, inner) => eval_const(inner),
        Expr::UnaryOp(op, inner) => {
            let v = eval_int(inner)?;
            let r = match op {
                UnaryOp::Negate => v.wrapping_neg(),
                UnaryOp::BitNot => !v,
                UnaryOp::LogNot => (v == 0) as i64,
            };
            Ok(ConstValue::Int(r))
        }
        Expr::BinaryOp(op, l, r) => {
            let a = eval_int(l)?;
            let b = eval_int(r)?;
            let value = match op {
                BinaryOp::Div | BinaryOp::Mod if b == 0 => {
                    return Err(CompileError::new(
                        "division by zero in constant expression",
                        e.span,
                    ));
                }
                BinaryOp::Add => a.wrapping_add(b),
                BinaryOp::Sub => a.wrapping_sub(b),
                BinaryOp::Mul => a.wrapping_mul(b),
                BinaryOp::Div => a.wrapping_div(b),
                BinaryOp::Mod => a.wrapping_rem(b),
                BinaryOp::Lt => (a < b) as i64,
                BinaryOp::Lte => (a <= b) as i64,
                BinaryOp::Gt => (a > b) as i64,
                BinaryOp::Gte => (a >= b) as i64,
                BinaryOp::Eq => (a == b) as i64,
                BinaryOp::Neq => (a != b) as i64,
                BinaryOp::LogicalAnd => (a != 0 && b != 0) as i64,
                BinaryOp::LogicalOr => (a != 0 || b != 0) as i64,
            };
            Ok(ConstValue::Int(value))
        }
        _ => Err(CompileError::new(
            "initializer element is not a constant",
            e.span,
        )),
    }
}

/// Fold `e` and require an integer result.
fn eval_int(e: &TypedExpr) -> Result<i64, CompileError> {
    match eval_const(e)? {
        ConstValue::Int(n) => Ok(n),
        ConstValue::Bytes(_) => Err(CompileError::new(
            "expected an integer constant, found a string",
            e.span,
        )),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    /// Fold the initializer of `int g = <src>;` (or the given full program).
    fn fold(src: &str) -> Result<ConstValue, CompileError> {
        let program_src = format!("int g = {src};");
        let tokens = Lexer::new(&program_src).tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        eval_const(program.globals[0].init.as_ref().unwrap())
    }

    #[test]
    fn folds_a_literal() {
        assert_eq!(fold("5").unwrap(), ConstValue::Int(5));
    }

    #[test]
    fn folds_arithmetic_with_precedence() {
        assert_eq!(fold("2 + 3 * 4").unwrap(), ConstValue::Int(14));
    }

    #[test]
    fn folds_unary_minus() {
        assert_eq!(fold("-7").unwrap(), ConstValue::Int(-7));
    }

    #[test]
    fn folds_a_comparison_to_zero_or_one() {
        assert_eq!(fold("5 > 2").unwrap(), ConstValue::Int(1));
        assert_eq!(fold("1 == 2").unwrap(), ConstValue::Int(0));
    }

    #[test]
    fn rejects_division_by_zero() {
        let err = fold("1 / 0").unwrap_err();
        assert!(err.message.contains("division by zero"), "got: {}", err.message);
    }

    #[test]
    fn rejects_a_non_constant() {
        let err = fold("foo()").unwrap_err();
        assert!(err.message.contains("not a constant"), "got: {}", err.message);
    }

    #[test]
    fn folds_a_string_to_nul_terminated_bytes() {
        let tokens = Lexer::new("char s[] = \"hi\";").tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        assert_eq!(
            eval_const(program.globals[0].init.as_ref().unwrap()).unwrap(),
            ConstValue::Bytes(vec![b'h', b'i', 0]),
        );
    }
}