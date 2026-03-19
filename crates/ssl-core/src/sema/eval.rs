use std::collections::HashMap;

use crate::ast::expr::{BinOp, Expr, ExprKind, UnaryOp};
use crate::lexer::NumericLiteral;
use crate::span::Span;
use super::SemaError;

/// A compile-time constant value produced by the const evaluator.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    UInt(u128),
    Int(i128),
    Bool(bool),
    Float(f64),
    String(String),
}

/// Evaluates AST expressions to compile-time constants.
///
/// Used for resolving generic parameters like `UInt<8>`, array sizes,
/// and `static_assert` conditions.
pub struct ConstEval {
    bindings: HashMap<String, ConstValue>,
}

impl ConstEval {
    /// Create a new evaluator with no pre-bound names.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Create a new evaluator with the given pre-bound constant names.
    pub fn with_bindings(bindings: HashMap<String, ConstValue>) -> Self {
        Self { bindings }
    }

    /// Bind a name to a compile-time constant value.
    pub fn bind(&mut self, name: String, value: ConstValue) {
        self.bindings.insert(name, value);
    }

    /// Evaluate an expression to a compile-time constant.
    ///
    /// Returns a `SemaError::ConstEvalError` if the expression cannot be
    /// reduced at compile time (e.g., it references a runtime variable).
    pub fn eval_expr(&self, expr: &Expr) -> Result<ConstValue, SemaError> {
        let span = expr.span;
        match &expr.node {
            ExprKind::IntLiteral(lit) => Ok(self.eval_numeric_literal(lit)),

            ExprKind::BoolLiteral(b) => Ok(ConstValue::Bool(*b)),

            ExprKind::StringLiteral(s) => Ok(ConstValue::String(s.clone())),

            ExprKind::Ident(name) => {
                self.bindings.get(name).cloned().ok_or_else(|| {
                    SemaError::ConstEvalError {
                        message: format!(
                            "name `{name}` is not a compile-time constant"
                        ),
                        span,
                    }
                })
            }

            ExprKind::Paren(inner) => self.eval_expr(inner),

            ExprKind::Binary { op, lhs, rhs } => {
                self.eval_binary(span, *op, lhs, rhs)
            }

            ExprKind::Unary { op, operand } => {
                self.eval_unary(span, *op, operand)
            }

            ExprKind::IfExpr {
                condition,
                then_expr,
                else_expr,
            } => {
                let cond = self.eval_expr(condition)?;
                match cond {
                    ConstValue::Bool(true) => self.eval_expr(then_expr),
                    ConstValue::Bool(false) => self.eval_expr(else_expr),
                    _ => Err(SemaError::ConstEvalError {
                        message: "if-expression condition must be a boolean".into(),
                        span: condition.span,
                    }),
                }
            }

            _ => Err(SemaError::ConstEvalError {
                message: "expression not supported in const context".into(),
                span,
            }),
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn eval_numeric_literal(&self, lit: &NumericLiteral) -> ConstValue {
        match lit {
            NumericLiteral::Decimal(v) => ConstValue::UInt(*v),
            NumericLiteral::Hex(v) => ConstValue::UInt(*v),
            NumericLiteral::Binary(v) => ConstValue::UInt(*v),
            NumericLiteral::Sized { value, .. } => ConstValue::UInt(*value),
        }
    }

    fn eval_binary(
        &self,
        span: Span,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Result<ConstValue, SemaError> {
        let lval = self.eval_expr(lhs)?;
        let rval = self.eval_expr(rhs)?;

        match op {
            // Arithmetic — both operands must be UInt
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let (l, r) = self.require_uint2(span, op, &lval, &rval)?;
                let result = match op {
                    BinOp::Add => l.wrapping_add(r),
                    BinOp::Sub => l.wrapping_sub(r),
                    BinOp::Mul => l.wrapping_mul(r),
                    BinOp::Div => {
                        if r == 0 {
                            return Err(SemaError::ConstEvalError {
                                message: "division by zero in const expression".into(),
                                span,
                            });
                        }
                        l / r
                    }
                    BinOp::Mod => {
                        if r == 0 {
                            return Err(SemaError::ConstEvalError {
                                message: "modulo by zero in const expression".into(),
                                span,
                            });
                        }
                        l % r
                    }
                    _ => unreachable!(),
                };
                Ok(ConstValue::UInt(result))
            }

            // Exponentiation — both operands must be UInt
            BinOp::Pow => {
                let (base, exp) = self.require_uint2(span, op, &lval, &rval)?;
                let exp32 = u32::try_from(exp).map_err(|_| SemaError::ConstEvalError {
                    message: format!(
                        "exponent {exp} is too large for const pow (max {})",
                        u32::MAX
                    ),
                    span,
                })?;
                let result = base.checked_pow(exp32).ok_or_else(|| {
                    SemaError::ConstEvalError {
                        message: "overflow in const pow expression".into(),
                        span,
                    }
                })?;
                Ok(ConstValue::UInt(result))
            }

            // Comparisons — both operands must be UInt, produce Bool
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let (l, r) = self.require_uint2(span, op, &lval, &rval)?;
                let b = match op {
                    BinOp::Eq => l == r,
                    BinOp::Ne => l != r,
                    BinOp::Lt => l < r,
                    BinOp::Gt => l > r,
                    BinOp::Le => l <= r,
                    BinOp::Ge => l >= r,
                    _ => unreachable!(),
                };
                Ok(ConstValue::Bool(b))
            }

            // Logical — both operands must be Bool, produce Bool
            BinOp::And | BinOp::Or => {
                let (l, r) = self.require_bool2(span, op, &lval, &rval)?;
                let b = match op {
                    BinOp::And => l && r,
                    BinOp::Or => l || r,
                    _ => unreachable!(),
                };
                Ok(ConstValue::Bool(b))
            }

            // Everything else is not supported in const context
            _ => Err(SemaError::ConstEvalError {
                message: format!(
                    "operator `{op:?}` is not supported in const context"
                ),
                span,
            }),
        }
    }

    fn eval_unary(
        &self,
        span: Span,
        op: UnaryOp,
        operand: &Expr,
    ) -> Result<ConstValue, SemaError> {
        let val = self.eval_expr(operand)?;
        match op {
            UnaryOp::Neg => match val {
                ConstValue::UInt(v) => Ok(ConstValue::Int(-(v as i128))),
                ConstValue::Int(v) => Ok(ConstValue::Int(-v)),
                _ => Err(SemaError::ConstEvalError {
                    message: "unary negation requires a numeric operand".into(),
                    span,
                }),
            },
            UnaryOp::LogicalNot => match val {
                ConstValue::Bool(b) => Ok(ConstValue::Bool(!b)),
                _ => Err(SemaError::ConstEvalError {
                    message: "logical not requires a boolean operand".into(),
                    span,
                }),
            },
            UnaryOp::BitNot => Err(SemaError::ConstEvalError {
                message: "bitwise not is not supported in const context".into(),
                span,
            }),
        }
    }

    /// Extract two `UInt` values or return a typed error.
    fn require_uint2(
        &self,
        span: Span,
        op: BinOp,
        lval: &ConstValue,
        rval: &ConstValue,
    ) -> Result<(u128, u128), SemaError> {
        match (lval, rval) {
            (ConstValue::UInt(l), ConstValue::UInt(r)) => Ok((*l, *r)),
            _ => Err(SemaError::ConstEvalError {
                message: format!(
                    "operator `{op:?}` requires integer operands in const context"
                ),
                span,
            }),
        }
    }

    /// Extract two `Bool` values or return a typed error.
    fn require_bool2(
        &self,
        span: Span,
        op: BinOp,
        lval: &ConstValue,
        rval: &ConstValue,
    ) -> Result<(bool, bool), SemaError> {
        match (lval, rval) {
            (ConstValue::Bool(l), ConstValue::Bool(r)) => Ok((*l, *r)),
            _ => Err(SemaError::ConstEvalError {
                message: format!(
                    "operator `{op:?}` requires boolean operands in const context"
                ),
                span,
            }),
        }
    }
}

impl Default for ConstEval {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;
    use crate::span::Spanned;

