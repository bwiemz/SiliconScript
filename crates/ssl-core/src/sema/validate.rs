//! Hardware validation pass — enforces hardware-semantic constraints that are
//! beyond type-checking and name resolution.
//!
//! Checks performed:
//! 1. **Comb block completeness** — every signal assigned in a `comb` block
//!    must be assigned on *all* control-flow paths (no latch inference).
//! 2. **Reg block reset coverage** — every signal written in `on_tick` must
//!    also have a reset value in `on_reset`.
//! 3. **Output port driven** — every `out` port must be assigned somewhere in
//!    the module body.
//! 4. **Immutability** — `const` and `let` bindings cannot be reassigned.

use std::collections::HashSet;

use crate::ast::expr::ExprKind;
use crate::ast::item::{Item, ItemKind, ModuleDef, SourceFile};
use crate::ast::stmt::{RegBlock, Stmt, StmtKind};
use crate::ast::types::Direction;

use super::error::SemaError;
use super::resolve::ScopeMap;
use super::scope::{ScopeId, SymbolKind, SymbolTable};
use super::types::Ty;

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Third analysis pass: validates hardware-specific constraints using the
/// symbol table and scope map populated by earlier passes.
pub struct Validator<'a> {
    table: &'a SymbolTable,
    scope_map: &'a ScopeMap,
    errors: Vec<SemaError>,
    /// Current module scope for symbol lookups.
    current_scope: ScopeId,
}

impl<'a> Validator<'a> {
    /// Create a new validator rooted at the file (root) scope.
    pub fn new(table: &'a SymbolTable, scope_map: &'a ScopeMap) -> Self {
        Validator {
            table,
            scope_map,
            errors: Vec::new(),
            current_scope: table.root_scope(),
        }
    }

    /// Run hardware validation over an entire source file.
    pub fn validate_file(&mut self, file: &SourceFile) {
        for item in &file.items {
            self.validate_item(item);
        }
    }

    /// Consume the validator and return the accumulated errors.
    pub fn into_errors(self) -> Vec<SemaError> {
        self.errors
    }

    // -----------------------------------------------------------------------
    // Scope helpers
    // -----------------------------------------------------------------------

    fn enter_scope(&mut self, key: u32) -> ScopeId {
        let prev = self.current_scope;
        if let Some(&scope_id) = self.scope_map.get(&key) {
            self.current_scope = scope_id;
        }
        prev
    }

    fn restore_scope(&mut self, prev: ScopeId) {
        self.current_scope = prev;
    }

    // -----------------------------------------------------------------------
    // Item validation
    // -----------------------------------------------------------------------

    fn validate_item(&mut self, item: &Item) {
        if let ItemKind::Module(def) = &item.node {
            self.validate_module(def);
        }
    }

    fn validate_module(&mut self, def: &ModuleDef) {
        let prev = self.enter_scope(def.name.span.start);

        // Collect every signal name assigned anywhere in the module body.
        let mut module_assigned: HashSet<String> = HashSet::new();
        for body_item in &def.body {
            if let ItemKind::Stmt(stmt) = &body_item.node {
                collect_assigned_in_stmt(stmt, &mut module_assigned);
            }
        }

        // Check output ports are driven.
        for port in &def.ports {
            if port.direction == Direction::Out {
                let name = &port.name.node;
                if !module_assigned.contains(name) {
                    self.errors.push(SemaError::UnconnectedPort {
                        port: name.clone(),
                        // For module-level port checks the "instance" is the module itself.
                        inst: def.name.node.clone(),
                        span: port.span,
                    });
                }
            }
        }

        // Walk body statements for comb/reg checks and immutability.
        for body_item in &def.body {
            if let ItemKind::Stmt(stmt) = &body_item.node {
                self.validate_stmt(stmt);
            }
            // Recurse into nested modules.
            if let ItemKind::Module(inner) = &body_item.node {
                self.validate_module(inner);
            }
        }

        self.restore_scope(prev);
    }

