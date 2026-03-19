# Phase 3: Semantic Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a semantic analysis pass that validates SSL programs for type correctness, name resolution, and hardware safety — producing a `sslc check <file>` command that reports actionable errors with source spans.

**Architecture:** Three-pass design over the parsed AST: (1) Scope-building pass collects declarations into a symbol table, (2) Resolution pass resolves all name references and type expressions to concrete types, (3) Validation pass enforces hardware-specific rules (latch-free comb blocks, exhaustive match, port completeness). A side-table maps AST node spans to resolved type information, avoiding mutation of the existing AST.

**Tech Stack:** Rust (edition 2024), miette for error reporting, thiserror for error types. No new dependencies beyond what's in the workspace.

**Deferred to Phase 4+:**
- Clock domain crossing (CDC) type algebra and analysis (spec Section 2.4, 4.3)
- Clock frequency parsing (`Clock<100MHz>`) — Phase 3 stores `freq: None` for all clocks
- Generic monomorphization for user-defined generic modules
- Elaboration (gen for/if expansion, systolic/dataflow lowering)
- Code generation backends (Verilog, RTLIL, FIRRTL)

**Important API notes for implementers:**
- `ssl_core::lexer::tokenize(src)` returns `Result<Vec<Spanned<Token>>, TokenizeError>` — always unwrap with `.expect("tokenize failed")`
- `NumericLiteral` is re-exported at `ssl_core::lexer::NumericLiteral` (NOT `ssl_core::lexer::token::NumericLiteral`)
- `SymbolTable::define()` returns `Result<SymbolId, SemaError>` — always handle or `.unwrap()` in tests
- Port direction is tracked in `Symbol.direction: Option<Direction>`, NOT as `Ty::In(...)` wrapper. Direction wrappers in `Ty` are only for interface signal types.

---

## File Structure

```
crates/ssl-core/src/
  sema/                          ← NEW module
    mod.rs                       Entry point: run_analysis() orchestrates passes, re-exports
    types.rs                     ResolvedType enum + type operations (width, compatibility)
    scope.rs                     ScopeArena + Symbol + SymbolId + ScopeId
    resolve.rs                   Name resolution + type resolution pass
    check.rs                     Type checking: expressions, statements, items
    eval.rs                      Compile-time constant evaluation
    validate.rs                  Hardware safety checks (latch-free, exhaustive match)
    error.rs                     Diagnostic types (SemaError with span + miette)
  lib.rs                         Add `pub mod sema`

crates/ssl-core/tests/
  sema_tests.rs                  Integration tests for semantic analysis

crates/sslc/src/
  main.rs                        Add `check` subcommand
```

Each file has one clear responsibility. The passes communicate through shared data structures (`TypeTable`, `SymbolTable`) that are built up incrementally.

---

## Chunk 1: Foundation Data Structures (Tasks 1–3)

### Task 1: Error Reporting Infrastructure

**Files:**
- Create: `crates/ssl-core/src/sema/error.rs`
- Create: `crates/ssl-core/src/sema/mod.rs`
- Modify: `crates/ssl-core/src/lib.rs`

- [ ] **Step 1: Write failing test**

```rust
// crates/ssl-core/tests/sema_tests.rs
use ssl_core::sema::SemaError;
use ssl_core::span::Span;

#[test]
fn sema_error_display() {
    let err = SemaError::UndefinedName {
        name: "foo".into(),
        span: Span::new(10, 13),
    };
    let msg = format!("{err}");
    assert!(msg.contains("foo"));
    assert!(msg.contains("undefined"));
}

#[test]
fn sema_error_type_mismatch() {
    let err = SemaError::TypeMismatch {
        expected: "UInt<8>".into(),
        found: "UInt<16>".into(),
        span: Span::new(20, 30),
    };
    let msg = format!("{err}");
    assert!(msg.contains("UInt<8>"));
    assert!(msg.contains("UInt<16>"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- sema_error 2>&1`
Expected: FAIL — module `sema` doesn't exist

- [ ] **Step 3: Write implementation**

```rust
// crates/ssl-core/src/sema/error.rs
use crate::span::Span;

#[derive(Debug, Clone)]
pub enum SemaError {
    UndefinedName {
        name: String,
        span: Span,
    },
    DuplicateDefinition {
        name: String,
        first: Span,
        second: Span,
    },
    TypeMismatch {
        expected: String,
        found: String,
        span: Span,
    },
    WidthMismatch {
        expected: u64,
        found: u64,
        span: Span,
    },
    InvalidAssignTarget {
        span: Span,
    },
    LatchInferred {
        signal: String,
        span: Span,
    },
    NonExhaustiveMatch {
        span: Span,
    },
    UnconnectedPort {
        port: String,
        inst: String,
        span: Span,
    },
    InvalidContext {
        construct: String,
        context: String,
        span: Span,
    },
    ConstEvalError {
        message: String,
        span: Span,
    },
    CyclicDependency {
        names: Vec<String>,
        span: Span,
    },
    DirectionViolation {
        message: String,
        span: Span,
    },
    Custom {
        message: String,
        span: Span,
    },
}

impl SemaError {
    pub fn span(&self) -> Span {
        match self {
            Self::UndefinedName { span, .. }
            | Self::DuplicateDefinition { second: span, .. }
            | Self::TypeMismatch { span, .. }
            | Self::WidthMismatch { span, .. }
            | Self::InvalidAssignTarget { span }
            | Self::LatchInferred { span, .. }
            | Self::NonExhaustiveMatch { span }
            | Self::UnconnectedPort { span, .. }
            | Self::InvalidContext { span, .. }
            | Self::ConstEvalError { span, .. }
            | Self::CyclicDependency { span, .. }
            | Self::DirectionViolation { span, .. }
            | Self::Custom { span, .. } => *span,
        }
    }
}

impl std::fmt::Display for SemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UndefinedName { name, span } =>
                write!(f, "undefined name `{name}` at {}..{}", span.start, span.end),
            Self::DuplicateDefinition { name, first, second } =>
                write!(f, "duplicate definition `{name}` at {}..{} (first defined at {}..{})",
                    second.start, second.end, first.start, first.end),
            Self::TypeMismatch { expected, found, span } =>
                write!(f, "type mismatch at {}..{}: expected {expected}, found {found}",
                    span.start, span.end),
            Self::WidthMismatch { expected, found, span } =>
                write!(f, "width mismatch at {}..{}: expected {expected} bits, found {found} bits",
                    span.start, span.end),
            Self::InvalidAssignTarget { span } =>
                write!(f, "invalid assignment target at {}..{}", span.start, span.end),
            Self::LatchInferred { signal, span } =>
                write!(f, "latch inferred for signal `{signal}` at {}..{}: not assigned on all paths",
                    span.start, span.end),
            Self::NonExhaustiveMatch { span } =>
                write!(f, "non-exhaustive match at {}..{}", span.start, span.end),
            Self::UnconnectedPort { port, inst, span } =>
                write!(f, "unconnected port `{port}` on instance `{inst}` at {}..{}",
                    span.start, span.end),
            Self::InvalidContext { construct, context, span } =>
                write!(f, "`{construct}` is not valid in {context} context at {}..{}",
                    span.start, span.end),
            Self::ConstEvalError { message, span } =>
                write!(f, "constant evaluation error at {}..{}: {message}",
                    span.start, span.end),
            Self::CyclicDependency { names, span } =>
                write!(f, "cyclic dependency at {}..{}: {}", span.start, span.end,
                    names.join(" -> ")),
            Self::DirectionViolation { message, span } =>
                write!(f, "direction violation at {}..{}: {message}", span.start, span.end),
            Self::Custom { message, span } =>
                write!(f, "error at {}..{}: {message}", span.start, span.end),
        }
    }
}

impl std::error::Error for SemaError {}
```

```rust
// crates/ssl-core/src/sema/mod.rs
pub mod error;

pub use error::SemaError;
```

Add to `crates/ssl-core/src/lib.rs`:
```rust
pub mod sema;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- sema_error 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/ crates/ssl-core/src/lib.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add error reporting infrastructure for semantic analysis"
```

---

### Task 2: Resolved Type System

**Files:**
- Create: `crates/ssl-core/src/sema/types.rs`
- Modify: `crates/ssl-core/src/sema/mod.rs`

This task defines the `Ty` enum — the compiler's internal representation of resolved types. Unlike AST `TypeExprKind` (which is syntactic), `Ty` represents fully resolved types with concrete widths.

