use std::collections::HashMap;

use crate::ast::item::{
    EnumDef, ExternModuleDef, FnDef, FsmDef, GenFor, GenIf, InstDecl,
    InterfaceDef, Item, ItemKind, ModuleDef, PipelineDef, SourceFile,
    StructDef, TestBlock,
};
use crate::ast::stmt::{ConstDecl, LetDecl, SignalDecl, Stmt, StmtKind, TypeAliasDecl};
use crate::ast::types::{Direction, GenericArg, TypeExpr, TypeExprKind};
use crate::span::Span;

use super::error::SemaError;
use super::eval::{ConstEval, ConstValue};
use super::scope::{ScopeId, ScopeKind, SymbolKind, SymbolTable};
use super::types::{EnumId, InterfaceId, StructId, Ty};

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

/// Name resolution + type resolution pass.
///
/// Walks the AST, registers all declarations into the `SymbolTable`, and
/// resolves AST `TypeExprKind` nodes to concrete `Ty` values.
pub struct Resolver {
    table: SymbolTable,
    errors: Vec<SemaError>,
    scope_stack: Vec<ScopeId>,
    /// Const values available for use as generic params (e.g. `UInt<W>`).
    const_values: HashMap<String, ConstValue>,
    /// Monotonically-increasing counters for user-defined type IDs.
    next_struct_id: u32,
    next_enum_id: u32,
    next_interface_id: u32,
}

impl Resolver {
    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Create a new resolver with an empty symbol table and no errors.
    pub fn new() -> Self {
        let table = SymbolTable::new();
        let root = table.root_scope();
        Resolver {
            table,
            errors: Vec::new(),
            scope_stack: vec![root],
            const_values: HashMap::new(),
            next_struct_id: 0,
            next_enum_id: 0,
            next_interface_id: 0,
        }
    }

    /// Walk the top-level AST and collect all declarations.
    pub fn collect_declarations(&mut self, file: &SourceFile) {
        for item in &file.items {
            self.resolve_item(item);
        }
    }

    /// Consume the resolver and return the completed symbol table and any errors.
    pub fn finish(self) -> (SymbolTable, Vec<SemaError>) {
        (self.table, self.errors)
    }

    // -----------------------------------------------------------------------
    // Scope helpers
    // -----------------------------------------------------------------------

    fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().expect("scope stack should never be empty")
    }

    fn push_scope(&mut self, kind: ScopeKind) -> ScopeId {
        let parent = self.current_scope();
        let child = self.table.push_scope(parent, kind);
        self.scope_stack.push(child);
        child
    }

    fn pop_scope(&mut self) {
        // Never pop the root scope.
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop();
        }
    }

    // -----------------------------------------------------------------------
    // Symbol definition helper
    // -----------------------------------------------------------------------

    /// Define a symbol in the current scope, recording any duplicate error.
    fn define(
        &mut self,
        name: &str,
        kind: SymbolKind,
        ty: Ty,
        span: Span,
    ) -> Option<super::scope::SymbolId> {
        let scope = self.current_scope();
        match self.table.define(scope, name, kind, ty, span) {
            Ok(id) => Some(id),
            Err(e) => {
                self.errors.push(e);
                None
            }
        }
    }

    // -----------------------------------------------------------------------
    // Item resolution
    // -----------------------------------------------------------------------

    fn resolve_item(&mut self, item: &Item) {
        match &item.node {
            ItemKind::Module(def) => self.resolve_module(def),
            ItemKind::Struct(def) => self.resolve_struct(def),
            ItemKind::Enum(def) => self.resolve_enum(def),
            ItemKind::Interface(def) => self.resolve_interface(def),
            ItemKind::FnDef(def) => self.resolve_fn(def),
            ItemKind::Fsm(def) => self.resolve_fsm(def),
            ItemKind::Pipeline(def) => self.resolve_pipeline(def),
            ItemKind::Test(block) => self.resolve_test(block),
            ItemKind::ExternModule(def) => self.resolve_extern_module(def),
            // Deferred / skip:
            ItemKind::Import(_) => {}
            ItemKind::Inst(decl) => self.resolve_inst(decl),
            ItemKind::GenFor(gf) => self.resolve_gen_for(gf),
            ItemKind::GenIf(gi) => self.resolve_gen_if(gi),
            ItemKind::Stmt(stmt) => self.resolve_stmt(stmt),
        }
    }

    fn resolve_module(&mut self, def: &ModuleDef) {
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Module, Ty::Void, span);

        self.push_scope(ScopeKind::Module);

        // Register ports.
        for port in &def.ports {
            let port_ty = self.resolve_type(&port.ty);
            let port_name = &port.name.node;
            let port_span = port.name.span;
            if let Some(id) = self.define(port_name, SymbolKind::Port, port_ty, port_span) {
                // Attach direction to the symbol.
                let sym = self.table.get_symbol_mut(id);
                sym.direction = Some(port.direction);
            }
        }

        // Walk body items.
        for item in &def.body {
            self.resolve_item(item);
        }

        self.pop_scope();
    }

    fn resolve_struct(&mut self, def: &StructDef) {
        let id = StructId(self.next_struct_id);
        self.next_struct_id += 1;
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Struct, Ty::Struct(id), span);
    }

    fn resolve_enum(&mut self, def: &EnumDef) {
        let id = EnumId(self.next_enum_id);
        self.next_enum_id += 1;
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Enum, Ty::Enum(id), span);

        // Register variants in the enclosing scope (they are file/module-level names).
        for variant in &def.variants {
            let vname = &variant.name.node;
            let vspan = variant.name.span;
            self.define(vname, SymbolKind::EnumVariant, Ty::Enum(id), vspan);
        }
    }

    fn resolve_interface(&mut self, def: &InterfaceDef) {
        let id = InterfaceId(self.next_interface_id);
        self.next_interface_id += 1;
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Interface, Ty::Interface(id), span);
    }

    fn resolve_fn(&mut self, def: &FnDef) {
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Fn, Ty::Void, span);

        self.push_scope(ScopeKind::Function);

        for param in &def.params {
            let param_ty = self.resolve_type(&param.ty);
            let pname = &param.name.node;
            let pspan = param.name.span;
            self.define(pname, SymbolKind::GenericParam, param_ty, pspan);
        }

        for stmt in &def.body {
            self.resolve_stmt(stmt);
        }

        self.pop_scope();
    }

    fn resolve_fsm(&mut self, def: &FsmDef) {
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Fsm, Ty::Void, span);
        // FSM body analysis is deferred.
    }

    fn resolve_pipeline(&mut self, def: &PipelineDef) {
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Pipeline, Ty::Void, span);
        // Pipeline stage analysis is deferred.
    }

    fn resolve_test(&mut self, block: &TestBlock) {
        // Test blocks are not named in the symbol table; body is deferred.
        let _ = block;
    }

    fn resolve_extern_module(&mut self, def: &ExternModuleDef) {
        let name = &def.name.node;
        let span = def.name.span;
        self.define(name, SymbolKind::Module, Ty::Void, span);

        self.push_scope(ScopeKind::Module);
        for port in &def.ports {
            let port_ty = self.resolve_type(&port.ty);
            let port_name = &port.name.node;
            let port_span = port.name.span;
            if let Some(id) = self.define(port_name, SymbolKind::Port, port_ty, port_span) {
                let sym = self.table.get_symbol_mut(id);
                sym.direction = Some(port.direction);
            }
        }
        self.pop_scope();
    }

    fn resolve_inst(&mut self, _decl: &InstDecl) {
        // Module instantiation checking is deferred to a later pass.
    }

    fn resolve_gen_for(&mut self, gf: &GenFor) {
        // Walk body items for declaration collection; full elaboration deferred.
        for item in &gf.body {
            self.resolve_item(item);
        }
    }

    fn resolve_gen_if(&mut self, gi: &GenIf) {
        for item in &gi.then_body {
            self.resolve_item(item);
        }
        if let Some(else_body) = &gi.else_body {
            for item in else_body {
                self.resolve_item(item);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Statement resolution
    // -----------------------------------------------------------------------

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::Signal(decl) => self.resolve_signal(decl),
            StmtKind::Const(decl) => self.resolve_const(decl),
            StmtKind::Let(decl) => self.resolve_let(decl),
            StmtKind::TypeAlias(decl) => self.resolve_type_alias(decl),

            // Control-flow: recurse to find nested declarations.
            StmtKind::If(if_stmt) => {
                for s in &if_stmt.then_body {
                    self.resolve_stmt(s);
                }
                for (_, branch) in &if_stmt.elif_branches {
                    for s in branch {
                        self.resolve_stmt(s);
                    }
                }
                if let Some(else_body) = &if_stmt.else_body {
                    for s in else_body {
                        self.resolve_stmt(s);
                    }
                }
            }

            StmtKind::Match(match_stmt) => {
                for arm in &match_stmt.arms {
                    for s in &arm.body {
                        self.resolve_stmt(s);
                    }
                }
            }

            StmtKind::For(for_stmt) => {
                // Loop variable is a LoopVar; body declarations are scoped.
                self.push_scope(ScopeKind::Block);
                let var_name = &for_stmt.var.node;
                let var_span = for_stmt.var.span;
                self.define(var_name, SymbolKind::LoopVar, Ty::MetaUInt, var_span);
                for s in &for_stmt.body {
                    self.resolve_stmt(s);
                }
                self.pop_scope();
            }

            StmtKind::CombBlock(stmts) => {
                for s in stmts {
                    self.resolve_stmt(s);
                }
            }

            StmtKind::RegBlock(reg) => {
                for s in &reg.on_reset {
                    self.resolve_stmt(s);
                }
                for s in &reg.on_tick {
                    self.resolve_stmt(s);
                }
            }

            StmtKind::PriorityBlock(pb) => {
                for arm in &pb.arms {
                    for s in &arm.body {
                        self.resolve_stmt(s);
                    }
                }
                if let Some(otherwise) = &pb.otherwise {
                    for s in otherwise {
                        self.resolve_stmt(s);
                    }
                }
            }

            StmtKind::ParallelBlock(pb) => {
                for arm in &pb.arms {
                    for s in &arm.body {
                        self.resolve_stmt(s);
                    }
                }
            }

            StmtKind::UncheckedBlock(stmts) => {
                for s in stmts {
                    self.resolve_stmt(s);
                }
            }

            // These don't introduce new declarations.
            StmtKind::Assign { .. }
            | StmtKind::Assert(_)
            | StmtKind::Assume { .. }
            | StmtKind::Cover { .. }
            | StmtKind::StaticAssert { .. }
            | StmtKind::ExprStmt(_) => {}
        }
    }

    fn resolve_signal(&mut self, decl: &SignalDecl) {
        let ty = self.resolve_type(&decl.ty);
        let name = &decl.name.node;
        let span = decl.name.span;
        self.define(name, SymbolKind::Signal, ty, span);
    }

    fn resolve_const(&mut self, decl: &ConstDecl) {
        // Resolve the declared type (if present); otherwise use MetaUInt.
        let ty = decl
            .ty
            .as_ref()
            .map(|t| self.resolve_type(t))
            .unwrap_or(Ty::MetaUInt);

        let name = &decl.name.node;
        let span = decl.name.span;
        self.define(name, SymbolKind::Const, ty, span);

        // Try to evaluate the const so it can be used as a generic param.
        let evaluator = ConstEval::with_bindings(self.const_values.clone());
        match evaluator.eval_expr(&decl.value) {
            Ok(val) => {
                self.const_values.insert(name.clone(), val);
            }
            Err(_) => {
                // Non-constant initialiser — we can't use it in type params,
                // but that is not an error at this stage.
            }
        }
    }

    fn resolve_let(&mut self, decl: &LetDecl) {
        let ty = decl
            .ty
            .as_ref()
            .map(|t| self.resolve_type(t))
            .unwrap_or(Ty::Void);
        let name = &decl.name.node;
        let span = decl.name.span;
        self.define(name, SymbolKind::Let, ty, span);
    }

    fn resolve_type_alias(&mut self, decl: &TypeAliasDecl) {
        // Resolve the target type so we can catch undefined-type errors early.
        let ty = self.resolve_type(&decl.ty);
        let name = &decl.name.node;
        let span = decl.name.span;
        self.define(name, SymbolKind::TypeAlias, ty, span);
    }

    // -----------------------------------------------------------------------
    // Type expression resolution
    // -----------------------------------------------------------------------

    /// Convert an AST `TypeExpr` to a resolved `Ty`.
    ///
    /// Unknown / unresolvable types produce `Ty::Error` and push a
    /// `SemaError` onto `self.errors`.
    pub fn resolve_type(&mut self, ty: &TypeExpr) -> Ty {
        let span = ty.span;
        match &ty.node {
            // ── Named types ───────────────────────────────────────────────
            TypeExprKind::Named(name) => self.resolve_named_type(name, span),

            // ── Generic types ─────────────────────────────────────────────
            TypeExprKind::Generic { name, params } => {
                self.resolve_generic_type(name, params, span)
            }

            // ── Array ─────────────────────────────────────────────────────
            TypeExprKind::Array { element, size } => {
                let elem_ty = self.resolve_type(element);
                let evaluator = ConstEval::with_bindings(self.const_values.clone());
                match evaluator.eval_expr(size) {
                    Ok(ConstValue::UInt(n)) => Ty::Array {
                        element: Box::new(elem_ty),
                        size: n as u64,
                    },
                    Ok(other) => {
                        self.errors.push(SemaError::ConstEvalError {
                            message: format!(
                                "array size must be a non-negative integer, got `{other:?}`"
                            ),
                            span,
                        });
                        Ty::Error
                    }
                    Err(e) => {
                        self.errors.push(e);
                        Ty::Error
                    }
                }
            }

            // ── Clock ─────────────────────────────────────────────────────
            TypeExprKind::Clock { freq, .. } => {
                let freq_val = freq.as_ref().and_then(|f| {
                    let ev = ConstEval::with_bindings(self.const_values.clone());
                    match ev.eval_expr(f) {
                        Ok(ConstValue::UInt(n)) => Some(n as u64),
                        _ => None,
                    }
                });
                Ty::Clock { freq: freq_val }
            }

            // ── Reset types ───────────────────────────────────────────────
            TypeExprKind::SyncReset { .. } => Ty::SyncReset,
            TypeExprKind::AsyncReset { .. } => Ty::AsyncReset,

            // ── Direction wrappers ────────────────────────────────────────
            TypeExprKind::DirectionWrapper { dir, inner } => {
                let inner_ty = self.resolve_type(inner);
                match dir {
                    Direction::In => Ty::In(Box::new(inner_ty)),
                    Direction::Out => Ty::Out(Box::new(inner_ty)),
                    Direction::InOut => Ty::InOut(Box::new(inner_ty)),
                }
            }

            TypeExprKind::Flip(inner) => {
                let inner_ty = self.resolve_type(inner);
                Ty::Flip(Box::new(inner_ty))
            }

            // ── Domain-annotated ──────────────────────────────────────────
            TypeExprKind::DomainAnnotated { ty: inner, .. } => {
                // Domain tracking is deferred; just resolve the inner type.
                self.resolve_type(inner)
            }

            // ── Memory ────────────────────────────────────────────────────
            TypeExprKind::Memory { element, params } => {
                let elem_ty = self.resolve_type(element);
                let depth = self.eval_named_param(params, "depth", span);
                Ty::Memory {
                    element: Box::new(elem_ty),
                    depth,
                }
            }

            TypeExprKind::DualPortMemory { element, params } => {
                let elem_ty = self.resolve_type(element);
                let depth = self.eval_named_param(params, "depth", span);
                Ty::Memory {
                    element: Box::new(elem_ty),
                    depth,
                }
            }

            // ── Partial interface ─────────────────────────────────────────
            TypeExprKind::PartialInterface { .. } => {
                // Deferred — full interface group resolution is a later pass.
                Ty::Error
            }
        }
    }

    // -----------------------------------------------------------------------
    // Type resolution helpers
    // -----------------------------------------------------------------------

    fn resolve_named_type(&mut self, name: &str, span: Span) -> Ty {
        match name {
            // Hardware primitive named types
            "Bool" => Ty::Bool,
            "Clock" => Ty::Clock { freq: None },
            "SyncReset" => Ty::SyncReset,
            "AsyncReset" => Ty::AsyncReset,

            // Lowercase meta / compile-time-only primitive types
            // (used in const and generic parameter declarations)
            "uint" => Ty::MetaUInt,
            "int" => Ty::MetaInt,
            "bool" => Ty::MetaBool,
            "float" => Ty::MetaFloat,
            "string" => Ty::MetaString,

            // UInt/SInt/Bits/Fixed without params are invalid — report an error.
            "UInt" | "SInt" | "Bits" | "Fixed" => {
                self.errors.push(SemaError::Custom {
                    message: format!(
                        "`{name}` requires a generic parameter (e.g. `{name}<8>`)"
                    ),
                    span,
                });
                Ty::Error
            }
            other => {
                // Look up in scope chain.
                let scope = self.current_scope();
                match self.table.lookup(scope, other) {
                    Some(sym) => sym.ty.clone(),
                    None => {
                        self.errors.push(SemaError::UndefinedName {
                            name: other.to_owned(),
                            span,
                        });
                        Ty::Error
                    }
                }
            }
        }
    }

    fn resolve_generic_type(
        &mut self,
        name: &str,
        params: &[GenericArg],
        span: Span,
    ) -> Ty {
        match name {
            "UInt" => {
                let n = self.eval_first_uint_param(params, name, span);
                Ty::UInt(n)
            }
            "SInt" => {
                let n = self.eval_first_uint_param(params, name, span);
                Ty::SInt(n)
            }
            "Bits" => {
                let n = self.eval_first_uint_param(params, name, span);
                Ty::Bits(n)
            }
            "Fixed" => {
                let int_bits = self.eval_param_at(params, 0, name, span);
                let frac_bits = self.eval_param_at(params, 1, name, span);
                Ty::Fixed { int_bits, frac_bits }
            }
            "Clock" => {
                let freq = if params.is_empty() {
                    None
                } else {
                    Some(self.eval_first_uint_param(params, name, span))
                };
                Ty::Clock { freq }
            }
            other => {
                // User-defined generic type — look up in scope.
                let scope = self.current_scope();
                match self.table.lookup(scope, other) {
                    Some(sym) => sym.ty.clone(),
                    None => {
                        self.errors.push(SemaError::UndefinedName {
                            name: other.to_owned(),
                            span,
                        });
                        Ty::Error
                    }
                }
            }
        }
    }

    /// Evaluate the first generic argument as a `u64` width.
    /// Returns 0 and pushes an error if evaluation fails.
    fn eval_first_uint_param(&mut self, params: &[GenericArg], type_name: &str, span: Span) -> u64 {
        self.eval_param_at(params, 0, type_name, span)
    }

    /// Evaluate the generic argument at position `idx` as a `u64`.
    fn eval_param_at(
        &mut self,
        params: &[GenericArg],
        idx: usize,
        type_name: &str,
        span: Span,
    ) -> u64 {
        let Some(arg) = params.get(idx) else {
            self.errors.push(SemaError::Custom {
                message: format!(
                    "`{type_name}` requires at least {} generic parameter(s)",
                    idx + 1
                ),
                span,
            });
            return 0;
        };

        match arg {
            GenericArg::Expr(expr) => {
                let ev = ConstEval::with_bindings(self.const_values.clone());
                match ev.eval_expr(expr) {
                    Ok(ConstValue::UInt(n)) => n as u64,
                    Ok(other) => {
                        self.errors.push(SemaError::ConstEvalError {
                            message: format!(
                                "generic parameter for `{type_name}` must be a non-negative integer, got `{other:?}`"
                            ),
                            span,
                        });
                        0
                    }
                    Err(e) => {
                        self.errors.push(e);
                        0
                    }
                }
            }
            GenericArg::Type(inner) => {
                // The parser treats uppercase single-word args as type args, not
                // expression args (see `is_type_start`). This handles the case
                // where a const name like `W` is written in an all-caps style:
                // `UInt<W>` — `W` is parsed as `GenericArg::Type(Named("W"))`.
                // Try to resolve it as a const value before reporting an error.
                if let TypeExprKind::Named(const_name) = &inner.node {
                    if let Some(ConstValue::UInt(n)) = self.const_values.get(const_name).cloned() {
                        return n as u64;
                    }
                    // Also try the symbol table for a const binding.
                    let scope = self.current_scope();
                    if let Some(sym) = self.table.lookup(scope, const_name)
                        && matches!(sym.kind, SymbolKind::Const | SymbolKind::GenericParam)
                    {
                        // The const exists but its value isn't yet in const_values
                        // (e.g., its value couldn't be evaluated). Return 0 without
                        // an error to avoid false positives.
                        return 0;
                    }
                }
                // A genuine type argument where we expected a width.
                let _ = self.resolve_type(inner);
                self.errors.push(SemaError::Custom {
                    message: format!(
                        "expected a numeric width for `{type_name}`, got a type argument"
                    ),
                    span,
                });
                0
            }
        }
    }

    /// Evaluate a named `CallArg` from a memory param list.
    /// Returns `0` if the argument is absent or cannot be evaluated.
    fn eval_named_param(
        &mut self,
        params: &[crate::ast::expr::CallArg],
        param_name: &str,
        span: Span,
    ) -> u64 {
        for arg in params {
            if arg.name.as_ref().map(|n| n.node.as_str()) == Some(param_name) {
                let ev = ConstEval::with_bindings(self.const_values.clone());
                match ev.eval_expr(&arg.value) {
                    Ok(ConstValue::UInt(n)) => return n as u64,
                    Ok(other) => {
                        self.errors.push(SemaError::ConstEvalError {
                            message: format!(
                                "memory param `{param_name}` must be a non-negative integer, got `{other:?}`"
                            ),
                            span,
                        });
                        return 0;
                    }
                    Err(e) => {
                        self.errors.push(e);
                        return 0;
                    }
                }
            }
        }
        // Param not found — could be positional; return 0 as sentinel.
        0
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}
