//! Stage 1: turning C source text into a token stream.
//!
//! [`Lexer::tokenize`] scans the source once and returns a `Vec<SpannedToken>`, or a
//! [`CompileError`] pointing at the first character it could not recognise.
//!
//! Every token carries a [`Span`] recording where it began and ended in the original
//! text. Spans are threaded through the parser and type checker untouched so that an
//! error discovered several stages later can still be reported against the exact source
//! it came from.
//!
//! # Quirks
//!
//! - `=` and `==` produce distinct tokens (`Token::Assign` and `Token::Equals`). The
//!   parser depends on that distinction to tell an assignment from an equality test, and
//!   collapsing them produces confusing downstream errors.
//! - A `#` reaching the lexer is a **hard error**. Directive lines are consumed by
//!   [`crate::preprocessor`] before the lexer ever sees them, so a stray `#` means
//!   either a `#` in the middle of a line or a preprocessor bug.
//! - The lexer is no longer stage 1. The preprocessor owns the read loop and calls
//!   [`Lexer::for_region`] / [`Lexer::retarget`] to tokenize one logical line at a
//!   time. [`Lexer::new`] still tokenizes a whole string as file 0, which is what
//!   the tests use.
//! - [`Lexer::tokenize`] appends `Token::EOF`; [`Lexer::tokenize_region`] does not.
//!   The preprocessor needs the latter, since one `EOF` per line would end the
//!   parse early.

use crate::diagnostic::{CompileError, Span, FileId};

#[derive(Debug, Clone, PartialEq)]  // so we can use '{:?}' and compare tokens. Clone for duplicating tokens when needed.
pub enum Token {

    // literals + identifiers
    IntLiteral(i64),
    CharLiteral(i64),
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
    Assign,
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
    Ampersand,
    LBracket,
    RBracket,

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
            Token::CharLiteral(_) => "character literal".to_string(),
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
            Token::Assign => "`=`".to_string(),
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
            Token::LBracket => "`[`".to_string(),
            Token::RBracket => "`]`".to_string(),
            Token::Semicolon => "`;`".to_string(),
            Token::Ampersand => "`&`".to_string(),
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

    /// Render this token back to C source text, for `-E` output.
    ///
    /// Mirrors [`Token::describe`], but without the surrounding backticks:
    /// `describe` is for prose inside a diagnostic, this is for text a C
    /// compiler could re-read.
    pub fn to_source(&self) -> String {
        match self {
            Token::IntLiteral(v) => v.to_string(),
            Token::CharLiteral(v) => format!("'{}'", (*v as u8) as char),
            Token::StringLiteral(s) => format!("\"{s}\""),
            Token::Ident(name) => name.clone(),
            Token::Int => "int".to_string(),
            Token::Char => "char".to_string(),
            Token::Long => "long".to_string(),
            Token::Void => "void".to_string(),
            Token::Return => "return".to_string(),
            Token::For => "for".to_string(),
            Token::While => "while".to_string(),
            Token::If => "if".to_string(),
            Token::Else => "else".to_string(),
            Token::Minus => "-".to_string(),
            Token::Plus => "+".to_string(),
            Token::Star => "*".to_string(),
            Token::Slash => "/".to_string(),
            Token::Modulo => "%".to_string(),
            Token::PlusPlus => "++".to_string(),
            Token::MinusMinus => "--".to_string(),
            Token::Assign => "=".to_string(),
            Token::Equals => "==".to_string(),
            Token::NotEquals => "!=".to_string(),
            Token::PlusEquals => "+=".to_string(),
            Token::MinusEquals => "-=".to_string(),
            Token::StarEquals => "*=".to_string(),
            Token::SlashEquals => "/=".to_string(),
            Token::ModuloEquals => "%=".to_string(),
            Token::LogicalAnd => "&&".to_string(),
            Token::LogicalOr => "||".to_string(),
            Token::LParen => "(".to_string(),
            Token::RParen => ")".to_string(),
            Token::LBrace => "{".to_string(),
            Token::RBrace => "}".to_string(),
            Token::LBracket => "[".to_string(),
            Token::RBracket => "]".to_string(),
            Token::Semicolon => ";".to_string(),
            Token::Ampersand => "&".to_string(),
            Token::Bang => "!".to_string(),
            Token::Tilde => "~".to_string(),
            Token::Comma => ",".to_string(),
            Token::Colon => ":".to_string(),
            Token::LessThan => "<".to_string(),
            Token::LessThanEquals => "<=".to_string(),
            Token::GreaterThan => ">".to_string(),
            Token::GreaterThanEquals => ">=".to_string(),
            Token::EOF => String::new(),
        }
    }
}

