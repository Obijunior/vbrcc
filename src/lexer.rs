use crate::diagnostic::{CompileError, Span};

#[derive(Debug, Clone, PartialEq)]  // so we can use '{:?}' and compare tokens. Clone for duplicating tokens when needed.
pub enum Token {

    // literals + identifiers
    IntLiteral(i64),
    StringLiteral(String),
    // Register(String), <-- commenting to keep the warnings quiet
    Ident(String),

    // types
    Int,
    Char,
    Long,
    Void,

    // keywords
    Return,
    For,
    While,
    If,
    Else,

    // operators
    Minus,
    Plus,
    Star,
    Slash,
    Modulo,
    PlusPlus,
    MinusMinus,
    Equals,
    NotEquals,
    PlusEquals,
    MinusEquals,
    StarEquals,
    SlashEquals,
    ModuloEquals,
    LogicalAnd,
    LogicalOr,
    // Hashtag, // <- later, for preprocessor directives. commenting to avoid compiler warnings

    // symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Bang,
    Tilde,
    Comma,
    Colon,
    LessThan,
    LessThanEquals,
    GreaterThan,
    GreaterThanEquals,

    EOF,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

impl Token {
    /// Human-readable name for diagnostics
    pub fn describe(&self) -> String {
        match self {
            Token::IntLiteral(_) => "integer literal".to_string(),
            Token::StringLiteral(_) => "string literal".to_string(),
            Token::Ident(_) => "identifier".to_string(),
            Token::Int => "`int`".to_string(),
            Token::Char => "`char`".to_string(),
            Token::Long => "`long`".to_string(),
            Token::Void => "`void`".to_string(),
            Token::Return => "`return`".to_string(),
            Token::For => "`for`".to_string(),
            Token::While => "`while`".to_string(),
            Token::If => "`if`".to_string(),
            Token::Else => "`else`".to_string(),
            Token::Minus => "`-`".to_string(),
            Token::Plus => "`+`".to_string(),
            Token::Star => "`*`".to_string(),
            Token::Slash => "`/`".to_string(),
            Token::Modulo => "`%`".to_string(),
            Token::PlusPlus => "`++`".to_string(),
            Token::MinusMinus => "`--`".to_string(),
            Token::Equals => "`==`".to_string(),
            Token::NotEquals => "`!=`".to_string(),
            Token::PlusEquals => "`+=`".to_string(),
            Token::MinusEquals => "`-=`".to_string(),
            Token::StarEquals => "`*=`".to_string(),
            Token::SlashEquals => "`/=`".to_string(),
            Token::ModuloEquals => "`%=`".to_string(),
            Token::LogicalAnd => "`&&`".to_string(),
            Token::LogicalOr => "`||`".to_string(),
            Token::LParen => "`(`".to_string(),
            Token::RParen => "`)`".to_string(),
            Token::LBrace => "`{`".to_string(),
            Token::RBrace => "`}`".to_string(),
            Token::Semicolon => "`;`".to_string(),
            Token::Bang => "`!`".to_string(),
            Token::Tilde => "`~`".to_string(),
            Token::Comma => "`,`".to_string(),
            Token::Colon => "`:`".to_string(),
            Token::LessThan => "`<`".to_string(),
            Token::LessThanEquals => "`<=`".to_string(),
            Token::GreaterThan => "`>`".to_string(),
            Token::GreaterThanEquals => "`>=`".to_string(),
            Token::EOF => "end of file".to_string(),
        }
    }
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

    fn read_number(&mut self) -> Result<Token, CompileError> {
        let start = self.position;
        let mut num = String::new();

        while matches!(self.current(), Some(c) if c.is_ascii_digit()) {
            num.push(self.advance().unwrap());
        }

        let value: i64 = num.parse().map_err(|_| {
            CompileError::new(
                format!("integer literal `{num}` out of range for i64"),
                Span::new(start, self.position),
            )
        })?;
        Ok(Token::IntLiteral(value))
    }

