use crate::ast::expr::{BinOp, Expr, ExprKind, UnaryOp};
use crate::ast::item::{Item, ItemKind, ModuleDef, SourceFile};
use crate::ast::stmt::{RegBlock, Stmt, StmtKind};
use crate::ast::types::Direction;
use crate::lexer::NumericLiteral;
use crate::span::Span;

use super::error::SemaError;
use super::scope::{ScopeId, SymbolKind, SymbolTable};
use super::types::Ty;
use super::resolve::ScopeMap;

// ---------------------------------------------------------------------------
// TypeChecker
// ---------------------------------------------------------------------------

/// Second analysis pass: type-checks expressions and statements using the
/// symbol table populated by the `Resolver`.
pub struct TypeChecker<'a> {
    table: &'a SymbolTable,
    scope_map: &'a ScopeMap,
    errors: Vec<SemaError>,
    /// Current scope for identifier lookups.
    current_scope: ScopeId,
}

impl<'a> TypeChecker<'a> {
    /// Create a new checker rooted at the file (root) scope.
    pub fn new(table: &'a SymbolTable, scope_map: &'a ScopeMap) -> Self {
        TypeChecker {
            table,
            scope_map,
            errors: Vec::new(),
            current_scope: table.root_scope(),
        }
    }

    /// Run type checking over an entire source file.
    pub fn check_file(&mut self, file: &SourceFile) {
        for item in &file.items {
            self.check_item(item);
        }
    }

    /// Consume the checker and return the accumulated errors.
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
    // Item checking
    // -----------------------------------------------------------------------

    fn check_item(&mut self, item: &Item) {
        // Structs/enums/interfaces are already validated by the resolver.
        // Other items are deferred for now.
        if let ItemKind::Module(def) = &item.node {
            self.check_module(def);
        }
    }

    fn check_module(&mut self, def: &ModuleDef) {
        let prev = self.enter_scope(def.name.span.start);

        for body_item in &def.body {
            self.check_item(body_item);
            // Statements embedded as items (e.g. comb/reg blocks at module level)
            if let ItemKind::Stmt(stmt) = &body_item.node {
                self.check_stmt(stmt);
            }
        }

        self.restore_scope(prev);
    }

    // -----------------------------------------------------------------------
    // Statement checking
    // -----------------------------------------------------------------------

    fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::Assign { target, value } => {
                self.check_assignment(target, value, stmt.span);
            }

            StmtKind::CombBlock(stmts) => {
                for s in stmts {
                    self.check_stmt(s);
                }
            }

            StmtKind::RegBlock(reg) => {
                self.check_reg_block(reg);
            }

