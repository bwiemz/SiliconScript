use crate::ast::expr::ExprKind;
use crate::ast::stmt::*;
use crate::lexer::Token;
use crate::span::Spanned;
use super::expr::parse_expr;
use super::types::{parse_generic_params, parse_type_expr};
use super::{ParseError, Parser};

pub fn parse_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    match p.peek().cloned() {
        Some(Token::KwSignal) => parse_signal_decl(p),
        Some(Token::KwLet) => parse_let_decl(p),
        Some(Token::KwConst) => parse_const_decl(p),
        Some(Token::KwType) => parse_type_alias(p),
        Some(Token::KwIf) => parse_if_stmt(p),
        Some(Token::KwMatch) => parse_match_stmt(p),
        Some(Token::KwFor) => parse_for_stmt(p),
        Some(Token::KwComb) => parse_comb_block(p),
        Some(Token::KwReg) => parse_reg_block(p),
        Some(Token::KwPriority) => parse_priority_block(p),
        Some(Token::KwParallel) => parse_parallel_block(p),
        Some(Token::KwAssert) => parse_assert_stmt(p),
        Some(Token::KwAssume) => parse_assume_stmt(p),
        Some(Token::KwCover) => parse_cover_stmt(p),
        Some(Token::KwStaticAssert) => parse_static_assert(p),
        Some(Token::KwUnchecked) => parse_unchecked(p),
        _ => parse_assign_or_expr_stmt(p),
    }
}

fn parse_signal_decl(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwSignal)?;
    let name = p.expect_ident()?;
    p.expect_token(Token::Colon)?;
    let ty = parse_type_expr(p)?;
    let domain = if p.eat(Token::At).is_some() { Some(p.expect_ident()?) } else { None };
    let init = if p.eat(Token::Eq).is_some() { Some(parse_expr(p)?) } else { None };
    Ok(Spanned::new(StmtKind::Signal(SignalDecl { name, ty, domain, init }), start.merge(p.prev_span())))
}

fn parse_let_decl(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwLet)?;
    let name = p.expect_ident()?;
    let ty = if p.eat(Token::Colon).is_some() { Some(parse_type_expr(p)?) } else { None };
    p.expect_token(Token::Eq)?;
    let value = parse_expr(p)?;
    Ok(Spanned::new(StmtKind::Let(LetDecl { name, ty, value }), start.merge(p.prev_span())))
}

fn parse_const_decl(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwConst)?;
    let name = p.expect_ident()?;
    let ty = if p.eat(Token::Colon).is_some() { Some(parse_type_expr(p)?) } else { None };
    p.expect_token(Token::Eq)?;
    let value = parse_expr(p)?;
    Ok(Spanned::new(StmtKind::Const(ConstDecl { name, ty, value }), start.merge(p.prev_span())))
}

fn parse_type_alias(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwType)?;
    let name = p.expect_ident()?;
    let generics = parse_generic_params(p)?;
    p.expect_token(Token::Eq)?;
    let ty = parse_type_expr(p)?;
    Ok(Spanned::new(StmtKind::TypeAlias(TypeAliasDecl { name, generics, ty }), start.merge(p.prev_span())))
}

fn parse_assign_or_expr_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    let lhs = parse_expr(p)?;
    if p.eat(Token::Eq).is_some() {
        let rhs = parse_expr(p)?;
        Ok(Spanned::new(StmtKind::Assign { target: lhs, value: rhs }, start.merge(p.prev_span())))
    } else {
        Ok(Spanned::new(StmtKind::ExprStmt(lhs.clone()), lhs.span))
    }
}

fn parse_if_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwIf)?;
    let condition = parse_expr(p)?;
    let then_body = p.parse_block(|p| parse_stmt(p))?;
    let mut elif_branches = Vec::new();
    while p.eat(Token::KwElif).is_some() {
        let c = parse_expr(p)?;
        let b = p.parse_block(|p| parse_stmt(p))?;
        elif_branches.push((c, b));
    }
    let else_body = if p.eat(Token::KwElse).is_some() { Some(p.parse_block(|p| parse_stmt(p))?) } else { None };
    Ok(Spanned::new(StmtKind::If(IfStmt { condition, then_body, elif_branches, else_body }), start.merge(p.prev_span())))
}

