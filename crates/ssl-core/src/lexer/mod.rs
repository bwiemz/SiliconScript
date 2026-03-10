mod token;
mod numeric;
mod lex;
mod indent;

pub use token::{Token, NumericLiteral, NumericBase};
pub use lex::{lex, LexError};
pub use indent::{process_indentation, IndentError};
// parse_numeric is pub(crate) — only used by logos callbacks in token.rs

use crate::span::Spanned;

pub fn tokenize(source: &str) -> Result<Vec<Spanned<Token>>, TokenizeError> {
    let raw = lex(source).map_err(TokenizeError::Lex)?;
    // Strip comments BEFORE indentation processing
    let no_comments: Vec<Spanned<Token>> = raw
        .into_iter()
        .filter(|t| !matches!(t.node, Token::LineComment | Token::BlockComment))
        .collect();
    let indented = process_indentation(source, no_comments).map_err(TokenizeError::Indent)?;
    Ok(indented)
}

#[derive(Debug)]
pub enum TokenizeError {
    Lex(LexError),
    Indent(IndentError),
}

impl std::fmt::Display for TokenizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizeError::Lex(e) => write!(f, "{}", e),
            TokenizeError::Indent(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for TokenizeError {}
