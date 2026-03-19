use ssl_core::sema::SemaError;
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