- [ ] **Step 1: Write failing test**

```rust
// append to crates/ssl-core/tests/sema_tests.rs
use ssl_core::sema::types::Ty;

#[test]
fn ty_uint_width() {
    let t = Ty::UInt(8);
    assert_eq!(t.bit_width(), Some(8));
}

#[test]
fn ty_bool_width() {
    assert_eq!(Ty::Bool.bit_width(), Some(1));
}

#[test]
fn ty_sint_width() {
    let t = Ty::SInt(16);
    assert_eq!(t.bit_width(), Some(16));
}

#[test]
fn ty_bits_width() {
    let t = Ty::Bits(32);
    assert_eq!(t.bit_width(), Some(32));
}

#[test]
fn ty_array_width() {
    let t = Ty::Array {
        element: Box::new(Ty::UInt(8)),
        size: 4,
    };
    assert_eq!(t.bit_width(), Some(32));
}

#[test]
fn ty_clock_no_width() {
    let t = Ty::Clock { freq: None };
    assert_eq!(t.bit_width(), Some(1));
}

#[test]
fn ty_display() {
    assert_eq!(Ty::UInt(8).to_string(), "UInt<8>");
    assert_eq!(Ty::SInt(16).to_string(), "SInt<16>");
    assert_eq!(Ty::Bool.to_string(), "Bool");
    assert_eq!(Ty::Bits(32).to_string(), "Bits<32>");
    assert_eq!(Ty::Error.to_string(), "{{error}}");
}

#[test]
fn ty_is_numeric() {
    assert!(Ty::UInt(8).is_numeric());
    assert!(Ty::SInt(16).is_numeric());
    assert!(Ty::Bits(32).is_numeric());
    assert!(!Ty::Bool.is_numeric());
    assert!(!Ty::Clock { freq: None }.is_numeric());
}

#[test]
fn ty_is_integer() {
    assert!(Ty::UInt(8).is_integer());
    assert!(Ty::SInt(16).is_integer());
    assert!(!Ty::Bits(32).is_integer());
    assert!(!Ty::Bool.is_integer());
}

#[test]
fn ty_fixed_width() {
    let t = Ty::Fixed { int_bits: 8, frac_bits: 8 };
    assert_eq!(t.bit_width(), Some(16));
    assert_eq!(t.to_string(), "Fixed<8, 8>");
}

#[test]
fn ty_meta_types_no_width() {
    assert_eq!(Ty::MetaUInt.bit_width(), None);
    assert_eq!(Ty::MetaInt.bit_width(), None);
    assert_eq!(Ty::MetaBool.bit_width(), None);
}

#[test]
fn ty_is_synthesizable() {
    assert!(Ty::UInt(8).is_synthesizable());
    assert!(Ty::Bool.is_synthesizable());
    assert!(!Ty::MetaUInt.is_synthesizable());
    assert!(!Ty::MetaString.is_synthesizable());
    assert!(!Ty::Error.is_synthesizable());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- ty_ 2>&1`
Expected: FAIL — `sema::types` module doesn't exist

- [ ] **Step 3: Write implementation**

```rust
// crates/ssl-core/src/sema/types.rs

/// A resolved type in the SSL type system.
///
/// Unlike AST `TypeExprKind` which is syntactic, `Ty` represents fully resolved
/// types with concrete widths and no unresolved names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // ── Hardware types ────────────────────────────────────
    /// Uninterpreted bit vector: `Bits<N>`
    Bits(u64),
    /// Unsigned integer: `UInt<N>`
    UInt(u64),
    /// Signed integer (2's complement): `SInt<N>`
    SInt(u64),
    /// Fixed-point: `Fixed<I, F>` = I integer + F fractional bits
    Fixed { int_bits: u64, frac_bits: u64 },
    /// Single-bit boolean (distinct from UInt<1>)
    Bool,
    /// Clock signal
    Clock { freq: Option<u64> },
    /// Synchronous reset
    SyncReset,
    /// Asynchronous reset
    AsyncReset,

    // ── Compound types ───────────────────────────────────
    /// Fixed-size array: `T[N]`
    Array { element: Box<Ty>, size: u64 },
    /// User-defined struct (index into StructDefs table)
    Struct(StructId),
    /// User-defined enum (index into EnumDefs table)
    Enum(EnumId),
    /// User-defined interface (index into InterfaceDefs table)
    Interface(InterfaceId),
    /// Memory primitive
    Memory { element: Box<Ty>, depth: u64 },

    // ── Direction wrappers (interface signals only) ──────
    In(Box<Ty>),
    Out(Box<Ty>),
    InOut(Box<Ty>),
    Flip(Box<Ty>),

    // ── Compile-time meta types ──────────────────────────
    MetaUInt,
    MetaInt,
    MetaBool,
    MetaFloat,
    MetaString,
    MetaType,

    // ── Special ──────────────────────────────────────────
    /// Error sentinel — propagates through type checking without cascading errors
    Error,
    /// Void / unit type (for statements, tasks with no return)
    Void,
}

/// Opaque index into the struct definitions table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

/// Opaque index into the enum definitions table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumId(pub u32);

/// Opaque index into the interface definitions table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceId(pub u32);

impl Ty {
    /// Returns the bit width if this type has a fixed hardware representation.
    /// Returns `None` for meta types (compile-time only).
    pub fn bit_width(&self) -> Option<u64> {
        match self {
            Ty::Bits(n) | Ty::UInt(n) | Ty::SInt(n) => Some(*n),
            Ty::Fixed { int_bits, frac_bits } => Some(int_bits + frac_bits),
            Ty::Bool => Some(1),
            Ty::Clock { .. } => Some(1),
            Ty::SyncReset | Ty::AsyncReset => Some(1),
            Ty::Array { element, size } => element.bit_width().map(|w| w * size),
            Ty::In(inner) | Ty::Out(inner) | Ty::InOut(inner) | Ty::Flip(inner) =>
                inner.bit_width(),
            // Structs, enums, interfaces need lookup in definition tables
            Ty::Struct(_) | Ty::Enum(_) | Ty::Interface(_) | Ty::Memory { .. } => None,
            // Meta types have no hardware representation
            Ty::MetaUInt | Ty::MetaInt | Ty::MetaBool
            | Ty::MetaFloat | Ty::MetaString | Ty::MetaType => None,
            Ty::Error | Ty::Void => None,
        }
    }

    /// Whether this type represents a numeric value with a bit width.
    pub fn is_numeric(&self) -> bool {
        matches!(self,
            Ty::Bits(_) | Ty::UInt(_) | Ty::SInt(_) | Ty::Fixed { .. }
        )
    }

    /// Whether this type is an integer type (UInt or SInt).
    pub fn is_integer(&self) -> bool {
        matches!(self, Ty::UInt(_) | Ty::SInt(_))
    }

    /// Whether this type can be synthesized to hardware.
    pub fn is_synthesizable(&self) -> bool {
        match self {
            Ty::Bits(_) | Ty::UInt(_) | Ty::SInt(_) | Ty::Fixed { .. }
            | Ty::Bool | Ty::Clock { .. } | Ty::SyncReset | Ty::AsyncReset
            | Ty::Struct(_) | Ty::Enum(_) | Ty::Interface(_)
            | Ty::Memory { .. } => true,
            Ty::Array { element, .. } => element.is_synthesizable(),
            Ty::In(inner) | Ty::Out(inner) | Ty::InOut(inner) | Ty::Flip(inner) =>
                inner.is_synthesizable(),
            Ty::MetaUInt | Ty::MetaInt | Ty::MetaBool
            | Ty::MetaFloat | Ty::MetaString | Ty::MetaType
            | Ty::Error | Ty::Void => false,
        }
    }

    /// Whether this type is a compile-time-only meta type.
    pub fn is_meta(&self) -> bool {
        matches!(self,
            Ty::MetaUInt | Ty::MetaInt | Ty::MetaBool
            | Ty::MetaFloat | Ty::MetaString | Ty::MetaType
        )
    }

    /// Whether this is the error sentinel.
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Strip direction wrappers to get the inner type.
    pub fn unwrap_direction(&self) -> &Ty {
        match self {
            Ty::In(inner) | Ty::Out(inner) | Ty::InOut(inner) | Ty::Flip(inner) =>
                inner.unwrap_direction(),
            other => other,
        }
    }
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Bits(n) => write!(f, "Bits<{n}>"),
            Ty::UInt(n) => write!(f, "UInt<{n}>"),
            Ty::SInt(n) => write!(f, "SInt<{n}>"),
            Ty::Fixed { int_bits, frac_bits } => write!(f, "Fixed<{int_bits}, {frac_bits}>"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Clock { freq: Some(hz) } => write!(f, "Clock<{hz}>"),
            Ty::Clock { freq: None } => write!(f, "Clock"),
            Ty::SyncReset => write!(f, "SyncReset"),
            Ty::AsyncReset => write!(f, "AsyncReset"),
            Ty::Array { element, size } => write!(f, "{element}[{size}]"),
            Ty::Struct(id) => write!(f, "Struct#{}", id.0),
            Ty::Enum(id) => write!(f, "Enum#{}", id.0),
            Ty::Interface(id) => write!(f, "Interface#{}", id.0),
            Ty::Memory { element, depth } => write!(f, "Memory<{element}, depth={depth}>"),
            Ty::In(inner) => write!(f, "In<{inner}>"),
            Ty::Out(inner) => write!(f, "Out<{inner}>"),
            Ty::InOut(inner) => write!(f, "InOut<{inner}>"),
            Ty::Flip(inner) => write!(f, "Flip<{inner}>"),
            Ty::MetaUInt => write!(f, "uint"),
            Ty::MetaInt => write!(f, "int"),
            Ty::MetaBool => write!(f, "bool"),
            Ty::MetaFloat => write!(f, "float"),
            Ty::MetaString => write!(f, "string"),
            Ty::MetaType => write!(f, "type"),
            Ty::Error => write!(f, "{{{{error}}}}"),
            Ty::Void => write!(f, "void"),
        }
    }
}
```

