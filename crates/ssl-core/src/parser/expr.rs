use crate::ast::expr::{BinOp, CallArg, Expr, ExprKind, UnaryOp};
use crate::lexer::Token;
use crate::span::{Span, Spanned};

use super::{ParseError, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
}

fn infix_binding_power(token: &Token) -> Option<(BinOp, u8, Assoc)> {
    match token {
        Token::KwImplies => Some((BinOp::Implies, 1, Assoc::Right)),
        Token::KwOr => Some((BinOp::Or, 2, Assoc::Left)),
        Token::KwAnd => Some((BinOp::And, 3, Assoc::Left)),
        Token::Pipe => Some((BinOp::BitOr, 4, Assoc::Left)),
        Token::Caret => Some((BinOp::BitXor, 5, Assoc::Left)),
        Token::Ampersand => Some((BinOp::BitAnd, 6, Assoc::Left)),
        Token::EqEq => Some((BinOp::Eq, 7, Assoc::Left)),
        Token::NotEq => Some((BinOp::Ne, 7, Assoc::Left)),
        Token::Less => Some((BinOp::Lt, 8, Assoc::Left)),
        Token::Greater => Some((BinOp::Gt, 8, Assoc::Left)),
        Token::LessEq => Some((BinOp::Le, 8, Assoc::Left)),
        Token::GreaterEq => Some((BinOp::Ge, 8, Assoc::Left)),
        Token::RangeExclusive => Some((BinOp::Concat, 9, Assoc::Left)), // placeholder op, handled specially
        Token::RangeInclusive => Some((BinOp::Concat, 9, Assoc::Left)), // placeholder op, handled specially
        Token::ShiftLeft => Some((BinOp::Shl, 10, Assoc::Left)),
        Token::ShiftRight => Some((BinOp::Shr, 10, Assoc::Left)),
        Token::ArithShiftRight => Some((BinOp::ArithShr, 10, Assoc::Left)),
        Token::Concat => Some((BinOp::Concat, 11, Assoc::Left)),
        Token::Plus => Some((BinOp::Add, 12, Assoc::Left)),
        Token::Minus => Some((BinOp::Sub, 12, Assoc::Left)),
        Token::Star => Some((BinOp::Mul, 13, Assoc::Left)),
        Token::Slash => Some((BinOp::Div, 13, Assoc::Left)),
        Token::Percent => Some((BinOp::Mod, 13, Assoc::Left)),
        Token::StarStar => Some((BinOp::Pow, 14, Assoc::Right)),
        _ => None,
    }
}

/// Entry point for expression parsing.
pub fn parse_expr(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    parse_pipe_expr(p)
}

/// Parse pipe expressions: `expr |> call_expr`. Lowest precedence.
fn parse_pipe_expr(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    let mut lhs = parse_pratt(p, 0)?;
    while p.eat(Token::PipeOp).is_some() {
        let rhs = parse_pratt(p, 0)?;
        // Decompose RHS: must be a Call node; pipe inserts LHS as first arg
        let span = lhs.span.merge(rhs.span);
        match rhs.node {
            ExprKind::Call { callee, args } => {
                lhs = Spanned::new(
                    ExprKind::Pipe {
                        input: Box::new(lhs),
                        callee,
                        args,
                    },
                    span,
                );
            }
            ExprKind::Ident(_) => {
                // bare identifier: treat as zero-arg call
                lhs = Spanned::new(
                    ExprKind::Pipe {
                        input: Box::new(lhs),
                        callee: Box::new(rhs),
                        args: vec![],
                    },
                    span,
                );
            }
            _ => {
                return Err(ParseError {
                    message: "pipe operator RHS must be a function call or identifier".into(),
                    span,
                });
            }
        }
    }
    Ok(lhs)
}