fn parse_match_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwMatch)?;
    let scrutinee = parse_expr(p)?;
    p.expect_token(Token::Colon)?;
    p.skip_newlines();
    p.expect_token(Token::Indent)?;
    let mut arms = Vec::new();
    while !p.check(Token::Dedent) && !p.is_at_end() {
        p.skip_newlines();
        if p.check(Token::Dedent) || p.is_at_end() { break; }
        let arm_start = p.peek_span();
        let pattern = parse_expr(p)?;
        p.expect_token(Token::FatArrow)?;
        let body = if p.check(Token::Newline) || p.check(Token::Colon) {
            if p.check(Token::Colon) { p.parse_block(|p| parse_stmt(p))? }
            else {
                p.skip_newlines();
                if p.check(Token::Indent) {
                    p.advance();
                    let mut stmts = Vec::new();
                    while !p.check(Token::Dedent) && !p.is_at_end() {
                        p.skip_newlines();
                        if p.check(Token::Dedent) || p.is_at_end() { break; }
                        stmts.push(parse_stmt(p)?);
                        p.skip_newlines();
                    }
                    p.expect_token(Token::Dedent)?;
                    stmts
                } else { vec![parse_stmt(p)?] }
            }
        } else { vec![parse_stmt(p)?] };
        arms.push(MatchArm { pattern, body, span: arm_start.merge(p.prev_span()) });
        p.skip_newlines();
    }
    p.expect_token(Token::Dedent)?;
    Ok(Spanned::new(StmtKind::Match(MatchStmt { scrutinee, arms }), start.merge(p.prev_span())))
}

fn parse_for_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwFor)?;
    let var = p.expect_ident()?;
    p.expect_token(Token::KwIn)?;
    let iterable = parse_expr(p)?;
    let body = p.parse_block(|p| parse_stmt(p))?;
    Ok(Spanned::new(StmtKind::For(ForStmt { var, iterable, body }), start.merge(p.prev_span())))
}

fn parse_comb_block(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwComb)?;
    let stmts = p.parse_block(|p| parse_stmt(p))?;
    Ok(Spanned::new(StmtKind::CombBlock(stmts), start.merge(p.prev_span())))
}

fn parse_reg_block(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwReg)?;
    p.expect_token(Token::LParen)?;
    let clock = parse_expr(p)?;
    p.expect_token(Token::Comma)?;
    let reset = parse_expr(p)?;
    let enable = if p.eat(Token::Comma).is_some() {
        if p.check_ident() {
            let saved = p.pos;
            p.advance();
            if p.eat(Token::Eq).is_some() { Some(parse_expr(p)?) }
            else { p.pos = saved; Some(parse_expr(p)?) }
        } else { Some(parse_expr(p)?) }
    } else { None };
    p.expect_token(Token::RParen)?;
    p.expect_token(Token::Colon)?;
    p.skip_newlines();
    p.expect_token(Token::Indent)?;
    let (mut on_reset, mut on_tick) = (Vec::new(), Vec::new());
    while !p.check(Token::Dedent) && !p.is_at_end() {
        p.skip_newlines();
        if p.check(Token::Dedent) || p.is_at_end() { break; }
        p.expect_token(Token::KwOn)?;
        if p.check(Token::KwReset) { p.advance(); on_reset = p.parse_block(|p| parse_stmt(p))?; }
        else if p.check(Token::KwTick) { p.advance(); on_tick = p.parse_block(|p| parse_stmt(p))?; }
        else { return Err(p.error("expected 'reset' or 'tick' after 'on'")); }
        p.skip_newlines();
    }
    p.expect_token(Token::Dedent)?;
    Ok(Spanned::new(StmtKind::RegBlock(RegBlock { clock, reset, enable, on_reset, on_tick }), start.merge(p.prev_span())))
}

fn parse_priority_block(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwPriority)?;
    p.expect_token(Token::Colon)?;
    p.skip_newlines();
    p.expect_token(Token::Indent)?;
    let (mut arms, mut otherwise) = (Vec::new(), None);
    while !p.check(Token::Dedent) && !p.is_at_end() {
        p.skip_newlines();
        if p.check(Token::Dedent) || p.is_at_end() { break; }
        if p.eat(Token::KwOtherwise).is_some() {
            p.expect_token(Token::FatArrow)?;
            otherwise = Some(vec![parse_stmt(p)?]);
        } else {
            let as_ = p.peek_span();
            p.expect_token(Token::KwWhen)?;
            let cond = parse_expr(p)?;
            p.expect_token(Token::FatArrow)?;
            let stmt = parse_stmt(p)?;
            arms.push(PriorityArm { condition: cond, body: vec![stmt], span: as_.merge(p.prev_span()) });
        }
        p.skip_newlines();
    }
    p.expect_token(Token::Dedent)?;
    Ok(Spanned::new(StmtKind::PriorityBlock(PriorityBlock { arms, otherwise }), start.merge(p.prev_span())))
}

