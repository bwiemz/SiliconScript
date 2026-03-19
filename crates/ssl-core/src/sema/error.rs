use std::fmt;

use crate::span::Span;

/// Errors produced during semantic analysis of a SiliconScript module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemaError {
    /// A name was used but never declared.
    UndefinedName { name: String, span: Span },

    /// A name was declared more than once in the same scope.
    /// `first` is the original declaration site; `second` is the conflicting one.
    DuplicateDefinition {
        name: String,
        first: Span,
        second: Span,
    },

    /// The type of an expression does not match what the context requires.
    TypeMismatch {
        expected: String,
        found: String,
        span: Span,
    },

    /// The bit-width of a signal does not match what the context requires.
    WidthMismatch {
        expected: u64,
        found: u64,
        span: Span,
    },

    /// The left-hand side of an assignment is not a valid lvalue.
    InvalidAssignTarget { span: Span },

    /// A combinational signal is not assigned in all branches — a latch would be inferred.
    LatchInferred { signal: String, span: Span },

    /// A `match` expression does not cover all possible values.
    NonExhaustiveMatch { span: Span },

    /// A port on a module instantiation was left unconnected.
    UnconnectedPort {
        port: String,
        inst: String,
        span: Span,
    },

    /// A language construct was used in a context where it is not permitted.
    InvalidContext {
        construct: String,
        context: String,
        span: Span,
    },

    /// A constant expression could not be evaluated at compile time.
    ConstEvalError { message: String, span: Span },

    /// A cyclic dependency was detected among the listed names.
    CyclicDependency { names: Vec<String>, span: Span },

    /// A signal was driven in a direction that violates port or interface rules.
    DirectionViolation { message: String, span: Span },

    /// A catch-all for errors that do not fit the other categories.
    Custom { message: String, span: Span },
}

impl SemaError {
    /// Return the source span associated with this error.
    ///
    /// For `DuplicateDefinition`, this is the *second* (conflicting) site.
    pub fn span(&self) -> Span {
        match self {
            Self::UndefinedName { span, .. } => *span,
            Self::DuplicateDefinition { second, .. } => *second,
            Self::TypeMismatch { span, .. } => *span,
            Self::WidthMismatch { span, .. } => *span,
            Self::InvalidAssignTarget { span } => *span,
            Self::LatchInferred { span, .. } => *span,
            Self::NonExhaustiveMatch { span } => *span,
            Self::UnconnectedPort { span, .. } => *span,
            Self::InvalidContext { span, .. } => *span,
            Self::ConstEvalError { span, .. } => *span,
            Self::CyclicDependency { span, .. } => *span,
            Self::DirectionViolation { span, .. } => *span,
            Self::Custom { span, .. } => *span,
        }
    }
}

impl fmt::Display for SemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndefinedName { name, span } => {
                write!(
                    f,
                    "undefined name `{name}` at {start}..{end}",
                    start = span.start,
                    end = span.end
                )
            }
            Self::DuplicateDefinition {
                name,
                first,
                second,
            } => {
                write!(
                    f,
                    "duplicate definition of `{name}`: first defined at {fs}..{fe}, \
                     redefined at {ss}..{se}",
                    fs = first.start,
                    fe = first.end,
                    ss = second.start,
                    se = second.end
                )
            }
            Self::TypeMismatch {
                expected,
                found,
                span,
            } => {
                write!(
                    f,
                    "type mismatch at {start}..{end}: expected `{expected}`, found `{found}`",
                    start = span.start,
                    end = span.end
                )
            }
            Self::WidthMismatch {
                expected,
                found,
                span,
            } => {
                write!(
                    f,
                    "width mismatch at {start}..{end}: expected {expected} bits, found {found} bits",
                    start = span.start,
                    end = span.end
                )
            }
            Self::InvalidAssignTarget { span } => {
                write!(
                    f,
                    "invalid assignment target at {start}..{end}",
                    start = span.start,
                    end = span.end
                )
            }
            Self::LatchInferred { signal, span } => {
                write!(
                    f,
                    "latch inferred for signal `{signal}` at {start}..{end}: \
                     signal is not assigned in all branches",
                    start = span.start,
                    end = span.end
                )
            }
            Self::NonExhaustiveMatch { span } => {
                write!(
                    f,
                    "non-exhaustive match at {start}..{end}: not all values are covered",
                    start = span.start,
                    end = span.end
                )
            }
            Self::UnconnectedPort { port, inst, span } => {
                write!(
                    f,
                    "port `{port}` of instance `{inst}` is unconnected at {start}..{end}",
                    start = span.start,
                    end = span.end
                )
            }
            Self::InvalidContext {
                construct,
                context,
                span,
            } => {
                write!(
                    f,
                    "`{construct}` is not valid in a `{context}` context at {start}..{end}",
                    start = span.start,
                    end = span.end
                )
            }
            Self::ConstEvalError { message, span } => {
                write!(
                    f,
                    "constant evaluation error at {start}..{end}: {message}",
                    start = span.start,
                    end = span.end
                )
            }
            Self::CyclicDependency { names, span } => {
                let cycle = names.join(" -> ");
                write!(
                    f,
                    "cyclic dependency at {start}..{end}: {cycle}",
                    start = span.start,
                    end = span.end
                )
            }
            Self::DirectionViolation { message, span } => {
                write!(
                    f,
                    "direction violation at {start}..{end}: {message}",
                    start = span.start,
                    end = span.end
                )
            }
            Self::Custom { message, span } => {
                write!(
                    f,
                    "semantic error at {start}..{end}: {message}",
                    start = span.start,
                    end = span.end
                )
            }
        }
    }
}

impl std::error::Error for SemaError {}
