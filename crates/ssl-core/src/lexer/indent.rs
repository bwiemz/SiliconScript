use super::token::Token;
use crate::span::{Span, Spanned};

pub fn process_indentation(
    source: &str,
    tokens: Vec<Spanned<Token>>,
) -> Result<Vec<Spanned<Token>>, IndentError> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut indent_stack: Vec<u32> = vec![0];
    let mut i = 0;

    while i < tokens.len() {
        let tok = &tokens[i];
        if tok.node == Token::Newline {
            let newline_span = tok.span;
            let line_start = newline_span.end as usize;

            if is_blank_line(source, line_start) {
                result.push(Spanned::new(Token::Newline, newline_span));
                i += 1;
                continue;
            }

            let indent_level = measure_indent(source, line_start);
            let current = *indent_stack.last().unwrap();

            if indent_level > current {
                result.push(Spanned::new(Token::Newline, newline_span));
                result.push(Spanned::new(
                    Token::Indent,
                    Span::new(line_start as u32, (line_start as u32) + indent_level),
                ));
                indent_stack.push(indent_level);
            } else if indent_level < current {
                result.push(Spanned::new(Token::Newline, newline_span));
                while indent_stack.len() > 1 && *indent_stack.last().unwrap() > indent_level {
                    indent_stack.pop();
                    result.push(Spanned::new(
                        Token::Dedent,
                        Span::new(line_start as u32, (line_start as u32) + indent_level),
                    ));
                }
                if *indent_stack.last().unwrap() != indent_level {
                    return Err(IndentError {
                        message: format!(
                            "dedent to level {} does not match any outer indentation level",
                            indent_level
                        ),
                        span: Span::new(line_start as u32, (line_start as u32) + indent_level),
                    });
                }
            } else {
                result.push(Spanned::new(Token::Newline, newline_span));
            }
        } else {
            result.push(tok.clone());
        }
        i += 1;
    }

    let eof_pos = source.len() as u32;
    while indent_stack.len() > 1 {
        indent_stack.pop();
        result.push(Spanned::new(Token::Dedent, Span::new(eof_pos, eof_pos)));
    }

    Ok(result)
}

fn is_blank_line(source: &str, pos: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = pos;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' => i += 1,
            b'\n' | b'\r' => return true,
            _ => return false,
        }
    }
    true
}

fn measure_indent(source: &str, pos: usize) -> u32 {
    let bytes = source.as_bytes();
    let mut i = pos;
    let mut count = 0u32;
    while i < bytes.len() {
        match bytes[i] {
            b' ' => {
                count += 1;
                i += 1;
            }
            b'\t' => {
                count += 4;
                i += 1;
            }
            _ => break,
        }
    }
    count
}

#[derive(Debug, Clone)]
pub struct IndentError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for IndentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "indentation error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for IndentError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn get_structural_tokens(source: &str) -> Vec<Token> {
        let raw = lex(source).expect("lex failed");
        let processed = process_indentation(source, raw).expect("indent failed");
        processed
            .into_iter()
            .map(|s| s.node)
            .filter(|t| {
                matches!(
                    t,
                    Token::Indent
                        | Token::Dedent
                        | Token::Newline
                        | Token::KwModule
                        | Token::KwComb
                        | Token::KwReg
                        | Token::KwSignal
                        | Token::KwIf
                        | Token::KwMatch
                        | Token::Ident
                        | Token::Colon
                )
            })
            .collect()
    }

    #[test]
    fn simple_indent() {
        let source = "module Foo:\n    signal x";
        let tokens = get_structural_tokens(source);
        assert!(tokens.contains(&Token::Indent));
    }

    #[test]
    fn indent_and_dedent() {
        let source = "comb:\n    x = y\nmodule Bar";
        let tokens = get_structural_tokens(source);
        assert!(tokens.contains(&Token::Indent));
        assert!(tokens.contains(&Token::Dedent));
    }

    #[test]
    fn nested_indent() {
        let source = "comb:\n    if cond:\n        x = y\n    z = w";
        let tokens = get_structural_tokens(source);
        let indent_count = tokens.iter().filter(|t| **t == Token::Indent).count();
        let dedent_count = tokens.iter().filter(|t| **t == Token::Dedent).count();
        assert_eq!(indent_count, 2);
        assert_eq!(dedent_count, 2);
    }

    #[test]
    fn eof_closes_all_indents() {
        let source = "comb:\n    match x:\n        y = z";
        let tokens = get_structural_tokens(source);
        let indent_count = tokens.iter().filter(|t| **t == Token::Indent).count();
        let dedent_count = tokens.iter().filter(|t| **t == Token::Dedent).count();
        assert_eq!(indent_count, dedent_count);
    }
}