/// Pratt parser for binary operators.
fn parse_pratt(p: &mut Parser<'_>, min_prec: u8) -> Result<Expr, ParseError> {
    let mut lhs = parse_unary(p)?;
    lhs = parse_postfix(p, lhs)?;

    loop {
        let tok = match p.peek() {
            Some(t) => t.clone(),
            None => break,
        };
        let (op, prec, assoc) = match infix_binding_power(&tok) {
            Some(info) => info,
            None => break,
        };
        if prec < min_prec {
            break;
        }

        // Check for range operators — produce Range nodes, not Binary
        let is_range_exclusive = matches!(tok, Token::RangeExclusive);
        let is_range_inclusive = matches!(tok, Token::RangeInclusive);

        p.advance(); // consume the operator

        let next_min = if assoc == Assoc::Right { prec } else { prec + 1 };
        let rhs = parse_pratt(p, next_min)?;

        let span = lhs.span.merge(rhs.span);

        if is_range_exclusive {
            lhs = Spanned::new(
                ExprKind::Range {
                    start: Box::new(lhs),
                    end: Box::new(rhs),
                    inclusive: false,
                },
                span,
            );
        } else if is_range_inclusive {
            lhs = Spanned::new(
                ExprKind::Range {
                    start: Box::new(lhs),
                    end: Box::new(rhs),
                    inclusive: true,
                },
                span,
            );
        } else {
            lhs = Spanned::new(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
    }

    Ok(lhs)
}

/// Parse prefix unary operators: `not`, `~`, `-`.
fn parse_unary(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    let start = p.peek_span();
    match p.peek() {
        Some(Token::KwNot) => {
            p.advance();
            let operand = parse_unary(p)?;
            let span = start.merge(operand.span);
            Ok(Spanned::new(
                ExprKind::Unary {
                    op: UnaryOp::LogicalNot,
                    operand: Box::new(operand),
                },
                span,
            ))
        }
        Some(Token::Tilde) => {
            p.advance();
            let operand = parse_unary(p)?;
            let span = start.merge(operand.span);
            Ok(Spanned::new(
                ExprKind::Unary {
                    op: UnaryOp::BitNot,
                    operand: Box::new(operand),
                },
                span,
            ))
        }
        Some(Token::Minus) => {
            p.advance();
            let operand = parse_unary(p)?;
            let span = start.merge(operand.span);
            Ok(Spanned::new(
                ExprKind::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                },
                span,
            ))
        }
        _ => parse_atom(p),
    }
}

/// Parse postfix operations: `.field`, `.method(args)`, `[index]`, `[H:L]`, `(args)`.
fn parse_postfix(p: &mut Parser<'_>, mut lhs: Expr) -> Result<Expr, ParseError> {
    loop {
        match p.peek() {
            Some(Token::Dot) => {
                p.advance(); // consume `.`
                let field = p.expect_ident()?;
                // Check if this is a method call: `.method(`
                if p.check(Token::LParen) {
                    p.advance(); // consume `(`
                    let args = parse_call_args(p)?;
                    let end_pos = p.pos.saturating_sub(1);
                    let end = p.tokens.get(end_pos).map_or(lhs.span.end, |t| t.span.end);
                    let span = lhs.span.merge(Span::new(end, end));
                    lhs = Spanned::new(
                        ExprKind::MethodCall {
                            object: Box::new(lhs),
                            method: field,
                            args,
                        },
                        span,
                    );
                } else {
                    let span = lhs.span.merge(field.span);
                    lhs = Spanned::new(
                        ExprKind::FieldAccess {
                            object: Box::new(lhs),
                            field,
                        },
                        span,
                    );
                }
            }
            Some(Token::LBracket) => {
                p.advance(); // consume `[`
                let index_expr = parse_expr(p)?;
                // Check for bit slice: `[H:L]`
                if p.eat(Token::Colon).is_some() {
                    let low = parse_expr(p)?;
                    let close = p.expect_token(Token::RBracket)?;
                    let span = lhs.span.merge(close.span);
                    lhs = Spanned::new(
                        ExprKind::BitSlice {
                            value: Box::new(lhs),
                            high: Box::new(index_expr),
                            low: Box::new(low),
                        },
                        span,
                    );
                } else {
                    let close = p.expect_token(Token::RBracket)?;
                    let span = lhs.span.merge(close.span);
                    lhs = Spanned::new(
                        ExprKind::Index {
                            array: Box::new(lhs),
                            index: Box::new(index_expr),
                        },
                        span,
                    );
                }
            }
            Some(Token::LParen) => {
                p.advance(); // consume `(`
                let args = parse_call_args(p)?;
                let end_pos = p.pos.saturating_sub(1);
                let end = p.tokens.get(end_pos).map_or(lhs.span.end, |t| t.span.end);
                let span = lhs.span.merge(Span::new(end, end));
                lhs = Spanned::new(
                    ExprKind::Call {
                        callee: Box::new(lhs),
                        args,
                    },
                    span,
                );
            }
            _ => break,
        }
    }
    Ok(lhs)
}

/// Parse an atomic expression (literals, identifiers, parenthesized, etc.).
fn parse_atom(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    let start = p.peek_span();
    match p.peek().cloned() {
        Some(Token::Numeric(n)) => {
            let tok = p.advance();
            Ok(Spanned::new(ExprKind::IntLiteral(n), tok.span))
        }
        Some(Token::StringLit(s)) => {
            let tok = p.advance();
            Ok(Spanned::new(ExprKind::StringLiteral(s), tok.span))
        }
        Some(Token::KwTrue) => {
            let tok = p.advance();
            Ok(Spanned::new(ExprKind::BoolLiteral(true), tok.span))
        }
        Some(Token::KwFalse) => {
            let tok = p.advance();
            Ok(Spanned::new(ExprKind::BoolLiteral(false), tok.span))
        }
        Some(Token::KwNext) => {
            let tok = p.advance();
            p.expect_token(Token::LParen)?;
            let expr = parse_expr(p)?;
            let count = if p.eat(Token::Comma).is_some() {
                Some(Box::new(parse_expr(p)?))
            } else {
                None
            };
            let close = p.expect_token(Token::RParen)?;
            let span = tok.span.merge(close.span);
            Ok(Spanned::new(
                ExprKind::Next {
                    expr: Box::new(expr),
                    count,
                },
                span,
            ))
        }
        Some(Token::KwEventually) => {
            let tok = p.advance();
            p.expect_token(Token::LParen)?;
            let expr = parse_expr(p)?;
            p.expect_token(Token::Comma)?;
            // Handle `depth=N` named arg syntax
            let depth = if p.check_ident() {
                let saved_pos = p.pos;
                let _maybe_name = p.advance();
                if p.eat(Token::Eq).is_some() {
                    // named arg: `depth=N`
                    parse_expr(p)?
                } else {
                    // not named, rewind and parse as expr
                    p.pos = saved_pos;
                    parse_expr(p)?
                }
            } else {
                parse_expr(p)?
            };
            let close = p.expect_token(Token::RParen)?;
            let span = tok.span.merge(close.span);
            Ok(Spanned::new(
                ExprKind::Eventually {
                    expr: Box::new(expr),
                    depth: Box::new(depth),
                },
                span,
            ))
        }
        Some(Token::KwIf) => {
            let tok = p.advance();
            let condition = parse_expr(p)?;
            p.expect_token(Token::KwThen)?;
            let then_expr = parse_expr(p)?;
            p.expect_token(Token::KwElse)?;
            let else_expr = parse_expr(p)?;
            let span = tok.span.merge(else_expr.span);
            Ok(Spanned::new(
                ExprKind::IfExpr {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                },
                span,
            ))
        }
        Some(Token::Ident) => {
            let tok = p.advance();
            let text = p.text(tok.span).to_string();
            Ok(Spanned::new(ExprKind::Ident(text), tok.span))
        }
        Some(Token::LParen) => {
            p.advance();
            p.skip_newlines();
            let expr = parse_expr(p)?;
            p.skip_newlines();
            let close = p.expect_token(Token::RParen)?;
            let span = start.merge(close.span);
            Ok(Spanned::new(ExprKind::Paren(Box::new(expr)), span))
        }
        Some(Token::LBracket) => {
            p.advance();
            p.skip_newlines();
            let mut elements = Vec::new();
            while !p.check(Token::RBracket) && !p.is_at_end() {
                elements.push(parse_expr(p)?);
                p.skip_newlines();
                if p.eat(Token::Comma).is_none() {
                    break;
                }
                p.skip_newlines();
            }
            let close = p.expect_token(Token::RBracket)?;
            let span = start.merge(close.span);
            Ok(Spanned::new(ExprKind::ArrayLiteral(elements), span))
        }
        other => Err(ParseError {
            message: format!("expected expression, found {:?}", other),
            span: start,
        }),
    }
}

/// Parse call arguments: `[name=]expr, ...` ending at `)`.
/// The opening `(` has already been consumed. Consumes the closing `)`.
pub fn parse_call_args(p: &mut Parser<'_>) -> Result<Vec<CallArg>, ParseError> {
    let mut args = Vec::new();
    p.skip_newlines();
    while !p.check(Token::RParen) && !p.is_at_end() {
        // Try to parse named argument: `name = expr`
        let arg = if p.check_ident() {
            let saved_pos = p.pos;
            let maybe_name = p.advance();
            if p.eat(Token::Eq).is_some() {
                let name_text = p.text(maybe_name.span).to_string();
                let value = parse_expr(p)?;
                CallArg {
                    name: Some(Spanned::new(name_text, maybe_name.span)),
                    value,
                }
            } else {
                // Not a named arg — rewind and parse as positional
                p.pos = saved_pos;
                let value = parse_expr(p)?;
                CallArg { name: None, value }
            }
        } else {
            let value = parse_expr(p)?;
            CallArg { name: None, value }
        };
        args.push(arg);
        p.skip_newlines();
        if p.eat(Token::Comma).is_none() {
            break;
        }
        p.skip_newlines();
    }
    p.expect_token(Token::RParen)?;
    Ok(args)
}
