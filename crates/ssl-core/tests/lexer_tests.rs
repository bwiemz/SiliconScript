use ssl_core::lexer::{lex, Token, NumericLiteral, NumericBase};

fn token_types(source: &str) -> Vec<Token> {
    lex(source)
        .expect("lex failed")
        .into_iter()
        .map(|s| s.node)
        .filter(|t| !matches!(t, Token::Newline | Token::LineComment | Token::BlockComment))
        .collect()
}

#[test]
fn keywords() {
    let tokens = token_types("module signal reg comb");
    assert_eq!(
        tokens,
        vec![Token::KwModule, Token::KwSignal, Token::KwReg, Token::KwComb]
    );
}

#[test]
fn operators() {
    let tokens = token_types("+ - * / == != <= >= << >> >>> ++ |>");
    assert_eq!(
        tokens,
        vec![
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::EqEq,
            Token::NotEq,
            Token::LessEq,
            Token::GreaterEq,
            Token::ShiftLeft,
            Token::ShiftRight,
            Token::ArithShiftRight,
            Token::Concat,
            Token::PipeOp,
        ]
    );
}

#[test]
fn numeric_literals() {
    let tokens = token_types("42 0xFF 0b1010 8'hAB 16'b1010_0011");
    assert_eq!(tokens.len(), 5);
    assert_eq!(tokens[0], Token::Numeric(NumericLiteral::Decimal(42)));
    assert_eq!(tokens[1], Token::Numeric(NumericLiteral::Hex(0xFF)));
    assert_eq!(tokens[2], Token::Numeric(NumericLiteral::Binary(0b1010)));
    assert_eq!(
        tokens[3],
        Token::Numeric(NumericLiteral::Sized {
            width: 8,
            value: 0xAB,
            base: NumericBase::Hex,
            dont_care_mask: 0,
        })
    );
    assert_eq!(
        tokens[4],
        Token::Numeric(NumericLiteral::Sized {
            width: 16,
            value: 0b1010_0011,
            base: NumericBase::Binary,
            dont_care_mask: 0,
        })
    );
}

#[test]
fn string_literal() {
    let tokens = token_types(r#""hello world""#);
    assert_eq!(tokens, vec![Token::StringLit("hello world".to_string())]);
}

#[test]
fn identifier_vs_keyword() {
    let tokens = token_types("module my_module signal count");
    assert_eq!(
        tokens,
        vec![Token::KwModule, Token::Ident, Token::KwSignal, Token::Ident]
    );
}

#[test]
fn delimiters() {
    let tokens = token_types("( ) [ ] { } : , .");
    assert_eq!(
        tokens,
        vec![
            Token::LParen,
            Token::RParen,
            Token::LBracket,
            Token::RBracket,
            Token::LBrace,
            Token::RBrace,
            Token::Colon,
            Token::Comma,
            Token::Dot,
        ]
    );
}

#[test]
fn range_operators() {
    let tokens = token_types("0..8 0..=7");
    // Expect: Numeric(0), RangeExclusive, Numeric(8), Numeric(0), RangeInclusive, Numeric(7)
    assert!(tokens.contains(&Token::RangeExclusive));
    assert!(tokens.contains(&Token::RangeInclusive));
}

#[test]
fn line_comment() {
    let raw: Vec<Token> = lex("// this is a comment\n")
        .expect("lex failed")
        .into_iter()
        .map(|s| s.node)
        .collect();
    assert!(raw.contains(&Token::LineComment));
}

#[test]
fn nestable_block_comment() {
    let tokens = token_types("signal /* outer /* inner */ still outer */ x");
    assert_eq!(tokens, vec![Token::KwSignal, Token::Ident]);
}

#[test]
fn module_port_list() {
    let source = "module ALU(in a: UInt, out b: UInt):";
    let tokens = token_types(source);
    assert_eq!(tokens[0], Token::KwModule);
    assert_eq!(tokens[1], Token::Ident); // ALU
    assert_eq!(tokens[2], Token::LParen);
}

#[test]
fn arrows() {
    let tokens = token_types("-> => -->");
    assert_eq!(tokens, vec![Token::ThinArrow, Token::FatArrow, Token::LongArrow]);
}

#[test]
fn exponentiation_operator() {
    let tokens = token_types("2 ** 10");
    assert_eq!(
        tokens,
        vec![
            Token::Numeric(NumericLiteral::Decimal(2)),
            Token::StarStar,
            Token::Numeric(NumericLiteral::Decimal(10)),
        ]
    );
}

#[test]
fn underscore_vs_identifier() {
    let tokens = token_types("_ _foo");
    assert_eq!(tokens, vec![Token::Underscore, Token::Ident]);
}

#[test]
fn at_operator() {
    let raw: Vec<Token> = lex("@clk")
        .expect("lex failed")
        .into_iter()
        .map(|s| s.node)
        .collect();
    assert!(raw.contains(&Token::At));
}

#[test]
fn empty_source() {
    let tokens = token_types("");
    assert!(tokens.is_empty());
}

#[test]
fn only_comments() {
    let tokens = token_types("// just a comment");
    assert!(tokens.is_empty());
}

#[test]
fn fsm_transition_syntax() {
    let tokens = token_types("Idle --(condition)--> Fetch");
    // Ident DashDash LParen Ident RParen LongArrow Ident
    assert!(tokens.contains(&Token::DashDash));
    assert!(tokens.contains(&Token::LParen));
    assert!(tokens.contains(&Token::LongArrow));
}

#[test]
fn doc_comment() {
    let raw: Vec<Token> = lex("/// doc comment\nmodule Foo")
        .expect("lex failed")
        .into_iter()
        .map(|s| s.node)
        .collect();
    assert!(raw.contains(&Token::DocComment));
}
