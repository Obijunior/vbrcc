//! The constant-expression evaluator for `#if` and `#elif`.
//!
//! The caller resolves `defined` and expands the macros first. This module therefore
//! sees only literals, operators, parentheses, and identifiers. C gives an identifier
//! that is not a macro the value `0`, so an unknown name is not an error here.
//!
//! All arithmetic uses `i64`. C99 asks for `intmax_t`, which is `long long` on this
//! target, so 64 bits is the correct width. Do not read this as the width of `long`.
//! This target is LLP64, so `long` is 32 bits and `long long` is 64.
//!
//! # Limits
//!
//! The comma operator is missing. To add a binary operator, add a row to
//! `binding_power` and an arm to `Eval::apply`.
//!
//! Every operand is signed. C99 asks for `uintmax_t` arithmetic when either operand
//! is unsigned, so `#if` will disagree with the compiler proper once `unsigned`
//! exists. Fix both together.

use crate::diagnostic::{CompileError, Span};
use crate::lexer::{SpannedToken, Token};

/// Evaluate one already-expanded directive line.
///
/// `span` covers the whole directive. It is the fallback location for an error
/// that has no token to point at, such as a line that ends too early.
pub fn eval(tokens: &[SpannedToken], span: Span) -> Result<i64, CompileError> {
    let mut ev = Eval { tokens, pos: 0, span, live: true };
    let value = ev.expr(0)?;
    if ev.pos < ev.tokens.len() {
        return Err(ev.error_at(
            ev.pos,
            format!(
                "expected an operator, found {}",
                ev.tokens[ev.pos].token.describe()
            ),
        ));
    }
    Ok(value)
}

struct Eval<'a> {
    tokens: &'a [SpannedToken],
    pos: usize,
    span: Span,
    /// False while parsing an operand that `&&` or `||` already skipped past.
    /// The operand is still parsed, so syntax errors surface, but a division by
    /// zero inside it is not reported.
    live: bool,
}

/// Binding power, lowest first. Every operator here is left-associative.
/// The ternary `?:` sits below all of these and is handled in `expr`.
fn binding_power(tok: &Token) -> Option<u8> {
    Some(match tok {
        Token::LogicalOr => 1,
        Token::LogicalAnd => 2,
        Token::Pipe => 3,
        Token::Caret => 4,
        Token::Ampersand => 5,
        Token::Equals | Token::NotEquals => 6,
        Token::LessThan | Token::LessThanEquals
        | Token::GreaterThan | Token::GreaterThanEquals => 7,
        Token::Shl | Token::Shr => 8,
        Token::Plus | Token::Minus => 9,
        Token::Star | Token::Slash | Token::Modulo => 10,
        _ => return None,
    })
}

