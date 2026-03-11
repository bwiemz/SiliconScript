use ssl_core::lexer::Token;
use ssl_core::span::{Span, Spanned};
use ssl_core::parser::Parser;

fn s(start: u32, end: u32) -> Span {
    Span::new(start, end)
}

fn tok(token: Token, start: u32, end: u32) -> Spanned<Token> {
    Spanned::new(token, s(start, end))
}

#[test]
fn parser_peek_and_advance() {
    let source = "a b";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Ident, 2, 3),
    ];
    let mut p = Parser::new(source, tokens);
    assert_eq!(p.peek(), Some(&Token::Ident));
    assert!(!p.is_at_end());
    let first = p.advance();
    assert_eq!(first.span, s(0, 1));
    assert_eq!(p.text(first.span), "a");
    let second = p.advance();
    assert_eq!(p.text(second.span), "b");
    assert!(p.is_at_end());
    assert_eq!(p.peek(), None);
}

#[test]
fn parser_check_discriminant() {
    let source = "42";
    let tokens = vec![
        tok(Token::Numeric(ssl_core::lexer::NumericLiteral::Decimal(42)), 0, 2),
    ];
    let p = Parser::new(source, tokens);
    assert!(p.check(Token::Numeric(ssl_core::lexer::NumericLiteral::Decimal(0))));
    assert!(!p.check(Token::Ident));
}

#[test]
fn parser_eat_and_expect() {
    let source = "x + y";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Plus, 2, 3),
        tok(Token::Ident, 4, 5),
    ];
    let mut p = Parser::new(source, tokens);
    assert!(p.eat(Token::Plus).is_none());
    assert!(p.eat(Token::Ident).is_some());
    let plus = p.expect_token(Token::Plus);
    assert!(plus.is_ok());
    let err = p.expect_token(Token::Plus);
    assert!(err.is_err());
}

#[test]
fn parser_expect_ident() {
    let source = "counter";
    let tokens = vec![tok(Token::Ident, 0, 7)];
    let mut p = Parser::new(source, tokens);
    let ident = p.expect_ident().unwrap();
    assert_eq!(ident.node, "counter");
    assert_eq!(ident.span, s(0, 7));
}

#[test]
fn parser_expect_ident_fail() {
    let source = "+";
    let tokens = vec![tok(Token::Plus, 0, 1)];
    let mut p = Parser::new(source, tokens);
    assert!(p.expect_ident().is_err());
}

#[test]
fn parser_skip_newlines() {
    let source = "a\n\n\nb";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Newline, 1, 2),
        tok(Token::Newline, 2, 3),
        tok(Token::Newline, 3, 4),
        tok(Token::Ident, 4, 5),
    ];
    let mut p = Parser::new(source, tokens);
    p.advance();
    p.skip_newlines();
    assert_eq!(p.text(p.peek_span()), "b");
}

#[test]
fn parser_parse_block() {
    let source = ":\n  x\n  y\n";
    let tokens = vec![
        tok(Token::Colon, 0, 1),
        tok(Token::Newline, 1, 2),
        tok(Token::Indent, 2, 2),
        tok(Token::Ident, 4, 5),
        tok(Token::Newline, 5, 6),
        tok(Token::Ident, 8, 9),
        tok(Token::Newline, 9, 10),
        tok(Token::Dedent, 10, 10),
    ];
    let mut p = Parser::new(source, tokens);
    let items = p.parse_block(|p| {
        let t = p.advance();
        Ok(p.text(t.span).to_string())
    });
    let items = items.unwrap();
    assert_eq!(items, vec!["x".to_string(), "y".to_string()]);
}

#[test]
fn parser_parse_comma_list() {
    let source = "x, y, z)";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Comma, 1, 2),
        tok(Token::Ident, 3, 4),
        tok(Token::Comma, 4, 5),
        tok(Token::Ident, 6, 7),
        tok(Token::RParen, 7, 8),
    ];
    let mut p = Parser::new(source, tokens);
    let items = p.parse_comma_list(Token::RParen, |p| {
        let t = p.advance();
        Ok(p.text(t.span).to_string())
    });
    let items = items.unwrap();
    assert_eq!(items, vec!["x".to_string(), "y".to_string(), "z".to_string()]);
}

#[test]
fn parser_error_display() {
    let err = ssl_core::parser::ParseError {
        message: "unexpected token".into(),
        span: s(10, 15),
    };
    assert_eq!(format!("{}", err), "parse error at 10..15: unexpected token");
}