    // -----------------------------------------------------------------------
    // Statement validation
    // -----------------------------------------------------------------------

    fn validate_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::CombBlock(stmts) => {
                self.validate_comb_block(stmts, stmt.span);
            }

            StmtKind::RegBlock(reg) => {
                self.validate_reg_block(reg);
            }

            StmtKind::Assign { target, .. } => {
                self.check_immutability(target);
            }

            StmtKind::If(if_stmt) => {
                for s in &if_stmt.then_body {
                    self.validate_stmt(s);
                }
                for (_, branch) in &if_stmt.elif_branches {
                    for s in branch {
                        self.validate_stmt(s);
                    }
                }
                if let Some(else_body) = &if_stmt.else_body {
                    for s in else_body {
                        self.validate_stmt(s);
                    }
                }
            }

            StmtKind::Match(match_stmt) => {
                for arm in &match_stmt.arms {
                    for s in &arm.body {
                        self.validate_stmt(s);
                    }
                }
            }

            StmtKind::For(for_stmt) => {
                for s in &for_stmt.body {
                    self.validate_stmt(s);
                }
            }

            StmtKind::PriorityBlock(pb) => {
                for arm in &pb.arms {
                    for s in &arm.body {
                        self.validate_stmt(s);
                    }
                }
                if let Some(otherwise) = &pb.otherwise {
                    for s in otherwise {
                        self.validate_stmt(s);
                    }
                }
            }

            StmtKind::ParallelBlock(pb) => {
                for arm in &pb.arms {
                    for s in &arm.body {
                        self.validate_stmt(s);
                    }
                }
            }

            StmtKind::UncheckedBlock(stmts) => {
                for s in stmts {
                    self.validate_stmt(s);
                }
            }

            // Declarations and other statements don't need hardware validation.
            StmtKind::Signal(_)
            | StmtKind::Let(_)
            | StmtKind::Const(_)
            | StmtKind::TypeAlias(_)
            | StmtKind::Assert(_)
            | StmtKind::Assume { .. }
            | StmtKind::Cover { .. }
            | StmtKind::StaticAssert { .. }
            | StmtKind::ExprStmt(_) => {}
        }
    }

    // -----------------------------------------------------------------------
    // Comb block completeness
    // -----------------------------------------------------------------------

    fn validate_comb_block(&mut self, stmts: &[Stmt], block_span: crate::span::Span) {
        // Step 1: collect every signal assigned anywhere in the block.
        let mut all_assigned = HashSet::new();
        for s in stmts {
            collect_assigned_in_stmt(s, &mut all_assigned);
        }

        // Step 2: find signals assigned on ALL paths.
        let definitely_assigned = assigned_on_all_paths(stmts);

        // Step 3: any signal in step 1 but not step 2 may infer a latch.
        for name in &all_assigned {
            if !definitely_assigned.contains(name) {
                // Skip signals with error types (don't cascade errors).
                if let Some(sym) = self.table.lookup(self.current_scope, name)
                    && sym.ty == Ty::Error
                {
                    continue;
                }
                self.errors.push(SemaError::LatchInferred {
                    signal: name.clone(),
                    span: block_span,
                });
            }
        }

        // Also recurse into nested statements for inner comb/reg blocks.
        for s in stmts {
            self.validate_stmt(s);
        }
    }

    // -----------------------------------------------------------------------
    // Reg block reset coverage
    // -----------------------------------------------------------------------

    fn validate_reg_block(&mut self, reg: &RegBlock) {
        let mut tick_signals = HashSet::new();
        for s in &reg.on_tick {
            collect_assigned_in_stmt(s, &mut tick_signals);
        }

        let mut reset_signals = HashSet::new();
        for s in &reg.on_reset {
            collect_assigned_in_stmt(s, &mut reset_signals);
        }

        for name in &tick_signals {
            if !reset_signals.contains(name) {
                // Skip error-typed signals to avoid cascades.
                if let Some(sym) = self.table.lookup(self.current_scope, name)
                    && sym.ty == Ty::Error
                {
                    continue;
                }
                // Find a representative span from the first tick assignment.
                let span = find_assign_span(&reg.on_tick, name)
                    .or_else(|| reg.on_tick.first().map(|s| s.span))
                    .unwrap_or(crate::span::Span::new(0, 0));
                self.errors.push(SemaError::Custom {
                    message: format!(
                        "signal `{name}` is assigned in `on tick` but has no reset value in `on reset`"
                    ),
                    span,
                });
            }
        }

        // Recurse into nested blocks inside the reg body.
        for s in &reg.on_reset {
            self.validate_stmt(s);
        }
        for s in &reg.on_tick {
            self.validate_stmt(s);
        }
    }

    // -----------------------------------------------------------------------
    // Immutability check
    // -----------------------------------------------------------------------

    fn check_immutability(&mut self, target: &crate::ast::expr::Expr) {
        if let ExprKind::Ident(name) = &target.node
            && let Some(sym) = self.table.lookup(self.current_scope, name)
            && matches!(sym.kind, SymbolKind::Const | SymbolKind::Let)
        {
            self.errors.push(SemaError::InvalidAssignTarget { span: target.span });
        }
    }
}