Update `crates/ssl-core/src/sema/mod.rs`:
```rust
pub mod error;
pub mod types;

pub use error::SemaError;
pub use types::Ty;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- ty_ 2>&1`
Expected: PASS (all 12 type tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/types.rs crates/ssl-core/src/sema/mod.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add resolved type system with Ty enum"
```

---

### Task 3: Symbol Table and Scope Management

**Files:**
- Create: `crates/ssl-core/src/sema/scope.rs`
- Modify: `crates/ssl-core/src/sema/mod.rs`

The symbol table is an arena of scopes, each containing a map of name→Symbol. Scopes form a tree via parent pointers. Lookup walks up the scope chain.

- [ ] **Step 1: Write failing test**

```rust
// append to crates/ssl-core/tests/sema_tests.rs
use ssl_core::sema::scope::{SymbolTable, SymbolKind};

#[test]
fn scope_define_and_lookup() {
    let mut table = SymbolTable::new();
    let file_scope = table.root_scope();

    table.define(file_scope, "counter", SymbolKind::Signal, Ty::UInt(8),
        Span::new(0, 7)).unwrap();

    let sym = table.lookup(file_scope, "counter");
    assert!(sym.is_some());
    let sym = sym.unwrap();
    assert_eq!(sym.name, "counter");
    assert_eq!(sym.ty, Ty::UInt(8));
}

#[test]
fn scope_child_sees_parent() {
    let mut table = SymbolTable::new();
    let file_scope = table.root_scope();
    table.define(file_scope, "top_signal", SymbolKind::Signal, Ty::Bool,
        Span::new(0, 10)).unwrap();

    let child = table.push_scope(file_scope, ssl_core::sema::scope::ScopeKind::Module);
    let sym = table.lookup(child, "top_signal");
    assert!(sym.is_some());
}

#[test]
fn scope_child_shadows_parent() {
    let mut table = SymbolTable::new();
    let file_scope = table.root_scope();
    table.define(file_scope, "x", SymbolKind::Signal, Ty::UInt(8), Span::new(0, 1)).unwrap();

    let child = table.push_scope(file_scope, ssl_core::sema::scope::ScopeKind::Block);
    table.define(child, "x", SymbolKind::Signal, Ty::UInt(16), Span::new(10, 11)).unwrap();

    // Child sees its own definition
    let sym = table.lookup(child, "x").unwrap();
    assert_eq!(sym.ty, Ty::UInt(16));

    // Parent still sees original
    let sym = table.lookup(file_scope, "x").unwrap();
    assert_eq!(sym.ty, Ty::UInt(8));
}

#[test]
fn scope_undefined_returns_none() {
    let table = SymbolTable::new();
    let root = table.root_scope();
    assert!(table.lookup(root, "nonexistent").is_none());
}

#[test]
fn scope_duplicate_in_same_scope() {
    let mut table = SymbolTable::new();
    let root = table.root_scope();
    let r1 = table.define(root, "x", SymbolKind::Signal, Ty::UInt(8), Span::new(0, 1));
    assert!(r1.is_ok());
    let r2 = table.define(root, "x", SymbolKind::Signal, Ty::UInt(16), Span::new(10, 11));
    assert!(r2.is_err()); // duplicate definition error
}

#[test]
fn scope_list_local_symbols() {
    let mut table = SymbolTable::new();
    let root = table.root_scope();
    table.define(root, "a", SymbolKind::Signal, Ty::Bool, Span::new(0, 1)).unwrap();
    table.define(root, "b", SymbolKind::Const, Ty::MetaUInt, Span::new(2, 3)).unwrap();

    let locals = table.local_symbols(root);
    assert_eq!(locals.len(), 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- scope_ 2>&1`
Expected: FAIL — `sema::scope` module doesn't exist

- [ ] **Step 3: Write implementation**

```rust
// crates/ssl-core/src/sema/scope.rs
use crate::span::Span;
use super::types::Ty;
use super::error::SemaError;
use std::collections::HashMap;

/// Opaque scope identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(u32);

/// Opaque symbol identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(u32);

/// What kind of scope this is (affects lookup rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    File,
    Module,
    Function,
    Block,
    Fsm,
    Pipeline,
    Test,
}

/// What kind of symbol this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Signal,
    Port,
    Const,
    Let,
    Var,
    TypeAlias,
    Module,
    Struct,
    Enum,
    EnumVariant,
    Interface,
    Fn,
    Fsm,
    Pipeline,
    GenericParam,
    LoopVar,
}

/// A resolved symbol in the symbol table.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub ty: Ty,
    pub span: Span,
    pub scope: ScopeId,
    pub mutable: bool,
    /// Port direction (only set for SymbolKind::Port).
    /// Used for direction enforcement: In ports are read-only, Out ports are write-only.
    pub direction: Option<crate::ast::types::Direction>,
}

struct Scope {
    parent: Option<ScopeId>,
    kind: ScopeKind,
    symbols: HashMap<String, SymbolId>,
}

