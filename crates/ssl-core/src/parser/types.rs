use crate::ast::expr::CallArg;
use crate::ast::types::*;
use crate::lexer::Token;
use crate::span::{Span, Spanned};
use super::expr::{parse_expr, parse_expr_in_generic};
use super::{ParseError, Parser};

/// Expect a closing `>` for a generic type. Handles the `>>` (ShiftRight) case by
/// replacing the current `ShiftRight` token with a single `Greater` and re-inserting
/// a second `Greater` at the current position.
fn expect_close_angle(p: &mut Parser<'_>) -> Result<Spanned<Token>, ParseError> {
    if p.check(Token::Greater) {
        return Ok(p.advance());
    }
    // Handle `>>` lexed as ShiftRight: split it into two `>`
    if p.check(Token::ShiftRight) {
        let sr = p.tokens[p.pos].clone();
        // Replace this token with `>` and insert another `>` after
        let mid = (sr.span.start + sr.span.end) / 2;
        let g1 = Spanned::new(Token::Greater, Span::new(sr.span.start, mid));
        let g2 = Spanned::new(Token::Greater, Span::new(mid, sr.span.end));
        p.tokens[p.pos] = g2;
        p.tokens.insert(p.pos, g1);
        return Ok(p.advance());
    }
    let found = p.peek().cloned();
    Err(ParseError {
        message: format!("expected {:?}, found {:?}", Token::Greater, found),
        span: p.peek_span(),
    })
}

pub fn parse_type_expr(p: &mut Parser<'_>) -> Result<TypeExpr, ParseError> {
    let mut ty = parse_base_type(p)?;
    while p.check(Token::LBracket) {
        p.advance();
        let size = Box::new(parse_expr(p)?);
        let close = p.expect_token(Token::RBracket)?;
        let ty_span = ty.span;
        ty = Spanned::new(TypeExprKind::Array { element: Box::new(ty), size }, ty_span.merge(close.span));
    }
    Ok(ty)
}

/// Parse a type expression followed by optional `@ domain` annotation.
/// Use this in contexts where domain annotation is unambiguous (port declarations).
pub fn parse_type_expr_with_domain(p: &mut Parser<'_>) -> Result<TypeExpr, ParseError> {
    let start = p.peek_span();
    let mut ty = parse_type_expr(p)?;
    if p.eat(Token::At).is_some() {
        let domain = p.expect_ident()?;
        ty = Spanned::new(TypeExprKind::DomainAnnotated { ty: Box::new(ty), domain: domain.clone() }, start.merge(domain.span));
    }
    Ok(ty)
}

fn parse_base_type(p: &mut Parser<'_>) -> Result<TypeExpr, ParseError> {
    let start = p.peek_span();
    if let Some(dir) = try_direction_keyword(p) {
        p.advance();
        p.expect_token(Token::Less)?;
        let inner = parse_type_expr(p)?;
        let close = expect_close_angle(p)?;
        return Ok(Spanned::new(TypeExprKind::DirectionWrapper { dir, inner: Box::new(inner) }, start.merge(close.span)));
    }
    let name = p.expect_ident()?;
    let name_str = name.node.clone();

    if name_str == "Flip" && p.check(Token::Less) {
        p.advance();
        let inner = parse_type_expr(p)?;
        let close = expect_close_angle(p)?;
        return Ok(Spanned::new(TypeExprKind::Flip(Box::new(inner)), start.merge(close.span)));
    }
    if name_str == "Clock" && p.check(Token::Less) { return parse_clock_type(p, start); }
    if name_str == "SyncReset" {
        return if p.check(Token::Less) { parse_reset_type(p, start, true) }
        else { Ok(Spanned::new(TypeExprKind::SyncReset { polarity: None }, name.span)) };
    }
    if name_str == "AsyncReset" {
        return if p.check(Token::Less) { parse_reset_type(p, start, false) }
        else { Ok(Spanned::new(TypeExprKind::AsyncReset { polarity: None }, name.span)) };
    }
    if name_str == "Memory" && p.check(Token::Less) { return parse_memory_type(p, start, false); }
    if name_str == "DualPortMemory" && p.check(Token::Less) { return parse_memory_type(p, start, true); }
    if p.check(Token::Less) {
        p.advance();
        let params = parse_generic_arg_list(p)?;
        return Ok(Spanned::new(TypeExprKind::Generic { name: name_str, params }, start.merge(p.prev_span())));
    }
    // PartialInterface: Name.{group1, group2}
    if p.check(Token::Dot) {
        let saved = p.pos;
        p.advance(); // consume `.`
        if p.check(Token::LBrace) {
            p.advance(); // consume `{`
            let groups = p.parse_comma_list(Token::RBrace, |p| p.expect_ident())?;
            return Ok(Spanned::new(TypeExprKind::PartialInterface { name: name_str, groups }, start.merge(p.prev_span())));
        } else {
            p.pos = saved; // not a partial interface, backtrack
        }
    }
    Ok(Spanned::new(TypeExprKind::Named(name_str), name.span))
}