// ---------------------------------------------------------------------------
// Path analysis helpers (free functions)
// ---------------------------------------------------------------------------

/// Walk `stmts` and collect the names of all signals assigned anywhere,
/// including inside if/match branches (regardless of path coverage).
fn collect_assigned_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match &stmt.node {
        StmtKind::Assign { target, .. } => {
            if let ExprKind::Ident(name) = &target.node {
                out.insert(name.clone());
            }
        }

        StmtKind::If(if_stmt) => {
            for s in &if_stmt.then_body {
                collect_assigned_in_stmt(s, out);
            }
            for (_, branch) in &if_stmt.elif_branches {
                for s in branch {
                    collect_assigned_in_stmt(s, out);
                }
            }
            if let Some(else_body) = &if_stmt.else_body {
                for s in else_body {
                    collect_assigned_in_stmt(s, out);
                }
            }
        }

        StmtKind::Match(match_stmt) => {
            for arm in &match_stmt.arms {
                for s in &arm.body {
                    collect_assigned_in_stmt(s, out);
                }
            }
        }

        StmtKind::CombBlock(stmts)
        | StmtKind::UncheckedBlock(stmts) => {
            for s in stmts {
                collect_assigned_in_stmt(s, out);
            }
        }

        StmtKind::RegBlock(reg) => {
            for s in &reg.on_reset {
                collect_assigned_in_stmt(s, out);
            }
            for s in &reg.on_tick {
                collect_assigned_in_stmt(s, out);
            }
        }

        StmtKind::For(for_stmt) => {
            for s in &for_stmt.body {
                collect_assigned_in_stmt(s, out);
            }
        }

        StmtKind::PriorityBlock(pb) => {
            for arm in &pb.arms {
                for s in &arm.body {
                    collect_assigned_in_stmt(s, out);
                }
            }
            if let Some(otherwise) = &pb.otherwise {
                for s in otherwise {
                    collect_assigned_in_stmt(s, out);
                }
            }
        }

        StmtKind::ParallelBlock(pb) => {
            for arm in &pb.arms {
                for s in &arm.body {
                    collect_assigned_in_stmt(s, out);
                }
            }
        }

        // Declarations and non-assignment statements contribute nothing.
        StmtKind::Signal(_)
        | StmtKind::Let(_)
        | StmtKind::Const(_)
        | StmtKind::TypeAlias(_)
        | StmtKind::Assert(_)
        | StmtKind::Assume { .. }
        | StmtKind::Cover { .. }
        | StmtKind::StaticAssert { .. }
        | StmtKind::ExprStmt(_) => {}
    }
}

