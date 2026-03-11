use crate::span::Spanned;
use super::Ident;
use super::expr::{Expr, CallArg};

pub type TypeExpr = Spanned<TypeExprKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    Named(String),
    Generic { name: String, params: Vec<GenericArg> },
    Array { element: Box<TypeExpr>, size: Box<Expr> },
    Clock { freq: Option<Expr>, edge: Option<ClockEdge> },
    SyncReset { polarity: Option<ResetPolarity> },
    AsyncReset { polarity: Option<ResetPolarity> },
    DirectionWrapper { dir: Direction, inner: Box<TypeExpr> },
    Flip(Box<TypeExpr>),
    DomainAnnotated { ty: Box<TypeExpr>, domain: Ident },
    PartialInterface { name: String, groups: Vec<Ident> },
    Memory { element: Box<TypeExpr>, params: Vec<CallArg> },
    DualPortMemory { element: Box<TypeExpr>, params: Vec<CallArg> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum GenericArg {
    Expr(Expr),
    Type(TypeExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockEdge { Rising, Falling, Dual }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetPolarity { ActiveHigh, ActiveLow }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { In, Out, InOut }

#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub name: Ident,
    pub kind: GenericKind,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericKind { Uint, Int, Bool, Float, StringKind, Type }
