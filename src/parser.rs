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
        // int
        self.expect(&Token::Int)?;

        // function name
        let name = match self.advance().clone() {
            Token::Ident(s) => s,
            other => return Err(format!("[ ERROR ] :: Expected function name, got {:?}", other)),
        };

        // ()
        self.expect(&Token::LParen)?;
        self.expect(&Token::RParen)?;

        // { ... }
        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            body.push(self.parse_statement()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(Function { name, body })
    }

    fn parse_statement(&mut self) -> Result<Stmt, String> {
        match self.current().clone() {
            Token::Return => {
                self.advance(); // consume 'return'
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(Stmt::Return(expr))
            }
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
        self.parse_additive()
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
                Token::Star  => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _            => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinaryOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
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
}
