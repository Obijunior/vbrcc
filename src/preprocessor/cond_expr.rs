//! The `#if` and `#elif` constant-expression evaluator.
//!
//! The caller resolves `defined` and expands macros first, so this sees only
//! literals, operators, parentheses, and identifiers. C says an identifier that
//! is not a macro evaluates to `0`, which is why an undefined name is not an
//! error here.
//!
//! Arithmetic is `i64` throughout. C99 requires the widest integer type, and
//! `long` is 64 bits on this target.
//!
//! # Limits
//!
//! Bitwise `&`, `|`, `^`, the shifts, and `?:` are missing. So is the comma
//! operator. Adding them is one row each in [`binding_power`] and [`apply`].

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
fn binding_power(tok: &Token) -> Option<u8> {
    Some(match tok {
        Token::LogicalOr => 1,
        Token::LogicalAnd => 2,
        Token::Equals | Token::NotEquals => 3,
        Token::LessThan | Token::LessThanEquals
        | Token::GreaterThan | Token::GreaterThanEquals => 4,
        Token::Plus | Token::Minus => 5,
        Token::Star | Token::Slash | Token::Modulo => 6,
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
        // `&` lexes, but bitwise and is not implemented here.
        let err = ev("1 & 2").unwrap_err();
        assert!(err.message.contains("expected an operator"), "got: {}", err.message);
    }
}
