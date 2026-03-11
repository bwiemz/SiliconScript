pub mod expr;
pub mod types;
pub mod stmt;
pub mod item;

use crate::ast::item::SourceFile;
use crate::lexer::Token;
use crate::span::{Span, Spanned};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}
impl std::error::Error for ParseError {}

pub struct Parser<'src> {
    source: &'src str,
    pub(crate) tokens: Vec<Spanned<Token>>,
    pub(crate) pos: usize,
}

impl<'src> Parser<'src> {
    pub fn new(source: &'src str, tokens: Vec<Spanned<Token>>) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
        }
    }

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.node)
    }

    pub fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or_else(|| {
                let end = self.source.len() as u32;
                Span::new(end, end)
            })
    }

    pub fn advance(&mut self) -> Spanned<Token> {
        assert!(self.pos < self.tokens.len(), "advance() called at end of token stream");
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    pub fn check(&self, expected: Token) -> bool {
        match self.peek() {
            Some(tok) => std::mem::discriminant(tok) == std::mem::discriminant(&expected),
            None => false,
        }
    }

    pub fn check_ident(&self) -> bool {
        matches!(self.peek(), Some(Token::Ident))
    }

    pub fn eat(&mut self, expected: Token) -> Option<Spanned<Token>> {
        if self.check(expected) {
            Some(self.advance())
        } else {
            None
        }
    }

    pub fn expect_token(&mut self, expected: Token) -> Result<Spanned<Token>, ParseError> {
        if self.check(expected.clone()) {
            Ok(self.advance())
        } else {
            let found = self.peek().cloned();
            Err(ParseError {
                message: format!("expected {:?}, found {:?}", expected, found),
                span: self.peek_span(),
            })
        }
    }

    pub fn expect_ident(&mut self) -> Result<Spanned<String>, ParseError> {
        if self.check_ident() {
            let tok = self.advance();
            let text = self.text(tok.span).to_string();
            Ok(Spanned::new(text, tok.span))
        } else {
            let found = self.peek().cloned();
            Err(ParseError {
                message: format!("expected identifier, found {:?}", found),
                span: self.peek_span(),
            })
        }
    }

    pub fn text(&self, span: Span) -> &str {
        &self.source[span.start as usize..span.end as usize]
    }

    pub fn skip_newlines(&mut self) {
        while matches!(self.peek(), Some(Token::Newline)) {
            self.advance();
        }
    }

    pub fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.wrapping_sub(1))
            .map(|t| t.span)
            .unwrap_or_else(|| {
                let end = self.source.len() as u32;
                Span::new(end, end)
            })
    }

    pub fn error(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            message: msg.into(),
            span: self.peek_span(),
        }
    }

    pub fn parse_block<T>(
        &mut self,
        mut f: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;
        let mut items = Vec::new();
        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() {
                break;
            }
            items.push(f(self)?);
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;
        Ok(items)
    }

    pub fn parse_comma_list<T>(
        &mut self,
        close_token: Token,
        mut f: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.check(close_token.clone()) && !self.is_at_end() {
            items.push(f(self)?);
            self.skip_newlines();
            if self.eat(Token::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        self.expect_token(close_token)?;
        Ok(items)
    }

    pub fn parse(source: &str, tokens: Vec<Spanned<Token>>) -> Result<SourceFile, ParseError> {
        let mut parser = Parser::new(source, tokens);
        parser.parse_file()
    }
}
