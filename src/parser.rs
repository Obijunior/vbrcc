use crate::lexer::Token;
use crate::ast::*;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    // --- Token navigation ---

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::EOF)
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).unwrap_or(&Token::EOF);
        self.pos += 1;
        tok
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos + 1).unwrap_or(&Token::EOF)
    }

    // make sure next token matches what we expect.
    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let tok = self.advance();
        if tok == expected {
            Ok(())
        } else {
            Err(format!("[ ERROR ] :: Expected {:?}, got {:?}", expected, tok))
        }
    }

    // --- Grammar rules ---

    pub fn parse_program(&mut self) -> Result<Program, String> {
        let mut functions = Vec::new();
        while self.current() != &Token::EOF {
            functions.push(self.parse_function()?);
        }
        Ok(Program { functions })
    }

    fn parse_function(&mut self) -> Result<Function, String> {
        // int for now
        self.expect(&Token::Int)?;
        let return_type = "int".to_string();

        // function name
        let name = match self.advance().clone() {
            Token::Ident(s) => s,
            other => return Err(format!("[ ERROR ] :: Expected function name, got {:?}", other)),
        };

        // () parse params
        let mut params = Vec::new();
        self.expect(&Token::LParen)?;
        while self.current() != &Token::RParen {
            self.expect(&Token::Int)?;
            let new_param = match self.advance().clone() {
                Token::Ident(s) => s,
                other => return Err(format!("[ ERROR ] :: Expected param name, got {:?}", other)),
            };
            params.push(new_param);
        }
        self.expect(&Token::RParen)?;

        // { ... }
        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(Function { name, params, return_type, body })
    }

    fn parse_statement(&mut self) -> Result<Stmt, String> {
        match self.current().clone() {
            Token::Return => {
                self.advance(); // consume 'return'
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(Stmt::Return(expr))
            }
            Token::Int => self.parse_int(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::If => self.parse_if(),
            _ => {
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(Stmt::Expr(expr))
            }
            // other => Err(format!("[ ERROR ] :: Unexpected token in statement: {:?}", other)),
        }
    }

    // --- Expression parsing with precedence climbing ---
    //
    // Precedence (low to high):
    //   1. + -          (additive)
    //   2. * /          (multiplicative)
    //   3. unary - ~ !  (unary)
    //   4. literals, identifiers, ( expr )

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        if let Token::Ident(name) = self.current().clone() {
            // i++ / i--
            if *self.peek() == Token::PlusPlus {
                self.advance(); // ident
                self.advance(); // ++
                return Ok(Expr::Assign(
                    name.clone(),
                    Box::new(Expr::BinaryOp(BinaryOp::Add, Box::new(Expr::Var(name)), Box::new(Expr::IntLiteral(1)))),
                ));
            }
            if *self.peek() == Token::MinusMinus {
                self.advance(); // ident
                self.advance(); // --
                return Ok(Expr::Assign(
                    name.clone(),
                    Box::new(Expr::BinaryOp(BinaryOp::Sub, Box::new(Expr::Var(name)), Box::new(Expr::IntLiteral(1)))),
                ));
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

            return Ok(match assign_op {
                None => Expr::Assign(name, Box::new(rhs)),
                Some(op) => Expr::Assign(
                    name.clone(),
                    Box::new(Expr::BinaryOp(op, Box::new(Expr::Var(name.clone())), Box::new(rhs))),
                ),
            });
        }

        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_logical_and()?;
        loop {
            match self.current() {
                Token::LogicalOr => {
                    self.advance(); // consume '||'
                    let right = self.parse_logical_and()?;
                    left = Expr::BinaryOp(BinaryOp::LogicalOr, Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        loop {
            match self.current() {
                Token::LogicalAnd => {
                    self.advance();
                    let right = self.parse_comparison()?;
                    left = Expr::BinaryOp(BinaryOp::LogicalAnd, Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
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
            left = Expr::BinaryOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.current() {
                Token::Plus  => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _            => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::BinaryOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.current() {
                Token::Star   => BinaryOp::Mul,
                Token::Slash  => BinaryOp::Div,
                Token::Modulo => BinaryOp::Mod,
                _             => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinaryOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_for(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'for'
        self.expect(&Token::LParen)?;

        let init = Box::new(self.parse_statement()?);
        let cond = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        let update = Box::new(Stmt::Expr(self.parse_expr()?));

        self.expect(&Token::RParen)?;

        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(Stmt::For { init, cond, update, body })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'while'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'if'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        self.expect(&Token::LBrace)?;
        let mut then_branch = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            then_branch.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;
        if self.current() == &Token::Else {
            self.advance(); // consume 'else'
            self.expect(&Token::LBrace)?;
            let mut else_branch = Vec::new();
            while self.current() != &Token::RBrace && self.current() != &Token::EOF {
                else_branch.push(self.parse_statement()?);
            }
            self.expect(&Token::RBrace)?;
            Ok(Stmt::If { cond, then_branch, else_branch })
        } else {
            Ok(Stmt::If { cond, then_branch, else_branch: Vec::new() })
        }
    }

    fn parse_int(&mut self) -> Result<Stmt, String> {
        self.advance(); // consume 'int'
        let name = match self.advance().clone() {
            Token::Ident(s) => s,
            other => return Err(format!("[ ERROR ] :: Expected variable name, got {:?}", other)),
        };
        let init = if self.current() == &Token::Equals {
            self.advance(); // consume '='
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(&Token::Semicolon)?;
        Ok(Stmt::VarDecl { name, init })
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        let op = match self.current() {
            Token::Minus => Some(UnaryOp::Negate),
            Token::Tilde => Some(UnaryOp::BitNot),
            Token::Bang  => Some(UnaryOp::LogNot),
            _            => None,
        };
        if let Some(op) = op {
            self.advance();
            let operand = self.parse_unary()?; // recursive, handles --x etc.
            Ok(Expr::UnaryOp(op, Box::new(operand)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.advance().clone() {
            Token::IntLiteral(n) => Ok(Expr::IntLiteral(n)),
            Token::StringLiteral(s) => Ok(Expr::StringLiteral(s)),
            Token::Ident(name) => {
                // if next token is '(' it's a function call
                if self.current() == &Token::LParen {
                    self.advance(); // consume '('
                    let mut args = Vec::new();
                    while self.current() != &Token::RParen {
                        args.push(self.parse_expr()?);
                        if self.current() == &Token::Comma {
                            self.advance(); // consume ','
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::FunctionCall { name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            other => Err(format!("[ ERROR ] :: Unexpected token in expression: {:?}", other)),
        }
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expect_reports_mismatch() {
        let mut parser = Parser::new(vec![Token::Return, Token::EOF]);
        let err = parser.expect(&Token::Int).unwrap_err();
        assert_eq!(err, "[ ERROR ] :: Expected Int, got Return");
    }

    #[test]
    fn parse_unary_negation_expression() {
        let mut parser = Parser::new(vec![Token::Minus, Token::IntLiteral(7), Token::EOF]);
        let expr = parser.parse_unary().unwrap();
        assert_eq!(
            expr,
            Expr::UnaryOp(UnaryOp::Negate, Box::new(Expr::IntLiteral(7)))
        );
    }

    #[test]
    fn parse_parenthesized_primary_expression() {
        let mut parser = Parser::new(vec![
            Token::LParen,
            Token::IntLiteral(9),
            Token::RParen,
            Token::EOF,
        ]);
        let expr = parser.parse_primary().unwrap();
        assert_eq!(expr, Expr::IntLiteral(9));
    }

    #[test]
    fn parse_expression_statement() {
        let mut parser = Parser::new(vec![Token::IntLiteral(7), Token::Semicolon, Token::EOF]);
        let stmt = parser.parse_statement().unwrap();
        assert_eq!(stmt, Stmt::Expr(Expr::IntLiteral(7)));
    }

    #[test]
    fn parse_var_decl_with_init() {
        // int x = 5;
        let mut parser = Parser::new(vec![
            Token::Int, Token::Ident("x".into()), Token::Equals,
            Token::IntLiteral(5), Token::Semicolon, Token::EOF,
        ]);
        let stmt = parser.parse_statement().unwrap();
        assert_eq!(stmt, Stmt::VarDecl {
            name: "x".into(),
            init: Some(Expr::IntLiteral(5)),
        });
    }

    #[test]
    fn parse_var_decl_without_init() {
        // int x;
        let mut parser = Parser::new(vec![
            Token::Int, Token::Ident("x".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = parser.parse_statement().unwrap();
        assert_eq!(stmt, Stmt::VarDecl {
            name: "x".into(),
            init: None,
        });
    }

    #[test]
    fn parse_assignment() {
        // x = 10;
        let mut parser = Parser::new(vec![
            Token::Ident("x".into()), Token::Equals,
            Token::IntLiteral(10), Token::Semicolon, Token::EOF,
        ]);
        let stmt = parser.parse_statement().unwrap();
        assert_eq!(stmt, Stmt::Expr(Expr::Assign("x".into(), Box::new(Expr::IntLiteral(10)))));
    }

    #[test]
    fn parse_compound_assignment() {
        // x += 3;
        let mut parser = Parser::new(vec![
            Token::Ident("x".into()), Token::PlusEquals,
            Token::IntLiteral(3), Token::Semicolon, Token::EOF,
        ]);
        let stmt = parser.parse_statement().unwrap();
        assert_eq!(stmt, Stmt::Expr(Expr::Assign(
            "x".into(),
            Box::new(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(Expr::Var("x".into())),
                Box::new(Expr::IntLiteral(3)),
            )),
        )));
    }

    #[test]
    fn parse_post_increment() {
        // i++;
        let mut parser = Parser::new(vec![
            Token::Ident("i".into()), Token::PlusPlus, Token::Semicolon, Token::EOF,
        ]);
        let stmt = parser.parse_statement().unwrap();
        assert_eq!(stmt, Stmt::Expr(Expr::Assign(
            "i".into(),
            Box::new(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(Expr::Var("i".into())),
                Box::new(Expr::IntLiteral(1)),
            )),
        )));
    }

    #[test]
    fn parse_comparison_less_than() {
        // i < 10 (as an expression)
        let mut parser = Parser::new(vec![
            Token::Ident("i".into()), Token::LessThan,
            Token::IntLiteral(10), Token::EOF,
        ]);
        let expr = parser.parse_expr().unwrap();
        assert_eq!(expr, Expr::BinaryOp(
            BinaryOp::Lt,
            Box::new(Expr::Var("i".into())),
            Box::new(Expr::IntLiteral(10)),
        ));
    }

    #[test]
    fn parse_comparison_binds_looser_than_additive() {
        // i < 3 + 1 should parse as i < (3 + 1)
        let mut parser = Parser::new(vec![
            Token::Ident("i".into()), Token::LessThan,
            Token::IntLiteral(3), Token::Plus, Token::IntLiteral(1),
            Token::EOF,
        ]);
        let expr = parser.parse_expr().unwrap();
        assert_eq!(expr, Expr::BinaryOp(
            BinaryOp::Lt,
            Box::new(Expr::Var("i".into())),
            Box::new(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(Expr::IntLiteral(3)),
                Box::new(Expr::IntLiteral(1)),
            )),
        ));
    }
}