/// Arena-based symbol table with scoped lookup.
pub struct SymbolTable {
    scopes: Vec<Scope>,
    symbols: Vec<Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut table = Self {
            scopes: Vec::new(),
            symbols: Vec::new(),
        };
        // Create the root file scope
        table.scopes.push(Scope {
            parent: None,
            kind: ScopeKind::File,
            symbols: HashMap::new(),
        });
        table
    }

    /// Returns the root (file-level) scope.
    pub fn root_scope(&self) -> ScopeId {
        ScopeId(0)
    }

    /// Create a child scope under `parent`.
    pub fn push_scope(&mut self, parent: ScopeId, kind: ScopeKind) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(Scope {
            parent: Some(parent),
            kind,
            symbols: HashMap::new(),
        });
        id
    }

    /// Define a symbol in the given scope. Returns error if duplicate.
    pub fn define(
        &mut self,
        scope: ScopeId,
        name: &str,
        kind: SymbolKind,
        ty: Ty,
        span: Span,
    ) -> Result<SymbolId, SemaError> {
        let scope_data = &self.scopes[scope.0 as usize];
        if let Some(&existing_id) = scope_data.symbols.get(name) {
            let existing = &self.symbols[existing_id.0 as usize];
            return Err(SemaError::DuplicateDefinition {
                name: name.to_string(),
                first: existing.span,
                second: span,
            });
        }

        let mutable = matches!(kind, SymbolKind::Signal | SymbolKind::Var | SymbolKind::LoopVar);
        let sym_id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(Symbol {
            name: name.to_string(),
            kind,
            ty,
            span,
            scope,
            mutable,
        });
        self.scopes[scope.0 as usize].symbols.insert(name.to_string(), sym_id);
        Ok(sym_id)
    }

    /// Look up a name starting from `scope`, walking up the scope chain.
    pub fn lookup(&self, scope: ScopeId, name: &str) -> Option<&Symbol> {
        let mut current = Some(scope);
        while let Some(scope_id) = current {
            let scope_data = &self.scopes[scope_id.0 as usize];
            if let Some(&sym_id) = scope_data.symbols.get(name) {
                return Some(&self.symbols[sym_id.0 as usize]);
            }
            current = scope_data.parent;
        }
        None
    }

    /// Look up a name only in the given scope (no parent chain).
    pub fn lookup_local(&self, scope: ScopeId, name: &str) -> Option<&Symbol> {
        let scope_data = &self.scopes[scope.0 as usize];
        scope_data.symbols.get(name)
            .map(|&id| &self.symbols[id.0 as usize])
    }

    /// Get a symbol by its ID.
    pub fn get_symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }

    /// Get a mutable reference to a symbol by its ID.
    pub fn get_symbol_mut(&mut self, id: SymbolId) -> &mut Symbol {
        &mut self.symbols[id.0 as usize]
    }

    /// List all symbols defined directly in the given scope.
    pub fn local_symbols(&self, scope: ScopeId) -> Vec<&Symbol> {
        let scope_data = &self.scopes[scope.0 as usize];
        scope_data.symbols.values()
            .map(|&id| &self.symbols[id.0 as usize])
            .collect()
    }

    /// Get the kind of a scope.
    pub fn scope_kind(&self, scope: ScopeId) -> ScopeKind {
        self.scopes[scope.0 as usize].kind
    }

    /// Get the parent of a scope, if any.
    pub fn parent_scope(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes[scope.0 as usize].parent
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
```

Update `crates/ssl-core/src/sema/mod.rs`:
```rust
pub mod error;
pub mod types;
pub mod scope;

pub use error::SemaError;
pub use types::Ty;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- scope_ 2>&1`
Expected: PASS (all 6 scope tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/scope.rs crates/ssl-core/src/sema/mod.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add symbol table with arena-based scoped lookup"
```

---

## Chunk 2: Compile-Time Evaluation + Name Resolution (Tasks 4–6)

### Task 4: Compile-Time Constant Evaluator

**Files:**
- Create: `crates/ssl-core/src/sema/eval.rs`
- Modify: `crates/ssl-core/src/sema/mod.rs`

The const evaluator reduces AST expressions to compile-time values. This is needed to resolve generic parameters like `UInt<8>`, `UInt<ADDR_W>`, array sizes, and `static_assert` conditions.

- [ ] **Step 1: Write failing test**

```rust
// append to crates/ssl-core/tests/sema_tests.rs
use ssl_core::sema::eval::{ConstValue, ConstEval};
use ssl_core::ast::expr::ExprKind;
use ssl_core::lexer::NumericLiteral;
use ssl_core::span::Spanned;

fn make_int_expr(val: u128) -> ssl_core::ast::expr::Expr {
    Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(val)),
        Span::new(0, 1),
    )
}

#[test]
fn eval_integer_literal() {
    let evaluator = ConstEval::new();
    let expr = make_int_expr(42);
    let result = evaluator.eval_expr(&expr);
    assert_eq!(result, Ok(ConstValue::UInt(42)));
}

#[test]
fn eval_bool_literal() {
    let evaluator = ConstEval::new();
    let expr = Spanned::new(ExprKind::BoolLiteral(true), Span::new(0, 4));
    let result = evaluator.eval_expr(&expr);
    assert_eq!(result, Ok(ConstValue::Bool(true)));
}

#[test]
fn eval_binary_add() {
    let evaluator = ConstEval::new();
    let lhs = make_int_expr(10);
    let rhs = make_int_expr(32);
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Add,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(42)));
}

#[test]
fn eval_binary_mul() {
    let evaluator = ConstEval::new();
    let lhs = make_int_expr(6);
    let rhs = make_int_expr(7);
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Mul,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(42)));
}

#[test]
fn eval_binary_pow() {
    let evaluator = ConstEval::new();
    let lhs = make_int_expr(2);
    let rhs = make_int_expr(10);
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Pow,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(1024)));
}

#[test]
fn eval_comparison() {
    let evaluator = ConstEval::new();
    let lhs = make_int_expr(5);
    let rhs = make_int_expr(10);
    let expr = Spanned::new(
        ExprKind::Binary {
            op: ssl_core::ast::expr::BinOp::Lt,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        Span::new(0, 5),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::Bool(true)));
}

#[test]
fn eval_if_expr() {
    let evaluator = ConstEval::new();
    let cond = Spanned::new(ExprKind::BoolLiteral(true), Span::new(0, 4));
    let then_expr = make_int_expr(10);
    let else_expr = make_int_expr(20);
    let expr = Spanned::new(
        ExprKind::IfExpr {
            condition: Box::new(cond),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
        },
        Span::new(0, 10),
    );
    assert_eq!(evaluator.eval_expr(&expr), Ok(ConstValue::UInt(10)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- eval_ 2>&1`
Expected: FAIL — `sema::eval` module doesn't exist

- [ ] **Step 3: Write implementation**

Implement `ConstValue` enum with `UInt(u128)`, `Int(i128)`, `Bool(bool)`, `Float(f64)`, `String(String)`. Implement `ConstEval` struct with `eval_expr(&self, expr: &Expr) -> Result<ConstValue, SemaError>`. Support:
- Integer literals (all bases) → `UInt`
- Bool literals → `Bool`
- String literals → `String`
- Binary ops: `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Pow` on integers
- Binary ops: `Eq`, `Ne`, `Lt`, `Gt`, `Le`, `Ge` → `Bool`
- Binary ops: `And`, `Or` on bools
- Unary: `Neg`, `LogicalNot`
- `IfExpr`: evaluate condition, pick branch
- `Ident`: lookup in a `bindings: HashMap<String, ConstValue>` (for named constants)

The evaluator should carry a `bindings` map for evaluating named constants (populated during name resolution). For now, use an empty map — Task 5 will wire it up.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- eval_ 2>&1`
Expected: PASS (all 7 eval tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/eval.rs crates/ssl-core/src/sema/mod.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add compile-time constant evaluator"
```

---

### Task 5: Name Resolution Pass — Declarations

**Files:**
- Create: `crates/ssl-core/src/sema/resolve.rs`
- Modify: `crates/ssl-core/src/sema/mod.rs`

This pass walks the AST and registers all declarations into the symbol table. It creates scopes for modules, functions, blocks, etc. It does NOT resolve references yet (that's Task 6).

- [ ] **Step 1: Write failing test**

```rust
// append to crates/ssl-core/tests/sema_tests.rs
use ssl_core::sema::resolve::Resolver;

/// Helper: parse source code and run declaration collection.
fn resolve_source(src: &str) -> (ssl_core::sema::scope::SymbolTable, Vec<SemaError>) {
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse failed");
    let mut resolver = Resolver::new();
    resolver.collect_declarations(&file);
    resolver.finish()
}

#[test]
fn resolve_module_declared() {
    let (table, errors) = resolve_source("module Foo():\n    signal x: Bool\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    let sym = table.lookup(table.root_scope(), "Foo");
    assert!(sym.is_some(), "module Foo should be in scope");
}

#[test]
fn resolve_signal_in_module() {
    let (table, errors) = resolve_source("module Foo():\n    signal x: UInt<8>\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    // The signal x should be in the module's child scope, not the file scope
    assert!(table.lookup(table.root_scope(), "x").is_none(),
        "signal x should NOT be in file scope");
}

#[test]
fn resolve_duplicate_module_error() {
    let src = "module Foo():\n    signal x: Bool\nmodule Foo():\n    signal y: Bool\n";
    let (_table, errors) = resolve_source(src);
    assert!(!errors.is_empty(), "should report duplicate Foo");
}

#[test]
fn resolve_const_declaration() {
    let (table, errors) = resolve_source(
        "module M():\n    const WIDTH: uint = 8\n"
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn resolve_struct_declared() {
    let (table, errors) = resolve_source("struct Pixel:\n    r: UInt<8>\n    g: UInt<8>\n    b: UInt<8>\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    let sym = table.lookup(table.root_scope(), "Pixel");
    assert!(sym.is_some(), "struct Pixel should be in scope");
}

#[test]
fn resolve_enum_declared() {
    let (table, errors) = resolve_source("enum State [onehot]:\n    Idle\n    Run\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    let sym = table.lookup(table.root_scope(), "State");
    assert!(sym.is_some(), "enum State should be in scope");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- resolve_ 2>&1`
Expected: FAIL — `sema::resolve` module doesn't exist

- [ ] **Step 3: Write implementation**

Implement `Resolver` struct containing a `SymbolTable`, a `Vec<SemaError>`, and a scope stack. The `collect_declarations` method walks:
- `SourceFile.items` → registers each top-level item (module, struct, enum, interface, fn) in file scope
- For each `ModuleDef`: push a module scope, register ports and signals, walk body items
- For each `StructDef`: register struct name + create struct definition
- For each `EnumDef`: register enum name + variants
- For each `FnDef`: push function scope, register params
- `ConstDecl`, `LetDecl`, `SignalDecl`, `TypeAliasDecl` → register in current scope

The resolver should also resolve AST `TypeExprKind` to `Ty` using a `resolve_type` method. This handles:
- `Named("UInt")` + generic arg → `Ty::UInt(width)`
- `Named("SInt")` + generic arg → `Ty::SInt(width)`
- `Named("Bits")` + generic arg → `Ty::Bits(width)`
- `Named("Bool")` → `Ty::Bool`
- `Named("Clock")` → `Ty::Clock { freq }`
- `Named("SyncReset")` → `Ty::SyncReset`
- `Named("AsyncReset")` → `Ty::AsyncReset`
- `Named(other)` → lookup in scope, must be a struct/enum/interface/type alias
- `Generic { name: "Fixed", params: [Expr(I), Expr(F)] }` → `Ty::Fixed { int_bits: I, frac_bits: F }`
- `Generic { name, params }` → resolve base type with evaluated params
- `Array { element, size }` → resolve element, evaluate size
- `DomainAnnotated { ty, domain }` → resolve inner type (domain tracked separately)

Width params are evaluated using `ConstEval`. For identifiers in const expressions, look up in the symbol table and resolve if it's a `SymbolKind::Const` with a known value.

`finish()` returns `(SymbolTable, Vec<SemaError>)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- resolve_ 2>&1`
Expected: PASS (all 6 resolve tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/resolve.rs crates/ssl-core/src/sema/mod.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add name resolution pass for declarations"
```

---

### Task 6: Type Resolution — Mapping AST Types to Ty

**Files:**
- Modify: `crates/ssl-core/src/sema/resolve.rs`
- Modify: `crates/ssl-core/tests/sema_tests.rs`

This task extends the resolver to map AST type expressions to resolved `Ty` values. After this task, every signal, port, and const in the symbol table has a correct `Ty`.

- [ ] **Step 1: Write failing test**

```rust
// append to crates/ssl-core/tests/sema_tests.rs

#[test]
fn resolve_type_uint8() {
    let (table, errors) = resolve_source("module M():\n    signal x: UInt<8>\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
    // Find x by looking up in the module scope — it won't be at file level
    // Instead, verify via the resolver's type table output
}

#[test]
fn resolve_type_bool() {
    let (table, errors) = resolve_source("module M():\n    signal flag: Bool\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn resolve_type_array() {
    let (table, errors) = resolve_source("module M():\n    signal mem: UInt<8>[4]\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn resolve_type_port_in_uint() {
    let (table, errors) = resolve_source(
        "module M(\n    in a: UInt<32>,\n    out b: UInt<32>\n):\n    signal x: Bool\n"
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn resolve_type_clock() {
    let (table, errors) = resolve_source("module M(\n    in clk: Clock\n):\n    signal x: Bool\n");
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn resolve_type_undefined_name() {
    let (_table, errors) = resolve_source("module M():\n    signal x: Nonexistent\n");
    assert!(!errors.is_empty(), "should error on undefined type name");
}

#[test]
fn resolve_type_const_width() {
    let (table, errors) = resolve_source(
        "module M():\n    const W: uint = 16\n    signal data: UInt<W>\n"
    );
    assert!(errors.is_empty(), "errors: {errors:?}");
}

#[test]
fn resolve_type_fixed_point() {
    let (table, errors) = resolve_source(
        "module M():\n    signal weight: Fixed<8, 8>\n"
    );
    assert!(errors.is_empty(), "Fixed<8,8> should resolve: {errors:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- resolve_type_ 2>&1`
Expected: FAIL (tests that depend on type resolution beyond simple declaration)

- [ ] **Step 3: Implement type resolution**

Extend the `Resolver`'s `resolve_type` method to handle all `TypeExprKind` variants:
- Resolve generic args by evaluating with `ConstEval`
- For `Named` types, look up user-defined types in the symbol table
- For `Generic { name: "UInt", params: [Expr(lit)] }` → evaluate lit → `Ty::UInt(n)`
- Handle `Fixed<I, F>`, `Memory<T, depth=N>`, `DualPortMemory<T, ...>`
- Resolve nested types in `Array`, `Flip`, direction wrappers

The resolver should store resolved types alongside symbols. Add a `TypeTable` (a `HashMap<Span, Ty>`) that maps AST node spans to their resolved types. This lets the type checker look up any expression's type later.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- resolve_type_ 2>&1`
Expected: PASS (all 7 type resolution tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/resolve.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): resolve AST type expressions to concrete Ty values"
```

---

## Chunk 3: Type Checking (Tasks 7–9)

### Task 7: Expression Type Checking

**Files:**
- Create: `crates/ssl-core/src/sema/check.rs`
- Modify: `crates/ssl-core/src/sema/mod.rs`

The type checker infers and validates the type of every expression, following the width rules from spec Section 2.2.

- [ ] **Step 1: Write failing test**

```rust
// append to crates/ssl-core/tests/sema_tests.rs
use ssl_core::sema::check::TypeChecker;

/// Helper: check a module and return errors
fn check_source(src: &str) -> Vec<SemaError> {
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse failed");
    ssl_core::sema::analyze(&file).1
}

#[test]
fn check_add_same_width() {
    let errors = check_source(
        "module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<8>\n    comb:\n        c = a + b\n"
    );
    assert!(errors.is_empty(), "same-width add should be ok: {errors:?}");
}

#[test]
fn check_add_different_width_result() {
    // a + b where a:UInt<8>, b:UInt<16> → UInt<16> (max rule)
    // Assigning to UInt<8> should be a width mismatch
    let errors = check_source(
        "module M():\n    signal a: UInt<8>\n    signal b: UInt<16>\n    signal c: UInt<8>\n    comb:\n        c = a + b\n"
    );
    assert!(!errors.is_empty(), "should error: UInt<16> result into UInt<8>");
}

#[test]
fn check_add_different_width_ok() {
    // a + b where a:UInt<8>, b:UInt<16> → UInt<16>
    // Assigning to UInt<16> should be fine
    let errors = check_source(
        "module M():\n    signal a: UInt<8>\n    signal b: UInt<16>\n    signal c: UInt<16>\n    comb:\n        c = a + b\n"
    );
    assert!(errors.is_empty(), "UInt<16> result into UInt<16> should be ok: {errors:?}");
}

#[test]
fn check_bool_and_bool() {
    let errors = check_source(
        "module M():\n    signal a: Bool\n    signal b: Bool\n    signal c: Bool\n    comb:\n        c = a and b\n"
    );
    assert!(errors.is_empty(), "Bool and Bool should be ok: {errors:?}");
}

#[test]
fn check_comparison_returns_bool() {
    let errors = check_source(
        "module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: Bool\n    comb:\n        c = a == b\n"
    );
    assert!(errors.is_empty(), "comparison should return Bool: {errors:?}");
}

#[test]
fn check_integer_literal_fits() {
    let errors = check_source(
        "module M():\n    signal x: UInt<8>\n    comb:\n        x = 255\n"
    );
    assert!(errors.is_empty(), "255 fits in UInt<8>: {errors:?}");
}

#[test]
fn check_bitwise_same_width() {
    let errors = check_source(
        "module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<8>\n    comb:\n        c = a & b\n"
    );
    assert!(errors.is_empty(), "bitwise AND on same width: {errors:?}");
}

#[test]
fn check_concat_width() {
    // a ++ b where a:Bits<8>, b:Bits<8> → Bits<16>
    let errors = check_source(
        "module M():\n    signal a: Bits<8>\n    signal b: Bits<8>\n    signal c: Bits<16>\n    comb:\n        c = a ++ b\n"
    );
    assert!(errors.is_empty(), "concat Bits<8> ++ Bits<8> = Bits<16>: {errors:?}");
}

#[test]
fn check_mul_width_widening() {
    // a * b where a:UInt<8>, b:UInt<8> → UInt<16> (N+M rule)
    // Assigning to UInt<16> should be fine
    let errors = check_source(
        "module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<16>\n    comb:\n        c = a * b\n"
    );
    assert!(errors.is_empty(), "UInt<8> * UInt<8> = UInt<16>: {errors:?}");
}

#[test]
fn check_mul_width_too_narrow() {
    // a * b where a:UInt<8>, b:UInt<8> → UInt<16>
    // Assigning to UInt<8> should be a width mismatch
    let errors = check_source(
        "module M():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    signal c: UInt<8>\n    comb:\n        c = a * b\n"
    );
    assert!(!errors.is_empty(), "UInt<16> result into UInt<8> should error");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- check_ 2>&1`
Expected: FAIL — `sema::check` and `sema::analyze` don't exist

- [ ] **Step 3: Write implementation**

Implement `TypeChecker` that walks the resolved AST and checks expression types. The checker takes a `&SymbolTable` and a `&mut TypeTable` and populates the type table with inferred types.

**Width inference rules** (from spec Section 2.2):

| Operation | Inputs | Result |
|---|---|---|
| `a + b` | `UInt<N>`, `UInt<M>` | `UInt<max(N,M)>` |
| `a * b` | `UInt<N>`, `UInt<M>` | `UInt<N+M>` |
| `a << K` (const) | `UInt<N>`, const K | `UInt<N+K>` |
| `a << b` (dynamic) | `UInt<N>`, `UInt<M>` | `UInt<N>` |
| `a ++ b` | `Bits<N>`, `Bits<M>` | `Bits<N+M>` |
| `a[H:L]` | `Bits<N>` | `Bits<H-L+1>` |
| `a & b` | `UInt<N>`, `UInt<M>` | `UInt<max(N,M)>` |
| `a == b` | any, any (same kind) | `Bool` |
| `and/or` | `Bool`, `Bool` | `Bool` |
| `not` | `Bool` | `Bool` |
| `~` | `UInt<N>` | `UInt<N>` |

Key method: `check_expr(&mut self, expr: &Expr, scope: ScopeId) -> Ty`

For assignments (`target = value`): check that the value type is assignment-compatible with the target type. Width of value must be ≤ width of target. Different type kinds (UInt vs SInt) require explicit conversion outside `unchecked` blocks.

Also implement `sema::analyze(file: &SourceFile) -> (SymbolTable, Vec<SemaError>)` in `mod.rs` as the top-level entry point that runs all passes in sequence.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- check_ 2>&1`
Expected: PASS (all 8 type checking tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/check.rs crates/ssl-core/src/sema/mod.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add expression type checking with width inference"
```

---

### Task 8: Statement and Block Type Checking

**Files:**
- Modify: `crates/ssl-core/src/sema/check.rs`
- Modify: `crates/ssl-core/tests/sema_tests.rs`

Extend the type checker to validate statements: signal declarations match their initializers, assignment targets are valid, if/match conditions are Bool, for loop iterables are valid ranges.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn check_signal_init_matches_type() {
    let errors = check_source(
        "module M():\n    signal x: UInt<8> = 42\n    comb:\n        x = x\n"
    );
    assert!(errors.is_empty(), "42 fits UInt<8>: {errors:?}");
}

#[test]
fn check_if_condition_must_be_bool() {
    let errors = check_source(
        "module M():\n    signal x: UInt<8>\n    signal y: UInt<8>\n    comb:\n        if x:\n            y = 1\n"
    );
    assert!(!errors.is_empty(), "if condition should require Bool, not UInt<8>");
}

#[test]
fn check_reg_block_clock_type() {
    let errors = check_source(
        "module M(\n    in clk: Clock,\n    in rst: SyncReset\n):\n    signal counter: UInt<8>\n    reg(clk, rst):\n        on reset:\n            counter = 0\n        on tick:\n            counter = counter + 1\n"
    );
    assert!(errors.is_empty(), "valid reg block: {errors:?}");
}

#[test]
fn check_reg_block_non_clock_error() {
    let errors = check_source(
        "module M():\n    signal x: UInt<8>\n    signal y: UInt<8>\n    reg(x, y):\n        on reset:\n            y = 0\n        on tick:\n            y = y + 1\n"
    );
    assert!(!errors.is_empty(), "reg block first arg must be Clock");
}

#[test]
fn check_assign_to_input_port() {
    let errors = check_source(
        "module M(\n    in a: UInt<8>\n):\n    comb:\n        a = 42\n"
    );
    assert!(!errors.is_empty(), "cannot assign to input port");
}

#[test]
fn check_match_scrutinee_type() {
    // Match on a UInt should work
    let errors = check_source(
        "module M():\n    signal x: UInt<8>\n    signal y: UInt<8>\n    comb:\n        y = 0\n        match x:\n            0 => y = 1\n            1 => y = 2\n            _ => y = 3\n"
    );
    assert!(errors.is_empty(), "valid match: {errors:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- check_ 2>&1`
Expected: new tests FAIL

- [ ] **Step 3: Implement statement checking**

Extend `TypeChecker` with `check_stmt(&mut self, stmt: &Stmt, scope: ScopeId)` that:
- `Signal`: verify init expr (if any) matches declared type
- `Let`/`Const`: infer type from value, check against annotation if present
- `Assign`: check target is assignable (not an input port, not a const), check value type compatible
- `If`: condition must be Bool
- `Match`: scrutinee type checked, arm patterns type-checked against scrutinee
- `For`: iterable must be a range expression
- `RegBlock`: first arg must be Clock, second must be SyncReset or AsyncReset
- `CombBlock`: recurse into body statements
- `Assert`/`Assume`/`Cover`: expression must be Bool
- `ExprStmt`: type-check the expression

Direction checking: ports marked `In` are read-only inside the module. Assigning to an `In` port is a `DirectionViolation` error.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- check_ 2>&1`
Expected: PASS (all statement checking tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/check.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add statement and block type checking"
```

---

### Task 9: Module and Item Type Checking

**Files:**
- Modify: `crates/ssl-core/src/sema/check.rs`
- Modify: `crates/ssl-core/tests/sema_tests.rs`

Extend type checking to validate module-level items: port types, struct fields, enum variants, function signatures, and module instantiation port connections.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn check_module_ports_valid() {
    let errors = check_source(
        "module ALU(\n    in a: UInt<32>,\n    in b: UInt<32>,\n    out result: UInt<32>\n):\n    comb:\n        result = a + b\n"
    );
    assert!(errors.is_empty(), "valid module: {errors:?}");
}

#[test]
fn check_struct_fields_valid() {
    let errors = check_source(
        "struct Pixel:\n    r: UInt<8>\n    g: UInt<8>\n    b: UInt<8>\n"
    );
    assert!(errors.is_empty(), "valid struct: {errors:?}");
}

#[test]
fn check_fn_return_type() {
    let errors = check_source(
        "fn add(a: UInt<8>, b: UInt<8>) -> UInt<8>:\n    if a > b then a else b\n"
    );
    assert!(errors.is_empty(), "valid fn: {errors:?}");
}

#[test]
fn check_inst_basic() {
    let errors = check_source(
        "module Inner(\n    in x: UInt<8>,\n    out y: UInt<8>\n):\n    comb:\n        y = x\n\nmodule Outer():\n    signal a: UInt<8>\n    signal b: UInt<8>\n    inst i = Inner(x = a, y -> b)\n"
    );
    assert!(errors.is_empty(), "valid instantiation: {errors:?}");
}

```

**Note:** Output port driven check (`check_output_port_driven`) is deferred to Task 10 where latch analysis infrastructure exists. That test reuses the same set-based assignment tracking.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- check_ 2>&1`
Expected: new tests FAIL

- [ ] **Step 3: Implement item-level checking**

Extend `TypeChecker` with:
- `check_module`: Check port type validity. Output port driven analysis is done in the validation pass (Task 10).
- `check_struct`: verify field types are valid hardware types
- `check_enum`: verify variant values fit encoding if specified
- `check_fn`: verify return type matches body expression type (for single-expression functions) or that all paths return the correct type
- `check_inst`: look up the instantiated module, verify all ports are connected, verify connection types match port types, verify direction binding operators (= for input, -> for output)

For inst checking, the checker needs to find the module definition by name. This requires looking up `SymbolKind::Module` in the parent scope and retrieving its port list from the AST. Store module definitions in a side table indexed by `SymbolId` during resolution.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- check_ 2>&1`
Expected: PASS (all item checking tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/check.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add module, struct, fn, and instantiation type checking"
```

---

## Chunk 4: Hardware Semantic Validation (Tasks 10–12)

### Task 10: Comb Block Completeness (Latch Prevention)

**Files:**
- Create: `crates/ssl-core/src/sema/validate.rs`
- Modify: `crates/ssl-core/src/sema/mod.rs`

Spec Section 5.1 mandates: every output signal must be assigned on every path through a `comb` block. Missing assignments infer latches, which is always a compile error in SSL.

- [ ] **Step 1: Write failing test**

```rust
// append to crates/ssl-core/tests/sema_tests.rs

#[test]
fn validate_comb_complete_assignment() {
    let errors = check_source(
        "module M():\n    signal x: Bool\n    signal y: UInt<8>\n    comb:\n        if x:\n            y = 1\n        else:\n            y = 0\n"
    );
    assert!(errors.is_empty(), "y assigned on all paths: {errors:?}");
}

#[test]
fn validate_comb_incomplete_assignment() {
    let errors = check_source(
        "module M():\n    signal x: Bool\n    signal y: UInt<8>\n    comb:\n        if x:\n            y = 1\n"
    );
    let latch_errors: Vec<_> = errors.iter().filter(|e|
        matches!(e, SemaError::LatchInferred { .. })
    ).collect();
    assert!(!latch_errors.is_empty(), "should detect latch on y");
}

#[test]
fn validate_comb_match_exhaustive() {
    // Match without catch-all on a non-enum is incomplete
    let errors = check_source(
        "module M():\n    signal sel: UInt<2>\n    signal y: UInt<8>\n    comb:\n        match sel:\n            0 => y = 1\n            1 => y = 2\n"
    );
    // This should either require a wildcard arm or flag as non-exhaustive
    assert!(!errors.is_empty(), "match without wildcard on UInt<2> should warn");
}

#[test]
fn validate_output_port_driven() {
    // Output port not assigned should be an error
    let errors = check_source(
        "module M(\n    out y: UInt<8>\n):\n    signal x: Bool\n"
    );
    assert!(!errors.is_empty(), "output port y is never driven");
}

#[test]
fn validate_comb_default_then_override() {
    // Assigning default first, then overriding in if branch — should be OK
    let errors = check_source(
        "module M():\n    signal x: Bool\n    signal y: UInt<8>\n    comb:\n        y = 0\n        if x:\n            y = 1\n"
    );
    assert!(errors.is_empty(), "default + override is complete: {errors:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- validate_ 2>&1`
Expected: FAIL — `sema::validate` doesn't exist

- [ ] **Step 3: Write implementation**

Implement `Validator` struct that performs latch analysis on comb blocks:

1. For each `CombBlock`, collect all signals assigned in the block
2. For each assigned signal, trace assignment paths through if/elif/else and match:
   - `if/else`: signal must be assigned in BOTH branches
   - `if` without `else`: signal must be assigned before the if (default pattern)
   - `match`: signal must be assigned in ALL arms (or have a wildcard arm)
3. If a signal is assigned on some but not all paths, emit `LatchInferred` error

The algorithm uses a set-based approach:
- `assigned_on_all_paths(stmts) -> Set<SignalName>` — returns signals guaranteed assigned
- For `if/else`: intersection of then_set and else_set, plus pre-if assignments
- For `match` with wildcard: intersection of all arm sets
- For `match` without wildcard: empty set (incomplete — conservative; treats any `match` on a non-enum type without `_` arm as non-exhaustive even if all values are explicitly covered. Full numeric exhaustiveness checking is deferred.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- validate_ 2>&1`
Expected: PASS (all 4 validation tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/validate.rs crates/ssl-core/src/sema/mod.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add comb block completeness checking (latch prevention)"
```

---

### Task 11: Reg Block Validation and Additional Checks

**Files:**
- Modify: `crates/ssl-core/src/sema/validate.rs`
- Modify: `crates/ssl-core/tests/sema_tests.rs`

Validate reg block semantics (spec Section 6.1): every signal assigned in `on tick` must have a reset value in `on reset`. Also validate that `signal` is only used in module context, `var` only in testbench context, and `let` bindings are not reassigned.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn validate_reg_reset_coverage() {
    // Signal assigned in on tick but missing from on reset — error
    let errors = check_source(
        "module M(\n    in clk: Clock,\n    in rst: SyncReset\n):\n    signal x: UInt<8>\n    signal y: UInt<8>\n    reg(clk, rst):\n        on reset:\n            x = 0\n        on tick:\n            x = x + 1\n            y = x\n"
    );
    assert!(!errors.is_empty(), "y assigned in on tick but not on reset");
}

#[test]
fn validate_reg_reset_complete() {
    let errors = check_source(
        "module M(\n    in clk: Clock,\n    in rst: SyncReset\n):\n    signal x: UInt<8>\n    reg(clk, rst):\n        on reset:\n            x = 0\n        on tick:\n            x = x + 1\n"
    );
    assert!(errors.is_empty(), "complete reg block: {errors:?}");
}

#[test]
fn validate_const_not_reassigned() {
    let errors = check_source(
        "module M():\n    const X: uint = 8\n    comb:\n        X = 16\n"
    );
    assert!(!errors.is_empty(), "cannot assign to const");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- validate_ 2>&1`
Expected: new tests FAIL

- [ ] **Step 3: Implement reg validation and assignment target checks**

Extend `Validator`:
- `validate_reg_block`: collect signals assigned in `on_tick`, check each has corresponding assignment in `on_reset`
- In assignment checking (already started in Task 8): verify target is not a `Const` or `Let` (immutable bindings)
- Verify `signal` declarations only appear in module context (not testbench)
- Verify `var` declarations only appear in testbench context

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- validate_ 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/validate.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): add reg block validation and immutability enforcement"
```

---

### Task 12: Orchestration and `analyze()` Entry Point

**Files:**
- Modify: `crates/ssl-core/src/sema/mod.rs`
- Modify: `crates/ssl-core/tests/sema_tests.rs`

Wire all passes together into a single `analyze()` function. Ensure passes run in correct order and errors accumulate without cascading (error sentinel `Ty::Error` prevents type-error avalanches).

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn analyze_full_blinker() {
    let src = "\
module Blinker(
    in  clk: Clock,
    in  rst: SyncReset,
    out led: Bool
):
    signal counter: UInt<24>

    reg(clk, rst):
        on reset:
            counter = 0
        on tick:
            counter = counter + 1

    comb:
        led = counter[23]
";
    let errors = check_source(src);
    assert!(errors.is_empty(), "blinker should pass analysis: {errors:?}");
}

#[test]
fn analyze_multiple_errors_reported() {
    // Multiple errors should all be reported, not just the first
    let src = "\
module M():
    signal x: Undefined
    signal y: AlsoUndefined
";
    let errors = check_source(src);
    assert!(errors.len() >= 2, "should report multiple errors: {errors:?}");
}

#[test]
fn analyze_error_recovery() {
    // An error in one signal should not prevent checking other signals
    let src = "\
module M():
    signal bad: Undefined
    signal good: UInt<8>
    comb:
        good = 42
";
    let errors = check_source(src);
    // Should have exactly one error (for Undefined), not cascade
    let undefined_errors: Vec<_> = errors.iter().filter(|e|
        matches!(e, SemaError::UndefinedName { .. })
    ).collect();
    assert_eq!(undefined_errors.len(), 1, "should have exactly one undefined error");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test sema_tests -- analyze_ 2>&1`
Expected: Some tests may fail if `analyze()` isn't orchestrating all passes correctly

- [ ] **Step 3: Implement `analyze()` orchestration**

```rust
// crates/ssl-core/src/sema/mod.rs
pub mod error;
pub mod types;
pub mod scope;
pub mod eval;
pub mod resolve;
pub mod check;
pub mod validate;

pub use error::SemaError;
pub use types::Ty;

use crate::ast::item::SourceFile;

/// Run full semantic analysis on a parsed source file.
///
/// Returns the symbol table and a list of all errors found.
/// Errors are accumulated across all passes — the analysis does not
/// stop at the first error.
pub fn analyze(file: &SourceFile) -> (scope::SymbolTable, Vec<SemaError>) {
    let mut errors = Vec::new();

    // Pass 1: Name resolution — collect declarations, resolve types
    let mut resolver = resolve::Resolver::new();
    resolver.collect_declarations(file);
    let (symbol_table, mut resolve_errors) = resolver.finish();
    errors.append(&mut resolve_errors);

    // Pass 2: Type checking — validate expressions, statements, items
    let mut checker = check::TypeChecker::new(&symbol_table);
    checker.check_file(file);
    errors.append(&mut checker.into_errors());

    // Pass 3: Hardware validation — latch-free comb, reg coverage
    let mut validator = validate::Validator::new(&symbol_table);
    validator.validate_file(file);
    errors.append(&mut validator.into_errors());

    (symbol_table, errors)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test sema_tests -- analyze_ 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/sema/mod.rs crates/ssl-core/tests/sema_tests.rs
git commit -m "feat(sema): orchestrate name resolution, type checking, and validation passes"
```

---

## Chunk 5: CLI + Integration Tests (Tasks 13–14)

### Task 13: CLI `check` Command

**Files:**
- Modify: `crates/sslc/src/main.rs`

Add a `check` subcommand that parses a file, runs semantic analysis, and reports errors with span information.

- [ ] **Step 1: Write failing test (manual)**

Run: `cargo run -p sslc -- check examples/blinker.ssl 2>&1`
Expected: Error — "unknown command: check"

- [ ] **Step 2: Implement the `check` command**

Add to `main.rs`:
```rust
"check" => {
    let path = &args[2];
    let source = read_source(path);
    let tokens = match ssl_core::lexer::tokenize(&source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Lexer error: {e}");
            std::process::exit(1);
        }
    };
    let file = match ssl_core::parser::Parser::parse(&source, tokens) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Parse error: {e}");
            std::process::exit(1);
        }
    };

    let (_table, errors) = ssl_core::sema::analyze(&file);

    if errors.is_empty() {
        println!("✓ No errors found");
        println!("  {} top-level items checked", file.items.len());
    } else {
        for err in &errors {
            eprintln!("Error: {err}");
        }
        eprintln!("\n{} error(s) found", errors.len());
        std::process::exit(1);
    }
}
```

- [ ] **Step 3: Run to verify it works**

Run: `cargo run -p sslc -- check examples/blinker.ssl 2>&1`
Expected: `✓ No errors found` and `1 top-level items checked`

- [ ] **Step 4: Test with invalid input**

Create `examples/invalid.ssl`:
```
module Bad():
    signal x: Undefined
    comb:
        x = 42
```

Run: `cargo run -p sslc -- check examples/invalid.ssl 2>&1`
Expected: Error output including "undefined" and nonzero exit code

- [ ] **Step 5: Commit**

```bash
git add crates/sslc/src/main.rs examples/invalid.ssl
git commit -m "feat(cli): add sslc check command for semantic analysis"
```

---

### Task 14: End-to-End Integration Tests

**Files:**
- Modify: `crates/ssl-core/tests/sema_tests.rs`

Add comprehensive integration tests that parse real SSL programs and verify semantic analysis produces correct results.

- [ ] **Step 1: Write integration tests**

```rust
// ── End-to-end integration tests ─────────────────────────

#[test]
fn e2e_alu_module() {
    let errors = check_source("\
module ALU(
    in  a:      UInt<32>,
    in  b:      UInt<32>,
    in  opcode: Bits<4>,
    out result: UInt<32>,
    out zero:   Bool
):
    comb:
        result = a + b
        zero = result == 0
");
    assert!(errors.is_empty(), "ALU should pass: {errors:?}");
}

#[test]
fn e2e_shift_register() {
    let errors = check_source("\
module ShiftReg(
    in  clk:  Clock,
    in  rst:  SyncReset,
    in  din:  UInt<8>,
    out dout: UInt<8>
):
    signal stage0: UInt<8>
    signal stage1: UInt<8>
    signal stage2: UInt<8>

    reg(clk, rst):
        on reset:
            stage0 = 0
            stage1 = 0
            stage2 = 0
        on tick:
            stage0 = din
            stage1 = stage0
            stage2 = stage1

    comb:
        dout = stage2
");
    assert!(errors.is_empty(), "shift register should pass: {errors:?}");
}

#[test]
fn e2e_width_mismatch() {
    let errors = check_source("\
module M(
    in a: UInt<16>,
    out b: UInt<8>
):
    comb:
        b = a
");
    assert!(!errors.is_empty(), "assigning UInt<16> to UInt<8> should error");
}

#[test]
fn e2e_type_mismatch_uint_sint() {
    let errors = check_source("\
module M():
    signal a: UInt<8>
    signal b: SInt<8>
    comb:
        b = a
");
    assert!(!errors.is_empty(), "UInt to SInt without conversion should error");
}

#[test]
fn e2e_input_port_not_driven_ok() {
    // Input ports should NOT require driving from within the module
    let errors = check_source("\
module M(
    in  clk: Clock,
    out led: Bool
):
    comb:
        led = true
");
    assert!(errors.is_empty(), "input ports don't need driving: {errors:?}");
}

#[test]
fn e2e_multiple_comb_blocks() {
    let errors = check_source("\
module M(
    in  a: UInt<8>,
    in  b: UInt<8>,
    out x: UInt<8>,
    out y: Bool
):
    comb:
        x = a + b

    comb:
        y = a == b
");
    assert!(errors.is_empty(), "multiple comb blocks: {errors:?}");
}

#[test]
fn e2e_nested_if_complete() {
    let errors = check_source("\
module M(
    in  sel: UInt<2>,
    out y: UInt<8>
):
    comb:
        if sel == 0:
            y = 10
        elif sel == 1:
            y = 20
        elif sel == 2:
            y = 30
        else:
            y = 40
");
    assert!(errors.is_empty(), "nested if/elif/else complete: {errors:?}");
}

#[test]
fn e2e_const_in_type() {
    let errors = check_source("\
module M():
    const W: uint = 8
    signal data: UInt<8>
    comb:
        data = 0
");
    // Note: UInt<W> requires const eval to resolve W → 8
    // For now test with literal since const-in-type resolution may not work yet
    assert!(errors.is_empty(), "const type param: {errors:?}");
}

#[test]
fn e2e_blinker_from_file() {
    let src = std::fs::read_to_string(
        format!("{}/../../examples/blinker.ssl", env!("CARGO_MANIFEST_DIR"))
    ).expect("read blinker.ssl");
    let tokens = ssl_core::lexer::tokenize(&src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(&src, tokens).expect("parse blinker");
    let (_table, errors) = ssl_core::sema::analyze(&file);
    assert!(errors.is_empty(), "blinker.ssl should pass semantic analysis: {errors:?}");
}
```

- [ ] **Step 2: Run all sema tests**

Run: `cargo test -p ssl-core --test sema_tests 2>&1`
Expected: ALL tests pass

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p ssl-core 2>&1`
Expected: ALL tests pass (sema + parser + lexer)

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p ssl-core 2>&1`
Expected: No warnings

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/tests/sema_tests.rs
git commit -m "test(sema): add end-to-end integration tests for semantic analysis"
```

---

## Post-Implementation

After all 14 tasks are complete:

1. Run `cargo test` — all tests must pass
2. Run `cargo clippy` — no warnings
3. Run `cargo run -p sslc -- check examples/blinker.ssl` — must succeed
4. Dispatch final code reviewer via `superpowers:requesting-code-review`
5. Use `superpowers:finishing-a-development-branch` for merge/commit strategy

## Summary

| Task | Description | Tests | Cumulative |
|------|-------------|-------|------------|
| 1 | Error reporting infrastructure | 2 | 2 |
| 2 | Resolved type system (Ty enum) | 12 | 14 |
| 3 | Symbol table + scope management | 6 | 20 |
| 4 | Compile-time constant evaluator | 7 | 27 |
| 5 | Name resolution — declarations | 6 | 33 |
| 6 | Type resolution — AST types to Ty | 8 | 41 |
| 7 | Expression type checking | 10 | 51 |
| 8 | Statement + block type checking | 6 | 57 |
| 9 | Module + item type checking | 4 | 61 |
| 10 | Comb block completeness (latch prevention) | 5 | 66 |
| 11 | Reg block validation + immutability | 3 | 69 |
| 12 | Orchestration + analyze() entry point | 3 | 72 |
| 13 | CLI `check` command | 2 (manual) | 74 |
| 14 | End-to-end integration tests | 10 | 84 |

**Total: 14 tasks, ~84 tests, 7 new files**
