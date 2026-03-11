pub mod expr;
pub mod types;
pub mod stmt;
pub mod item;

use crate::span::{Span, Spanned};

pub type Ident = Spanned<String>;

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: Ident,
    pub args: Vec<Spanned<expr::Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocComment {
    pub text: String,
    pub span: Span,
}
