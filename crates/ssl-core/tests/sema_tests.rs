use ssl_core::sema::SemaError;
use ssl_core::sema::types::Ty;
use ssl_core::sema::scope::{SymbolTable, SymbolKind};
use ssl_core::sema::eval::{ConstValue, ConstEval};
use ssl_core::ast::expr::ExprKind;
use ssl_core::lexer::NumericLiteral;
use ssl_core::span::Span;

#[test]
fn sema_error_display() {
    let err = SemaError::UndefinedName {
        name: "foo".into(),
        span: Span::new(10, 13),
    };
    let msg = format!("{err}");
    assert!(msg.contains("foo"));
    assert!(msg.contains("undefined"));
}

#[test]
fn sema_error_type_mismatch() {
    let err = SemaError::TypeMismatch {
        expected: "UInt<8>".into(),
        found: "UInt<16>".into(),
        span: Span::new(20, 30),
    };
    let msg = format!("{err}");
    assert!(msg.contains("UInt<8>"));
    assert!(msg.contains("UInt<16>"));
}

#[test]
fn sema_error_span() {
    let span = Span::new(5, 10);
    let err = SemaError::InvalidAssignTarget { span };
    assert_eq!(err.span(), span);
}

#[test]
fn sema_error_duplicate_definition() {
    let err = SemaError::DuplicateDefinition {
        name: "clk".into(),
        first: Span::new(0, 3),
        second: Span::new(10, 13),
    };
    let msg = format!("{err}");
    assert!(msg.contains("clk"));
    // span() should return the second span (site of conflict)
    assert_eq!(err.span(), Span::new(10, 13));
}

#[test]
fn sema_error_width_mismatch() {
    let err = SemaError::WidthMismatch {
        expected: 8,
        found: 16,
        span: Span::new(0, 5),
    };
    let msg = format!("{err}");
    assert!(msg.contains('8'));
    assert!(msg.contains("16"));
}

#[test]
fn sema_error_latch_inferred() {
    let err = SemaError::LatchInferred {
        signal: "data_out".into(),
        span: Span::new(100, 108),
    };
    let msg = format!("{err}");
    assert!(msg.contains("data_out"));
    assert!(msg.contains("latch"));
}

#[test]
fn sema_error_cyclic_dependency() {
    let err = SemaError::CyclicDependency {
        names: vec!["a".into(), "b".into(), "c".into()],
        span: Span::new(0, 1),
    };
    let msg = format!("{err}");
    assert!(msg.contains('a'));
    assert!(msg.contains('b'));
    assert!(msg.contains('c'));
}

#[test]
fn sema_error_is_std_error() {
    let err = SemaError::Custom {
        message: "something went wrong".into(),
        span: Span::new(0, 5),
    };
    // Must implement std::error::Error
    let _: &dyn std::error::Error = &err;
    let msg = format!("{err}");
    assert!(msg.contains("something went wrong"));
}

#[test]
fn ty_uint_width() {
    assert_eq!(Ty::UInt(8).bit_width(), Some(8));
}

#[test]
fn ty_bool_width() {
    assert_eq!(Ty::Bool.bit_width(), Some(1));
}

#[test]
fn ty_sint_width() {
    assert_eq!(Ty::SInt(16).bit_width(), Some(16));
}

#[test]
fn ty_bits_width() {
    assert_eq!(Ty::Bits(32).bit_width(), Some(32));
}

#[test]
fn ty_array_width() {
    let t = Ty::Array { element: Box::new(Ty::UInt(8)), size: 4 };
    assert_eq!(t.bit_width(), Some(32));
}

#[test]
fn ty_clock_width() {
    assert_eq!(Ty::Clock { freq: None }.bit_width(), Some(1));
}

#[test]
fn ty_display() {
    assert_eq!(Ty::UInt(8).to_string(), "UInt<8>");
    assert_eq!(Ty::SInt(16).to_string(), "SInt<16>");
    assert_eq!(Ty::Bool.to_string(), "Bool");
    assert_eq!(Ty::Bits(32).to_string(), "Bits<32>");
}

#[test]
fn ty_is_numeric() {
    assert!(Ty::UInt(8).is_numeric());
    assert!(Ty::SInt(16).is_numeric());
    assert!(Ty::Bits(32).is_numeric());
    assert!(!Ty::Bool.is_numeric());
    assert!(!Ty::Clock { freq: None }.is_numeric());
}

#[test]
fn ty_is_integer() {
    assert!(Ty::UInt(8).is_integer());
    assert!(Ty::SInt(16).is_integer());
    assert!(!Ty::Bits(32).is_integer());
    assert!(!Ty::Bool.is_integer());
}

#[test]
fn ty_fixed_width() {
    let t = Ty::Fixed { int_bits: 8, frac_bits: 8 };
    assert_eq!(t.bit_width(), Some(16));
    assert_eq!(t.to_string(), "Fixed<8, 8>");
}

#[test]
fn ty_meta_types_no_width() {
    assert_eq!(Ty::MetaUInt.bit_width(), None);
    assert_eq!(Ty::MetaInt.bit_width(), None);
    assert_eq!(Ty::MetaBool.bit_width(), None);
}