fn try_direction_keyword(p: &Parser<'_>) -> Option<Direction> {
    let dir = match p.peek() {
        Some(Token::KwIn) => Direction::In,
        Some(Token::KwOut) => Direction::Out,
        Some(Token::KwInout) => Direction::InOut,
        _ => return None,
    };
    if p.tokens.get(p.pos + 1).map(|t| &t.node) == Some(&Token::Less) { Some(dir) } else { None }
}

/// Parse a comma-separated list of generic args, closing with `>` (or split `>>`).
fn parse_generic_arg_list(p: &mut Parser<'_>) -> Result<Vec<GenericArg>, ParseError> {
    let mut items = Vec::new();
    p.skip_newlines();
    while !p.check(Token::Greater) && !p.check(Token::ShiftRight) && !p.is_at_end() {
        items.push(parse_generic_arg(p)?);
        p.skip_newlines();
        if p.eat(Token::Comma).is_none() {
            break;
        }
        p.skip_newlines();
    }
    p.skip_newlines();
    expect_close_angle(p)?;
    Ok(items)
}

fn parse_generic_arg(p: &mut Parser<'_>) -> Result<GenericArg, ParseError> {
    if is_type_start(p) { Ok(GenericArg::Type(parse_type_expr(p)?)) }
    else { Ok(GenericArg::Expr(parse_expr_in_generic(p)?)) }
}

fn is_type_start(p: &Parser<'_>) -> bool {
    match p.peek() {
        Some(Token::KwIn) | Some(Token::KwOut) | Some(Token::KwInout) =>
            p.tokens.get(p.pos + 1).map(|t| &t.node) == Some(&Token::Less),
        Some(Token::Ident) => p.text(p.peek_span()).starts_with(|c: char| c.is_ascii_uppercase()),
        _ => false,
    }
}

fn parse_clock_type(p: &mut Parser<'_>, start: Span) -> Result<TypeExpr, ParseError> {
    p.advance(); // consume `<`
    let freq = Some(parse_expr_in_generic(p)?);
    let edge = if p.eat(Token::Comma).is_some() {
        let e = p.expect_ident()?;
        Some(match e.node.as_str() {
            "rising" => ClockEdge::Rising,
            "falling" => ClockEdge::Falling,
            "dual" => ClockEdge::Dual,
            _ => return Err(ParseError { message: format!("expected clock edge, found '{}'", e.node), span: e.span }),
        })
    } else { None };
    let close = expect_close_angle(p)?;
    Ok(Spanned::new(TypeExprKind::Clock { freq, edge }, start.merge(close.span)))
}

fn parse_reset_type(p: &mut Parser<'_>, start: Span, is_sync: bool) -> Result<TypeExpr, ParseError> {
    p.advance(); // consume `<`
    let pi = p.expect_ident()?;
    let polarity = Some(match pi.node.as_str() {
        "active_high" => ResetPolarity::ActiveHigh,
        "active_low" => ResetPolarity::ActiveLow,
        _ => return Err(ParseError { message: format!("expected polarity, found '{}'", pi.node), span: pi.span }),
    });
    let close = expect_close_angle(p)?;
    let span = start.merge(close.span);
    Ok(Spanned::new(if is_sync { TypeExprKind::SyncReset { polarity } } else { TypeExprKind::AsyncReset { polarity } }, span))
}

fn parse_memory_type(p: &mut Parser<'_>, start: Span, dual: bool) -> Result<TypeExpr, ParseError> {
    p.advance(); // consume `<`
    let element = parse_type_expr(p)?;
    let mut params = Vec::new();
    while p.eat(Token::Comma).is_some() {
        p.skip_newlines();
        let pn = p.expect_ident()?;
        p.expect_token(Token::Eq)?;
        params.push(CallArg { name: Some(pn), value: parse_expr_in_generic(p)? });
    }
    let close = expect_close_angle(p)?;
    let span = start.merge(close.span);
    let kind = if dual { TypeExprKind::DualPortMemory { element: Box::new(element), params } }
    else { TypeExprKind::Memory { element: Box::new(element), params } };
    Ok(Spanned::new(kind, span))
}

pub fn parse_generic_params(p: &mut Parser<'_>) -> Result<Vec<GenericParam>, ParseError> {
    if p.eat(Token::Less).is_none() { return Ok(Vec::new()); }
    p.parse_comma_list(Token::Greater, |p| {
        let name = p.expect_ident()?;
        p.expect_token(Token::Colon)?;
        let ki = p.expect_ident()?;
        let kind = match ki.node.as_str() {
            "uint" => GenericKind::Uint, "int" => GenericKind::Int, "bool" => GenericKind::Bool,
            "float" => GenericKind::Float, "string" => GenericKind::StringKind, "type" => GenericKind::Type,
            _ => return Err(ParseError { message: format!("expected generic kind, found '{}'", ki.node), span: ki.span }),
        };
        let default = if p.eat(Token::Eq).is_some() { Some(parse_expr(p)?) } else { None };
        Ok(GenericParam { name, kind, default })
    })
}
