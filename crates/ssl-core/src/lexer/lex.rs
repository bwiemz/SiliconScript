use super::token::Token;
use crate::span::{Span, Spanned};
use logos::Logos;

pub fn lex(source: &str) -> Result<Vec<Spanned<Token>>, LexError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(source);

    while let Some(result) = lexer.next() {
        let span: Span = lexer.span().into();
        match result {
            Ok(token) => {
                tokens.push(Spanned::new(token, span));
            }
            Err(()) => {
                let slice = &source[span.start as usize..span.end as usize];
                if slice == "\n" || slice == "\r\n" || slice == "\r" {
                    tokens.push(Spanned::new(Token::Newline, span));
                } else {
                    return Err(LexError {
                        message: format!("unexpected character: {:?}", slice),
                        span,
                    });
                }
            }
        }
    }

    Ok(tokens)
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "lex error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for LexError {}
