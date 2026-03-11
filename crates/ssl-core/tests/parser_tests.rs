use ssl_core::lexer::Token;
use ssl_core::span::{Span, Spanned};
use ssl_core::parser::Parser;
use ssl_core::ast::expr::{BinOp, ExprKind, UnaryOp};
use ssl_core::lexer::NumericLiteral;
use ssl_core::parser::expr::parse_expr;

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

/// Helper: lex source, strip comments/whitespace tokens, feed to parser, parse one expression.
fn parse_one_expr(source: &str) -> ssl_core::ast::expr::Expr {
    let tokens = ssl_core::lexer::tokenize(source).expect("lexer failed");
    // Filter out tokens not needed for expression parsing
    let tokens: Vec<_> = tokens
        .into_iter()
        .filter(|t| !matches!(
            t.node,
            Token::Newline | Token::Indent | Token::Dedent
        ))
        .collect();
    let mut p = Parser::new(source, tokens);
    parse_expr(&mut p).expect("parse failed")
}

#[test]
fn expr_int_literal() {
    let expr = parse_one_expr("42");
    assert!(matches!(
        expr.node,
        ExprKind::IntLiteral(NumericLiteral::Decimal(42))
    ));
}

#[test]
fn expr_string_literal() {
    let expr = parse_one_expr("\"hello\"");
    match &expr.node {
        ExprKind::StringLiteral(s) => assert_eq!(s, "hello"),
        _ => panic!("expected StringLiteral, got {:?}", expr.node),
    }
}

#[test]
fn expr_bool_literal() {
    let t = parse_one_expr("true");
    assert!(matches!(t.node, ExprKind::BoolLiteral(true)));
    let f = parse_one_expr("false");
    assert!(matches!(f.node, ExprKind::BoolLiteral(false)));
}

#[test]
fn expr_ident() {
    let expr = parse_one_expr("counter");
    match &expr.node {
        ExprKind::Ident(name) => assert_eq!(name, "counter"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn expr_add_mul_precedence() {
    // a + b * c  =>  Binary(Add, a, Binary(Mul, b, c))
    let expr = parse_one_expr("a + b * c");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::Add);
            assert!(matches!(lhs.node, ExprKind::Ident(_)));
            match &rhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Mul),
                _ => panic!("expected Mul on rhs"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_unary_not_and() {
    // not x and y  =>  Binary(And, Unary(Not, x), y)
    let expr = parse_one_expr("not x and y");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::And);
            match &lhs.node {
                ExprKind::Unary { op, .. } => assert_eq!(*op, UnaryOp::LogicalNot),
                _ => panic!("expected Unary on lhs"),
            }
            assert!(matches!(rhs.node, ExprKind::Ident(_)));
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_field_access() {
    let expr = parse_one_expr("a.field");
    match &expr.node {
        ExprKind::FieldAccess { object, field } => {
            assert!(matches!(object.node, ExprKind::Ident(_)));
            assert_eq!(field.node, "field");
        }
        _ => panic!("expected FieldAccess, got {:?}", expr.node),
    }
}

#[test]
fn expr_index() {
    let expr = parse_one_expr("a[0]");
    match &expr.node {
        ExprKind::Index { array, index } => {
            assert!(matches!(array.node, ExprKind::Ident(_)));
            assert!(matches!(index.node, ExprKind::IntLiteral(_)));
        }
        _ => panic!("expected Index"),
    }
}

#[test]
fn expr_bit_slice() {
    let expr = parse_one_expr("a[7:0]");
    match &expr.node {
        ExprKind::BitSlice { value, high, low } => {
            assert!(matches!(value.node, ExprKind::Ident(_)));
            assert!(matches!(high.node, ExprKind::IntLiteral(NumericLiteral::Decimal(7))));
            assert!(matches!(low.node, ExprKind::IntLiteral(NumericLiteral::Decimal(0))));
        }
        _ => panic!("expected BitSlice"),
    }
}

#[test]
fn expr_call() {
    let expr = parse_one_expr("f(x, y)");
    match &expr.node {
        ExprKind::Call { callee, args } => {
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "f"),
                _ => panic!("expected Ident callee"),
            }
            assert_eq!(args.len(), 2);
            assert!(args[0].name.is_none());
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_call_named_arg() {
    let expr = parse_one_expr("f(name=x)");
    match &expr.node {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 1);
            assert_eq!(args[0].name.as_ref().unwrap().node, "name");
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_pipe() {
    let expr = parse_one_expr("a |> f(b)");
    match &expr.node {
        ExprKind::Pipe { input, callee, args } => {
            assert!(matches!(input.node, ExprKind::Ident(_)));
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "f"),
                _ => panic!("expected Ident callee"),
            }
            assert_eq!(args.len(), 1);
        }
        _ => panic!("expected Pipe, got {:?}", expr.node),
    }
}

#[test]
fn expr_if_then_else() {
    let expr = parse_one_expr("if x then y else z");
    match &expr.node {
        ExprKind::IfExpr {
            condition,
            then_expr,
            else_expr,
        } => {
            assert!(matches!(condition.node, ExprKind::Ident(_)));
            assert!(matches!(then_expr.node, ExprKind::Ident(_)));
            assert!(matches!(else_expr.node, ExprKind::Ident(_)));
        }
        _ => panic!("expected IfExpr"),
    }
}

#[test]
fn expr_implies() {
    let expr = parse_one_expr("a implies b");
    match &expr.node {
        ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Implies),
        _ => panic!("expected Binary Implies"),
    }
}

#[test]
fn expr_paren() {
    let expr = parse_one_expr("(a + b)");
    match &expr.node {
        ExprKind::Paren(inner) => {
            assert!(matches!(inner.node, ExprKind::Binary { op: BinOp::Add, .. }));
        }
        _ => panic!("expected Paren"),
    }
}

#[test]
fn expr_array_literal() {
    let expr = parse_one_expr("[1, 2, 3]");
    match &expr.node {
        ExprKind::ArrayLiteral(elems) => assert_eq!(elems.len(), 3),
        _ => panic!("expected ArrayLiteral"),
    }
}

#[test]
fn expr_unary_neg() {
    let expr = parse_one_expr("-x");
    match &expr.node {
        ExprKind::Unary { op, .. } => assert_eq!(*op, UnaryOp::Neg),
        _ => panic!("expected Unary Neg"),
    }
}

#[test]
fn expr_power_right_assoc() {
    // 2 ** 3 ** 4  =>  Binary(Pow, 2, Binary(Pow, 3, 4))
    let expr = parse_one_expr("2 ** 3 ** 4");
    match &expr.node {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(*op, BinOp::Pow);
            match &rhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Pow),
                _ => panic!("expected nested Pow"),
            }
        }
        _ => panic!("expected Binary Pow"),
    }
}