impl Eval<'_> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.pos).map(|t| t.token.clone())
    }

    fn error_at(&self, pos: usize, message: String) -> CompileError {
        let span = self.tokens.get(pos).map(|t| t.span).unwrap_or(self.span);
        CompileError::new(message, span)
    }

    /// Precedence climbing: parse a unary operand, then absorb every operator
    /// that binds at least as tightly as `min_bp`.
    fn expr(&mut self, min_bp: u8) -> Result<i64, CompileError> {
        let mut lhs = self.unary()?;

        while let Some(bp) = self.peek().as_ref().and_then(binding_power) {
            if bp < min_bp {
                break;
            }
            let op_pos = self.pos;
            let op = self.tokens[self.pos].token.clone();
            self.pos += 1;

            // `&&` and `||` must not evaluate an operand the left side already
            // settled. The operand is parsed either way, so `#if 0 && (` is
            // still a syntax error.
            let short_circuits = match op {
                Token::LogicalAnd => lhs == 0,
                Token::LogicalOr => lhs != 0,
                _ => false,
            };
            let was_live = self.live;
            if short_circuits {
                self.live = false;
            }
            let rhs = self.expr(bp + 1)?;
            self.live = was_live;

            lhs = self.apply(&op, lhs, rhs, op_pos)?;
        }

        // The ternary binds looser than every operator above and is
        // right-associative. Recognize it only at the top level (and inside
        // parens), which is where `expr` is entered with `min_bp == 0`.
        if min_bp == 0 {
            if let Some(Token::Question) = self.peek() {
                self.pos += 1;
                let cond_true = lhs != 0;
                let outer_live = self.live;

                self.live = outer_live && cond_true;
                let then_v = self.expr(0)?;

                match self.peek() {
                    Some(Token::Colon) => self.pos += 1,
                    _ => {
                        self.live = outer_live;
                        return Err(self.error_at(self.pos, "expected `:` in `?:`".to_string()));
                    }
                }

                self.live = outer_live && !cond_true;
                let else_v = self.expr(0)?;

                self.live = outer_live;
                lhs = if cond_true { then_v } else { else_v };
            }
        }

        Ok(lhs)
    }

    fn unary(&mut self) -> Result<i64, CompileError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.pos += 1;
                Ok(self.unary()?.wrapping_neg())
            }
            Some(Token::Plus) => {
                self.pos += 1;
                self.unary()
            }
            Some(Token::Bang) => {
                self.pos += 1;
                Ok((self.unary()? == 0) as i64)
            }
            Some(Token::Tilde) => {
                self.pos += 1;
                Ok(!self.unary()?)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<i64, CompileError> {
        let pos = self.pos;
        match self.peek() {
            None => Err(CompileError::new("expected an expression", self.span)),
            Some(Token::IntLiteral(v)) | Some(Token::CharLiteral(v)) => {
                self.pos += 1;
                Ok(v)
            }
            // Not a macro, so C makes it zero.
            Some(Token::Ident(_)) => {
                self.pos += 1;
                Ok(0)
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let value = self.expr(0)?;
                match self.peek() {
                    Some(Token::RParen) => {
                        self.pos += 1;
                        Ok(value)
                    }
                    _ => Err(self.error_at(self.pos, "expected `)`".to_string())),
                }
            }
            Some(other) => Err(self.error_at(
                pos,
                format!("expected an expression, found {}", other.describe()),
            )),
        }
    }

    fn apply(&self, op: &Token, l: i64, r: i64, op_pos: usize) -> Result<i64, CompileError> {
        let value = match op {
            Token::LogicalOr => (l != 0 || r != 0) as i64,
            Token::LogicalAnd => (l != 0 && r != 0) as i64,
            Token::Equals => (l == r) as i64,
            Token::NotEquals => (l != r) as i64,
            Token::LessThan => (l < r) as i64,
            Token::LessThanEquals => (l <= r) as i64,
            Token::GreaterThan => (l > r) as i64,
            Token::GreaterThanEquals => (l >= r) as i64,
            Token::Pipe => l | r,
            Token::Caret => l ^ r,
            Token::Ampersand => l & r,
            Token::Shl => l.wrapping_shl(r as u32),
            Token::Shr => l.wrapping_shr(r as u32), // arithmetic: `l` is i64
            Token::Plus => l.wrapping_add(r),
            Token::Minus => l.wrapping_sub(r),
            Token::Star => l.wrapping_mul(r),
            Token::Slash | Token::Modulo => {
                if r == 0 {
                    if !self.live {
                        return Ok(0);
                    }
                    return Err(self.error_at(
                        op_pos,
                        "division by zero in a preprocessor expression".to_string(),
                    ));
                }
                if *op == Token::Slash {
                    l.wrapping_div(r)
                } else {
                    l.wrapping_rem(r)
                }
            }
            other => {
                return Err(self.error_at(
                    op_pos,
                    format!("{} is not allowed in a preprocessor expression", other.describe()),
                ));
            }
        };
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    /// Lex `src` and drop the trailing EOF, which is not part of a directive.
    fn ev(src: &str) -> Result<i64, CompileError> {
        let mut toks = Lexer::new(src).tokenize().unwrap();
        toks.pop();
        eval(&toks, Span::new(0, src.len()))
    }

    #[test]
    fn a_literal_is_itself() {
        assert_eq!(ev("42").unwrap(), 42);
    }

    #[test]
    fn arithmetic_respects_precedence() {
        assert_eq!(ev("1 + 2 * 3").unwrap(), 7);
        assert_eq!(ev("(1 + 2) * 3").unwrap(), 9);
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(ev("10 - 3 - 2").unwrap(), 5);
    }

    #[test]
    fn comparisons_yield_one_or_zero() {
        assert_eq!(ev("2 > 1").unwrap(), 1);
        assert_eq!(ev("2 < 1").unwrap(), 0);
        assert_eq!(ev("2 >= 2").unwrap(), 1);
        assert_eq!(ev("1 == 1").unwrap(), 1);
        assert_eq!(ev("1 != 1").unwrap(), 0);
    }

    #[test]
    fn logical_operators_bind_looser_than_comparison() {
        assert_eq!(ev("1 < 2 && 3 > 2").unwrap(), 1);
        assert_eq!(ev("0 || 1 && 0").unwrap(), 0);
    }

    #[test]
    fn unary_operators_work() {
        assert_eq!(ev("-5").unwrap(), -5);
        assert_eq!(ev("!0").unwrap(), 1);
        assert_eq!(ev("!7").unwrap(), 0);
        assert_eq!(ev("~0").unwrap(), -1);
        assert_eq!(ev("- -3").unwrap(), 3);
    }

    #[test]
    fn an_identifier_is_zero() {
        assert_eq!(ev("NOPE").unwrap(), 0);
        assert_eq!(ev("NOPE == 0").unwrap(), 1);
    }

    #[test]
    fn a_character_literal_is_its_code() {
        assert_eq!(ev("'A'").unwrap(), 65);
    }

    #[test]
    fn division_by_zero_is_an_error_not_a_panic() {
        let err = ev("1 / 0").unwrap_err();
        assert!(err.message.contains("division by zero"), "got: {}", err.message);
        assert!(ev("1 % 0").is_err());
    }

    #[test]
    fn a_short_circuited_division_by_zero_is_not_reported() {
        // C does not evaluate the right operand here, so it cannot fail.
        assert_eq!(ev("0 && 1 / 0").unwrap(), 0);
        assert_eq!(ev("1 || 1 / 0").unwrap(), 1);
    }

    #[test]
    fn a_short_circuited_operand_is_still_parsed() {
        assert!(ev("0 && (1").is_err());
    }

    #[test]
    fn a_missing_operand_is_an_error() {
        let err = ev("1 +").unwrap_err();
        assert!(err.message.contains("expected an expression"), "got: {}", err.message);
    }

    #[test]
    fn an_unclosed_paren_is_an_error() {
        assert!(ev("(1 + 2").is_err());
    }

    #[test]
    fn trailing_junk_is_an_error() {
        let err = ev("1 2").unwrap_err();
        assert!(err.message.contains("expected an operator"), "got: {}", err.message);
    }

    #[test]
    fn an_empty_expression_is_an_error() {
        assert!(ev("").is_err());
    }

    #[test]
    fn an_unsupported_operator_is_reported_rather_than_ignored() {
        // `,` lexes, but the comma operator is not implemented here.
        let err = ev("1 , 2").unwrap_err();
        assert!(err.message.contains("expected an operator"), "got: {}", err.message);
    }

    #[test]
    fn bitwise_and_shift_operators_work() {
        assert_eq!(ev("(1 << 2) | 2").unwrap(), 6);
        assert_eq!(ev("6 & 3").unwrap(), 2);
        assert_eq!(ev("5 ^ 1").unwrap(), 4);
        assert_eq!(ev("240 & 60").unwrap(), 48);
        assert_eq!(ev("-8 >> 1").unwrap(), -4); // arithmetic
    }

    #[test]
    fn bitwise_precedence_matches_c() {
        // 1 | 2 & 3  ==  1 | (2 & 3)  ==  3
        assert_eq!(ev("1 | 2 & 3").unwrap(), 3);
        // equality binds tighter than `&`
        assert_eq!(ev("1 & 1 == 1").unwrap(), 1);
    }

    #[test]
    fn the_ternary_selects_a_branch() {
        assert_eq!(ev("1 ? 7 : 8").unwrap(), 7);
        assert_eq!(ev("0 ? 7 : 8").unwrap(), 8);
        // right-associative
        assert_eq!(ev("0 ? 1 : 1 ? 2 : 3").unwrap(), 2);
        // the untaken branch is not evaluated, so its division by zero is fine
        assert_eq!(ev("1 ? 5 : 1 / 0").unwrap(), 5);
    }

    #[test]
    fn a_ternary_without_a_colon_is_an_error() {
        assert!(ev("1 ? 2").is_err());
    }
}
