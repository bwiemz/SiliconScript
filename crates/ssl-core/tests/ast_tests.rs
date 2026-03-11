use ssl_core::ast::expr::{BinOp, CallArg, ExprKind};
use ssl_core::ast::types::{TypeExprKind, Direction, ClockEdge, GenericArg};
use ssl_core::ast::{Ident, Attribute};
use ssl_core::lexer::NumericLiteral;
use ssl_core::span::{Span, Spanned};

fn s(start: u32, end: u32) -> Span { Span::new(start, end) }
fn ident(name: &str) -> Ident { Spanned::new(name.to_string(), s(0, 0)) }

#[test]
fn construct_int_literal() {
    let expr = Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(42)),
        s(0, 2),
    );
    assert!(matches!(expr.node, ExprKind::IntLiteral(_)));
}

#[test]
fn construct_binary_expr() {
    let lhs = Box::new(Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(1)),
        s(0, 1),
    ));
    let rhs = Box::new(Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(2)),
        s(4, 5),
    ));
    let expr = Spanned::new(
        ExprKind::Binary { op: BinOp::Add, lhs, rhs },
        s(0, 5),
    );
    match &expr.node {
        ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Add),
        _ => panic!("expected Binary"),
    }
}

#[test]
fn construct_call_expr() {
    let callee = Box::new(Spanned::new(ExprKind::Ident("foo".into()), s(0, 3)));
    let arg = CallArg {
        name: Some(ident("x")),
        value: Spanned::new(ExprKind::IntLiteral(NumericLiteral::Decimal(5)), s(6, 7)),
    };
    let expr = Spanned::new(
        ExprKind::Call { callee, args: vec![arg] },
        s(0, 8),
    );
    match &expr.node {
        ExprKind::Call { args, .. } => assert_eq!(args.len(), 1),
        _ => panic!("expected Call"),
    }
}

#[test]
fn construct_named_type() {
    let ty = Spanned::new(TypeExprKind::Named("UInt".into()), s(0, 4));
    assert!(matches!(ty.node, TypeExprKind::Named(_)));
}

#[test]
fn construct_generic_type() {
    let width = Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(8)),
        s(5, 6),
    );
    let ty = Spanned::new(
        TypeExprKind::Generic {
            name: "UInt".into(),
            params: vec![GenericArg::Expr(width)],
        },
        s(0, 7),
    );
    match &ty.node {
        TypeExprKind::Generic { name, params } => {
            assert_eq!(name, "UInt");
            assert_eq!(params.len(), 1);
        }
        _ => panic!("expected Generic"),
    }
}

#[test]
fn construct_clock_type() {
    let ty = Spanned::new(
        TypeExprKind::Clock { freq: None, edge: Some(ClockEdge::Rising) },
        s(0, 5),
    );
    match &ty.node {
        TypeExprKind::Clock { edge, .. } => assert_eq!(*edge, Some(ClockEdge::Rising)),
        _ => panic!("expected Clock"),
    }
}

#[test]
fn construct_direction_wrapper() {
    let inner = Box::new(Spanned::new(TypeExprKind::Named("UInt".into()), s(3, 7)));
    let ty = Spanned::new(
        TypeExprKind::DirectionWrapper { dir: Direction::In, inner },
        s(0, 7),
    );
    assert!(matches!(ty.node, TypeExprKind::DirectionWrapper { dir: Direction::In, .. }));
}

#[test]
fn construct_attribute() {
    let attr = Attribute {
        name: ident("clock"),
        args: vec![],
        span: s(0, 6),
    };
    assert_eq!(attr.name.node, "clock");
}