    fn read_string(&mut self) -> Result<Token, CompileError> {
        self.advance(); // consume opening "
        let mut s = String::new();
        while let Some(c) = self.current() {
            if c == '"' { self.advance(); break; }
            if c == '\\' {
                let esc_start = self.position;
                self.advance();
                match self.current() {
                    Some('n')  => { s.push('\n'); self.advance(); }
                    Some('t')  => { s.push('\t'); self.advance(); }
                    Some('"')  => { s.push('"');  self.advance(); }
                    Some('\\') => { s.push('\\'); self.advance(); }
                    other => {
                        let shown = other.map(|c| c.to_string()).unwrap_or_else(|| "<eof>".to_string());
                        return Err(CompileError::new(
                            format!("unknown escape sequence `\\{shown}`"),
                            Span::new(esc_start, self.position + 1),
                        ))
                    }
                }
            } else {
                s.push(c);
                self.advance();
            }
        }
        Ok(Token::StringLiteral(s))
    }

    fn read_identifier(&mut self) -> Token {
        let mut ident = String::new();
        while matches!(self.current(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            ident.push(self.advance().unwrap());
        }
        match ident.as_str() {
            "int" => Token::Int,
            "char" => Token::Char,
            "long" => Token::Long,
            "void" => Token::Void,
            "return" => Token::Return,
            "for" => Token::For,
            "while" => Token::While,
            "if" => Token::If,
            "else" => Token::Else,
            _ => Token::Ident(ident),
        }
    }

    pub fn next_token(&mut self) -> Result<SpannedToken, CompileError> {
        self.skip_whitespace();
        let start = self.position;
        let token = match self.current() {
            Some(c) if c.is_ascii_digit() => self.read_number()?,
            Some(c) if c.is_ascii_alphabetic() || c == '_' => self.read_identifier(),
            Some('"') => self.read_string()?,
            Some('(') => { self.advance(); Token::LParen },
            Some(')') => { self.advance(); Token::RParen },
            Some('{') => { self.advance(); Token::LBrace },
            Some('}') => { self.advance(); Token::RBrace },
            Some(';') => { self.advance(); Token::Semicolon },
            Some(',') => { self.advance(); Token::Comma },
            Some('-') => { 
                self.advance();
                match self.current() {
                    Some('-') => { self.advance(); Token::MinusMinus },
                    Some('=') => { self.advance(); Token::MinusEquals },
                    _ => Token::Minus,
                } 
            },
            Some('+') => { 
                self.advance(); // consume first '+', so we can check for '++' or '+='
                match self.current() {
                    Some('+') => { self.advance(); Token::PlusPlus },
                    Some('=') => { self.advance(); Token::PlusEquals },
                    _ => Token::Plus,
                }
            },
            Some('*') => { 
                self.advance();
                match self.current() {
                    Some('=') => { self.advance(); Token::StarEquals },
                    _ => Token::Star,
                } 
            },
            Some('/') => { 
                self.advance(); 
                match self.current() {
                    Some('=') => { self.advance(); Token::SlashEquals },
                    Some('/') => { 
                        // skip single-line comment
                        while self.current() != Some('\n') && self.current().is_some() {
                            self.advance();
                        }
                        return self.next_token(); // get the next token after the comment
                    },
                    _ => Token::Slash,
                }
            },
            Some('#') => {
                // for now just pretend preprocessor directives are comments and skip them
                self.advance();
                while self.current() != Some('\n') && self.current().is_some() {
                    self.advance();
                }
                return self.next_token();
            },
            Some('%') => { 
                self.advance(); 
                match self.current() {
                    Some('=') => { self.advance(); Token::ModuloEquals },
                    _ => Token::Modulo,
                }
            },
            Some('!') => { 
                self.advance();
                match self.current() {
                    Some('=') => { self.advance(); Token::NotEquals },
                    _ => Token::Bang,
                } 
            },
            Some('~') => { self.advance(); Token::Tilde },
            Some('=') => { 
                self.advance(); 
                match self.current() {
                    Some('=') => { self.advance(); Token::Equals },
                    _ => Token::Equals, // single '=' is for assignment, but we'll handle that in the parser
                } 
            },
            Some(':') => { self.advance(); Token::Colon },
            Some('<') => { 
                self.advance();
                match self.current() {
                    Some('=') => { self.advance(); Token::LessThanEquals },
                    _ => Token::LessThan,
                } 
            },
            Some('>') => { 
                self.advance(); 
                match self.current() {
                    Some('=') => { self.advance(); Token::GreaterThanEquals },
                    _ => Token::GreaterThan,
                }
            },
            Some('&') => {
                self.advance();
                if self.current() == Some('&') { 
                    self.advance(); 
                    Token::LogicalAnd
                } else {
                    panic!("Unexpected character: '&' (did you mean '&&'?)");
                }
            },
            Some('|') => {
                self.advance();
                if self.current() == Some('|') {
                    self.advance();
                    Token::LogicalOr
                } else {
                    panic!("Unexpected character: '|' (did you mean '||'?)");
                }
            },
            None => Token::EOF,
            Some(other) => {
                self.advance();
                return Err(CompileError::new(
                    format!("unexpected character `{other}`"),
                    Span::new(start, self.position),
                ));
            }
        };
        Ok(SpannedToken { token, span: Span::new(start, self.position) })
    }

    // For testing: tokenize entire input at once
    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, CompileError> {
        let mut tokens = Vec::new();
        loop {
            let st = self.next_token()?;
            let is_eof = st.token == Token::EOF;
            tokens.push(st);
            if is_eof { break; }
        }
        Ok(tokens)
    }

}

/* ===================================== */
//                                       //
//        Unit tests for the lexer       //
//                                       // 
/* ===================================== */

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: tokenize to bare Token kinds (spans stripped) for existing assertions.
    fn lex(src: &str) -> Vec<Token> {
        Lexer::new(src).tokenize().unwrap().into_iter().map(|st| st.token).collect()
    }