            StmtKind::If(if_stmt) => {
                let cond_ty = self.check_expr(&if_stmt.condition);
                if !cond_ty.is_error() && cond_ty != Ty::Bool {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Bool".to_owned(),
                        found: cond_ty.to_string(),
                        span: if_stmt.condition.span,
                    });
                }
                for s in &if_stmt.then_body {
                    self.check_stmt(s);
                }
                for (elif_cond, elif_body) in &if_stmt.elif_branches {
                    let elif_ty = self.check_expr(elif_cond);
                    if !elif_ty.is_error() && elif_ty != Ty::Bool {
                        self.errors.push(SemaError::TypeMismatch {
                            expected: "Bool".to_owned(),
                            found: elif_ty.to_string(),
                            span: elif_cond.span,
                        });
                    }
                    for s in elif_body {
                        self.check_stmt(s);
                    }
                }
                if let Some(else_body) = &if_stmt.else_body {
                    for s in else_body {
                        self.check_stmt(s);
                    }
                }
            }

            StmtKind::Match(match_stmt) => {
                let _ = self.check_expr(&match_stmt.scrutinee);
                for arm in &match_stmt.arms {
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
            }

            StmtKind::For(for_stmt) => {
                let _ = self.check_expr(&for_stmt.iterable);
                for s in &for_stmt.body {
                    self.check_stmt(s);
                }
            }

            StmtKind::PriorityBlock(pb) => {
                for arm in &pb.arms {
                    let cond_ty = self.check_expr(&arm.condition);
                    if !cond_ty.is_error() && cond_ty != Ty::Bool {
                        self.errors.push(SemaError::TypeMismatch {
                            expected: "Bool".to_owned(),
                            found: cond_ty.to_string(),
                            span: arm.condition.span,
                        });
                    }
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
                if let Some(otherwise) = &pb.otherwise {
                    for s in otherwise {
                        self.check_stmt(s);
                    }
                }
            }

            StmtKind::ParallelBlock(pb) => {
                for arm in &pb.arms {
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
            }

            StmtKind::UncheckedBlock(stmts) => {
                for s in stmts {
                    self.check_stmt(s);
                }
            }

            StmtKind::Assert(a) => {
                let _ = self.check_expr(&a.expr);
            }

            StmtKind::Assume { expr, .. } => {
                let _ = self.check_expr(expr);
            }

            StmtKind::Cover { expr, .. } => {
                let _ = self.check_expr(expr);
            }

            StmtKind::StaticAssert { expr, .. } => {
                let _ = self.check_expr(expr);
            }

            StmtKind::ExprStmt(expr) => {
                let _ = self.check_expr(expr);
            }

            // Declarations are already handled by the resolver.
            StmtKind::Signal(_)
            | StmtKind::Let(_)
            | StmtKind::Const(_)
            | StmtKind::TypeAlias(_) => {}
        }
    }

    fn check_reg_block(&mut self, reg: &RegBlock) {
        // Check that the clock argument has type Clock.
        let clk_ty = self.check_expr(&reg.clock);
        if !clk_ty.is_error() && !matches!(clk_ty, Ty::Clock { .. }) {
            self.errors.push(SemaError::TypeMismatch {
                expected: "Clock".to_owned(),
                found: clk_ty.to_string(),
                span: reg.clock.span,
            });
        }

        // Check that the reset argument is SyncReset or AsyncReset.
        let rst_ty = self.check_expr(&reg.reset);
        if !rst_ty.is_error()
            && !matches!(rst_ty, Ty::SyncReset | Ty::AsyncReset)
        {
            self.errors.push(SemaError::TypeMismatch {
                expected: "SyncReset or AsyncReset".to_owned(),
                found: rst_ty.to_string(),
                span: reg.reset.span,
            });
        }

        if let Some(en) = &reg.enable {
            let _ = self.check_expr(en);
        }

        for s in &reg.on_reset {
            self.check_stmt(s);
        }
        for s in &reg.on_tick {
            self.check_stmt(s);
        }
    }

    // -----------------------------------------------------------------------
    // Assignment checking
    // -----------------------------------------------------------------------

    fn check_assignment(&mut self, target: &Expr, value: &Expr, _stmt_span: Span) {
        // Determine the target type and check for direction violations.
        let target_ty = self.resolve_target_type(target);

        // Check direction violation: cannot assign to an input port.
        self.check_direction_violation(target);

        // Type-check the value expression.
        let value_ty = self.check_expr(value);

        // Skip further checks if either side is an error.
        if target_ty.is_error() || value_ty.is_error() {
            return;
        }

        self.check_type_compatibility(&target_ty, &value_ty, value.span);
    }

    /// Resolve the type of an assignment target (lvalue).
    fn resolve_target_type(&mut self, target: &Expr) -> Ty {
        match &target.node {
            ExprKind::Ident(name) => {
                match self.table.lookup(self.current_scope, name) {
                    Some(sym) => sym.ty.clone(),
                    None => {
                        // Undefined targets are already reported by the resolver;
                        // return Error to suppress cascade.
                        Ty::Error
                    }
                }
            }
            ExprKind::FieldAccess { object, .. } => {
                // Deferred — return the object type for now.
                self.resolve_target_type(object)
            }
            ExprKind::Index { array, .. } => {
                let arr_ty = self.resolve_target_type(array);
                match &arr_ty {
                    Ty::Array { element, .. } => *element.clone(),
                    Ty::UInt(_) | Ty::SInt(_) | Ty::Bits(_) => Ty::Bool,
                    _ => Ty::Error,
                }
            }
            ExprKind::BitSlice { value, high, low } => {
                // Try to compute concrete width from const high/low.
                let width = self.try_bitslice_width(high, low);
                let _ = self.resolve_target_type(value);
                Ty::Bits(width)
            }
            _ => {
                self.errors.push(SemaError::InvalidAssignTarget { span: target.span });
                Ty::Error
            }
        }
    }

    /// Check that an assignment target is not an input port.
    fn check_direction_violation(&mut self, target: &Expr) {
        if let ExprKind::Ident(name) = &target.node
            && let Some(sym) = self.table.lookup(self.current_scope, name)
            && sym.kind == SymbolKind::Port
            && sym.direction == Some(Direction::In)
        {
            self.errors.push(SemaError::DirectionViolation {
                message: format!("cannot assign to input port `{name}`"),
                span: target.span,
            });
        }
    }

    /// Check that `value_ty` is compatible with `target_ty`.
    /// Emits TypeMismatch or WidthMismatch errors as appropriate.
    fn check_type_compatibility(&mut self, target_ty: &Ty, value_ty: &Ty, span: Span) {
        // UInt(0) is the sentinel for an unsized literal — always compatible.
        if *value_ty == Ty::UInt(0) || *value_ty == Ty::MetaUInt || *value_ty == Ty::MetaInt {
            return;
        }

        // Strip direction wrappers before comparison.
        let target_inner = target_ty.unwrap_direction();
        let value_inner = value_ty.unwrap_direction();

        match (target_inner, value_inner) {
            (Ty::UInt(tw), Ty::UInt(vw)) => {
                if *vw > *tw && *vw != 0 {
                    self.errors.push(SemaError::WidthMismatch {
                        expected: *tw,
                        found: *vw,
                        span,
                    });
                }
            }
            (Ty::SInt(tw), Ty::SInt(vw)) => {
                if *vw > *tw && *vw != 0 {
                    self.errors.push(SemaError::WidthMismatch {
                        expected: *tw,
                        found: *vw,
                        span,
                    });
                }
            }
            (Ty::Bits(tw), Ty::Bits(vw)) => {
                if *vw != *tw && *vw != 0 {
                    self.errors.push(SemaError::WidthMismatch {
                        expected: *tw,
                        found: *vw,
                        span,
                    });
                }
            }
            (Ty::Bool, Ty::Bool) => {}
            (Ty::Clock { .. }, Ty::Clock { .. }) => {}
            (Ty::SyncReset, Ty::SyncReset) => {}
            (Ty::AsyncReset, Ty::AsyncReset) => {}
            // UInt/SInt mismatch
            (Ty::UInt(_), Ty::SInt(_)) | (Ty::SInt(_), Ty::UInt(_)) => {
                self.errors.push(SemaError::TypeMismatch {
                    expected: target_inner.to_string(),
                    found: value_inner.to_string(),
                    span,
                });
            }
            // Bool into UInt/SInt or vice versa
            (Ty::Bool, other) | (other, Ty::Bool) if !matches!(other, Ty::Bool) => {
                self.errors.push(SemaError::TypeMismatch {
                    expected: target_inner.to_string(),
                    found: value_inner.to_string(),
                    span,
                });
            }
            // User-defined and compound types: accept without deep checking for now.
            (Ty::Struct(_), Ty::Struct(_))
            | (Ty::Enum(_), Ty::Enum(_))
            | (Ty::Array { .. }, Ty::Array { .. }) => {}
            // Everything else that isn't Error is a mismatch.
            _ => {
                self.errors.push(SemaError::TypeMismatch {
                    expected: target_inner.to_string(),
                    found: value_inner.to_string(),
                    span,
                });
            }
        }
    }

    // -----------------------------------------------------------------------
    // Expression type inference
    // -----------------------------------------------------------------------

    /// Infer the type of an expression.  Returns `Ty::Error` on any error
    /// (but the error is also pushed onto `self.errors`).
    pub fn check_expr(&mut self, expr: &Expr) -> Ty {
        match &expr.node {
            ExprKind::BoolLiteral(_) => Ty::Bool,

            ExprKind::IntLiteral(lit) => {
                // Return UInt(0) as the "unsized integer" sentinel.
                // For sized literals we know the width.
                match lit {
                    NumericLiteral::Sized { width, .. } => Ty::UInt(*width as u64),
                    _ => Ty::UInt(0),
                }
            }

            ExprKind::StringLiteral(_) => Ty::MetaString,

            ExprKind::Ident(name) => {
                match self.table.lookup(self.current_scope, name) {
                    Some(sym) => sym.ty.clone(),
                    None => {
                        // Already reported by the resolver — suppress cascade.
                        Ty::Error
                    }
                }
            }

            ExprKind::Paren(inner) => self.check_expr(inner),

            ExprKind::Binary { op, lhs, rhs } => {
                self.check_binary(*op, lhs, rhs, expr.span)
            }

            ExprKind::Unary { op, operand } => {
                self.check_unary(*op, operand, expr.span)
            }

            ExprKind::IfExpr { condition, then_expr, else_expr } => {
                let cond_ty = self.check_expr(condition);
                if !cond_ty.is_error() && cond_ty != Ty::Bool {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Bool".to_owned(),
                        found: cond_ty.to_string(),
                        span: condition.span,
                    });
                }
                let then_ty = self.check_expr(then_expr);
                let else_ty = self.check_expr(else_expr);
                if then_ty.is_error() {
                    else_ty
                } else if else_ty.is_error() || then_ty == else_ty {
                    // Either branch errored or both branches have the same type.
                    then_ty
                } else {
                    // Mismatch — report and return first branch type.
                    self.errors.push(SemaError::TypeMismatch {
                        expected: then_ty.to_string(),
                        found: else_ty.to_string(),
                        span: else_expr.span,
                    });
                    then_ty
                }
            }

            ExprKind::Index { array, index: _ } => {
                let arr_ty = self.check_expr(array);
                match &arr_ty {
                    Ty::Array { element, .. } => *element.clone(),
                    Ty::Error => Ty::Error,
                    // Bit-indexing into UInt/SInt/Bits returns a single bit (Bool).
                    Ty::UInt(_) | Ty::SInt(_) | Ty::Bits(_) => Ty::Bool,
                    _ => {
                        self.errors.push(SemaError::TypeMismatch {
                            expected: "array or bit-vector".to_owned(),
                            found: arr_ty.to_string(),
                            span: array.span,
                        });
                        Ty::Error
                    }
                }
            }

            ExprKind::BitSlice { value, high, low } => {
                let _ = self.check_expr(value);
                let width = self.try_bitslice_width(high, low);
                Ty::Bits(width)
            }

            // Field access / calls / pipe — deferred.
            ExprKind::FieldAccess { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Call { .. }
            | ExprKind::Pipe { .. } => Ty::Error,

            ExprKind::TypeCast { ty, .. } => {
                // Type cast: the result type is the target type.
                // We don't have a resolve_type available here, so return Error
                // for now (casts are validated in a later pass).
                let _ = ty;
                Ty::Error
            }

            ExprKind::StructLiteral { .. } => Ty::Error,

            ExprKind::ArrayLiteral(elems) => {
                if elems.is_empty() {
                    return Ty::Error;
                }
                let elem_ty = self.check_expr(&elems[0]);
                for e in elems.iter().skip(1) {
                    let _ = self.check_expr(e);
                }
                Ty::Array {
                    element: Box::new(elem_ty),
                    size: elems.len() as u64,
                }
            }

            ExprKind::Range { .. } => Ty::MetaUInt,

            ExprKind::Next { expr: inner, .. } => self.check_expr(inner),
            ExprKind::Eventually { expr: inner, .. } => {
                let _ = self.check_expr(inner);
                Ty::Bool
            }

            ExprKind::Unchecked(inner) => self.check_expr(inner),
        }
    }

    // -----------------------------------------------------------------------
    // Binary expression type inference
    // -----------------------------------------------------------------------

    fn check_binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, span: Span) -> Ty {
        let lhs_ty = self.check_expr(lhs);
        let rhs_ty = self.check_expr(rhs);

        // Propagate errors without cascading.
        if lhs_ty.is_error() || rhs_ty.is_error() {
            return Ty::Error;
        }

        match op {
            // Arithmetic: result width = max(N, M)
            BinOp::Add | BinOp::Sub => {
                self.check_arithmetic_operands(&lhs_ty, &rhs_ty, op, span);
                let w = max_width(&lhs_ty, &rhs_ty);
                same_kind_with_width(&lhs_ty, w)
            }

            // Multiply: result width = N + M
            BinOp::Mul => {
                self.check_arithmetic_operands(&lhs_ty, &rhs_ty, op, span);
                let w = sum_width(&lhs_ty, &rhs_ty);
                same_kind_with_width(&lhs_ty, w)
            }

            // Divide / Mod: result width = dividend width
            BinOp::Div | BinOp::Mod => {
                self.check_arithmetic_operands(&lhs_ty, &rhs_ty, op, span);
                lhs_ty.clone()
            }

            // Power: unsized result (compile-time only)
            BinOp::Pow => Ty::UInt(0),

            // Bitwise: result width = max(N, M)
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                self.check_bitwise_operands(&lhs_ty, &rhs_ty, op, span);
                let w = max_width(&lhs_ty, &rhs_ty);
                same_kind_with_width(&lhs_ty, w)
            }

            // Shifts: static Shl returns UInt<N+K>, dynamic Shl returns UInt<N>
            BinOp::Shl => {
                let lhs_w = lhs_ty.bit_width().unwrap_or(0);
                // Try const-eval on RHS to detect static shifts
                let evaluator = crate::sema::eval::ConstEval::new();
                if let Ok(crate::sema::eval::ConstValue::UInt(k)) = evaluator.eval_expr(rhs) {
                    let k = u64::try_from(k).unwrap_or(0);
                    same_kind_with_width(&lhs_ty, lhs_w + k)
                } else {
                    // Dynamic shift preserves width
                    lhs_ty.clone()
                }
            }
            BinOp::Shr | BinOp::ArithShr => lhs_ty.clone(),

            // Comparisons: return Bool
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                self.check_comparison_operands(&lhs_ty, &rhs_ty, op, span);
                Ty::Bool
            }

            // Logical: require Bool operands, return Bool
            BinOp::And | BinOp::Or | BinOp::Implies => {
                if lhs_ty != Ty::Bool {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Bool".to_owned(),
                        found: lhs_ty.to_string(),
                        span: lhs.span,
                    });
                }
                if rhs_ty != Ty::Bool {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Bool".to_owned(),
                        found: rhs_ty.to_string(),
                        span: rhs.span,
                    });
                }
                Ty::Bool
            }

            // Concatenation: Bits<N+M>
            BinOp::Concat => {
                let lw = bits_width(&lhs_ty);
                let rw = bits_width(&rhs_ty);
                if lw == 0 && rw == 0 {
                    // Both unknown width (e.g. unsized literals) — skip
                    Ty::Bits(0)
                } else {
                    Ty::Bits(lw + rw)
                }
            }
        }
    }

    fn check_arithmetic_operands(&mut self, lhs: &Ty, rhs: &Ty, op: BinOp, span: Span) {
        let lhs_ok = lhs.is_numeric() || *lhs == Ty::UInt(0);
        let rhs_ok = rhs.is_numeric() || *rhs == Ty::UInt(0);
        if !lhs_ok || !rhs_ok {
            self.errors.push(SemaError::TypeMismatch {
                expected: "numeric type (UInt, SInt, Bits)".to_owned(),
                found: if !lhs_ok { lhs.to_string() } else { rhs.to_string() },
                span,
            });
        }
        // Ensure same type family (UInt with UInt, SInt with SInt).
        if lhs_ok && rhs_ok {
            let same = match (lhs, rhs) {
                (Ty::UInt(_), Ty::UInt(_)) => true,
                (Ty::SInt(_), Ty::SInt(_)) => true,
                (Ty::Bits(_), Ty::Bits(_)) => true,
                // UInt(0) (unsized literal) is compatible with any numeric family.
                (Ty::UInt(0), _) | (_, Ty::UInt(0)) => true,
                _ => false,
            };
            if !same {
                self.errors.push(SemaError::TypeMismatch {
                    expected: lhs.to_string(),
                    found: rhs.to_string(),
                    span,
                });
            }
        }
        let _ = op; // op is used only for context; no per-op check needed here
    }

    fn check_bitwise_operands(&mut self, lhs: &Ty, rhs: &Ty, op: BinOp, span: Span) {
        // Bitwise ops require numeric types.
        self.check_arithmetic_operands(lhs, rhs, op, span);
    }

    fn check_comparison_operands(&mut self, lhs: &Ty, rhs: &Ty, _op: BinOp, span: Span) {
        // Operands must be the same kind.
        if !types_compatible_for_compare(lhs, rhs) {
            self.errors.push(SemaError::TypeMismatch {
                expected: lhs.to_string(),
                found: rhs.to_string(),
                span,
            });
        }
    }

    // -----------------------------------------------------------------------
    // Unary expression type inference
    // -----------------------------------------------------------------------

    fn check_unary(&mut self, op: UnaryOp, operand: &Expr, _span: Span) -> Ty {
        let ty = self.check_expr(operand);
        if ty.is_error() {
            return Ty::Error;
        }
        match op {
            UnaryOp::Neg => ty,
            UnaryOp::BitNot => ty,
            UnaryOp::LogicalNot => {
                if ty != Ty::Bool {
                    self.errors.push(SemaError::TypeMismatch {
                        expected: "Bool".to_owned(),
                        found: ty.to_string(),
                        span: operand.span,
                    });
                }
                Ty::Bool
            }
        }
    }

    // -----------------------------------------------------------------------
    // Bit-slice width helper
    // -----------------------------------------------------------------------

    fn try_bitslice_width(&self, high: &Expr, low: &Expr) -> u64 {
        use super::eval::{ConstEval, ConstValue};
        let ev = ConstEval::new();
        match (ev.eval_expr(high), ev.eval_expr(low)) {
            (Ok(ConstValue::UInt(h)), Ok(ConstValue::UInt(l))) if h >= l => (h - l + 1) as u64,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Free helper functions
// ---------------------------------------------------------------------------

/// Return the max hardware width of two types (treating UInt(0) as 0).
fn max_width(a: &Ty, b: &Ty) -> u64 {
    let aw = a.bit_width().unwrap_or(0);
    let bw = b.bit_width().unwrap_or(0);
    aw.max(bw)
}

/// Return the sum of hardware widths (for multiplication widening).
fn sum_width(a: &Ty, b: &Ty) -> u64 {
    let aw = a.bit_width().unwrap_or(0);
    let bw = b.bit_width().unwrap_or(0);
    aw + bw
}

/// Construct a new type that is the same "family" as `base` but with a new width.
/// Falls back to UInt if the family is unknown.
fn same_kind_with_width(base: &Ty, width: u64) -> Ty {
    match base {
        Ty::UInt(_) => Ty::UInt(width),
        Ty::SInt(_) => Ty::SInt(width),
        Ty::Bits(_) => Ty::Bits(width),
        // For unsized literals (UInt(0)) the result is also unsized.
        _ => Ty::UInt(width),
    }
}

/// Return the bit width to use for concatenation (strips direction wrappers).
fn bits_width(ty: &Ty) -> u64 {
    ty.unwrap_direction().bit_width().unwrap_or(0)
}

/// Returns true if two types can be compared with == / < / etc.
fn types_compatible_for_compare(a: &Ty, b: &Ty) -> bool {
    let a = a.unwrap_direction();
    let b = b.unwrap_direction();
    match (a, b) {
        (Ty::UInt(_), Ty::UInt(_)) => true,
        (Ty::SInt(_), Ty::SInt(_)) => true,
        (Ty::Bits(_), Ty::Bits(_)) => true,
        (Ty::Bool, Ty::Bool) => true,
        // Unsized literal is compatible with any numeric.
        (Ty::UInt(0), _) | (_, Ty::UInt(0)) => true,
        _ => false,
    }
}