    fn uint_expr(v: u128) -> Expr {
        Spanned::new(
            ExprKind::IntLiteral(NumericLiteral::Decimal(v)),
            Span::new(0, 1),
        )
    }

    fn bool_expr(b: bool) -> Expr {
        Spanned::new(ExprKind::BoolLiteral(b), Span::new(0, 1))
    }

    #[test]
    fn literal_decimal() {
        let ev = ConstEval::new();
        assert_eq!(ev.eval_expr(&uint_expr(0)), Ok(ConstValue::UInt(0)));
        assert_eq!(ev.eval_expr(&uint_expr(255)), Ok(ConstValue::UInt(255)));
    }

    #[test]
    fn literal_hex() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::IntLiteral(NumericLiteral::Hex(0xFF)),
            Span::new(0, 4),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(255)));
    }

    #[test]
    fn literal_binary() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::IntLiteral(NumericLiteral::Binary(0b1010)),
            Span::new(0, 6),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(0b1010)));
    }

    #[test]
    fn literal_sized() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::IntLiteral(NumericLiteral::Sized {
                width: 8,
                value: 200,
                base: crate::lexer::NumericBase::Decimal,
                dont_care_mask: 0,
            }),
            Span::new(0, 5),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(200)));
    }

    #[test]
    fn paren_passthrough() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Paren(Box::new(uint_expr(7))),
            Span::new(0, 3),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(7)));
    }

    #[test]
    fn ident_bound() {
        let mut ev = ConstEval::new();
        ev.bind("N".into(), ConstValue::UInt(8));
        let expr = Spanned::new(ExprKind::Ident("N".into()), Span::new(0, 1));
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(8)));
    }

    #[test]
    fn ident_unbound_error() {
        let ev = ConstEval::new();
        let expr = Spanned::new(ExprKind::Ident("x".into()), Span::new(0, 1));
        assert!(ev.eval_expr(&expr).is_err());
    }

    #[test]
    fn binary_sub() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Binary {
                op: BinOp::Sub,
                lhs: Box::new(uint_expr(100)),
                rhs: Box::new(uint_expr(58)),
            },
            Span::new(0, 5),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(42)));
    }

    #[test]
    fn binary_div() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Binary {
                op: BinOp::Div,
                lhs: Box::new(uint_expr(84)),
                rhs: Box::new(uint_expr(2)),
            },
            Span::new(0, 5),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(42)));
    }

    #[test]
    fn binary_div_by_zero_error() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Binary {
                op: BinOp::Div,
                lhs: Box::new(uint_expr(1)),
                rhs: Box::new(uint_expr(0)),
            },
            Span::new(0, 3),
        );
        assert!(ev.eval_expr(&expr).is_err());
    }

    #[test]
    fn binary_logical_and() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Binary {
                op: BinOp::And,
                lhs: Box::new(bool_expr(true)),
                rhs: Box::new(bool_expr(false)),
            },
            Span::new(0, 5),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::Bool(false)));
    }

    #[test]
    fn unary_neg_uint() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(uint_expr(5)),
            },
            Span::new(0, 2),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::Int(-5)));
    }

    #[test]
    fn unary_logical_not() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Unary {
                op: UnaryOp::LogicalNot,
                operand: Box::new(bool_expr(false)),
            },
            Span::new(0, 2),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::Bool(true)));
    }

    #[test]
    fn unary_bitnot_unsupported() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::Unary {
                op: UnaryOp::BitNot,
                operand: Box::new(uint_expr(0xFF)),
            },
            Span::new(0, 2),
        );
        assert!(ev.eval_expr(&expr).is_err());
    }

    #[test]
    fn if_expr_false_branch() {
        let ev = ConstEval::new();
        let expr = Spanned::new(
            ExprKind::IfExpr {
                condition: Box::new(bool_expr(false)),
                then_expr: Box::new(uint_expr(1)),
                else_expr: Box::new(uint_expr(2)),
            },
            Span::new(0, 10),
        );
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(2)));
    }

    #[test]
    fn with_bindings_constructor() {
        let mut map = HashMap::new();
        map.insert("WIDTH".into(), ConstValue::UInt(32));
        let ev = ConstEval::with_bindings(map);
        let expr = Spanned::new(ExprKind::Ident("WIDTH".into()), Span::new(0, 5));
        assert_eq!(ev.eval_expr(&expr), Ok(ConstValue::UInt(32)));
    }

    #[test]
    fn unsupported_op_error() {
        let ev = ConstEval::new();
        // Concat is not supported in const context
        let expr = Spanned::new(
            ExprKind::Binary {
                op: BinOp::Concat,
                lhs: Box::new(uint_expr(1)),
                rhs: Box::new(uint_expr(2)),
            },
            Span::new(0, 5),
        );
        assert!(ev.eval_expr(&expr).is_err());
    }
}