#[test]
fn ty_is_synthesizable() {
    assert!(Ty::UInt(8).is_synthesizable());
    assert!(Ty::Bool.is_synthesizable());
    assert!(!Ty::MetaUInt.is_synthesizable());
    assert!(!Ty::MetaString.is_synthesizable());
    assert!(!Ty::Error.is_synthesizable());
}

#[test]
fn scope_define_and_lookup() {
    let mut table = SymbolTable::new();
    let file_scope = table.root_scope();
    table.define(file_scope, "counter", SymbolKind::Signal, Ty::UInt(8),
        Span::new(0, 7)).unwrap();
    let sym = table.lookup(file_scope, "counter");
    assert!(sym.is_some());
    let sym = sym.unwrap();
    assert_eq!(sym.name, "counter");
    assert_eq!(sym.ty, Ty::UInt(8));
}

#[test]
fn scope_child_sees_parent() {
    let mut table = SymbolTable::new();
    let file_scope = table.root_scope();
    table.define(file_scope, "top_signal", SymbolKind::Signal, Ty::Bool,
        Span::new(0, 10)).unwrap();
    let child = table.push_scope(file_scope, ssl_core::sema::scope::ScopeKind::Module);
    let sym = table.lookup(child, "top_signal");
    assert!(sym.is_some());
}

#[test]
fn scope_child_shadows_parent() {
    let mut table = SymbolTable::new();
    let file_scope = table.root_scope();
    table.define(file_scope, "x", SymbolKind::Signal, Ty::UInt(8), Span::new(0, 1)).unwrap();
    let child = table.push_scope(file_scope, ssl_core::sema::scope::ScopeKind::Block);
    table.define(child, "x", SymbolKind::Signal, Ty::UInt(16), Span::new(10, 11)).unwrap();
    let sym = table.lookup(child, "x").unwrap();
    assert_eq!(sym.ty, Ty::UInt(16));
    let sym = table.lookup(file_scope, "x").unwrap();
    assert_eq!(sym.ty, Ty::UInt(8));
}

#[test]
fn scope_undefined_returns_none() {
    let table = SymbolTable::new();
    let root = table.root_scope();
    assert!(table.lookup(root, "nonexistent").is_none());
}

#[test]
fn scope_duplicate_in_same_scope() {
    let mut table = SymbolTable::new();
    let root = table.root_scope();
    let r1 = table.define(root, "x", SymbolKind::Signal, Ty::UInt(8), Span::new(0, 1));
    assert!(r1.is_ok());
    let r2 = table.define(root, "x", SymbolKind::Signal, Ty::UInt(16), Span::new(10, 11));
    assert!(r2.is_err());
}

#[test]
fn scope_list_local_symbols() {
    let mut table = SymbolTable::new();
    let root = table.root_scope();
    table.define(root, "a", SymbolKind::Signal, Ty::Bool, Span::new(0, 1)).unwrap();
    table.define(root, "b", SymbolKind::Const, Ty::MetaUInt, Span::new(2, 3)).unwrap();
    let locals = table.local_symbols(root);
    assert_eq!(locals.len(), 2);
}

// ── Const Evaluator Tests ────────────────────────────────────────────────────

use ssl_core::span::Spanned;

fn make_int_expr(val: u128) -> ssl_core::ast::expr::Expr {
    Spanned::new(ExprKind::IntLiteral(NumericLiteral::Decimal(val)), Span::new(0, 1))
}

#[test]
fn eval_integer_literal() {
    let evaluator = ConstEval::new();
    let expr = make_int_expr(42);
    let result = evaluator.eval_expr(&expr);
    assert_eq!(result, Ok(ConstValue::UInt(42)));
}

#[test]
fn eval_bool_literal() {
    let evaluator = ConstEval::new();
    let expr = Spanned::new(ExprKind::BoolLiteral(true), Span::new(0, 4));
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::Bool(true)));
}

#[test]
fn eval_binary_add() {
    let evaluator = ConstEval::new();
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Add,
            lhs: Box::new(make_int_expr(10)),
            rhs: Box::new(make_int_expr(32)),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(42)));
}

#[test]
fn eval_binary_mul() {
    let evaluator = ConstEval::new();
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Mul,
            lhs: Box::new(make_int_expr(6)),
            rhs: Box::new(make_int_expr(7)),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(42)));
}

#[test]
fn eval_binary_pow() {
    let evaluator = ConstEval::new();
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Pow,
            lhs: Box::new(make_int_expr(2)),
            rhs: Box::new(make_int_expr(10)),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(1024)));
}

#[test]
fn eval_comparison() {
    let evaluator = ConstEval::new();
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Lt,
            lhs: Box::new(make_int_expr(5)),
            rhs: Box::new(make_int_expr(10)),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::Bool(true)));
}

#[test]
fn eval_if_expr() {
    let evaluator = ConstEval::new();
    let expr = Spanned::new(
        ExprKind::IfExpr {
            condition: Box::new(Spanned::new(ExprKind::BoolLiteral(true), Span::new(0, 4))),
            then_expr: Box::new(make_int_expr(10)),
            else_expr: Box::new(make_int_expr(20)),
        },
        Span::new(0, 10),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(10)));
}
