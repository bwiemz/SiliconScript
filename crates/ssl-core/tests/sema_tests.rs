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

// ── Resolver Tests ────────────────────────────────────────────────────────────

use ssl_core::sema::resolve::Resolver;

/// Parse source code and run name + type resolution.
fn resolve_source(src: &str) -> (ssl_core::sema::scope::SymbolTable, Vec<SemaError>) {
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse failed");
    let mut resolver = Resolver::new();
    resolver.collect_declarations(&file);
    let (table, _scope_map, errors) = resolver.finish();
    (table, errors)
}

#[test]
fn resolve_module_declared() {
    let (table, errors) = resolve_source("module Foo():\n    signal x: Bool\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    let sym = table.lookup(table.root_scope(), "Foo");
    assert!(sym.is_some(), "module Foo should be in scope");
}

#[test]
fn resolve_signal_in_module() {
    let (table, errors) = resolve_source("module Foo():\n    signal x: UInt<8>\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(table.lookup(table.root_scope(), "x").is_none(), "x should NOT be in file scope");
}

#[test]
fn resolve_duplicate_module_error() {
    let (_table, errors) = resolve_source("module Foo():\n    signal x: Bool\nmodule Foo():\n    signal y: Bool\n");
    assert!(!errors.is_empty(), "should report duplicate Foo");
}

#[test]
fn resolve_const_declaration() {
    let (_table, errors) = resolve_source("module M():\n    const WIDTH: uint = 8\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn resolve_struct_declared() {
    let (table, errors) = resolve_source("struct Pixel:\n    r: UInt<8>\n    g: UInt<8>\n    b: UInt<8>\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(table.lookup(table.root_scope(), "Pixel").is_some());
}

#[test]
fn resolve_enum_declared() {
    let (table, errors) = resolve_source("enum State [onehot]:\n    Idle\n    Run\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(table.lookup(table.root_scope(), "State").is_some());
}

#[test]
fn resolve_type_uint8() {
    let (_table, errors) = resolve_source("module M():\n    signal x: UInt<8>\n");
    assert!(errors.is_empty(), "UInt<8> should resolve: {errors:?}");
}

#[test]
fn resolve_type_bool() {
    let (_table, errors) = resolve_source("module M():\n    signal flag: Bool\n");
    assert!(errors.is_empty(), "Bool should resolve: {errors:?}");
}

#[test]
fn resolve_type_array() {
    let (_table, errors) = resolve_source("module M():\n    signal mem: UInt<8>[4]\n");
    assert!(errors.is_empty(), "array should resolve: {errors:?}");
}

#[test]
fn resolve_type_port_in_uint() {
    let (_table, errors) = resolve_source("module M(\n    in a: UInt<32>,\n    out b: UInt<32>\n):\n    signal x: Bool\n");
    assert!(errors.is_empty(), "port types should resolve: {errors:?}");
}

#[test]
fn resolve_type_clock() {
    let (_table, errors) = resolve_source("module M(\n    in clk: Clock\n):\n    signal x: Bool\n");
    assert!(errors.is_empty(), "Clock should resolve: {errors:?}");
}

#[test]
fn resolve_type_undefined_name() {
    let (_table, errors) = resolve_source("module M():\n    signal x: Nonexistent\n");
    assert!(!errors.is_empty(), "should error on undefined type name");
}

#[test]
fn resolve_type_const_width() {
    let (_table, errors) = resolve_source("module M():\n    const W: uint = 16\n    signal data: UInt<W>\n");
    assert!(errors.is_empty(), "const width should resolve: {errors:?}");
}

#[test]
fn resolve_type_fixed_point() {
    let (_table, errors) = resolve_source("module M():\n    signal weight: Fixed<8, 8>\n");
    assert!(errors.is_empty(), "Fixed<8,8> should resolve: {errors:?}");
}

// ── Type checker / analyze() tests ──────────────────────────────────────────

/// Helper: check a module and return errors (runs full analysis pipeline).
fn check_source(src: &str) -> Vec<SemaError> {
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse failed");
    ssl_core::sema::analyze(&file).1
}

// ── Expression type checking ──

#[test]
fn check_add_same_width() {
    let errors = check_source("module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<8>\n    comb:\n        c = a + b\n");
    assert!(errors.is_empty(), "same-width add: {errors:?}");
}

#[test]
fn check_add_different_width_error() {
    let errors = check_source("module M():\n    signal a: UInt<8>\n    signal b: UInt<16>\n    signal c: UInt<8>\n    comb:\n        c = a + b\n");
    assert!(!errors.is_empty(), "UInt<16> result into UInt<8> should error");
}

#[test]
fn check_add_different_width_ok() {
    let errors = check_source("module M():\n    signal a: UInt<8>\n    signal b: UInt<16>\n    signal c: UInt<16>\n    comb:\n        c = a + b\n");
    assert!(errors.is_empty(), "UInt<16> result into UInt<16>: {errors:?}");
}

#[test]
fn check_bool_and_bool() {
    let errors = check_source("module M():\n    signal a: Bool\n    signal b: Bool\n    signal c: Bool\n    comb:\n        c = a and b\n");
    assert!(errors.is_empty(), "Bool and Bool: {errors:?}");
}

#[test]
fn check_comparison_returns_bool() {
    let errors = check_source("module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: Bool\n    comb:\n        c = a == b\n");
    assert!(errors.is_empty(), "comparison returns Bool: {errors:?}");
}

#[test]
fn check_integer_literal_fits() {
    let errors = check_source("module M():\n    signal x: UInt<8>\n    comb:\n        x = 255\n");
    assert!(errors.is_empty(), "255 fits UInt<8>: {errors:?}");
}

#[test]
fn check_bitwise_same_width() {
    let errors = check_source("module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<8>\n    comb:\n        c = a & b\n");
    assert!(errors.is_empty(), "bitwise AND: {errors:?}");
}

#[test]
fn check_concat_width() {
    let errors = check_source("module M():\n    signal a: Bits<8>\n    signal b: Bits<8>\n    signal c: Bits<16>\n    comb:\n        c = a ++ b\n");
    assert!(errors.is_empty(), "concat Bits<8>++Bits<8>=Bits<16>: {errors:?}");
}

#[test]
fn check_mul_width_widening() {
    let errors = check_source("module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<16>\n    comb:\n        c = a * b\n");
    assert!(errors.is_empty(), "UInt<8>*UInt<8>=UInt<16>: {errors:?}");
}

#[test]
fn check_mul_width_too_narrow() {
    let errors = check_source("module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<8>\n    comb:\n        c = a * b\n");
    assert!(!errors.is_empty(), "UInt<16> into UInt<8> should error");
}

// ── Statement type checking ──

#[test]
fn check_if_condition_must_be_bool() {
    let errors = check_source("module M():\n    signal x: UInt<8>\n    signal y: UInt<8>\n    comb:\n        y = 0\n        if x:\n            y = 1\n");
    assert!(!errors.is_empty(), "if condition must be Bool, not UInt<8>");
}

#[test]
fn check_reg_block_clock_type() {
    let errors = check_source("module M(\n    in clk: Clock,\n    in rst: SyncReset\n):\n    signal counter: UInt<8>\n    reg(clk, rst):\n        on reset:\n            counter = 0\n        on tick:\n            counter = counter + 1\n");
    assert!(errors.is_empty(), "valid reg block: {errors:?}");
}

#[test]
fn check_reg_block_non_clock_error() {
    let errors = check_source("module M():\n    signal x: UInt<8>\n    signal y: UInt<8>\n    reg(x, y):\n        on reset:\n            y = 0\n        on tick:\n            y = y + 1\n");
    assert!(!errors.is_empty(), "reg first arg must be Clock");
}

#[test]
fn check_assign_to_input_port() {
    let errors = check_source("module M(\n    in a: UInt<8>\n):\n    comb:\n        a = 42\n");
    assert!(!errors.is_empty(), "cannot assign to input port");
}

// ── Item type checking ──

#[test]
fn check_module_ports_valid() {
    let errors = check_source("module ALU(\n    in a: UInt<32>,\n    in b: UInt<32>,\n    out result: UInt<32>\n):\n    comb:\n        result = a + b\n");
    assert!(errors.is_empty(), "valid ALU: {errors:?}");
}

#[test]
fn check_struct_fields_valid() {
    let errors = check_source("struct Pixel:\n    r: UInt<8>\n    g: UInt<8>\n    b: UInt<8>\n");
    assert!(errors.is_empty(), "valid struct: {errors:?}");
}

// ── Orchestration ──

#[test]
fn analyze_full_blinker() {
    let src = "module Blinker(\n    in  clk: Clock,\n    in  rst: SyncReset,\n    out led: Bool\n):\n    signal counter: UInt<24>\n\n    reg(clk, rst):\n        on reset:\n            counter = 0\n        on tick:\n            counter = counter + 1\n\n    comb:\n        led = counter[23]\n";
    let errors = check_source(src);
    assert!(errors.is_empty(), "blinker should pass: {errors:?}");
}

#[test]
fn analyze_multiple_errors_reported() {
    let errors = check_source("module M():\n    signal x: Undefined\n    signal y: AlsoUndefined\n");
    assert!(errors.len() >= 2, "should report multiple errors: {errors:?}");
}

#[test]
fn analyze_error_recovery() {
    let errors = check_source("module M():\n    signal bad: Undefined\n    signal good: UInt<8>\n    comb:\n        good = 42\n");
    let undefined_errors: Vec<_> = errors.iter().filter(|e| matches!(e, SemaError::UndefinedName { .. })).collect();
    assert_eq!(undefined_errors.len(), 1, "one undefined error: {errors:?}");
}

// ── Validation tests ──

#[test]
fn validate_comb_complete_assignment() {
    let errors = check_source("module M():\n    signal x: Bool\n    signal y: UInt<8>\n    comb:\n        if x:\n            y = 1\n        else:\n            y = 0\n");
    assert!(errors.is_empty(), "y assigned on all paths: {errors:?}");
}

#[test]
fn validate_comb_incomplete_assignment() {
    let errors = check_source("module M():\n    signal x: Bool\n    signal y: UInt<8>\n    comb:\n        if x:\n            y = 1\n");
    let latch_errors: Vec<_> = errors.iter().filter(|e| matches!(e, SemaError::LatchInferred { .. })).collect();
    assert!(!latch_errors.is_empty(), "should detect latch on y");
}

#[test]
fn validate_comb_default_then_override() {
    let errors = check_source("module M():\n    signal x: Bool\n    signal y: UInt<8>\n    comb:\n        y = 0\n        if x:\n            y = 1\n");
    assert!(errors.is_empty(), "default + override is complete: {errors:?}");
}

#[test]
fn validate_comb_match_no_wildcard() {
    let errors = check_source("module M():\n    signal sel: UInt<2>\n    signal y: UInt<8>\n    comb:\n        match sel:\n            0 => y = 1\n            1 => y = 2\n");
    assert!(!errors.is_empty(), "match without wildcard should error");
}

#[test]
fn validate_reg_reset_coverage() {
    let errors = check_source("module M(\n    in clk: Clock,\n    in rst: SyncReset\n):\n    signal x: UInt<8>\n    signal y: UInt<8>\n    reg(clk, rst):\n        on reset:\n            x = 0\n        on tick:\n            x = x + 1\n            y = x\n");
    assert!(!errors.is_empty(), "y in on_tick but not on_reset");
}

#[test]
fn validate_reg_reset_complete() {
    let errors = check_source("module M(\n    in clk: Clock,\n    in rst: SyncReset\n):\n    signal x: UInt<8>\n    reg(clk, rst):\n        on reset:\n            x = 0\n        on tick:\n            x = x + 1\n");
    assert!(errors.is_empty(), "complete reg block: {errors:?}");
}

#[test]
fn validate_const_not_reassigned() {
    let errors = check_source("module M():\n    const X: uint = 8\n    comb:\n        X = 16\n");
    assert!(!errors.is_empty(), "cannot assign to const");
}

#[test]
fn validate_output_port_driven() {
    let errors = check_source("module M(\n    out y: UInt<8>\n):\n    signal x: Bool\n");
    assert!(!errors.is_empty(), "output port y is never driven");
}
