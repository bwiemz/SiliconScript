use crate::span::{Span, Spanned};
use super::Ident;
use super::expr::Expr;
use super::types::{TypeExpr, GenericParam};

pub type Stmt = Spanned<StmtKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Signal(SignalDecl),
    Let(LetDecl),
    Const(ConstDecl),
    TypeAlias(TypeAliasDecl),
    Assign { target: Expr, value: Expr },
    If(IfStmt),
    Match(MatchStmt),
    For(ForStmt),
    CombBlock(Vec<Stmt>),
    RegBlock(RegBlock),
    PriorityBlock(PriorityBlock),
    ParallelBlock(ParallelBlock),
    Assert(AssertStmt),
    Assume { domain: Option<Ident>, expr: Expr, message: Option<Expr> },
    Cover { name: Option<Ident>, expr: Expr },
    StaticAssert { expr: Expr, message: Expr },
    UncheckedBlock(Vec<Stmt>),
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    pub domain: Option<Ident>,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetDecl {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDecl {
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_body: Vec<Stmt>,
    pub elif_branches: Vec<(Expr, Vec<Stmt>)>,
    pub else_body: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStmt {
    pub var: Ident,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegBlock {
    pub clock: Expr,
    pub reset: Expr,
    pub enable: Option<Expr>,
    pub on_reset: Vec<Stmt>,
    pub on_tick: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriorityBlock {
    pub arms: Vec<PriorityArm>,
    pub otherwise: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriorityArm {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParallelBlock {
    pub safe: Option<Expr>,
    pub arms: Vec<PriorityArm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssertStmt {
    pub always: bool,
    pub domain: Option<Ident>,
    pub expr: Expr,
    pub message: Option<Expr>,
}