pub struct Lexer {
    input: Vec<char>,
    position: usize,
    file: FileId,
    end: usize,
}

impl Lexer {

    pub fn new(source: &str) -> Self {
        let len = source.chars().count();
        Lexer::for_region(source, 0, 0, len)
    }

    /// Lex `source[start..end]`, stamping every span with `file`.
    ///
    /// `position` stays an **absolute** index into the whole of `source`, so the
    /// spans this produces are already in the file's coordinate system — no
    /// offset arithmetic needed at the call site.
    pub fn for_region(source: &str, file: FileId, start: usize, end: usize) -> Self {
        let input: Vec<char> = source.chars().collect();
        let end = end.min(input.len());
        Lexer { input, position: start.min(end), file, end }
    }

    /// The preprocessor keeps one lexer per open file and moves its window with
    /// [`Lexer::retarget`], rather than rebuilding a lexer per line.
    pub fn from_chars(input: Vec<char>, file: FileId) -> Self {
        Lexer { input, position: 0, file, end: 0 }
    }

    /// Point this lexer at `[start, end)` of the buffer it already holds.
    pub fn retarget(&mut self, start: usize, end: usize) {
        self.end = end.min(self.input.len());
        self.position = start.min(self.end);
    }


    fn current(&self) -> Option<char> {
        if self.position >= self.end {
            return None;
        }
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
                Span::in_file(self.file, start, self.position),
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
                s.push(self.read_escape()?);
            } else {
                s.push(c);
                self.advance();
            }
        }
        Ok(Token::StringLiteral(s))
    }

    fn read_char(&mut self) -> Result<Token, CompileError> {
        let start = self.position;
        self.advance(); // consume opening '
        let value = match self.current() {
            Some('\\') => self.read_escape()?,       // cursor is on '\', contract holds
            Some('\'') => {
                return Err(CompileError::new(
                    "empty character literal".to_string(),
                    Span::in_file(self.file, start, self.position + 1),
                ));
            }
            Some(c) => { self.advance(); c }
            None => {
                return Err(CompileError::new(
                    "unterminated character literal".to_string(),
                    Span::in_file(self.file, start, self.position),
                ));
            }
        };
        match self.current() {
            Some('\'') => { self.advance(); }        // consume cl
            _ => {
                return Err(CompileError::new(
                    "expected closing `'` for character literal".to_string(),
                    Span::in_file(self.file, start, self.position),
                ));
            }
        }
        return Ok(Token::CharLiteral(value as i64));
    }

    /// Decode a backslash escape. Cursor must be on the `\`; on return it sits
    /// just past the escape character. Returns the decoded char.
    fn read_escape(&mut self) -> Result<char, CompileError> {
        let esc_start = self.position;
        self.advance(); // consume '\'
        let decoded = match self.current() {
            Some('n')  => '\n',
            Some('t')  => '\t',
            Some('r')  => '\r',
            Some('0')  => '\0',
            Some('"')  => '"',
            Some('\'') => '\'',
            Some('\\') => '\\',
            other => {
                let shown = other.map(|c| c.to_string()).unwrap_or_else(|| "<eof>".to_string());
                return Err(CompileError::new(
                    format!("unknown escape sequence `\\{shown}`"),
                    Span::in_file(self.file, esc_start, self.position + 1),
                ));
            }
        };
        self.advance(); // consume the escape character
        return Ok(decoded);
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
            Some('\'') => self.read_char()?,
            Some('(') => { self.advance(); Token::LParen },
            Some(')') => { self.advance(); Token::RParen },
            Some('{') => { self.advance(); Token::LBrace },
            Some('}') => { self.advance(); Token::RBrace },
            Some('[') => { self.advance(); Token::LBracket },
            Some(']') => { self.advance(); Token::RBracket },
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
                    Some('*') => {
                        self.advance();
                        loop {
                            match self.current() {
                                None => {
                                    return Err(CompileError::new("unterminated block comment", Span::in_file(self.file, start, self.position)));
                                }
                                Some('*') => {
                                    self.advance();
                                    if self.current() == Some('/') {
                                        self.advance();
                                        break;
                                    }
                                }
                                Some(_) => { self.advance(); }
                            }
                        }
                        return self.next_token();
                    },
                    _ => Token::Slash,
                }
            },
            Some('#') => {
                // The preprocessor consumes every directive line before the lexer
                // sees it, so a `#` reaching here is either a stray one mid-line
                // or a preprocessor bug. Either way it should be loud.
                self.advance();
                return Err(CompileError::new(
                    "stray `#` in program",
                    Span::in_file(self.file, start, self.position),
                )
                .with_label("directives are handled by the preprocessor"));
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
                    _ => Token::Assign, // single '=' is assignment; '==' is Token::Equals
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
                    Token::Ampersand
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
                    Span::in_file(self.file, start, self.position),
                ));
            }
        };
        Ok(SpannedToken { token, span: Span::in_file(self.file, start, self.position) })
    }

    /// Tokenize to the end of this lexer's region, **without** a trailing `EOF`.
    ///
    /// The preprocessor calls this once per line and appends a single `EOF` after
    /// the last file; an `EOF` per region would terminate the parser early.
    pub fn tokenize_region(&mut self) -> Result<Vec<SpannedToken>, CompileError> {
        let mut tokens = Vec::new();
        loop {
            let st = self.next_token()?;
            if st.token == Token::EOF {
                break;
            }
            tokens.push(st);
        }
        Ok(tokens)
    }

    /// Tokenize the whole region and append `Token::EOF`.
    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, CompileError> {
        let mut tokens = self.tokenize_region()?;
        let at = self.position.min(self.end);
        tokens.push(SpannedToken {
            token: Token::EOF,
            span: Span::in_file(self.file, at, at),
        });
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
            Token::Assign,
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

     #[test]
    fn lex_ampersand_and_brackets() {
        assert_eq!(lex("& [ ]"), vec![Token::Ampersand, Token::LBracket, Token::RBracket, Token::EOF]);
    }

    #[test]
    fn lex_logical_and_still_wins() {
        assert_eq!(lex("&&"), vec![Token::LogicalAnd, Token::EOF]);
    }

    #[test]
    fn multi_line_comment_is_skipped() {
        let src = "int x; /* this is a comment\n spanning multiple lines */ int y;";
        assert_eq!(lex(src), vec![
            Token::Int,
            Token::Ident("x".to_string()),
            Token::Semicolon,
            Token::Int,
            Token::Ident("y".to_string()),
            Token::Semicolon,
            Token::EOF,
        ]);
    }
      #[test]
    fn for_region_stops_at_the_end_bound() {
        //            0123456789...
        let src = "int x; int y;";
        let mut lx = Lexer::for_region(src, 0, 0, 6);
        let toks: Vec<Token> = lx.tokenize_region().unwrap().into_iter().map(|t| t.token).collect();
        assert_eq!(toks, vec![Token::Int, Token::Ident("x".to_string()), Token::Semicolon]);
    }

    #[test]
    fn for_region_spans_are_absolute_file_offsets() {
        let src = "int x; int y;";
        let start = src.find("int y").unwrap(); // 7
        let mut lx = Lexer::for_region(src, 3, start, src.chars().count());
        let toks = lx.tokenize_region().unwrap();
        assert_eq!(toks[0].token, Token::Int);
        assert_eq!(toks[0].span.file, 3, "region must stamp the file id");
        assert_eq!(toks[0].span.start, start, "spans are file offsets, not slice offsets");
    }

    #[test]
    fn tokenize_region_omits_eof_but_tokenize_keeps_it() {
        let src = "int x;";
        let region = Lexer::for_region(src, 0, 0, src.chars().count())
            .tokenize_region().unwrap();
        assert_ne!(region.last().unwrap().token, Token::EOF);

        let whole = Lexer::new(src).tokenize().unwrap();
        assert_eq!(whole.last().unwrap().token, Token::EOF);
    }

    #[test]
    fn lexer_new_still_means_whole_file_zero() {
        let toks = Lexer::new("int x;").tokenize().unwrap();
        assert_eq!(toks[0].span.file, 0);
        assert_eq!(toks[0].span.start, 0);
    }

    #[test]
    fn to_source_round_trips_a_simple_program() {
        let src = "int main() { return 42; }";
        let rendered: Vec<String> = Lexer::new(src).tokenize().unwrap()
            .into_iter()
            .filter(|t| t.token != Token::EOF)
            .map(|t| t.token.to_source())
            .collect();
        assert_eq!(rendered.join(" "), "int main ( ) { return 42 ; }");
    }

    #[test]
    fn to_source_quotes_string_literals() {
        assert_eq!(Token::StringLiteral("hi".to_string()).to_source(), "\"hi\"");
    }

    #[test]
    fn stray_hash_is_now_an_error() {
        // The preprocessor consumes every directive line, so a `#` reaching the
        // lexer means a preprocessor bug rather than user error.
        let err = Lexer::new("int x = 1; # oops").tokenize().unwrap_err();
        assert!(err.message.contains('#'), "got: {}", err.message);
    }
}
