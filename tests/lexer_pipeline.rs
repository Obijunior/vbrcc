use rust_c_compiler::lexer::{Lexer, Token};

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
