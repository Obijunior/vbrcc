//! Stage 2: turning the token stream into an abstract syntax tree.
//!
//! [`Parser::parse_program`] consumes the tokens from [`crate::lexer`] and produces a
//! [`crate::ast::Program`]: a list of functions, each with typed parameters, a return
//! type, and a body.
//!
//! The parser is recursive descent: one method per grammar rule, with expressions
//! handled by precedence climbing, where each level calls the next-higher-precedence
//! level and combines results as it unwinds.
//!
//! # What this stage does not do
//!
//! The parser checks *shape*, not *meaning*. It accepts an assignment whose left-hand
//! side could never be an lvalue; rejecting that is [`crate::typeck`]'s job. This split
//! lets type errors carry useful messages instead of surfacing as syntax errors.
//!
//! Accordingly, every expression is wrapped in a `TypedExpr` whose type field is
//! initialised to `Type::Unknown`. The type checker fills those in later by mutating the
//! tree in place. Statements are wrapped in `Spanned<Stmt>` to preserve source locations
//! for diagnostics.

use crate::lexer::{Token, SpannedToken};
use crate::ast::*;
use crate::diagnostic::{CompileError, Span, Spanned};

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser { tokens, pos: 0 }
    }

    // --- Token navigation ---

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).map(|st| &st.token).unwrap_or(&Token::EOF)
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|st| st.span)
            .unwrap_or_else(|| self.tokens.last().map(|st| st.span).unwrap_or(Span::dummy()))
    }

    fn previous_span(&self) -> Span {
        if self.pos == 0 {
            Span::dummy()
        } else {
            self.tokens.get(self.pos - 1).map(|st| st.span).unwrap_or(Span::dummy())
        }
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).map(|st| &st.token).unwrap_or(&Token::EOF);
        self.pos += 1;
        tok
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos + 1).map(|st| &st.token).unwrap_or(&Token::EOF)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), CompileError> {
        let span = self.current_span();
        let tok = self.advance().clone();
        if tok == *expected {
            Ok(())
        } else {
            Err(CompileError::new(
                format!("expected {}, found {}", expected.describe(), tok.describe()),
                span,
            )
            .with_label(format!("expected {} here", expected.describe())))
        }
    }

    // --- Grammar rules ---

    pub fn parse_program(&mut self) -> Result<Program, CompileError> {
        let mut functions = Vec::new();
        while self.current() != &Token::EOF {
            functions.push(self.parse_function()?);
        }
        Ok(Program { functions })
    }

    fn is_type_start(tok: &Token) -> bool {
        matches!(tok, Token::Int | Token::Char | Token::Long | Token::Void)
    }

    fn parse_block(&mut self) -> Result<Vec<Spanned<Stmt>>, CompileError> {
        if self.current() == &Token::LBrace {
            // Braced body: consume statements until the matching RBrace.
            self.advance(); // consume '{'
            let mut stmts = Vec::new();
            while self.current() != &Token::RBrace && self.current() != &Token::EOF {
                stmts.push(self.parse_statement()?);
            }
            self.expect(&Token::RBrace)?;
            return Ok(stmts);
        }
        // Brace-less body: exactly one statement.
        return Ok(vec![self.parse_statement()?]);
    }

    fn parse_type(&mut self) -> Result<Type, CompileError> {
        let span = self.current_span();
        let mut ty = match self.advance().clone() {
            Token::Int => Type::Int,
            Token::Char => Type::Char,
            Token::Long => Type::Long,
            Token::Void => Type::Void,
            other => {
                return Err(CompileError::new(
                    format!("expected a type, found {}", other.describe()),
                    span,
                )
                .with_label("expected `int`, `char`, `long`, or `void`"));
            }
        };
        while self.current() == &Token::Star {
            self.advance();
            ty = Type::Pointer(Box::new(ty));
        }
        Ok(ty)
    }

    fn parse_function(&mut self) -> Result<Function, CompileError> {
        let start = self.current_span();
        let return_type = self.parse_type()?;

        // function name
        let name = match self.advance().clone() {
            Token::Ident(s) => s,
            other => {
                return Err(CompileError::new(
                    format!("expected function name, found {}", other.describe()),
                    self.previous_span(),
                ));
            }
        };

        let mut params = Vec::new();
        self.expect(&Token::LParen)?;
        while self.current() != &Token::RParen {
            let ptype = self.parse_type()?;
            let pname = match self.advance().clone() {
                Token::Ident(s) => s,
                other => {
                    return Err(CompileError::new(
                        format!("expected parameter name, found {}", other.describe()),
                        self.previous_span(),
                    ));
                }
            };
            let ptype = if self.current() == &Token::LBracket {
                self.advance();                                    // consume '['
                if matches!(self.current(), Token::IntLiteral(_)) {
                    self.advance();
                }
                self.expect(&Token::RBracket)?;
                Type::Pointer(Box::new(ptype))
            } else {
                ptype
            };
            params.push((ptype, pname));
            if self.current() == &Token::Comma {
                self.advance();
            }
        }
        self.expect(&Token::RParen)?;

        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;

        let span = start.to(self.previous_span());
        Ok(Function { name, params, return_type, body, span })
    }

    fn parse_statement(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        let node = match self.current().clone() {
            Token::Return => {
                self.advance(); // consume 'return'
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Stmt::Return(expr)
            }
            Token::Int | Token::Char | Token::Long | Token::Void => return self.parse_decl(),
            Token::For => return self.parse_for(),
            Token::While => return self.parse_while(),
            Token::If => return self.parse_if(),
            _ => {
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Stmt::Expr(expr)
            }
        };
        Ok(Spanned::new(node, start.to(self.previous_span())))
    }

    // --- Expression parsing with precedence climbing ---
    //
    // Precedence (low to high):
    //   1. + -          (additive)
    //   2. * /          (multiplicative)
    //   3. unary - ~ !  (unary)
    //   4. literals, identifiers, ( expr )

    fn parse_expr(&mut self) -> Result<TypedExpr, CompileError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<TypedExpr, CompileError> {
        let lhs = self.parse_logical_or()?;
        let start_span = lhs.span;

        // postfix ++ / --
        if self.current() == &Token::PlusPlus || self.current() == &Token::MinusMinus {
            let op = if self.current() == &Token::PlusPlus { BinaryOp::Add } else { BinaryOp::Sub };
            self.advance();
            let span = start_span.to(self.previous_span());
            let one = TypedExpr::new(Expr::IntLiteral(1), span);
            let combined = TypedExpr::new(Expr::BinaryOp(op, Box::new(lhs.clone()), Box::new(one)), span);
            return Ok(TypedExpr::new(Expr::Assign(Box::new(lhs), Box::new(combined)), span));
        }

        let assign_op = match self.current() {
            Token::Assign => Some(None),
            Token::PlusEquals => Some(Some(BinaryOp::Add)),
            Token::MinusEquals => Some(Some(BinaryOp::Sub)),
            Token::StarEquals => Some(Some(BinaryOp::Mul)),
            Token::SlashEquals => Some(Some(BinaryOp::Div)),
            Token::ModuloEquals => Some(Some(BinaryOp::Mod)),
            _ => None,
        };
        let Some(assign_op) = assign_op else { return Ok(lhs); };
        self.advance(); // consume the assignment operator
        let rhs = self.parse_assignment()?;
        let span = start_span.to(self.previous_span());
        let value = match assign_op {
            None => rhs,
            Some(op) => TypedExpr::new(
                Expr::BinaryOp(op, Box::new(lhs.clone()), Box::new(rhs)),
                span,
            ),
        };
        Ok(TypedExpr::new(Expr::Assign(Box::new(lhs), Box::new(value)), span))
    }

    fn parse_logical_or(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_logical_and()?;
        while let Token::LogicalOr = self.current() {
            self.advance();
            let right = self.parse_logical_and()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(BinaryOp::LogicalOr, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_comparison()?;
        while let Token::LogicalAnd = self.current() {
            self.advance();
            let right = self.parse_comparison()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(BinaryOp::LogicalAnd, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.current() {
                Token::LessThan => BinaryOp::Lt,
                Token::LessThanEquals => BinaryOp::Lte,
                Token::GreaterThan => BinaryOp::Gt,
                Token::GreaterThanEquals => BinaryOp::Gte,
                Token::Equals => BinaryOp::Eq,
                Token::NotEquals => BinaryOp::Neq,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.current() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.current() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Modulo => BinaryOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_for(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'for'
        self.expect(&Token::LParen)?;

        let init = Box::new(self.parse_statement()?);
        let cond = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        let update_start = self.current_span();
        let update_expr = self.parse_expr()?;
        let update = Box::new(Spanned::new(
            Stmt::Expr(update_expr),
            update_start.to(self.previous_span()),
        ));

        self.expect(&Token::RParen)?;
        let body = self.parse_block()?;

        Ok(Spanned::new(Stmt::For { init, cond, update, body }, start.to(self.previous_span())))
    }

    fn parse_while(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'while'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        let body = self.parse_block()?;
        Ok(Spanned::new(Stmt::While { cond, body }, start.to(self.previous_span())))
    }

    fn parse_if(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'if'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        let then_branch = self.parse_block()?;

        let else_branch = if self.current() == &Token::Else {
            self.advance();
            self.parse_block()?
        } else {
            Vec::new()
        };

        Ok(Spanned::new(Stmt::If { cond, then_branch, else_branch }, start.to(self.previous_span())))
    }

    fn parse_decl(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        let base = self.parse_type()?;
        let name = match self.advance().clone() {
            Token::Ident(s) => s,
            other => {
                return Err(CompileError::new(
                    format!("expected variable name, found {}", other.describe()),
                    self.previous_span(),
                ));
            }
        };
        let ty = if self.current() == &Token::LBracket {
            self.advance();
            let len = match self.advance().clone() {
                Token::IntLiteral(n) if n >= 0 => n as usize,
                other => {
                    return Err(CompileError::new(
                        format!("expected array length, found {}", other.describe()),
                        self.previous_span(),
                    ));
                }
            };
            self.expect(&Token::RBracket)?;
            Type::Array(Box::new(base), len)
        } else {
            base
        };
        let init = if self.current() == &Token::Assign {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(&Token::Semicolon)?;
        Ok(Spanned::new(Stmt::VarDecl { ty, name, init }, start.to(self.previous_span())))
    }

    fn parse_unary(&mut self) -> Result<TypedExpr, CompileError> {
        let start = self.current_span();

        // cast: ( type ) unary
        if self.current() == &Token::LParen && Self::is_type_start(self.peek()) {
            self.advance(); // (
            let ty = self.parse_type()?;
            self.expect(&Token::RParen)?;
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::Cast(ty, Box::new(operand)), span));
        }

        if let Some(op) = match self.current() {
            Token::Minus => Some(UnaryOp::Negate),
            Token::Tilde => Some(UnaryOp::BitNot),
            Token::Bang => Some(UnaryOp::LogNot),
            _ => None,
        } {
            self.advance();
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::UnaryOp(op, Box::new(operand)), span));
        }

        if self.current() == &Token::Ampersand {
            self.advance();
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::AddressOf(Box::new(operand)), span));
        }
        if self.current() == &Token::Star {
            self.advance();
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::Deref(Box::new(operand)), span));
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<TypedExpr, CompileError> {
        let mut expr = self.parse_primary()?;
        while self.current() == &Token::LBracket {
            let start = expr.span;
            self.advance(); // [
            let idx = self.parse_expr()?;
            self.expect(&Token::RBracket)?;
            let span = start.to(self.previous_span());
            expr = TypedExpr::new(Expr::Index(Box::new(expr), Box::new(idx)), span);
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<TypedExpr, CompileError> {
        let start = self.current_span();
        let node = match self.advance().clone() {
            Token::IntLiteral(n) => Expr::IntLiteral(n),
            Token::CharLiteral(n) => Expr::IntLiteral(n),
            Token::StringLiteral(s) => Expr::StringLiteral(s),
            Token::Ident(name) => {
                if self.current() == &Token::LParen {
                    self.advance();
                    let mut args = Vec::new();
                    while self.current() != &Token::RParen {
                        args.push(self.parse_expr()?);
                        if self.current() == &Token::Comma {
                            self.advance();
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Expr::FunctionCall { name, args }
                } else {
                    Expr::Var(name)
                }
            }
            Token::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                // Re-span the parenthesized expression to include the parens.
                return Ok(TypedExpr::new(inner.node, start.to(self.previous_span())));
            }
            other => {
                return Err(CompileError::new(
                    format!("unexpected {} in expression", other.describe()),
                    self.previous_span(),
                ));
            }
        };
        Ok(TypedExpr::new(node, start.to(self.previous_span())))
    }
}



/* ===================================== */
//                                       //
//        Unit tests for the parser      //
//                                       // 
/* ===================================== */

#[cfg(test)]
mod tests {
    use super::*;
    
    // Attach dummy spans so tests can keep writing bare Token vectors.
    fn parser(tokens: Vec<Token>) -> Parser {
        use crate::lexer::SpannedToken;
        use crate::diagnostic::Span;
        let spanned = tokens.into_iter().map(|t| SpannedToken { token: t, span: Span::dummy() }).collect();
        Parser::new(spanned)
    }

    use crate::diagnostic::{Span, Spanned};
    fn e(x: Expr) -> TypedExpr { TypedExpr::new(x, Span::dummy()) }
    fn s(x: Stmt) -> Spanned<Stmt> { Spanned::new(x, Span::dummy()) }

    #[test]
    fn parse_unary_negation_expression() {
        let mut p = parser(vec![Token::Minus, Token::IntLiteral(7), Token::EOF]);
        let expr = p.parse_unary().unwrap();
        assert_eq!(expr, e(Expr::UnaryOp(UnaryOp::Negate, Box::new(e(Expr::IntLiteral(7))))));
    }

    #[test]
    fn parse_parenthesized_primary_expression() {
        let mut p = parser(vec![Token::LParen, Token::IntLiteral(9), Token::RParen, Token::EOF]);
        let expr = p.parse_primary().unwrap();
        assert_eq!(expr, e(Expr::IntLiteral(9)));
    }

    #[test]
    fn parse_expression_statement() {
        let mut p = parser(vec![Token::IntLiteral(7), Token::Semicolon, Token::EOF]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::IntLiteral(7)))));
    }

    #[test]
    fn parse_var_decl_with_init() {
        let mut p = parser(vec![
            Token::Int, Token::Ident("x".into()), Token::Assign,
            Token::IntLiteral(5), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { ty: Type::Int, name: "x".into(), init: Some(e(Expr::IntLiteral(5))) }));
    }

    #[test]
    fn parse_var_decl_without_init() {
        let mut p = parser(vec![
            Token::Int, Token::Ident("x".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { ty: Type::Int, name: "x".into(), init: None }));
    }

    #[test]
    fn parse_assignment() {
        let mut p = parser(vec![
            Token::Ident("x".into()), Token::Assign,
            Token::IntLiteral(10), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(Box::new(e(Expr::Var("x".into()))), Box::new(e(Expr::IntLiteral(10))))))));
    }

    #[test]
    fn parse_compound_assignment() {
        let mut p = parser(vec![
            Token::Ident("x".into()), Token::PlusEquals,
            Token::IntLiteral(3), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(
            Box::new(e(Expr::Var("x".into()))),
            Box::new(e(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(e(Expr::Var("x".into()))),
                Box::new(e(Expr::IntLiteral(3))),
            ))),
        )))));
    }

    #[test]
    fn parse_post_increment() {
        let mut p = parser(vec![
            Token::Ident("i".into()), Token::PlusPlus, Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(
            Box::new(e(Expr::Var("i".into()))),
            Box::new(e(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(e(Expr::Var("i".into()))),
                Box::new(e(Expr::IntLiteral(1))),
            ))),
        )))));
    }

    #[test]
    fn parse_comparison_less_than() {
        let mut p = parser(vec![
            Token::Ident("i".into()), Token::LessThan, Token::IntLiteral(10), Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::BinaryOp(
            BinaryOp::Lt,
            Box::new(e(Expr::Var("i".into()))),
            Box::new(e(Expr::IntLiteral(10))),
        )));
    }

    #[test]
    fn parse_comparison_binds_looser_than_additive() {
        let mut p = parser(vec![
            Token::Ident("i".into()), Token::LessThan,
            Token::IntLiteral(3), Token::Plus, Token::IntLiteral(1), Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::BinaryOp(
            BinaryOp::Lt,
            Box::new(e(Expr::Var("i".into()))),
            Box::new(e(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(e(Expr::IntLiteral(3))),
                Box::new(e(Expr::IntLiteral(1))),
            ))),
        )));
    }

        #[test]
    fn parse_char_var_decl() {
        let mut p = parser(vec![
            Token::Char, Token::Ident("c".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { ty: Type::Char, name: "c".into(), init: None }));
    }

    #[test]
    fn parse_long_var_decl_with_init() {
        let mut p = parser(vec![
            Token::Long, Token::Ident("n".into()), Token::Assign,
            Token::IntLiteral(7), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Long, name: "n".into(), init: Some(e(Expr::IntLiteral(7))),
        }));
    }

    #[test]
    fn parse_pointer_var_decl() {
        let mut p = parser(vec![
            Token::Int, Token::Star, Token::Ident("p".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Pointer(Box::new(Type::Int)), name: "p".into(), init: None,
        }));
    }

    #[test]
    fn parse_array_var_decl() {
        let mut p = parser(vec![
            Token::Int, Token::Ident("a".into()), Token::LBracket,
            Token::IntLiteral(10), Token::RBracket, Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Array(Box::new(Type::Int), 10), name: "a".into(), init: None,
        }));
    }

        #[test]
    fn parse_address_of() {
        let mut p = parser(vec![Token::Ampersand, Token::Ident("x".into()), Token::EOF]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::AddressOf(Box::new(e(Expr::Var("x".into()))))));
    }

    #[test]
    fn parse_deref() {
        let mut p = parser(vec![Token::Star, Token::Ident("p".into()), Token::EOF]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::Deref(Box::new(e(Expr::Var("p".into()))))));
    }

    #[test]
    fn parse_index() {
        let mut p = parser(vec![
            Token::Ident("a".into()), Token::LBracket, Token::IntLiteral(2), Token::RBracket, Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::Index(
            Box::new(e(Expr::Var("a".into()))),
            Box::new(e(Expr::IntLiteral(2))),
        )));
    }

    #[test]
    fn parse_cast() {
        let mut p = parser(vec![
            Token::LParen, Token::Char, Token::Star, Token::RParen, Token::Ident("p".into()), Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::Cast(
            Type::Pointer(Box::new(Type::Char)),
            Box::new(e(Expr::Var("p".into()))),
        )));
    }

    #[test]
    fn parse_deref_assignment() {
        let mut p = parser(vec![
            Token::Star, Token::Ident("p".into()), Token::Assign,
            Token::IntLiteral(5), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(
            Box::new(e(Expr::Deref(Box::new(e(Expr::Var("p".into())))))),
            Box::new(e(Expr::IntLiteral(5))),
        )))));
    }
}