    #[test]
    fn test_single_number() {
        assert_eq!(lex("42"), vec![Token::IntLiteral(42), Token::EOF]);
    }

    #[test]
    fn test_keyword_recognition() {
        assert_eq!(lex("int return"), vec![
            Token::Int,
            Token::Return,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_control_flow_keywords() {
        assert_eq!(lex("for while if else"), vec![
            Token::For,
            Token::While,
            Token::If,
            Token::Else,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_whitespace_is_ignored() {
        assert_eq!(lex("   42   "), vec![Token::IntLiteral(42), Token::EOF]);
    }

    #[test]
    fn test_multi_digit_number() {
        assert_eq!(lex("1234"), vec![Token::IntLiteral(1234), Token::EOF]);
    }

    #[test]
    fn test_negative_number_tokens() {
        assert_eq!(
            lex("-42"),
            vec![Token::Minus, Token::IntLiteral(42), Token::EOF]
        );
    }

    #[test]
    fn test_ident_vs_keyword() {
        assert_eq!(lex("integer int"), vec![
            Token::Ident("integer".to_string()),
            Token::Int,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_increment_decrement() {
        assert_eq!(lex("i++ j--"), vec![
            Token::Ident("i".to_string()),
            Token::PlusPlus,
            Token::Ident("j".to_string()),
            Token::MinusMinus,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_compound_assignment() {
        assert_eq!(lex("+= -= *= /= %="), vec![
            Token::PlusEquals,
            Token::MinusEquals,
            Token::StarEquals,
            Token::SlashEquals,
            Token::ModuloEquals,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_comparison_operators() {
        assert_eq!(lex("< <= > >="), vec![
            Token::LessThan,
            Token::LessThanEquals,
            Token::GreaterThan,
            Token::GreaterThanEquals,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_plus_not_confused_with_plus_plus() {
        assert_eq!(lex("a + b"), vec![
            Token::Ident("a".to_string()),
            Token::Plus,
            Token::Ident("b".to_string()),
            Token::EOF,
        ]);
    }

    #[test]
    fn test_for_loop_tokens() {
        assert_eq!(lex("for (int i = 0; i < 10; i++)"), vec![
            Token::For,
            Token::LParen,
            Token::Int,
            Token::Ident("i".to_string()),
            Token::Equals,
            Token::IntLiteral(0),
            Token::Semicolon,
            Token::Ident("i".to_string()),
            Token::LessThan,
            Token::IntLiteral(10),
            Token::Semicolon,
            Token::Ident("i".to_string()),
            Token::PlusPlus,
            Token::RParen,
            Token::EOF,
        ]);
    }
    

    #[test]
    fn token_carries_span() {
        let toks = Lexer::new("  42").tokenize().unwrap();
        assert_eq!(toks[0].token, Token::IntLiteral(42));
        assert_eq!(toks[0].span.start, 2);
        assert_eq!(toks[0].span.end, 4);
    }

    #[test]
    fn unexpected_character_is_a_located_error() {
        let err = Lexer::new("int x = @;").tokenize().unwrap_err();
        assert!(err.message.contains('@'), "message: {}", err.message);
        assert_eq!(err.span.start, 8); // position of '@'
    }
}