/// Compute the set of signal names that are **definitely** assigned on **all**
/// control-flow paths through `stmts`.
///
/// Algorithm:
/// - Walk statements linearly, accumulating a "definitely assigned" set.
/// - Direct assignments always contribute.
/// - `if` with an `else` contributes the intersection of all branches.
/// - `if` without `else` contributes nothing (the else path assigns nothing).
/// - `match` with a wildcard arm (`_`) contributes the intersection of all arms.
/// - `match` without a wildcard contributes nothing (not exhaustive).
/// - Other statements are skipped.
fn assigned_on_all_paths(stmts: &[Stmt]) -> HashSet<String> {
    let mut definitely: HashSet<String> = HashSet::new();

    for stmt in stmts {
        match &stmt.node {
            StmtKind::Assign { target, .. } => {
                if let ExprKind::Ident(name) = &target.node {
                    definitely.insert(name.clone());
                }
            }

            StmtKind::If(if_stmt) => {
                // Only contributes if there is an else branch.
                if let Some(else_body) = &if_stmt.else_body {
                    // Compute definitely-assigned sets for each branch.
                    let mut sets: Vec<HashSet<String>> = Vec::new();

                    // then branch
                    sets.push(assigned_on_all_paths(&if_stmt.then_body));

                    // elif branches
                    for (_, branch) in &if_stmt.elif_branches {
                        sets.push(assigned_on_all_paths(branch));
                    }

                    // else branch
                    sets.push(assigned_on_all_paths(else_body));

                    // Intersection of all branches.
                    if let Some(first) = sets.first() {
                        let intersection: HashSet<String> = first
                            .iter()
                            .filter(|name| sets.iter().all(|s| s.contains(*name)))
                            .cloned()
                            .collect();
                        for name in intersection {
                            definitely.insert(name);
                        }
                    }
                }
                // No else → contributes nothing to definitely-assigned.
            }

            StmtKind::Match(match_stmt) => {
                // Only contributes if there is a wildcard arm.
                if match_has_wildcard(match_stmt) {
                    let mut sets: Vec<HashSet<String>> = match_stmt
                        .arms
                        .iter()
                        .map(|arm| assigned_on_all_paths(&arm.body))
                        .collect();

                    if let Some(first) = sets.first() {
                        let intersection: HashSet<String> = first
                            .iter()
                            .filter(|name| sets.iter().all(|s| s.contains(*name)))
                            .cloned()
                            .collect();
                        for name in intersection {
                            definitely.insert(name);
                        }
                    }
                    // Suppress unused warning on `sets` — it is used above.
                    let _ = &mut sets;
                }
                // No wildcard → not exhaustive → contributes nothing.
            }

            // Nested comb/unchecked blocks: treat their sequential assignments
            // as contributing to the outer definitely-assigned set.
            StmtKind::CombBlock(inner) | StmtKind::UncheckedBlock(inner) => {
                let inner_set = assigned_on_all_paths(inner);
                for name in inner_set {
                    definitely.insert(name);
                }
            }

            // All other statement kinds contribute nothing.
            _ => {}
        }
    }

    definitely
}

/// Return `true` if any arm of the match statement is a wildcard (`_`).
fn match_has_wildcard(match_stmt: &crate::ast::stmt::MatchStmt) -> bool {
    match_stmt.arms.iter().any(|arm| {
        matches!(&arm.pattern.node, ExprKind::Ident(name) if name == "_")
    })
}

/// Find the source span of the first assignment to `name` in `stmts`.
fn find_assign_span(stmts: &[Stmt], name: &str) -> Option<crate::span::Span> {
    for stmt in stmts {
        if let StmtKind::Assign { target, .. } = &stmt.node
            && let ExprKind::Ident(n) = &target.node
            && n == name
        {
            return Some(stmt.span);
        }
    }
    None
}
