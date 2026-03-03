#[derive(Debug, Clone, PartialEq)]  // so we can use '{:?}' and compare tokens. Clone for duplicating tokens when needed.
pub enum Token {

    // literals + identifiers
    IntLiteral(i64),
    Ident(String),

    // keywords
    Int,
    Return,

    // symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Minus,
    Plus,
    Star,
    Slash,
    Bang,
    Tilde,
    Equals,

    EOF,
}

pub struct Lexer {
    input: Vec<char>,
    position: usize,
}

impl Lexer {

    pub fn new(source: &str) -> Self {
        Lexer {
            input: source.chars().collect(),
            position: 0,
        }
    }

    fn current(&self) -> Option<char> {
        self.input.get(self.position).copied()
    }
    
    fn advance(&mut self) -> Option<char> {
        let c = self.current();
        self.position += 1;
        c
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.current(), Some(c) if c.is_whitespace()) {
            self.advance();
        }
    }

    fn read_number(&mut self) -> Token {
        let mut num = String::new();
        while matches!(self.current(), Some(c) if c.is_ascii_digit()) {
            num.push(self.advance().unwrap());
        }
        Token::IntLiteral(num.parse().unwrap())
    }

    fn read_identifier(&mut self) -> Token {
        let mut ident = String::new();
        while matches!(self.current(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            ident.push(self.advance().unwrap());
        }
        match ident.as_str() {
            "int" => Token::Int,
            "return" => Token::Return,
            _ => Token::Ident(ident),
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        match self.current() {
            Some(c) if c.is_ascii_digit() => self.read_number(),
            Some(c) if c.is_ascii_alphabetic() || c == '_' => self.read_identifier(),
            Some('(') => { self.advance(); Token::LParen },
            Some(')') => { self.advance(); Token::RParen },
            Some('{') => { self.advance(); Token::LBrace },
            Some('}') => { self.advance(); Token::RBrace },
            Some(';') => { self.advance(); Token::Semicolon },
            Some('-') => { self.advance(); Token::Minus },
            Some('+') => { self.advance(); Token::Plus },
            Some('*') => { self.advance(); Token::Star },
            Some('/') => { self.advance(); Token::Slash },
            Some('!') => { self.advance(); Token::Bang },
            Some('~') => { self.advance(); Token::Tilde },
            Some('=') => { self.advance(); Token::Equals },
            None => Token::EOF,
            other => panic!("Unexpected character: {:?}", other),
        }
    }

    // For testing: tokenize entire input at once
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token == Token::EOF;
            tokens.push(token);
            if is_eof { break; }
        }
        tokens
    }

}

/* ===================================== */
//                                       //
//        Unit tests for the lexer       //
//                                       // 
/* ===================================== */

#[cfg(test)]
mod tests {
    use super::*;  // pulls in everything from the parent module

    #[test]
    fn test_single_number() {
        let mut lexer = Lexer::new("42");
        assert_eq!(lexer.tokenize(), vec![
            Token::IntLiteral(42),
            Token::EOF,
        ]);
    }

    #[test]
    fn test_keyword_recognition() {
        let mut lexer = Lexer::new("int return");
        assert_eq!(lexer.tokenize(), vec![
            Token::Int,
            Token::Return,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_whitespace_is_ignored() {
        let mut lexer = Lexer::new("   42   ");
        assert_eq!(lexer.tokenize(), vec![Token::IntLiteral(42), Token::EOF]);
    }

    #[test]
    fn test_multi_digit_number() {
        let mut lexer = Lexer::new("1234");
        assert_eq!(lexer.tokenize(), vec![Token::IntLiteral(1234), Token::EOF]);
    }

    #[test]
    fn test_ident_vs_keyword() {
        let mut lexer = Lexer::new("integer int");
        assert_eq!(lexer.tokenize(), vec![
            Token::Ident("integer".to_string()),  // not a keyword
            Token::Int,                            // is a keyword
            Token::EOF,
        ]);
    }
}