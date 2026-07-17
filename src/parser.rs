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

    fn parse_function(&mut self) -> Result<Function, CompileError> {
        let start = self.current_span();
        // int for now
        self.expect(&Token::Int)?;
        let return_type = "int".to_string();

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

        // () parse params
        let mut params = Vec::new();
        self.expect(&Token::LParen)?;
        while self.current() != &Token::RParen {
            self.expect(&Token::Int)?;
            let new_param = match self.advance().clone() {
                Token::Ident(s) => s,
                other => {
                    return Err(CompileError::new(
                        format!("expected parameter name, found {}", other.describe()),
                        self.previous_span(),
                    ));
                }
            };
            params.push(new_param);
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
            Token::Int => return self.parse_int(),
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

    fn parse_expr(&mut self) -> Result<Spanned<Expr>, CompileError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Spanned<Expr>, CompileError> {
        if let Token::Ident(name) = self.current().clone() {
            let start = self.current_span();
            // i++ / i--
            if *self.peek() == Token::PlusPlus {
                self.advance();
                self.advance();
                let span = start.to(self.previous_span());
                let var = Spanned::new(Expr::Var(name.clone()), span);
                let one = Spanned::new(Expr::IntLiteral(1), span);
                let sum = Spanned::new(Expr::BinaryOp(BinaryOp::Add, Box::new(var), Box::new(one)), span);
                return Ok(Spanned::new(Expr::Assign(name, Box::new(sum)), span));
            }
            if *self.peek() == Token::MinusMinus {
                self.advance();
                self.advance();
                let span = start.to(self.previous_span());
                let var = Spanned::new(Expr::Var(name.clone()), span);
                let one = Spanned::new(Expr::IntLiteral(1), span);
                let diff = Spanned::new(Expr::BinaryOp(BinaryOp::Sub, Box::new(var), Box::new(one)), span);
                return Ok(Spanned::new(Expr::Assign(name, Box::new(diff)), span));
            }

            let assign_op = match self.peek() {
                Token::Equals => None,
                Token::PlusEquals => Some(BinaryOp::Add),
                Token::MinusEquals => Some(BinaryOp::Sub),
                Token::StarEquals => Some(BinaryOp::Mul),
                Token::SlashEquals => Some(BinaryOp::Div),
                Token::ModuloEquals => Some(BinaryOp::Mod),
                _ => return self.parse_logical_or(),
            };
            self.advance(); // ident
            self.advance(); // assignment token
            let rhs = self.parse_expr()?;
            let span = start.to(self.previous_span());

            return Ok(match assign_op {
                None => Spanned::new(Expr::Assign(name, Box::new(rhs)), span),
                Some(op) => {
                    let var = Spanned::new(Expr::Var(name.clone()), span);
                    let combined = Spanned::new(Expr::BinaryOp(op, Box::new(var), Box::new(rhs)), span);
                    Spanned::new(Expr::Assign(name, Box::new(combined)), span)
                }
            });
        }
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Spanned<Expr>, CompileError> {
        let mut left = self.parse_logical_and()?;
        while let Token::LogicalOr = self.current() {
            self.advance();
            let right = self.parse_logical_and()?;
            let span = left.span.to(right.span);
            left = Spanned::new(Expr::BinaryOp(BinaryOp::LogicalOr, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Spanned<Expr>, CompileError> {
        let mut left = self.parse_comparison()?;
        while let Token::LogicalAnd = self.current() {
            self.advance();
            let right = self.parse_comparison()?;
            let span = left.span.to(right.span);
            left = Spanned::new(Expr::BinaryOp(BinaryOp::LogicalAnd, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Spanned<Expr>, CompileError> {
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
            left = Spanned::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Spanned<Expr>, CompileError> {
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
            left = Spanned::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Spanned<Expr>, CompileError> {
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
            left = Spanned::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
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
        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(Spanned::new(Stmt::For { init, cond, update, body }, start.to(self.previous_span())))
    }

    fn parse_while(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'while'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(Spanned::new(Stmt::While { cond, body }, start.to(self.previous_span())))
    }

    fn parse_if(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'if'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        self.expect(&Token::LBrace)?;
        let mut then_branch = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            then_branch.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;

        let else_branch = if self.current() == &Token::Else {
            self.advance();
            self.expect(&Token::LBrace)?;
            let mut els = Vec::new();
            while self.current() != &Token::RBrace && self.current() != &Token::EOF {
                els.push(self.parse_statement()?);
            }
            self.expect(&Token::RBrace)?;
            els
        } else {
            Vec::new()
        };

        Ok(Spanned::new(Stmt::If { cond, then_branch, else_branch }, start.to(self.previous_span())))
    }

    fn parse_int(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'int'
        let name = match self.advance().clone() {
            Token::Ident(s) => s,
            other => {
                return Err(CompileError::new(
                    format!("expected variable name, found {}", other.describe()),
                    self.previous_span(),
                ));
            }
        };
        let init = if self.current() == &Token::Equals {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(&Token::Semicolon)?;
        Ok(Spanned::new(Stmt::VarDecl { name, init }, start.to(self.previous_span())))
    }

    fn parse_unary(&mut self) -> Result<Spanned<Expr>, CompileError> {
        let start = self.current_span();
        let op = match self.current() {
            Token::Minus => Some(UnaryOp::Negate),
            Token::Tilde => Some(UnaryOp::BitNot),
            Token::Bang => Some(UnaryOp::LogNot),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            Ok(Spanned::new(Expr::UnaryOp(op, Box::new(operand)), span))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Spanned<Expr>, CompileError> {
        let start = self.current_span();
        let node = match self.advance().clone() {
            Token::IntLiteral(n) => Expr::IntLiteral(n),
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
                return Ok(Spanned::new(inner.node, start.to(self.previous_span())));
            }
            other => {
                return Err(CompileError::new(
                    format!("unexpected {} in expression", other.describe()),
                    self.previous_span(),
                ));
            }
        };
        Ok(Spanned::new(node, start.to(self.previous_span())))
    }
}

// --- Tests ---
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
    fn e(x: Expr) -> Spanned<Expr> { Spanned::new(x, Span::dummy()) }
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
            Token::Int, Token::Ident("x".into()), Token::Equals,
            Token::IntLiteral(5), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { name: "x".into(), init: Some(e(Expr::IntLiteral(5))) }));
    }

    #[test]
    fn parse_var_decl_without_init() {
        let mut p = parser(vec![
            Token::Int, Token::Ident("x".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { name: "x".into(), init: None }));
    }

    #[test]
    fn parse_assignment() {
        let mut p = parser(vec![
            Token::Ident("x".into()), Token::Equals,
            Token::IntLiteral(10), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign("x".into(), Box::new(e(Expr::IntLiteral(10))))))));
    }

    #[test]
    fn parse_compound_assignment() {
        let mut p = parser(vec![
            Token::Ident("x".into()), Token::PlusEquals,
            Token::IntLiteral(3), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(
            "x".into(),
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
            "i".into(),
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
}
