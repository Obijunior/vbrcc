use vbrcc::lexer::{Lexer, Token};

#[test]
fn basic_tokenize() {
    let source = "int main() { return 42; }";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    assert_eq!(
        tokens,
        vec![
            Token::Int,
            Token::Ident("main".to_string()),
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::Return,
            Token::IntLiteral(42),
            Token::Semicolon,
            Token::RBrace,
            Token::EOF,
        ]
    );
}

#[test]
fn tokenize_for_loop_program() {
    let source = "int main() { int s = 0; for (int i = 0; i < 10; i++) { s += i; } return s; }";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    assert_eq!(
        tokens,
        vec![
            Token::Int, Token::Ident("main".into()), Token::LParen, Token::RParen, Token::LBrace,
            // int s = 0;
            Token::Int, Token::Ident("s".into()), Token::Equals, Token::IntLiteral(0), Token::Semicolon,
            // for (
            Token::For, Token::LParen,
            // int i = 0;
            Token::Int, Token::Ident("i".into()), Token::Equals, Token::IntLiteral(0), Token::Semicolon,
            // i < 10;
            Token::Ident("i".into()), Token::LessThan, Token::IntLiteral(10), Token::Semicolon,
            // i++)
            Token::Ident("i".into()), Token::PlusPlus, Token::RParen,
            // { s += i; }
            Token::LBrace,
            Token::Ident("s".into()), Token::PlusEquals, Token::Ident("i".into()), Token::Semicolon,
            Token::RBrace,
            // return s; }
            Token::Return, Token::Ident("s".into()), Token::Semicolon,
            Token::RBrace,
            Token::EOF,
        ]
    );
}
