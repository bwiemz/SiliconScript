use crate::span::Spanned;
use crate::lexer::NumericLiteral;
use super::Ident;
use super::types::TypeExpr;

pub type Expr = Spanned<ExprKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // Literals
    IntLiteral(NumericLiteral),
    StringLiteral(String),
    BoolLiteral(bool),
    Ident(String),

    // Operators
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary { op: UnaryOp, operand: Box<Expr> },

    // Access
    FieldAccess { object: Box<Expr>, field: Ident },
    MethodCall { object: Box<Expr>, method: Ident, args: Vec<CallArg> },
    Call { callee: Box<Expr>, args: Vec<CallArg> },
    Index { array: Box<Expr>, index: Box<Expr> },
    BitSlice { value: Box<Expr>, high: Box<Expr>, low: Box<Expr> },

    // Special
    Pipe { input: Box<Expr>, callee: Box<Expr>, args: Vec<CallArg> },
    IfExpr { condition: Box<Expr>, then_expr: Box<Expr>, else_expr: Box<Expr> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    StructLiteral { type_name: Ident, fields: Vec<(Ident, Expr)> },
    ArrayLiteral(Vec<Expr>),
    Paren(Box<Expr>),

    // Formal (bounded temporal)
    Next { expr: Box<Expr>, count: Option<Box<Expr>> },
    Eventually { expr: Box<Expr>, depth: Box<Expr> },

    // Type-related
    TypeCast { expr: Box<Expr>, ty: Box<TypeExpr> },

    // Safety
    Unchecked(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod, Pow,
    BitAnd, BitOr, BitXor,
    Shl, Shr, ArithShr,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or, Implies,
    Concat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    BitNot,
    LogicalNot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallArg {
    pub name: Option<Ident>,
    pub value: Expr,
}
