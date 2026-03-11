use ssl_core::lexer::Token;
use ssl_core::span::{Span, Spanned};
use ssl_core::parser::Parser;
use ssl_core::ast::expr::{BinOp, ExprKind, UnaryOp};
use ssl_core::lexer::NumericLiteral;
use ssl_core::parser::expr::parse_expr;
use ssl_core::ast::types::{TypeExprKind, Direction};
use ssl_core::parser::types::{parse_type_expr, parse_type_expr_with_domain};
use ssl_core::ast::stmt::StmtKind;
use ssl_core::parser::stmt::parse_stmt;

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

#[test]
fn expr_method_call() {
    let expr = parse_one_expr("a.truncate(8)");
    match &expr.node {
        ExprKind::MethodCall { object, method, args } => {
            assert!(matches!(object.node, ExprKind::Ident(_)));
            assert_eq!(method.node, "truncate");
            assert_eq!(args.len(), 1);
        }
        _ => panic!("expected MethodCall, got {:?}", expr.node),
    }
}

#[test]
fn expr_chained_field_access() {
    // a.b.c  =>  FieldAccess(FieldAccess(a, b), c)
    let expr = parse_one_expr("a.b.c");
    match &expr.node {
        ExprKind::FieldAccess { object, field } => {
            assert_eq!(field.node, "c");
            match &object.node {
                ExprKind::FieldAccess { field: inner_field, .. } => {
                    assert_eq!(inner_field.node, "b");
                }
                _ => panic!("expected nested FieldAccess"),
            }
        }
        _ => panic!("expected FieldAccess"),
    }
}

#[test]
fn expr_nested_call() {
    let expr = parse_one_expr("f(g(x))");
    match &expr.node {
        ExprKind::Call { callee, args } => {
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "f"),
                _ => panic!("expected f"),
            }
            assert_eq!(args.len(), 1);
            match &args[0].value.node {
                ExprKind::Call { callee, args } => {
                    match &callee.node {
                        ExprKind::Ident(name) => assert_eq!(name, "g"),
                        _ => panic!("expected g"),
                    }
                    assert_eq!(args.len(), 1);
                }
                _ => panic!("expected inner Call"),
            }
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_complex_precedence() {
    // a + b * c - d  =>  Binary(Sub, Binary(Add, a, Binary(Mul, b, c)), d)
    let expr = parse_one_expr("a + b * c - d");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::Sub);
            assert!(matches!(rhs.node, ExprKind::Ident(_)));
            match &lhs.node {
                ExprKind::Binary { op, rhs: inner_rhs, .. } => {
                    assert_eq!(*op, BinOp::Add);
                    assert!(matches!(inner_rhs.node, ExprKind::Binary { op: BinOp::Mul, .. }));
                }
                _ => panic!("expected Add"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_comparison_chain() {
    // a == b != c  =>  Binary(Ne, Binary(Eq, a, b), c)
    let expr = parse_one_expr("a == b != c");
    match &expr.node {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(*op, BinOp::Ne);
            match &lhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Eq),
                _ => panic!("expected Eq"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_bitwise_ops() {
    // a & b | c ^ d  =>  Binary(BitOr, Binary(BitAnd, a, b), Binary(BitXor, c, d))
    let expr = parse_one_expr("a & b | c ^ d");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::BitOr);
            match &lhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::BitAnd),
                _ => panic!("expected BitAnd"),
            }
            match &rhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::BitXor),
                _ => panic!("expected BitXor"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_shift_ops() {
    let expr = parse_one_expr("a << 2");
    match &expr.node {
        ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Shl),
        _ => panic!("expected Shl"),
    }
}

#[test]
fn expr_method_on_field() {
    // a.field.method(x)
    let expr = parse_one_expr("a.field.method(x)");
    match &expr.node {
        ExprKind::MethodCall { object, method, args } => {
            assert_eq!(method.node, "method");
            assert_eq!(args.len(), 1);
            match &object.node {
                ExprKind::FieldAccess { field, .. } => assert_eq!(field.node, "field"),
                _ => panic!("expected FieldAccess"),
            }
        }
        _ => panic!("expected MethodCall"),
    }
}

#[test]
fn expr_struct_literal_as_call() {
    // Point(x=1, y=2) — parsed as Call with named args
    let expr = parse_one_expr("Point(x=1, y=2)");
    match &expr.node {
        ExprKind::Call { callee, args } => {
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "Point"),
                _ => panic!("expected Ident"),
            }
            assert_eq!(args.len(), 2);
            assert_eq!(args[0].name.as_ref().unwrap().node, "x");
            assert_eq!(args[1].name.as_ref().unwrap().node, "y");
        }
        _ => panic!("expected Call (struct literal)"),
    }
}