fn parse_parallel_block(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwParallel)?;
    let safe = if p.eat(Token::LParen).is_some() {
        let saved = p.pos;
        let val = if p.check_ident() {
            p.advance();
            if p.eat(Token::Eq).is_some() { parse_expr(p)? }
            else { p.pos = saved; parse_expr(p)? }
        } else { parse_expr(p)? };
        p.expect_token(Token::RParen)?;
        Some(val)
    } else { None };
    p.expect_token(Token::Colon)?;
    p.skip_newlines();
    p.expect_token(Token::Indent)?;
    let mut arms = Vec::new();
    while !p.check(Token::Dedent) && !p.is_at_end() {
        p.skip_newlines();
        if p.check(Token::Dedent) || p.is_at_end() { break; }
        let as_ = p.peek_span();
        p.expect_token(Token::KwWhen)?;
        let cond = parse_expr(p)?;
        p.expect_token(Token::FatArrow)?;
        let stmt = parse_stmt(p)?;
        arms.push(PriorityArm { condition: cond, body: vec![stmt], span: as_.merge(p.prev_span()) });
        p.skip_newlines();
    }
    p.expect_token(Token::Dedent)?;
    Ok(Spanned::new(StmtKind::ParallelBlock(ParallelBlock { safe, arms }), start.merge(p.prev_span())))
}

fn parse_assert_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwAssert)?;
    let always = p.eat(Token::KwAlways).is_some();
    let domain = if p.eat(Token::At).is_some() { Some(p.expect_ident()?) } else { None };
    p.expect_token(Token::Colon)?;
    let expr = parse_expr(p)?;
    let message = if p.eat(Token::Comma).is_some() { Some(parse_expr(p)?) } else { None };
    Ok(Spanned::new(StmtKind::Assert(AssertStmt { always, domain, expr, message }), start.merge(p.prev_span())))
}

fn parse_assume_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwAssume)?;
    let domain = if p.eat(Token::At).is_some() { Some(p.expect_ident()?) } else { None };
    p.expect_token(Token::Colon)?;
    let expr = parse_expr(p)?;
    let message = if p.eat(Token::Comma).is_some() { Some(parse_expr(p)?) } else { None };
    Ok(Spanned::new(StmtKind::Assume { domain, expr, message }, start.merge(p.prev_span())))
}

fn parse_cover_stmt(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwCover)?;
    let name = if p.check_ident() {
        let saved = p.pos;
        let t = p.advance();
        if p.check(Token::Colon) { Some(Spanned::new(p.text(t.span).to_string(), t.span)) }
        else { p.pos = saved; None }
    } else { None };
    p.expect_token(Token::Colon)?;
    let expr = parse_expr(p)?;
    Ok(Spanned::new(StmtKind::Cover { name, expr }, start.merge(p.prev_span())))
}

fn parse_static_assert(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwStaticAssert)?;
    let expr = parse_expr(p)?;
    p.expect_token(Token::Comma)?;
    let message = parse_expr(p)?;
    Ok(Spanned::new(StmtKind::StaticAssert { expr, message }, start.merge(p.prev_span())))
}

/// Parse `unchecked:` block or `unchecked(expr)` inline form.
fn parse_unchecked(p: &mut Parser<'_>) -> Result<Stmt, ParseError> {
    let start = p.peek_span();
    p.expect_token(Token::KwUnchecked)?;
    if p.check(Token::Colon) {
        let stmts = p.parse_block(|p| parse_stmt(p))?;
        Ok(Spanned::new(StmtKind::UncheckedBlock(stmts), start.merge(p.prev_span())))
    } else if p.eat(Token::LParen).is_some() {
        let inner = parse_expr(p)?;
        p.expect_token(Token::RParen)?;
        let span = start.merge(p.prev_span());
        let expr = Spanned::new(ExprKind::Unchecked(Box::new(inner)), span);
        Ok(Spanned::new(StmtKind::ExprStmt(expr), span))
    } else {
        Err(p.error("expected ':' or '(' after 'unchecked'"))
    }
}