#[test]
fn expr_index_then_field() {
    // a[0].field
    let expr = parse_one_expr("a[0].field");
    match &expr.node {
        ExprKind::FieldAccess { object, field } => {
            assert_eq!(field.node, "field");
            assert!(matches!(object.node, ExprKind::Index { .. }));
        }
        _ => panic!("expected FieldAccess"),
    }
}

#[test]
fn expr_paren_changes_precedence() {
    // (a + b) * c  =>  Binary(Mul, Paren(Binary(Add, a, b)), c)
    let expr = parse_one_expr("(a + b) * c");
    match &expr.node {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(*op, BinOp::Mul);
            match &lhs.node {
                ExprKind::Paren(inner) => {
                    assert!(matches!(inner.node, ExprKind::Binary { op: BinOp::Add, .. }));
                }
                _ => panic!("expected Paren"),
            }
        }
        _ => panic!("expected Binary Mul"),
    }
}

#[test]
fn expr_mixed_named_positional_args() {
    let expr = parse_one_expr("f(a, name=b, c)");
    match &expr.node {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 3);
            assert!(args[0].name.is_none());
            assert_eq!(args[1].name.as_ref().unwrap().node, "name");
            assert!(args[2].name.is_none());
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_next_single() {
    let expr = parse_one_expr("next(valid)");
    match &expr.node {
        ExprKind::Next { count, .. } => assert!(count.is_none()),
        _ => panic!("expected Next"),
    }
}

#[test]
fn expr_next_with_count() {
    let expr = parse_one_expr("next(valid, 3)");
    match &expr.node {
        ExprKind::Next { count, .. } => assert!(count.is_some()),
        _ => panic!("expected Next with count"),
    }
}

#[test]
fn expr_eventually_with_depth() {
    let expr = parse_one_expr("eventually(resp_valid, depth=16)");
    match &expr.node {
        ExprKind::Eventually { .. } => {} // depth is always present (Box<Expr>)
        _ => panic!("expected Eventually"),
    }
}

#[test]
fn expr_range_exclusive() {
    let expr = parse_one_expr("0..8");
    assert!(matches!(&expr.node, ExprKind::Range { inclusive: false, .. }));
}

#[test]
fn expr_range_inclusive() {
    let expr = parse_one_expr("0..=7");
    assert!(matches!(&expr.node, ExprKind::Range { inclusive: true, .. }));
}

fn parse_one_type(src: &str) -> ssl_core::ast::types::TypeExpr {
    let tokens = ssl_core::lexer::tokenize(src).expect("lexer failed");
    let tokens: Vec<_> = tokens.into_iter()
        .filter(|t| !matches!(t.node, Token::LineComment | Token::BlockComment | Token::Newline | Token::Indent | Token::Dedent | Token::DocComment))
        .collect();
    let mut p = Parser::new(src, tokens);
    parse_type_expr(&mut p).expect("parse error")
}

fn parse_one_type_with_domain(src: &str) -> ssl_core::ast::types::TypeExpr {
    let tokens = ssl_core::lexer::tokenize(src).expect("lexer failed");
    let tokens: Vec<_> = tokens.into_iter()
        .filter(|t| !matches!(t.node, Token::LineComment | Token::BlockComment | Token::Newline | Token::Indent | Token::Dedent | Token::DocComment))
        .collect();
    let mut p = Parser::new(src, tokens);
    parse_type_expr_with_domain(&mut p).expect("parse error")
}

#[test]
fn type_named_bool() {
    assert!(matches!(parse_one_type("Bool").node, TypeExprKind::Named(ref n) if n == "Bool"));
}

#[test]
fn type_generic_uint8() {
    match &parse_one_type("UInt<8>").node {
        TypeExprKind::Generic { name, params } => { assert_eq!(name, "UInt"); assert_eq!(params.len(), 1); }
        other => panic!("expected Generic, got {:?}", other),
    }
}

#[test]
fn type_array_of_generic() {
    assert!(matches!(&parse_one_type("UInt<8>[32]").node, TypeExprKind::Array { element, .. } if matches!(element.node, TypeExprKind::Generic { .. })));
}

#[test]
fn type_flip_of_generic() {
    assert!(matches!(&parse_one_type("Flip<Stream<T>>").node, TypeExprKind::Flip(inner) if matches!(inner.node, TypeExprKind::Generic { .. })));
}

#[test]
fn type_direction_wrapper_in() {
    match &parse_one_type("in<Bool>").node {
        TypeExprKind::DirectionWrapper { dir, .. } => assert_eq!(*dir, Direction::In),
        other => panic!("expected DirectionWrapper, got {:?}", other),
    }
}

#[test]
fn type_direction_wrapper_out() {
    match &parse_one_type("out<UInt<8>>").node {
        TypeExprKind::DirectionWrapper { dir, .. } => assert_eq!(*dir, Direction::Out),
        other => panic!("expected DirectionWrapper, got {:?}", other),
    }
}

#[test]
fn type_domain_annotated() {
    match &parse_one_type_with_domain("UInt<8> @ sys_clk").node {
        TypeExprKind::DomainAnnotated { domain, .. } => assert_eq!(domain.node, "sys_clk"),
        other => panic!("expected DomainAnnotated, got {:?}", other),
    }
}

#[test]
fn type_fixed_two_params() {
    match &parse_one_type("Fixed<8, 8>").node {
        TypeExprKind::Generic { name, params } => { assert_eq!(name, "Fixed"); assert_eq!(params.len(), 2); }
        other => panic!("expected Generic, got {:?}", other),
    }
}

#[test]
fn type_sync_reset_no_polarity() {
    assert!(matches!(parse_one_type("SyncReset").node, TypeExprKind::SyncReset { polarity: None }));
}

#[test]
fn type_async_reset_active_low() {
    match &parse_one_type("AsyncReset<active_low>").node {
        TypeExprKind::AsyncReset { polarity } => assert_eq!(*polarity, Some(ssl_core::ast::types::ResetPolarity::ActiveLow)),
        other => panic!("expected AsyncReset, got {:?}", other),
    }
}

#[test]
fn type_clock_with_edge() {
    match &parse_one_type("Clock<100, rising>").node {
        TypeExprKind::Clock { freq, edge } => { assert!(freq.is_some()); assert_eq!(*edge, Some(ssl_core::ast::types::ClockEdge::Rising)); }
        other => panic!("expected Clock, got {:?}", other),
    }
}

#[test]
fn type_memory() {
    match &parse_one_type("Memory<UInt<8>, depth=1024>").node {
        TypeExprKind::Memory { element, params } => { assert!(matches!(element.node, TypeExprKind::Generic { .. })); assert_eq!(params.len(), 1); }
        other => panic!("expected Memory, got {:?}", other),
    }
}

#[test]
fn type_partial_interface() {
    match &parse_one_type("AXI4Lite.{read_addr, read_data}").node {
        TypeExprKind::PartialInterface { name, groups } => { assert_eq!(name, "AXI4Lite"); assert_eq!(groups.len(), 2); }
        other => panic!("expected PartialInterface, got {:?}", other),
    }
}

fn parse_one_stmt(src: &str) -> ssl_core::ast::stmt::Stmt {
    let tokens = ssl_core::lexer::tokenize(src).expect("lexer failed");
    let tokens: Vec<_> = tokens.into_iter()
        .filter(|t| !matches!(t.node, Token::LineComment | Token::BlockComment | Token::Newline | Token::Indent | Token::Dedent | Token::DocComment))
        .collect();
    let mut p = Parser::new(src, tokens);
    parse_stmt(&mut p).expect("parse error")
}

#[test]
fn stmt_signal_decl() {
    match &parse_one_stmt("signal counter: UInt<8>").node {
        StmtKind::Signal(d) => {
            assert_eq!(d.name.node, "counter");
            assert!(d.domain.is_none());
            assert!(d.init.is_none());
        }
        other => panic!("expected Signal, got {:?}", other),
    }
}

#[test]
fn stmt_signal_with_domain_and_init() {
    match &parse_one_stmt("signal counter: UInt<8> @ sys_clk = 0").node {
        StmtKind::Signal(d) => {
            assert_eq!(d.domain.as_ref().unwrap().node, "sys_clk");
            assert!(d.init.is_some());
        }
        other => panic!("expected Signal, got {:?}", other),
    }
}

#[test]
fn stmt_let_decl() {
    assert!(matches!(&parse_one_stmt("let x = 42").node, StmtKind::Let(d) if d.name.node == "x" && d.ty.is_none()));
}

#[test]
fn stmt_let_with_type() {
    assert!(matches!(&parse_one_stmt("let x: UInt<8> = 42").node, StmtKind::Let(d) if d.ty.is_some()));
}

#[test]
fn stmt_const_decl() {
    assert!(matches!(&parse_one_stmt("const WIDTH: UInt<8> = 32").node, StmtKind::Const(d) if d.name.node == "WIDTH"));
}

#[test]
fn stmt_type_alias() {
    assert!(matches!(&parse_one_stmt("type Word = UInt<32>").node, StmtKind::TypeAlias(d) if d.name.node == "Word"));
}

#[test]
fn stmt_assignment() {
    assert!(matches!(parse_one_stmt("x = y + 1").node, StmtKind::Assign { .. }));
}

#[test]
fn stmt_expr_stmt() {
    assert!(matches!(parse_one_stmt("foo(bar)").node, StmtKind::ExprStmt(_)));
}

#[test]
fn stmt_static_assert() {
    assert!(matches!(parse_one_stmt("static_assert WIDTH > 0, \"width must be positive\"").node, StmtKind::StaticAssert { .. }));
}

fn parse_stmt_tokens(source: &str, tokens: Vec<Spanned<Token>>) -> ssl_core::ast::stmt::Stmt {
    let mut p = Parser::new(source, tokens);
    parse_stmt(&mut p).expect("parse error")
}

#[test]
fn block_if_simple() {
    let s = "if x:\n    y = 1\n";
    let t = vec![
        tok(Token::KwIf, 0, 2), tok(Token::Ident, 3, 4), tok(Token::Colon, 4, 5),
        tok(Token::Newline, 5, 6), tok(Token::Indent, 6, 6), tok(Token::Ident, 10, 11),
        tok(Token::Eq, 12, 13), tok(Token::Numeric(NumericLiteral::Decimal(1)), 14, 15),
        tok(Token::Newline, 15, 16), tok(Token::Dedent, 16, 16),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::If(i) => {
            assert_eq!(i.then_body.len(), 1);
            assert!(i.elif_branches.is_empty());
            assert!(i.else_body.is_none());
        }
        _ => panic!("expected If"),
    }
}

#[test]
fn block_if_elif_else() {
    let s = "if a:\n  x=1\nelif b:\n  x=2\nelse:\n  x=3\n";
    let t = vec![
        tok(Token::KwIf, 0, 2), tok(Token::Ident, 3, 4), tok(Token::Colon, 4, 5),
        tok(Token::Newline, 5, 6), tok(Token::Indent, 6, 6),
        tok(Token::Ident, 8, 9), tok(Token::Eq, 9, 10), tok(Token::Numeric(NumericLiteral::Decimal(1)), 10, 11),
        tok(Token::Newline, 11, 12), tok(Token::Dedent, 12, 12),
        tok(Token::KwElif, 12, 16), tok(Token::Ident, 17, 18), tok(Token::Colon, 18, 19),
        tok(Token::Newline, 19, 20), tok(Token::Indent, 20, 20),
        tok(Token::Ident, 22, 23), tok(Token::Eq, 23, 24), tok(Token::Numeric(NumericLiteral::Decimal(2)), 24, 25),
        tok(Token::Newline, 25, 26), tok(Token::Dedent, 26, 26),
        tok(Token::KwElse, 26, 30), tok(Token::Colon, 30, 31),
        tok(Token::Newline, 31, 32), tok(Token::Indent, 32, 32),
        tok(Token::Ident, 34, 35), tok(Token::Eq, 35, 36), tok(Token::Numeric(NumericLiteral::Decimal(3)), 36, 37),
        tok(Token::Newline, 37, 38), tok(Token::Dedent, 38, 38),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::If(i) => {
            assert_eq!(i.elif_branches.len(), 1);
            assert!(i.else_body.is_some());
        }
        _ => panic!("expected If"),
    }
}

#[test]
fn block_match_two_arms() {
    let s = "match st:\n  A=>x=0\n  B=>x=1\n";
    let t = vec![
        tok(Token::KwMatch, 0, 5), tok(Token::Ident, 6, 8), tok(Token::Colon, 8, 9),
        tok(Token::Newline, 9, 10), tok(Token::Indent, 10, 10),
        tok(Token::Ident, 12, 13), tok(Token::FatArrow, 13, 15),
        tok(Token::Ident, 15, 16), tok(Token::Eq, 16, 17), tok(Token::Numeric(NumericLiteral::Decimal(0)), 17, 18),
        tok(Token::Newline, 18, 19),
        tok(Token::Ident, 21, 22), tok(Token::FatArrow, 22, 24),
        tok(Token::Ident, 24, 25), tok(Token::Eq, 25, 26), tok(Token::Numeric(NumericLiteral::Decimal(1)), 26, 27),
        tok(Token::Newline, 27, 28),
        tok(Token::Dedent, 28, 28),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::Match(m) => assert_eq!(m.arms.len(), 2),
        _ => panic!("expected Match"),
    }
}

#[test]
fn block_comb() {
    let s = "comb:\n  x=a+b\n";
    let t = vec![
        tok(Token::KwComb, 0, 4), tok(Token::Colon, 4, 5),
        tok(Token::Newline, 5, 6), tok(Token::Indent, 6, 6),
        tok(Token::Ident, 8, 9), tok(Token::Eq, 9, 10),
        tok(Token::Ident, 10, 11), tok(Token::Plus, 11, 12), tok(Token::Ident, 12, 13),
        tok(Token::Newline, 13, 14), tok(Token::Dedent, 14, 14),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::CombBlock(v) => assert_eq!(v.len(), 1),
        _ => panic!("expected CombBlock"),
    }
}

#[test]
fn block_reg_reset_tick() {
    let s = "reg(c,r):\n on reset:\n  x=0\n on tick:\n  x=x+1\n";
    let t = vec![
        tok(Token::KwReg, 0, 3), tok(Token::LParen, 3, 4),
        tok(Token::Ident, 4, 5), tok(Token::Comma, 5, 6), tok(Token::Ident, 6, 7),
        tok(Token::RParen, 7, 8), tok(Token::Colon, 8, 9),
        tok(Token::Newline, 9, 10), tok(Token::Indent, 10, 10),
        tok(Token::KwOn, 11, 13), tok(Token::KwReset, 14, 19),
        tok(Token::Colon, 19, 20), tok(Token::Newline, 20, 21), tok(Token::Indent, 21, 21),
        tok(Token::Ident, 23, 24), tok(Token::Eq, 24, 25), tok(Token::Numeric(NumericLiteral::Decimal(0)), 25, 26),
        tok(Token::Newline, 26, 27), tok(Token::Dedent, 27, 27),
        tok(Token::KwOn, 28, 30), tok(Token::KwTick, 31, 35),
        tok(Token::Colon, 35, 36), tok(Token::Newline, 36, 37), tok(Token::Indent, 37, 37),
        tok(Token::Ident, 39, 40), tok(Token::Eq, 40, 41),
        tok(Token::Ident, 41, 42), tok(Token::Plus, 42, 43), tok(Token::Numeric(NumericLiteral::Decimal(1)), 43, 44),
        tok(Token::Newline, 44, 45), tok(Token::Dedent, 45, 45),
        tok(Token::Dedent, 45, 45),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::RegBlock(r) => {
            assert_eq!(r.on_reset.len(), 1);
            assert_eq!(r.on_tick.len(), 1);
            assert!(r.enable.is_none());
        }
        _ => panic!("expected RegBlock"),
    }
}

#[test]
fn block_priority() {
    let s = "priority:\n when a=>x=1\n when b=>x=2\n otherwise=>x=0\n";
    let t = vec![
        tok(Token::KwPriority, 0, 8), tok(Token::Colon, 8, 9),
        tok(Token::Newline, 9, 10), tok(Token::Indent, 10, 10),
        tok(Token::KwWhen, 11, 15), tok(Token::Ident, 16, 17), tok(Token::FatArrow, 17, 19),
        tok(Token::Ident, 19, 20), tok(Token::Eq, 20, 21), tok(Token::Numeric(NumericLiteral::Decimal(1)), 21, 22),
        tok(Token::Newline, 22, 23),
        tok(Token::KwWhen, 24, 28), tok(Token::Ident, 29, 30), tok(Token::FatArrow, 30, 32),
        tok(Token::Ident, 32, 33), tok(Token::Eq, 33, 34), tok(Token::Numeric(NumericLiteral::Decimal(2)), 34, 35),
        tok(Token::Newline, 35, 36),
        tok(Token::KwOtherwise, 37, 46), tok(Token::FatArrow, 46, 48),
        tok(Token::Ident, 48, 49), tok(Token::Eq, 49, 50), tok(Token::Numeric(NumericLiteral::Decimal(0)), 50, 51),
        tok(Token::Newline, 51, 52),
        tok(Token::Dedent, 52, 52),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::PriorityBlock(pb) => {
            assert_eq!(pb.arms.len(), 2);
            assert!(pb.otherwise.is_some());
        }
        _ => panic!("expected PriorityBlock"),
    }
}

#[test]
fn block_parallel() {
    let s = "parallel:\n when a=>x=1\n when b=>x=2\n";
    let t = vec![
        tok(Token::KwParallel, 0, 8), tok(Token::Colon, 8, 9),
        tok(Token::Newline, 9, 10), tok(Token::Indent, 10, 10),
        tok(Token::KwWhen, 11, 15), tok(Token::Ident, 16, 17), tok(Token::FatArrow, 17, 19),
        tok(Token::Ident, 19, 20), tok(Token::Eq, 20, 21), tok(Token::Numeric(NumericLiteral::Decimal(1)), 21, 22),
        tok(Token::Newline, 22, 23),
        tok(Token::KwWhen, 24, 28), tok(Token::Ident, 29, 30), tok(Token::FatArrow, 30, 32),
        tok(Token::Ident, 32, 33), tok(Token::Eq, 33, 34), tok(Token::Numeric(NumericLiteral::Decimal(2)), 34, 35),
        tok(Token::Newline, 35, 36),
        tok(Token::Dedent, 36, 36),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::ParallelBlock(pb) => {
            assert_eq!(pb.arms.len(), 2);
            assert!(pb.safe.is_none());
        }
        _ => panic!("expected ParallelBlock"),
    }
}

#[test]
fn block_assert_always() {
    let s = "assert always @ ck: x > 0, \"msg\"";
    let t = vec![
        tok(Token::KwAssert, 0, 6), tok(Token::KwAlways, 7, 13),
        tok(Token::At, 14, 15), tok(Token::Ident, 16, 18),
        tok(Token::Colon, 18, 19), tok(Token::Ident, 20, 21),
        tok(Token::Greater, 22, 23), tok(Token::Numeric(NumericLiteral::Decimal(0)), 24, 25),
        tok(Token::Comma, 25, 26), tok(Token::StringLit("msg".into()), 27, 32),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::Assert(a) => {
            assert!(a.always);
            assert!(a.domain.is_some());
            assert!(a.message.is_some());
        }
        _ => panic!("expected Assert"),
    }
}

#[test]
fn block_for_loop() {
    let s = "for i in 0..8:\n  x=i\n";
    let t = vec![
        tok(Token::KwFor, 0, 3), tok(Token::Ident, 4, 5), tok(Token::KwIn, 6, 8),
        tok(Token::Numeric(NumericLiteral::Decimal(0)), 9, 10),
        tok(Token::RangeExclusive, 10, 12),
        tok(Token::Numeric(NumericLiteral::Decimal(8)), 12, 13),
        tok(Token::Colon, 13, 14), tok(Token::Newline, 14, 15), tok(Token::Indent, 15, 15),
        tok(Token::Ident, 17, 18), tok(Token::Eq, 18, 19), tok(Token::Ident, 19, 20),
        tok(Token::Newline, 20, 21), tok(Token::Dedent, 21, 21),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::For(f) => {
            assert_eq!(f.var.node, "i");
            assert_eq!(f.body.len(), 1);
        }
        _ => panic!("expected For"),
    }
}

#[test]
fn block_assume_with_domain() {
    let s = "assume @ clk: ready";
    let t = vec![
        tok(Token::KwAssume, 0, 6), tok(Token::At, 7, 8), tok(Token::Ident, 9, 12),
        tok(Token::Colon, 12, 13), tok(Token::Ident, 14, 19),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::Assume { domain, message, .. } => {
            assert!(domain.is_some());
            assert!(message.is_none());
        }
        _ => panic!("expected Assume"),
    }
}
