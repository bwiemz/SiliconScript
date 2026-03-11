# Phase 2: Parser + AST — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a recursive descent parser that transforms SiliconScript's token stream into a typed Abstract Syntax Tree, covering the core language grammar (Sections 1-7) and basic formal verification.

**Architecture:** Hand-written recursive descent parser with Pratt parsing for expression precedence. Consumes `Vec<Spanned<Token>>` from Phase 1 lexer, produces AST with full source span information. Zero new dependencies. Indentation-based blocks use the existing Indent/Dedent tokens as delimiters.

**Tech Stack:** Rust 1.85+, no new dependencies (uses existing ssl-core types)

**Spec references:**
- `docs/superpowers/specs/2026-03-10-siliconscript-language-design.md` — Sections 1-7
- `docs/superpowers/specs/2026-03-10-siliconscript-advanced-features.md` — Addenda A.20-A.36

---

## Scope

**Included (Sections 1-7 + basic formal):**
- Full expression grammar with 16-level operator precedence
- Type expressions (generics, arrays, clock domains)
- Module definitions with port lists and generic parameters
- Signal, let, const declarations and type aliases
- Comb blocks with match/if/priority/parallel
- Reg blocks with on reset/on tick
- Struct, enum, interface definitions (with groups, properties)
- Fn definitions (pure combinational functions)
- FSM blocks with transitions, outputs, timeout
- Pipeline blocks with stages and backpressure modes
- Instance declarations (inst)
- Generate constructs (gen for, gen if)
- Basic formal: assert [always], assume, cover, static_assert
- Test blocks (Section 7 inline tests)
- Import statements, extern module declarations
- Attribute parsing (@annotation), doc comments

**Deferred to Phase 3:**
- Testbench blocks, task definitions, var declarations (Section 10)
- Prove blocks, equiv statements (Section 8 advanced)
- Systolic, dataflow blocks (Section 9)
- ISA blocks (Section 11)
- Multi-line expression continuation across indented lines

---

## File Structure

### New Files

| File | Responsibility | Est. Lines |
|------|---------------|------------|
| `crates/ssl-core/src/ast/mod.rs` | Re-exports, common types (Ident, Attribute, DocComment) | ~60 |
| `crates/ssl-core/src/ast/expr.rs` | Expression AST nodes (ExprKind enum, BinOp, UnaryOp) | ~150 |
| `crates/ssl-core/src/ast/types.rs` | Type expression AST nodes | ~80 |
| `crates/ssl-core/src/ast/stmt.rs` | Statement AST nodes (declarations, assignments, blocks) | ~180 |
| `crates/ssl-core/src/ast/item.rs` | Top-level item AST nodes (module, struct, enum, fn, etc.) | ~250 |
| `crates/ssl-core/src/parser/mod.rs` | Parser struct, error types, helpers, entry point | ~200 |
| `crates/ssl-core/src/parser/expr.rs` | Expression parsing (Pratt algorithm) | ~300 |
| `crates/ssl-core/src/parser/types.rs` | Type expression parsing | ~120 |
| `crates/ssl-core/src/parser/stmt.rs` | Statement and block parsing | ~300 |
| `crates/ssl-core/src/parser/item.rs` | Top-level item parsing | ~350 |
| `crates/ssl-core/tests/parser_tests.rs` | Parser integration tests | ~400 |

### Modified Files

| File | Change |
|------|--------|
| `crates/ssl-core/src/lib.rs` | Add `pub mod ast; pub mod parser;` |
| `crates/ssl-core/src/lexer/token.rs` | Add 18 new keyword tokens + `<->` operator + `is_keyword` update |
| `crates/ssl-core/tests/lexer_tests.rs` | Add test for new tokens |
| `crates/sslc/src/main.rs` | Add `parse` command |

---

## Design Notes

### Indentation-Based Block Parsing

The lexer emits `Indent`/`Dedent` tokens. A block is: `Colon Newline Indent BODY Dedent`. The parser helper:

```rust
fn parse_block<T>(&mut self, mut f: impl FnMut(&mut Self) -> Result<T, ParseError>) -> Result<Vec<T>, ParseError> {
    self.expect_token(Token::Colon)?;
    self.skip_newlines();
    self.expect_token(Token::Indent)?;
    let mut items = Vec::new();
    while !self.check(Token::Dedent) && !self.is_at_end() {
        self.skip_newlines();
        if self.check(Token::Dedent) || self.is_at_end() { break; }
        items.push(f(self)?);
        self.skip_newlines();
    }
    self.expect_token(Token::Dedent)?;
    Ok(items)
}
```

### Expression Precedence Table (Pratt Parser)

Higher number = tighter binding:

| Prec | Operators | Associativity |
|------|-----------|---------------|
| 1 | `implies` | Right |
| 2 | `or` | Left |
| 3 | `and` | Left |
| 4 | `\|` (bitwise OR) | Left |
| 5 | `^` (bitwise XOR) | Left |
| 6 | `&` (bitwise AND) | Left |
| 7 | `== !=` | Left |
| 8 | `< > <= >=` | Left |
| 9 | `.. ..=` | Non-assoc |
| 10 | `<< >> >>>` | Left |
| 11 | `++` | Left |
| 12 | `+ -` | Left |
| 13 | `* / %` | Left |
| 14 | `**` | Right |

Unary prefix (`not`, `~`, `-`) parsed in `parse_unary()` before entering the Pratt loop. Postfix ops (`.field`, `[idx]`, `[H:L]`, `(args)`) parsed inside the loop at highest priority. Pipe `|>` parsed specially — lowest precedence, RHS restricted to function call syntax.

### Identifier Text Extraction

`Token::Ident` doesn't carry text. The parser slices source via span:

```rust
fn text(&self, span: Span) -> &str {
    &self.source[span.start as usize..span.end as usize]
}
```

AST nodes use owned `String` for identifiers (avoids lifetime complexity).

### Newline Handling

- `Newline` tokens separate statements within blocks
- `skip_newlines()` consumes consecutive Newlines
- Inside delimited sequences (parenthesized port lists, generic params, function args), Newlines are skipped automatically
- Multi-line expression continuation (pipe across lines) is deferred to Phase 3

---

## Chunk 1: Lexer Additions + AST Type Definitions

### Task 1: Add New Lexer Tokens

**Files:** `crates/ssl-core/src/lexer/token.rs`, `crates/ssl-core/tests/lexer_tests.rs`

#### TDD Steps

- [ ] **1a. Write failing test in `lexer_tests.rs`**

Add this test to the end of `crates/ssl-core/tests/lexer_tests.rs`:

```rust
#[test]
fn phase2_new_keywords() {
    let tokens = token_types(
        "testbench task var drive peek settle print \
         systolic dataflow \
         isa instr format registers encoding_width \
         prove equiv constrain \
         override"
    );
    assert_eq!(tokens, vec![
        Token::KwTestbench, Token::KwTask, Token::KwVar,
        Token::KwDrive, Token::KwPeek, Token::KwSettle, Token::KwPrint,
        Token::KwSystolic, Token::KwDataflow,
        Token::KwIsa, Token::KwInstr, Token::KwFormat,
        Token::KwRegisters, Token::KwEncodingWidth,
        Token::KwProve, Token::KwEquiv, Token::KwConstrain,
        Token::KwOverride,
    ]);
}

#[test]
fn phase2_biarrow_operator() {
    let tokens = token_types("a <-> b");
    assert_eq!(tokens, vec![Token::Ident, Token::BiArrow, Token::Ident]);
}

#[test]
fn phase2_biarrow_no_conflict() {
    // Ensure <-> doesn't break < or ->
    let tokens = token_types("a < b -> c <-> d");
    assert_eq!(tokens, vec![
        Token::Ident, Token::Less, Token::Ident,
        Token::ThinArrow, Token::Ident,
        Token::BiArrow, Token::Ident,
    ]);
}
```

Run and confirm failure:

```bash
cargo test -p ssl-core --test lexer_tests phase2_ 2>&1 | head -20
# Expected: compilation error (unknown variants)
```

- [ ] **1b. Add token variants to `token.rs`**

In `crates/ssl-core/src/lexer/token.rs`, add these variants to the `Token` enum.

After the `KwTest` variant, add:

```rust
    // Simulation Keywords (Phase 2)
    #[token("testbench")]
    KwTestbench,
    #[token("task")]
    KwTask,
    #[token("var")]
    KwVar,
    #[token("drive")]
    KwDrive,
    #[token("peek")]
    KwPeek,
    #[token("settle")]
    KwSettle,
    #[token("print")]
    KwPrint,

    // AI Accelerator Keywords (Phase 2)
    #[token("systolic")]
    KwSystolic,
    #[token("dataflow")]
    KwDataflow,

    // ISA Keywords (Phase 2)
    #[token("isa")]
    KwIsa,
    #[token("instr")]
    KwInstr,
    #[token("format")]
    KwFormat,
    #[token("registers")]
    KwRegisters,
    #[token("encoding_width")]
    KwEncodingWidth,

    // Formal Verification Keywords (Phase 2)
    #[token("prove")]
    KwProve,
    #[token("equiv")]
    KwEquiv,
    #[token("constrain")]
    KwConstrain,

    // Other Keywords (Phase 2)
    #[token("override")]
    KwOverride,
```

In the Operators section, add before the `#[token("**")]` line (multi-char operators, longest first):

```rust
    #[token("<->", priority = 6)]
    BiArrow,
```

The `priority = 6` ensures `<->` is matched before `<` (priority unset/1) and `->` (no priority). Logos matches longest first for equal priority, but explicit priority guarantees correctness.

Update `is_keyword()` — append these lines before the closing `)`:

```rust
                | Token::KwTestbench
                | Token::KwTask
                | Token::KwVar
                | Token::KwDrive
                | Token::KwPeek
                | Token::KwSettle
                | Token::KwPrint
                | Token::KwSystolic
                | Token::KwDataflow
                | Token::KwIsa
                | Token::KwInstr
                | Token::KwFormat
                | Token::KwRegisters
                | Token::KwEncodingWidth
                | Token::KwProve
                | Token::KwEquiv
                | Token::KwConstrain
                | Token::KwOverride
```

- [ ] **1c. Verify tests pass**

```bash
cargo test -p ssl-core --test lexer_tests phase2_ 2>&1
# Expected: 3 tests passed
cargo test -p ssl-core --test lexer_tests 2>&1
# Expected: all existing tests still pass (no regressions)
```

- [ ] **1d. Commit**

```bash
git add crates/ssl-core/src/lexer/token.rs crates/ssl-core/tests/lexer_tests.rs
git commit -m "feat(lexer): add Phase 2 keyword tokens and <-> operator

Add 18 keyword tokens (testbench, task, var, drive, peek, settle, print,
systolic, dataflow, isa, instr, format, registers, encoding_width,
prove, equiv, constrain, override) and BiArrow (<->) operator.
Update is_keyword() and add lexer tests."
```

---

### Task 2: AST Expression and Type Nodes

**Files:** `crates/ssl-core/src/ast/mod.rs`, `crates/ssl-core/src/ast/expr.rs`, `crates/ssl-core/src/ast/types.rs`

#### TDD Steps

- [ ] **2a. Create `crates/ssl-core/src/ast/` directory and files**

Create `crates/ssl-core/src/ast/mod.rs`:

```rust
pub mod expr;
pub mod types;
pub mod stmt;
pub mod item;

use crate::span::{Span, Spanned};

pub type Ident = Spanned<String>;

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: Ident,
    pub args: Vec<Spanned<expr::Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocComment {
    pub text: String,
    pub span: Span,
}
```

Create `crates/ssl-core/src/ast/expr.rs`:

```rust
use crate::span::{Span, Spanned};
use crate::lexer::NumericLiteral;
use super::Ident;
use super::types::TypeExpr;

pub type Expr = Spanned<ExprKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // Literals
    IntLiteral(NumericLiteral),
    StringLiteral(String),
    BoolLiteral(bool),
    Ident(String),

    // Operators
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary { op: UnaryOp, operand: Box<Expr> },

    // Access
    FieldAccess { object: Box<Expr>, field: Ident },
    MethodCall { object: Box<Expr>, method: Ident, args: Vec<CallArg> },
    Call { callee: Box<Expr>, args: Vec<CallArg> },
    Index { array: Box<Expr>, index: Box<Expr> },
    BitSlice { value: Box<Expr>, high: Box<Expr>, low: Box<Expr> },

    // Special
    Pipe { input: Box<Expr>, callee: Box<Expr>, args: Vec<CallArg> },
    IfExpr { condition: Box<Expr>, then_expr: Box<Expr>, else_expr: Box<Expr> },
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    StructLiteral { type_name: Ident, fields: Vec<(Ident, Expr)> },
    ArrayLiteral(Vec<Expr>),
    Paren(Box<Expr>),

    // Formal (bounded temporal)
    Next { expr: Box<Expr>, count: Option<Box<Expr>> },
    Eventually { expr: Box<Expr>, depth: Box<Expr> },

    // Type-related
    TypeCast { expr: Box<Expr>, ty: TypeExpr },

    // Safety
    Unchecked(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod, Pow,
    BitAnd, BitOr, BitXor,
    Shl, Shr, ArithShr,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or, Implies,
    Concat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    BitNot,
    LogicalNot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallArg {
    pub name: Option<Ident>,
    pub value: Expr,
}
```

Create `crates/ssl-core/src/ast/types.rs`:

```rust
use crate::span::Spanned;
use super::Ident;
use super::expr::{Expr, CallArg};

pub type TypeExpr = Spanned<TypeExprKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    Named(String),
    Generic { name: String, params: Vec<GenericArg> },
    Array { element: Box<TypeExpr>, size: Expr },
    Clock { freq: Option<Expr>, edge: Option<ClockEdge> },
    SyncReset { polarity: Option<ResetPolarity> },
    AsyncReset { polarity: Option<ResetPolarity> },
    DirectionWrapper { dir: Direction, inner: Box<TypeExpr> },
    Flip(Box<TypeExpr>),
    DomainAnnotated { ty: Box<TypeExpr>, domain: Ident },
    PartialInterface { name: String, groups: Vec<Ident> },
    Memory { element: Box<TypeExpr>, params: Vec<CallArg> },
    DualPortMemory { element: Box<TypeExpr>, params: Vec<CallArg> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum GenericArg {
    Expr(Expr),
    Type(TypeExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockEdge { Rising, Falling, Dual }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetPolarity { ActiveHigh, ActiveLow }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { In, Out, InOut }

#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub name: Ident,
    pub kind: GenericKind,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericKind { Uint, Int, Bool, Float, StringKind, Type }
```

- [ ] **2b. Create stub files for `stmt.rs` and `item.rs`** (filled in Task 3)

Create `crates/ssl-core/src/ast/stmt.rs` (stub):

```rust
// Populated in Task 3
```

Create `crates/ssl-core/src/ast/item.rs` (stub):

```rust
// Populated in Task 3
```

- [ ] **2c. Verify compilation**

```bash
cargo check -p ssl-core 2>&1
# Expected: compiles successfully (stubs are valid empty modules)
```

- [ ] **2d. Write AST construction test**

Add `crates/ssl-core/tests/ast_tests.rs`:

```rust
use ssl_core::ast::expr::{BinOp, CallArg, Expr, ExprKind, UnaryOp};
use ssl_core::ast::types::{TypeExprKind, Direction, ClockEdge, GenericArg};
use ssl_core::ast::{Ident, Attribute};
use ssl_core::lexer::NumericLiteral;
use ssl_core::span::{Span, Spanned};

fn s(start: u32, end: u32) -> Span { Span::new(start, end) }
fn ident(name: &str) -> Ident { Spanned::new(name.to_string(), s(0, 0)) }

#[test]
fn construct_int_literal() {
    let expr = Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(42)),
        s(0, 2),
    );
    assert!(matches!(expr.node, ExprKind::IntLiteral(_)));
}

#[test]
fn construct_binary_expr() {
    let lhs = Box::new(Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(1)),
        s(0, 1),
    ));
    let rhs = Box::new(Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(2)),
        s(4, 5),
    ));
    let expr = Spanned::new(
        ExprKind::Binary { op: BinOp::Add, lhs, rhs },
        s(0, 5),
    );
    match &expr.node {
        ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Add),
        _ => panic!("expected Binary"),
    }
}

#[test]
fn construct_call_expr() {
    let callee = Box::new(Spanned::new(ExprKind::Ident("foo".into()), s(0, 3)));
    let arg = CallArg {
        name: Some(ident("x")),
        value: Spanned::new(ExprKind::IntLiteral(NumericLiteral::Decimal(5)), s(6, 7)),
    };
    let expr = Spanned::new(
        ExprKind::Call { callee, args: vec![arg] },
        s(0, 8),
    );
    match &expr.node {
        ExprKind::Call { args, .. } => assert_eq!(args.len(), 1),
        _ => panic!("expected Call"),
    }
}

#[test]
fn construct_named_type() {
    let ty = Spanned::new(TypeExprKind::Named("UInt".into()), s(0, 4));
    assert!(matches!(ty.node, TypeExprKind::Named(_)));
}

#[test]
fn construct_generic_type() {
    let width = Spanned::new(
        ExprKind::IntLiteral(NumericLiteral::Decimal(8)),
        s(5, 6),
    );
    let ty = Spanned::new(
        TypeExprKind::Generic {
            name: "UInt".into(),
            params: vec![GenericArg::Expr(width)],
        },
        s(0, 7),
    );
    match &ty.node {
        TypeExprKind::Generic { name, params } => {
            assert_eq!(name, "UInt");
            assert_eq!(params.len(), 1);
        }
        _ => panic!("expected Generic"),
    }
}

#[test]
fn construct_clock_type() {
    let ty = Spanned::new(
        TypeExprKind::Clock { freq: None, edge: Some(ClockEdge::Rising) },
        s(0, 5),
    );
    match &ty.node {
        TypeExprKind::Clock { edge, .. } => assert_eq!(*edge, Some(ClockEdge::Rising)),
        _ => panic!("expected Clock"),
    }
}

#[test]
fn construct_direction_wrapper() {
    let inner = Box::new(Spanned::new(TypeExprKind::Named("UInt".into()), s(3, 7)));
    let ty = Spanned::new(
        TypeExprKind::DirectionWrapper { dir: Direction::In, inner },
        s(0, 7),
    );
    assert!(matches!(ty.node, TypeExprKind::DirectionWrapper { dir: Direction::In, .. }));
}

#[test]
fn construct_attribute() {
    let attr = Attribute {
        name: ident("clock"),
        args: vec![],
        span: s(0, 6),
    };
    assert_eq!(attr.name.node, "clock");
}
```

- [ ] **2e. Verify tests pass**

```bash
cargo test -p ssl-core --test ast_tests 2>&1
# Expected: 8 tests passed
```

- [ ] **2f. Commit**

```bash
git add crates/ssl-core/src/ast/
git add crates/ssl-core/tests/ast_tests.rs
git commit -m "feat(ast): add expression and type AST nodes

Define ExprKind (literals, binary/unary ops, calls, field access,
pipe, if-expr, range, formal temporal), BinOp, UnaryOp, CallArg.
Define TypeExprKind (named, generic, array, clock, reset, direction,
memory), GenericParam, GenericKind, Direction, ClockEdge."
```

---

### Task 3: AST Statement and Item Nodes

**Files:** `crates/ssl-core/src/ast/stmt.rs`, `crates/ssl-core/src/ast/item.rs`, `crates/ssl-core/src/lib.rs`

#### TDD Steps

- [ ] **3a. Populate `crates/ssl-core/src/ast/stmt.rs`**

Replace the stub with:

```rust
use crate::span::{Span, Spanned};
use super::{Ident, Attribute};
use super::expr::{Expr, CallArg};
use super::types::{TypeExpr, Direction, GenericParam};

pub type Stmt = Spanned<StmtKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Signal(SignalDecl),
    Let(LetDecl),
    Const(ConstDecl),
    TypeAlias(TypeAliasDecl),
    Assign { target: Expr, value: Expr },
    If(IfStmt),
    Match(MatchStmt),
    For(ForStmt),
    CombBlock(Vec<Stmt>),
    RegBlock(RegBlock),
    PriorityBlock(PriorityBlock),
    ParallelBlock(ParallelBlock),
    Assert(AssertStmt),
    Assume { domain: Option<Ident>, expr: Expr, message: Option<Expr> },
    Cover { name: Option<Ident>, expr: Expr },
    StaticAssert { expr: Expr, message: Expr },
    UncheckedBlock(Vec<Stmt>),
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    pub domain: Option<Ident>,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetDecl {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDecl {
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_body: Vec<Stmt>,
    pub elif_branches: Vec<(Expr, Vec<Stmt>)>,
    pub else_body: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStmt {
    pub var: Ident,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegBlock {
    pub clock: Expr,
    pub reset: Expr,
    pub enable: Option<Expr>,
    pub on_reset: Vec<Stmt>,
    pub on_tick: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriorityBlock {
    pub arms: Vec<PriorityArm>,
    pub otherwise: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriorityArm {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParallelBlock {
    pub safe: Option<Expr>,
    pub arms: Vec<PriorityArm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssertStmt {
    pub always: bool,
    pub domain: Option<Ident>,
    pub expr: Expr,
    pub message: Option<Expr>,
}
```

- [ ] **3b. Populate `crates/ssl-core/src/ast/item.rs`**

Replace the stub with:

```rust
use crate::span::{Span, Spanned};
use super::{Ident, Attribute, DocComment};
use super::expr::{Expr, CallArg};
use super::types::{TypeExpr, Direction, GenericParam};
use super::stmt::Stmt;

pub type Item = Spanned<ItemKind>;

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Module(ModuleDef),
    Struct(StructDef),
    Enum(EnumDef),
    Interface(InterfaceDef),
    FnDef(FnDef),
    Fsm(FsmDef),
    Pipeline(PipelineDef),
    Test(TestBlock),
    Import(ImportStmt),
    ExternModule(ExternModuleDef),
    Inst(InstDecl),
    GenFor(GenFor),
    GenIf(GenIf),
    Stmt(Stmt),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub doc: Option<DocComment>,
    pub attrs: Vec<Attribute>,
    pub public: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub ports: Vec<Port>,
    pub default_domain: Option<Ident>,
    pub body: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    pub doc: Option<DocComment>,
    pub direction: Direction,
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Ident,
    pub ty: TypeExpr,
    pub bit_range: Option<(Expr, Expr)>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub encoding: Option<EnumEncoding>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumEncoding { Binary, Onehot, Gray, Custom }

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Ident,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub groups: Vec<InterfaceGroup>,
    pub signals: Vec<InterfaceSignal>,
    pub properties: Vec<InterfaceProperty>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceGroup {
    pub name: Ident,
    pub signals: Vec<InterfaceSignal>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceSignal {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceProperty {
    pub name: Ident,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub doc: Option<DocComment>,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub params: Vec<FnParam>,
    pub return_type: TypeExpr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnParam {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsmDef {
    pub name: Ident,
    pub clock: Expr,
    pub reset: Expr,
    pub states: Vec<Ident>,
    pub encoding: Option<EnumEncoding>,
    pub initial: Ident,
    pub transitions: Vec<FsmTransition>,
    pub on_tick: Option<Vec<Stmt>>,
    pub outputs: Vec<FsmOutput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsmTransition {
    pub from: FsmStateRef,
    pub condition: FsmCondition,
    pub to: FsmStateRef,
    pub actions: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FsmStateRef { Named(Ident), Wildcard(Span) }

#[derive(Debug, Clone, PartialEq)]
pub enum FsmCondition {
    Expr(Expr),
    Timeout(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FsmOutput {
    pub state: Ident,
    pub assignments: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineDef {
    pub name: Ident,
    pub clock: Expr,
    pub reset: Expr,
    pub backpressure: BackpressureMode,
    pub input: PipelinePort,
    pub output: PipelinePort,
    pub stages: Vec<PipelineStage>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackpressureMode {
    Auto(Vec<CallArg>),
    Manual,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelinePort {
    pub bindings: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineStage {
    pub index: Expr,
    pub label: Option<String>,
    pub stall_when: Option<Expr>,
    pub flush_when: Option<Expr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestBlock {
    pub name: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportStmt {
    pub names: Vec<Ident>,
    pub path: String,
    pub alias: Option<Ident>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternModuleDef {
    pub name: Ident,
    pub ports: Vec<Port>,
    pub backend: String,
    pub backend_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstDecl {
    pub name: Ident,
    pub module_name: Ident,
    pub generic_args: Vec<Expr>,
    pub connections: Vec<PortConnection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortConnection {
    pub port: Ident,
    pub binding: PortBinding,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PortBinding {
    Input(Expr),
    Output(Expr),
    Bidirectional(Expr),
    Discard,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenFor {
    pub var: Ident,
    pub iterable: Expr,
    pub body: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenIf {
    pub condition: Expr,
    pub then_body: Vec<Item>,
    pub else_body: Option<Vec<Item>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub items: Vec<Item>,
}
```

- [ ] **3c. Update `crates/ssl-core/src/lib.rs`**

Change from:

```rust
pub mod lexer;
pub mod span;
```

To:

```rust
pub mod ast;
pub mod lexer;
pub mod parser;
pub mod span;
```

Create `crates/ssl-core/src/parser/mod.rs` (empty module placeholder):

```rust
// Parser implementation — populated in Chunks 2-4
```

- [ ] **3d. Verify compilation**

```bash
cargo check -p ssl-core 2>&1
# Expected: compiles with no errors
# Warnings for unused imports/fields are acceptable at this stage
```

- [ ] **3e. Add item/stmt construction tests to `ast_tests.rs`**

Append to `crates/ssl-core/tests/ast_tests.rs`:

```rust
use ssl_core::ast::stmt::*;
use ssl_core::ast::item::*;
use ssl_core::ast::types::Direction;

#[test]
fn construct_signal_decl() {
    let decl = SignalDecl {
        name: ident("counter"),
        ty: Spanned::new(TypeExprKind::Named("UInt".into()), s(0, 4)),
        domain: Some(ident("sys_clk")),
        init: None,
    };
    assert_eq!(decl.name.node, "counter");
    assert!(decl.domain.is_some());
}

#[test]
fn construct_if_stmt() {
    let cond = Spanned::new(ExprKind::Ident("enable".into()), s(0, 6));
    let body_stmt = Spanned::new(
        StmtKind::Assign {
            target: Spanned::new(ExprKind::Ident("x".into()), s(0, 1)),
            value: Spanned::new(ExprKind::IntLiteral(NumericLiteral::Decimal(1)), s(4, 5)),
        },
        s(0, 5),
    );
    let if_stmt = IfStmt {
        condition: cond,
        then_body: vec![body_stmt],
        elif_branches: vec![],
        else_body: None,
    };
    assert_eq!(if_stmt.then_body.len(), 1);
}

#[test]
fn construct_module_def() {
    let port = Port {
        doc: None,
        direction: Direction::In,
        name: ident("clk"),
        ty: Spanned::new(
            TypeExprKind::Clock { freq: None, edge: None },
            s(0, 5),
        ),
        span: s(0, 10),
    };
    let module = ModuleDef {
        doc: None,
        attrs: vec![],
        public: true,
        name: ident("ALU"),
        generics: vec![],
        ports: vec![port],
        default_domain: None,
        body: vec![],
    };
    assert_eq!(module.name.node, "ALU");
    assert!(module.public);
    assert_eq!(module.ports.len(), 1);
}

#[test]
fn construct_enum_def() {
    let variant = EnumVariant {
        name: ident("Idle"),
        value: None,
        span: s(0, 4),
    };
    let def = EnumDef {
        doc: None,
        name: ident("State"),
        encoding: Some(EnumEncoding::Onehot),
        variants: vec![variant],
    };
    assert_eq!(def.encoding, Some(EnumEncoding::Onehot));
}

#[test]
fn construct_fsm_transition() {
    let trans = FsmTransition {
        from: FsmStateRef::Named(ident("Idle")),
        condition: FsmCondition::Expr(
            Spanned::new(ExprKind::Ident("start".into()), s(0, 5)),
        ),
        to: FsmStateRef::Named(ident("Running")),
        actions: vec![],
        span: s(0, 30),
    };
    assert!(matches!(trans.from, FsmStateRef::Named(_)));
    assert!(matches!(trans.condition, FsmCondition::Expr(_)));
}

#[test]
fn construct_pipeline_stage() {
    let stage = PipelineStage {
        index: Spanned::new(ExprKind::IntLiteral(NumericLiteral::Decimal(0)), s(0, 1)),
        label: Some("fetch".into()),
        stall_when: None,
        flush_when: None,
        body: vec![],
        span: s(0, 20),
    };
    assert_eq!(stage.label, Some("fetch".into()));
}

#[test]
fn construct_source_file() {
    let module_item = Spanned::new(
        ItemKind::Module(ModuleDef {
            doc: None,
            attrs: vec![],
            public: false,
            name: ident("Top"),
            generics: vec![],
            ports: vec![],
            default_domain: None,
            body: vec![],
        }),
        s(0, 50),
    );
    let file = SourceFile { items: vec![module_item] };
    assert_eq!(file.items.len(), 1);
}
```

- [ ] **3f. Verify all tests pass**

```bash
cargo test -p ssl-core --test ast_tests 2>&1
# Expected: 15 tests passed (8 from Task 2 + 7 from Task 3)
cargo test -p ssl-core 2>&1
# Expected: all tests pass, no regressions
```

- [ ] **3g. Commit**

```bash
git add crates/ssl-core/src/ast/stmt.rs crates/ssl-core/src/ast/item.rs
git add crates/ssl-core/src/lib.rs crates/ssl-core/src/parser/mod.rs
git add crates/ssl-core/tests/ast_tests.rs
git commit -m "feat(ast): add statement and item AST nodes, wire up modules

Define StmtKind (signal, let, const, assign, if, match, for, comb/reg
blocks, priority/parallel, assert/assume/cover) and ItemKind (module,
struct, enum, interface, fn, fsm, pipeline, test, import, extern,
inst, gen for/if). Add pub mod ast and empty parser module to lib.rs."
```

---

## Chunk 2: Parser Infrastructure + Expression Parsing

### Task 4: Parser Struct and Helpers

**Files:** `crates/ssl-core/src/parser/mod.rs`, `crates/ssl-core/src/parser/expr.rs`, `crates/ssl-core/src/parser/types.rs`, `crates/ssl-core/src/parser/stmt.rs`, `crates/ssl-core/src/parser/item.rs`

#### TDD Steps

- [ ] **4a. Create parser stub files**

Create `crates/ssl-core/src/parser/expr.rs`:

```rust
// Expression parsing — populated in Task 5
```

Create `crates/ssl-core/src/parser/types.rs`:

```rust
// Type expression parsing — populated in Chunk 3
```

Create `crates/ssl-core/src/parser/stmt.rs`:

```rust
// Statement parsing — populated in Chunk 3
```

Create `crates/ssl-core/src/parser/item.rs`:

```rust
// Item parsing — populated in Chunk 4
```

- [ ] **4b. Write `crates/ssl-core/src/parser/mod.rs`**

Replace the placeholder with:

```rust
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

    /// Look at the current token without consuming it.
    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.node)
    }

    /// Span of the current token, or a zero-width span at end-of-source.
    pub fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or_else(|| {
                let end = self.source.len() as u32;
                Span::new(end, end)
            })
    }

    /// Consume and return the current token.
    pub fn advance(&mut self) -> Spanned<Token> {
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    /// True if the current token matches `expected` by discriminant.
    /// For tokens with data (Ident, Numeric, StringLit, DocComment),
    /// compares only the variant tag, not the payload.
    pub fn check(&self, expected: Token) -> bool {
        match self.peek() {
            Some(tok) => std::mem::discriminant(tok) == std::mem::discriminant(&expected),
            None => false,
        }
    }

    /// True if the current token is `Token::Ident`.
    pub fn check_ident(&self) -> bool {
        matches!(self.peek(), Some(Token::Ident))
    }

    /// Consume current token if it matches `expected`, else return None.
    pub fn eat(&mut self, expected: Token) -> Option<Spanned<Token>> {
        if self.check(expected) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Consume current token if it matches, or return an error.
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

    /// Consume an Ident token and return its text extracted from source.
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

    /// Slice source text by span.
    pub fn text(&self, span: Span) -> &str {
        &self.source[span.start as usize..span.end as usize]
    }

    /// Skip consecutive Newline tokens.
    pub fn skip_newlines(&mut self) {
        while matches!(self.peek(), Some(Token::Newline)) {
            self.advance();
        }
    }

    /// True if all tokens have been consumed.
    pub fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Create a ParseError at the current position.
    pub fn error(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            message: msg.into(),
            span: self.peek_span(),
        }
    }

    /// Parse an indentation-delimited block: `: NEWLINE INDENT body DEDENT`.
    /// Calls `f` repeatedly until Dedent or end-of-input.
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

    /// Parse a comma-separated list inside already-consumed open delimiter,
    /// ending at `close_token`. Skips newlines between items.
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
            if !self.eat(Token::Comma).is_some() {
                break;
            }
            self.skip_newlines();
        }
        self.expect_token(close_token)?;
        Ok(items)
    }

    /// Top-level entry point: parse a full source file.
    pub fn parse(source: &str, tokens: Vec<Spanned<Token>>) -> Result<SourceFile, ParseError> {
        let mut parser = Parser::new(source, tokens);
        let mut items = Vec::new();
        parser.skip_newlines();
        while !parser.is_at_end() {
            items.push(item::parse_item(&mut parser)?);
            parser.skip_newlines();
        }
        Ok(SourceFile { items })
    }
}
```

- [ ] **4c. Verify compilation**

```bash
cargo check -p ssl-core 2>&1
# Expected: compiles (stub sub-modules are valid, item::parse_item is not yet called)
```

Note: compilation may fail because `item::parse_item` does not exist yet. If so, temporarily comment out the body of `Parser::parse` and replace with `Ok(SourceFile { items: vec![] })` until Task 5-6 are wired. Alternatively, add a placeholder to `parser/item.rs`:

```rust
use crate::ast::item::Item;
use super::{Parser, ParseError};

pub fn parse_item(_parser: &mut Parser<'_>) -> Result<Item, ParseError> {
    Err(_parser.error("item parsing not yet implemented"))
}
```

- [ ] **4d. Write parser helper tests**

Add `crates/ssl-core/tests/parser_tests.rs`:

```rust
use ssl_core::lexer::Token;
use ssl_core::span::{Span, Spanned};
use ssl_core::parser::Parser;

fn s(start: u32, end: u32) -> Span {
    Span::new(start, end)
}

fn tok(token: Token, start: u32, end: u32) -> Spanned<Token> {
    Spanned::new(token, s(start, end))
}

#[test]
fn parser_peek_and_advance() {
    let source = "a b";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Ident, 2, 3),
    ];
    let mut p = Parser::new(source, tokens);

    assert_eq!(p.peek(), Some(&Token::Ident));
    assert!(!p.is_at_end());

    let first = p.advance();
    assert_eq!(first.span, s(0, 1));
    assert_eq!(p.text(first.span), "a");

    let second = p.advance();
    assert_eq!(p.text(second.span), "b");
    assert!(p.is_at_end());
    assert_eq!(p.peek(), None);
}

#[test]
fn parser_check_discriminant() {
    let source = "42";
    let tokens = vec![
        tok(Token::Numeric(ssl_core::lexer::NumericLiteral::Decimal(42)), 0, 2),
    ];
    let p = Parser::new(source, tokens);
    // check matches by discriminant, ignoring payload
    assert!(p.check(Token::Numeric(ssl_core::lexer::NumericLiteral::Decimal(0))));
    assert!(!p.check(Token::Ident));
}

#[test]
fn parser_eat_and_expect() {
    let source = "x + y";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Plus, 2, 3),
        tok(Token::Ident, 4, 5),
    ];
    let mut p = Parser::new(source, tokens);

    // eat returns None when no match
    assert!(p.eat(Token::Plus).is_none());
    // eat returns Some when match
    assert!(p.eat(Token::Ident).is_some());

    // expect succeeds
    let plus = p.expect_token(Token::Plus);
    assert!(plus.is_ok());

    // expect fails
    let err = p.expect_token(Token::Plus);
    assert!(err.is_err());
}

#[test]
fn parser_expect_ident() {
    let source = "counter";
    let tokens = vec![tok(Token::Ident, 0, 7)];
    let mut p = Parser::new(source, tokens);

    let ident = p.expect_ident().unwrap();
    assert_eq!(ident.node, "counter");
    assert_eq!(ident.span, s(0, 7));
}

#[test]
fn parser_expect_ident_fail() {
    let source = "+";
    let tokens = vec![tok(Token::Plus, 0, 1)];
    let mut p = Parser::new(source, tokens);
    assert!(p.expect_ident().is_err());
}

#[test]
fn parser_skip_newlines() {
    let source = "a\n\n\nb";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Newline, 1, 2),
        tok(Token::Newline, 2, 3),
        tok(Token::Newline, 3, 4),
        tok(Token::Ident, 4, 5),
    ];
    let mut p = Parser::new(source, tokens);
    p.advance(); // consume 'a'
    p.skip_newlines();
    assert_eq!(p.text(p.peek_span()), "b");
}

#[test]
fn parser_parse_block() {
    // Simulates `: NL INDENT x NL y NL DEDENT`
    let source = ":\n  x\n  y\n";
    let tokens = vec![
        tok(Token::Colon, 0, 1),
        tok(Token::Newline, 1, 2),
        tok(Token::Indent, 2, 2),
        tok(Token::Ident, 4, 5),
        tok(Token::Newline, 5, 6),
        tok(Token::Ident, 8, 9),
        tok(Token::Newline, 9, 10),
        tok(Token::Dedent, 10, 10),
    ];
    let mut p = Parser::new(source, tokens);
    let items = p.parse_block(|p| {
        let t = p.advance();
        Ok(p.text(t.span).to_string())
    });
    let items = items.unwrap();
    assert_eq!(items, vec!["x".to_string(), "y".to_string()]);
}

#[test]
fn parser_parse_comma_list() {
    // Simulates `x, y, z)`  — open paren already consumed
    let source = "x, y, z)";
    let tokens = vec![
        tok(Token::Ident, 0, 1),
        tok(Token::Comma, 1, 2),
        tok(Token::Ident, 3, 4),
        tok(Token::Comma, 4, 5),
        tok(Token::Ident, 6, 7),
        tok(Token::RParen, 7, 8),
    ];
    let mut p = Parser::new(source, tokens);
    let items = p.parse_comma_list(Token::RParen, |p| {
        let t = p.advance();
        Ok(p.text(t.span).to_string())
    });
    let items = items.unwrap();
    assert_eq!(items, vec!["x".to_string(), "y".to_string(), "z".to_string()]);
}

#[test]
fn parser_error_display() {
    let err = ssl_core::parser::ParseError {
        message: "unexpected token".into(),
        span: s(10, 15),
    };
    assert_eq!(format!("{}", err), "parse error at 10..15: unexpected token");
}
```

- [ ] **4e. Verify tests pass**

```bash
cargo test -p ssl-core --test parser_tests 2>&1
# Expected: 9 tests passed
cargo test -p ssl-core 2>&1
# Expected: all tests pass
```

- [ ] **4f. Commit**

```bash
git add crates/ssl-core/src/parser/
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "feat(parser): add Parser struct with helper methods

Implement Parser with peek/advance/check/eat/expect_token/expect_ident,
text extraction, skip_newlines, parse_block, parse_comma_list, and
error reporting. Add 9 unit tests for all helper methods."
```

---

### Task 5: Atom and Unary Expression Parsing

**Files:** `crates/ssl-core/src/parser/expr.rs`

#### TDD Steps

- [ ] **5a. Implement expression parsing in `crates/ssl-core/src/parser/expr.rs`**

Replace the stub with:

```rust
use crate::ast::expr::{BinOp, CallArg, Expr, ExprKind, UnaryOp};
use crate::lexer::{NumericLiteral, Token};
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
            ExprKind::Call { callee, mut args } => {
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
        let mut rhs = parse_unary(p)?;
        rhs = parse_postfix(p, rhs)?;

        // Continue while next operator has higher precedence
        loop {
            let next_tok = match p.peek() {
                Some(t) => t.clone(),
                None => break,
            };
            let (_, next_prec, next_assoc) = match infix_binding_power(&next_tok) {
                Some(info) => info,
                None => break,
            };
            if next_prec < next_min {
                break;
            }
            // Recurse with the appropriate min precedence
            let recurse_prec = if next_assoc == Assoc::Right {
                next_prec
            } else {
                next_prec + 1
            };
            // We already have rhs from parse_unary; need to re-parse with full pratt
            // Instead, use the standard approach: recurse from current rhs position
            rhs = {
                let inner_rhs = parse_pratt_continue(p, rhs, next_min)?;
                inner_rhs
            };
            break;
        }

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

/// Continue Pratt parsing from an already-parsed LHS.
fn parse_pratt_continue(
    p: &mut Parser<'_>,
    mut lhs: Expr,
    min_prec: u8,
) -> Result<Expr, ParseError> {
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

        let is_range_exclusive = matches!(tok, Token::RangeExclusive);
        let is_range_inclusive = matches!(tok, Token::RangeInclusive);

        p.advance();

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
                    let span = lhs.span.merge(p.peek_span());
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
                // Use span up to closing paren (already consumed by parse_call_args)
                let span = lhs.span.merge(Span::new(
                    p.tokens.get(p.pos.saturating_sub(1)).map_or(0, |t| t.span.end),
                    p.tokens.get(p.pos.saturating_sub(1)).map_or(0, |t| t.span.end),
                ));
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
            // expect `depth` or positional — for now just parse the expression
            // Handle `depth=N` named arg syntax
            let depth = if p.check_ident() {
                let saved_pos = p.pos;
                let maybe_name = p.advance();
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
                if !p.eat(Token::Comma).is_some() {
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
        if !p.eat(Token::Comma).is_some() {
            break;
        }
        p.skip_newlines();
    }
    p.expect_token(Token::RParen)?;
    Ok(args)
}
```

- [ ] **5b. Write expression parser tests**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
use ssl_core::ast::expr::{BinOp, ExprKind, UnaryOp};
use ssl_core::lexer::NumericLiteral;
use ssl_core::parser::expr::parse_expr;

/// Helper: lex source, strip comments, feed to parser, parse one expression.
fn parse_one_expr(source: &str) -> ssl_core::ast::expr::Expr {
    use ssl_core::lexer::Lexer;
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    // Filter out comments and newlines for expression-only tests
    let tokens: Vec<_> = tokens
        .into_iter()
        .filter(|t| !matches!(t.node, Token::LineComment | Token::BlockComment | Token::Newline | Token::Indent | Token::Dedent | Token::DocComment))
        .collect();
    let mut p = Parser::new(source, tokens);
    parse_expr(&mut p).expect("parse failed")
}

#[test]
fn expr_int_literal() {
    let expr = parse_one_expr("42");
    assert!(matches!(
        expr.node,
        ExprKind::IntLiteral(NumericLiteral::Decimal(42))
    ));
}

#[test]
fn expr_string_literal() {
    let expr = parse_one_expr("\"hello\"");
    match &expr.node {
        ExprKind::StringLiteral(s) => assert_eq!(s, "hello"),
        _ => panic!("expected StringLiteral, got {:?}", expr.node),
    }
}

#[test]
fn expr_bool_literal() {
    let t = parse_one_expr("true");
    assert!(matches!(t.node, ExprKind::BoolLiteral(true)));
    let f = parse_one_expr("false");
    assert!(matches!(f.node, ExprKind::BoolLiteral(false)));
}

#[test]
fn expr_ident() {
    let expr = parse_one_expr("counter");
    match &expr.node {
        ExprKind::Ident(name) => assert_eq!(name, "counter"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn expr_add_mul_precedence() {
    // a + b * c  =>  Binary(Add, a, Binary(Mul, b, c))
    let expr = parse_one_expr("a + b * c");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::Add);
            assert!(matches!(lhs.node, ExprKind::Ident(_)));
            match &rhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Mul),
                _ => panic!("expected Mul on rhs"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_unary_not_and() {
    // not x and y  =>  Binary(And, Unary(Not, x), y)
    let expr = parse_one_expr("not x and y");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::And);
            match &lhs.node {
                ExprKind::Unary { op, .. } => assert_eq!(*op, UnaryOp::LogicalNot),
                _ => panic!("expected Unary on lhs"),
            }
            assert!(matches!(rhs.node, ExprKind::Ident(_)));
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_field_access() {
    let expr = parse_one_expr("a.field");
    match &expr.node {
        ExprKind::FieldAccess { object, field } => {
            assert!(matches!(object.node, ExprKind::Ident(_)));
            assert_eq!(field.node, "field");
        }
        _ => panic!("expected FieldAccess, got {:?}", expr.node),
    }
}

#[test]
fn expr_index() {
    let expr = parse_one_expr("a[0]");
    match &expr.node {
        ExprKind::Index { array, index } => {
            assert!(matches!(array.node, ExprKind::Ident(_)));
            assert!(matches!(index.node, ExprKind::IntLiteral(_)));
        }
        _ => panic!("expected Index"),
    }
}

#[test]
fn expr_bit_slice() {
    let expr = parse_one_expr("a[7:0]");
    match &expr.node {
        ExprKind::BitSlice { value, high, low } => {
            assert!(matches!(value.node, ExprKind::Ident(_)));
            assert!(matches!(high.node, ExprKind::IntLiteral(NumericLiteral::Decimal(7))));
            assert!(matches!(low.node, ExprKind::IntLiteral(NumericLiteral::Decimal(0))));
        }
        _ => panic!("expected BitSlice"),
    }
}

#[test]
fn expr_call() {
    let expr = parse_one_expr("f(x, y)");
    match &expr.node {
        ExprKind::Call { callee, args } => {
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "f"),
                _ => panic!("expected Ident callee"),
            }
            assert_eq!(args.len(), 2);
            assert!(args[0].name.is_none());
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_call_named_arg() {
    let expr = parse_one_expr("f(name=x)");
    match &expr.node {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 1);
            assert_eq!(args[0].name.as_ref().unwrap().node, "name");
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_pipe() {
    let expr = parse_one_expr("a |> f(b)");
    match &expr.node {
        ExprKind::Pipe { input, callee, args } => {
            assert!(matches!(input.node, ExprKind::Ident(_)));
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "f"),
                _ => panic!("expected Ident callee"),
            }
            assert_eq!(args.len(), 1);
        }
        _ => panic!("expected Pipe, got {:?}", expr.node),
    }
}

#[test]
fn expr_if_then_else() {
    let expr = parse_one_expr("if x then y else z");
    match &expr.node {
        ExprKind::IfExpr {
            condition,
            then_expr,
            else_expr,
        } => {
            assert!(matches!(condition.node, ExprKind::Ident(_)));
            assert!(matches!(then_expr.node, ExprKind::Ident(_)));
            assert!(matches!(else_expr.node, ExprKind::Ident(_)));
        }
        _ => panic!("expected IfExpr"),
    }
}

#[test]
fn expr_implies() {
    let expr = parse_one_expr("a implies b");
    match &expr.node {
        ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Implies),
        _ => panic!("expected Binary Implies"),
    }
}

#[test]
fn expr_paren() {
    let expr = parse_one_expr("(a + b)");
    match &expr.node {
        ExprKind::Paren(inner) => {
            assert!(matches!(inner.node, ExprKind::Binary { op: BinOp::Add, .. }));
        }
        _ => panic!("expected Paren"),
    }
}

#[test]
fn expr_array_literal() {
    let expr = parse_one_expr("[1, 2, 3]");
    match &expr.node {
        ExprKind::ArrayLiteral(elems) => assert_eq!(elems.len(), 3),
        _ => panic!("expected ArrayLiteral"),
    }
}

#[test]
fn expr_unary_neg() {
    let expr = parse_one_expr("-x");
    match &expr.node {
        ExprKind::Unary { op, .. } => assert_eq!(*op, UnaryOp::Neg),
        _ => panic!("expected Unary Neg"),
    }
}

#[test]
fn expr_unary_bitnot() {
    let expr = parse_one_expr("~x");
    match &expr.node {
        ExprKind::Unary { op, .. } => assert_eq!(*op, UnaryOp::BitNot),
        _ => panic!("expected Unary BitNot"),
    }
}

#[test]
fn expr_power_right_assoc() {
    // 2 ** 3 ** 4  =>  Binary(Pow, 2, Binary(Pow, 3, 4))
    let expr = parse_one_expr("2 ** 3 ** 4");
    match &expr.node {
        ExprKind::Binary { op, rhs, .. } => {
            assert_eq!(*op, BinOp::Pow);
            match &rhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Pow),
                _ => panic!("expected nested Pow"),
            }
        }
        _ => panic!("expected Binary Pow"),
    }
}
```

- [ ] **5c. Verify expression tests pass**

```bash
cargo test -p ssl-core --test parser_tests 2>&1
# Expected: 9 helper tests + 18 expression tests = 27 tests passed
```

- [ ] **5d. Commit**

```bash
git add crates/ssl-core/src/parser/expr.rs
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "feat(parser): implement expression parsing with Pratt algorithm

Implement full expression parser with 14-level precedence Pratt algorithm,
prefix unary operators (not, ~, -), postfix ops (field access, method call,
index, bit slice, function call), pipe operator, if-then-else expressions,
array literals, parenthesized expressions, and call argument parsing with
named argument support. Add 18 expression parser tests."
```

---

### Task 6: Call Arguments and Expression Edge Cases

**Files:** `crates/ssl-core/src/parser/expr.rs`, `crates/ssl-core/tests/parser_tests.rs`

#### TDD Steps

- [ ] **6a. Write edge-case expression tests**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
#[test]
fn expr_method_call() {
    let expr = parse_one_expr("a.truncate(8)");
    match &expr.node {
        ExprKind::MethodCall { object, method, args } => {
            assert!(matches!(object.node, ExprKind::Ident(_)));
            assert_eq!(method.node, "truncate");
            assert_eq!(args.len(), 1);
        }
        _ => panic!("expected MethodCall, got {:?}", expr.node),
    }
}

#[test]
fn expr_chained_field_access() {
    // a.b.c  =>  FieldAccess(FieldAccess(a, b), c)
    let expr = parse_one_expr("a.b.c");
    match &expr.node {
        ExprKind::FieldAccess { object, field } => {
            assert_eq!(field.node, "c");
            match &object.node {
                ExprKind::FieldAccess { field: inner_field, .. } => {
                    assert_eq!(inner_field.node, "b");
                }
                _ => panic!("expected nested FieldAccess"),
            }
        }
        _ => panic!("expected FieldAccess"),
    }
}

#[test]
fn expr_nested_call() {
    let expr = parse_one_expr("f(g(x))");
    match &expr.node {
        ExprKind::Call { callee, args } => {
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "f"),
                _ => panic!("expected f"),
            }
            assert_eq!(args.len(), 1);
            match &args[0].value.node {
                ExprKind::Call { callee, args } => {
                    match &callee.node {
                        ExprKind::Ident(name) => assert_eq!(name, "g"),
                        _ => panic!("expected g"),
                    }
                    assert_eq!(args.len(), 1);
                }
                _ => panic!("expected inner Call"),
            }
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_complex_precedence() {
    // a + b * c - d  =>  Binary(Sub, Binary(Add, a, Binary(Mul, b, c)), d)
    let expr = parse_one_expr("a + b * c - d");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::Sub);
            assert!(matches!(rhs.node, ExprKind::Ident(_)));
            match &lhs.node {
                ExprKind::Binary { op, rhs: inner_rhs, .. } => {
                    assert_eq!(*op, BinOp::Add);
                    assert!(matches!(inner_rhs.node, ExprKind::Binary { op: BinOp::Mul, .. }));
                }
                _ => panic!("expected Add"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_comparison_chain() {
    // a == b != c  =>  Binary(Ne, Binary(Eq, a, b), c)
    let expr = parse_one_expr("a == b != c");
    match &expr.node {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(*op, BinOp::Ne);
            match &lhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Eq),
                _ => panic!("expected Eq"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_bitwise_ops() {
    // a & b | c ^ d  =>  Binary(BitOr, Binary(BitAnd, a, b), Binary(BitXor, c, d))
    let expr = parse_one_expr("a & b | c ^ d");
    match &expr.node {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinOp::BitOr);
            match &lhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::BitAnd),
                _ => panic!("expected BitAnd"),
            }
            match &rhs.node {
                ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::BitXor),
                _ => panic!("expected BitXor"),
            }
        }
        _ => panic!("expected Binary"),
    }
}

#[test]
fn expr_shift_ops() {
    let expr = parse_one_expr("a << 2");
    match &expr.node {
        ExprKind::Binary { op, .. } => assert_eq!(*op, BinOp::Shl),
        _ => panic!("expected Shl"),
    }
}

#[test]
fn expr_method_on_field() {
    // a.field.method(x)
    let expr = parse_one_expr("a.field.method(x)");
    match &expr.node {
        ExprKind::MethodCall { object, method, args } => {
            assert_eq!(method.node, "method");
            assert_eq!(args.len(), 1);
            match &object.node {
                ExprKind::FieldAccess { field, .. } => assert_eq!(field.node, "field"),
                _ => panic!("expected FieldAccess"),
            }
        }
        _ => panic!("expected MethodCall"),
    }
}

#[test]
fn expr_struct_literal_as_call() {
    // Point(x=1, y=2) — parsed as Call with named args
    let expr = parse_one_expr("Point(x=1, y=2)");
    match &expr.node {
        ExprKind::Call { callee, args } => {
            match &callee.node {
                ExprKind::Ident(name) => assert_eq!(name, "Point"),
                _ => panic!("expected Ident"),
            }
            assert_eq!(args.len(), 2);
            assert_eq!(args[0].name.as_ref().unwrap().node, "x");
            assert_eq!(args[1].name.as_ref().unwrap().node, "y");
        }
        _ => panic!("expected Call (struct literal)"),
    }
}

#[test]
fn expr_index_then_field() {
    // a[0].field
    let expr = parse_one_expr("a[0].field");
    match &expr.node {
        ExprKind::FieldAccess { object, field } => {
            assert_eq!(field.node, "field");
            assert!(matches!(object.node, ExprKind::Index { .. }));
        }
        _ => panic!("expected FieldAccess"),
    }
}

#[test]
fn expr_paren_changes_precedence() {
    // (a + b) * c  =>  Binary(Mul, Paren(Binary(Add, a, b)), c)
    let expr = parse_one_expr("(a + b) * c");
    match &expr.node {
        ExprKind::Binary { op, lhs, .. } => {
            assert_eq!(*op, BinOp::Mul);
            match &lhs.node {
                ExprKind::Paren(inner) => {
                    assert!(matches!(inner.node, ExprKind::Binary { op: BinOp::Add, .. }));
                }
                _ => panic!("expected Paren"),
            }
        }
        _ => panic!("expected Binary Mul"),
    }
}

#[test]
fn expr_mixed_named_positional_args() {
    let expr = parse_one_expr("f(a, name=b, c)");
    match &expr.node {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 3);
            assert!(args[0].name.is_none());
            assert_eq!(args[1].name.as_ref().unwrap().node, "name");
            assert!(args[2].name.is_none());
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_next_single() {
    let expr = parse_one_expr("next(valid)");
    assert!(matches!(&expr.node, ExprKind::Next { expr: _, count: None }));
}

#[test]
fn expr_next_with_count() {
    let expr = parse_one_expr("next(valid, 3)");
    assert!(matches!(&expr.node, ExprKind::Next { expr: _, count: Some(_) }));
}

#[test]
fn expr_eventually_with_depth() {
    let expr = parse_one_expr("eventually(resp_valid, depth=16)");
    assert!(matches!(&expr.node, ExprKind::Eventually { expr: _, depth: Some(_) }));
}

#[test]
fn expr_range_exclusive() {
    let expr = parse_one_expr("0..8");
    assert!(matches!(&expr.node, ExprKind::Range { inclusive: false, .. }));
}

#[test]
fn expr_range_inclusive() {
    let expr = parse_one_expr("0..=7");
    assert!(matches!(&expr.node, ExprKind::Range { inclusive: true, .. }));
}
```

- [ ] **6b. Verify all tests pass**

```bash
cargo test -p ssl-core --test parser_tests 2>&1
# Expected: 9 helper + 18 expr + 17 edge-case = 44 tests passed
cargo test -p ssl-core 2>&1
# Expected: all tests pass (lexer, ast, parser)
```

- [ ] **6c. Commit**

```bash
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "test(parser): add edge-case expression tests

Add 17 tests for method calls, chained field access, nested calls,
complex precedence chains, bitwise ops, struct-literal-as-call,
index-then-field, paren precedence override, mixed named/positional args,
next, eventually, and range expressions."
```

---

## Chunk 3: Type Parsing + Statement and Block Parsing

### Task 7: Type Expression Parser

**Files:** `crates/ssl-core/src/parser/types.rs`, `crates/ssl-core/src/parser/mod.rs`

#### TDD Steps

- [ ] **7a. Add `prev_span()` helper to Parser**

In `crates/ssl-core/src/parser/mod.rs`, add inside the `impl<'src> Parser<'src>` block after `peek_span()`:

```rust
    pub fn prev_span(&self) -> Span {
        assert!(self.pos > 0, "prev_span called before any token consumed");
        self.tokens[self.pos - 1].span
    }
```

- [ ] **7b. Implement `crates/ssl-core/src/parser/types.rs`**

Replace the stub:

```rust
use crate::ast::expr::CallArg;
use crate::ast::types::*;
use crate::lexer::Token;
use crate::span::{Span, Spanned};
use super::expr::parse_expr;
use super::{ParseError, Parser};

pub fn parse_type_expr(p: &mut Parser<'_>) -> Result<TypeExpr, ParseError> {
    let start = p.peek_span();
    let mut ty = parse_base_type(p)?;
    while p.check(Token::LBracket) {
        p.advance();
        let size = parse_expr(p)?;
        let close = p.expect_token(Token::RBracket)?;
        ty = Spanned::new(TypeExprKind::Array { element: Box::new(ty), size }, ty.span.merge(close.span));
    }
    Ok(ty)
}

/// Parse a type expression followed by optional `@ domain` annotation.
/// Use this in contexts where domain annotation is unambiguous (port declarations).
/// Do NOT use in signal declarations where `@ domain` is a signal-level annotation.
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
        let close = p.expect_token(Token::Greater)?;
        return Ok(Spanned::new(TypeExprKind::DirectionWrapper { dir, inner: Box::new(inner) }, start.merge(close.span)));
    }
    let name = p.expect_ident()?;
    let name_str = name.node.clone();

    if name_str == "Flip" && p.check(Token::Less) {
        p.advance();
        let inner = parse_type_expr(p)?;
        let close = p.expect_token(Token::Greater)?;
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
        let params = p.parse_comma_list(Token::Greater, parse_generic_arg)?;
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

fn parse_generic_arg(p: &mut Parser<'_>) -> Result<GenericArg, ParseError> {
    if is_type_start(p) { Ok(GenericArg::Type(parse_type_expr(p)?)) }
    else { Ok(GenericArg::Expr(parse_expr(p)?)) }
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
    p.advance();
    let freq = Some(parse_expr(p)?);
    let edge = if p.eat(Token::Comma).is_some() {
        let e = p.expect_ident()?;
        Some(match e.node.as_str() {
            "rising" => ClockEdge::Rising, "falling" => ClockEdge::Falling, "dual" => ClockEdge::Dual,
            _ => return Err(ParseError { message: format!("expected clock edge, found '{}'", e.node), span: e.span }),
        })
    } else { None };
    let close = p.expect_token(Token::Greater)?;
    Ok(Spanned::new(TypeExprKind::Clock { freq, edge }, start.merge(close.span)))
}

fn parse_reset_type(p: &mut Parser<'_>, start: Span, is_sync: bool) -> Result<TypeExpr, ParseError> {
    p.advance();
    let pi = p.expect_ident()?;
    let polarity = Some(match pi.node.as_str() {
        "active_high" => ResetPolarity::ActiveHigh, "active_low" => ResetPolarity::ActiveLow,
        _ => return Err(ParseError { message: format!("expected polarity, found '{}'", pi.node), span: pi.span }),
    });
    let close = p.expect_token(Token::Greater)?;
    let span = start.merge(close.span);
    Ok(Spanned::new(if is_sync { TypeExprKind::SyncReset { polarity } } else { TypeExprKind::AsyncReset { polarity } }, span))
}

fn parse_memory_type(p: &mut Parser<'_>, start: Span, dual: bool) -> Result<TypeExpr, ParseError> {
    p.advance();
    let element = parse_type_expr(p)?;
    let mut params = Vec::new();
    while p.eat(Token::Comma).is_some() {
        p.skip_newlines();
        let pn = p.expect_ident()?;
        p.expect_token(Token::Eq)?;
        params.push(CallArg { name: Some(pn), value: parse_expr(p)? });
    }
    let close = p.expect_token(Token::Greater)?;
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
```

- [ ] **7c. Verify compilation**

```bash
cargo check -p ssl-core 2>&1
# Expected: compiles successfully
```

- [ ] **7d. Write type expression tests**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
use ssl_core::ast::types::{TypeExprKind, Direction, GenericArg};
use ssl_core::parser::types::{parse_type_expr, parse_type_expr_with_domain};

fn parse_one_type(src: &str) -> ssl_core::ast::types::TypeExpr {
    let tokens = ssl_core::lexer::lex(src).expect("lexer error");
    let mut p = Parser::new(src, tokens);
    parse_type_expr(&mut p).expect("parse error")
}

fn parse_one_type_with_domain(src: &str) -> ssl_core::ast::types::TypeExpr {
    let tokens = ssl_core::lexer::lex(src).expect("lexer error");
    let mut p = Parser::new(src, tokens);
    parse_type_expr_with_domain(&mut p).expect("parse error")
}

#[test] fn type_named_bool() { assert!(matches!(parse_one_type("Bool").node, TypeExprKind::Named(ref n) if n == "Bool")); }

#[test] fn type_generic_uint8() {
    match &parse_one_type("UInt<8>").node {
        TypeExprKind::Generic { name, params } => { assert_eq!(name, "UInt"); assert_eq!(params.len(), 1); }
        other => panic!("expected Generic, got {:?}", other),
    }
}

#[test] fn type_array_of_generic() {
    assert!(matches!(&parse_one_type("UInt<8>[32]").node, TypeExprKind::Array { element, .. } if matches!(element.node, TypeExprKind::Generic { .. })));
}

#[test] fn type_flip_of_generic() {
    assert!(matches!(&parse_one_type("Flip<Stream<T>>").node, TypeExprKind::Flip(inner) if matches!(inner.node, TypeExprKind::Generic { .. })));
}

#[test] fn type_direction_wrapper_in() {
    match &parse_one_type("in<Bool>").node {
        TypeExprKind::DirectionWrapper { dir, .. } => assert_eq!(*dir, Direction::In),
        other => panic!("expected DirectionWrapper, got {:?}", other),
    }
}

#[test] fn type_direction_wrapper_out() {
    match &parse_one_type("out<UInt<8>>").node {
        TypeExprKind::DirectionWrapper { dir, .. } => assert_eq!(*dir, Direction::Out),
        other => panic!("expected DirectionWrapper, got {:?}", other),
    }
}

#[test] fn type_domain_annotated() {
    match &parse_one_type_with_domain("UInt<8> @ sys_clk").node {
        TypeExprKind::DomainAnnotated { domain, .. } => assert_eq!(domain.node, "sys_clk"),
        other => panic!("expected DomainAnnotated, got {:?}", other),
    }
}

#[test] fn type_fixed_two_params() {
    match &parse_one_type("Fixed<8, 8>").node {
        TypeExprKind::Generic { name, params } => { assert_eq!(name, "Fixed"); assert_eq!(params.len(), 2); }
        other => panic!("expected Generic, got {:?}", other),
    }
}

#[test] fn type_sync_reset_no_polarity() { assert!(matches!(parse_one_type("SyncReset").node, TypeExprKind::SyncReset { polarity: None })); }

#[test] fn type_async_reset_active_low() {
    match &parse_one_type("AsyncReset<active_low>").node {
        TypeExprKind::AsyncReset { polarity } => assert_eq!(*polarity, Some(ssl_core::ast::types::ResetPolarity::ActiveLow)),
        other => panic!("expected AsyncReset, got {:?}", other),
    }
}

#[test] fn type_clock_with_edge() {
    match &parse_one_type("Clock<100, rising>").node {
        TypeExprKind::Clock { freq, edge } => { assert!(freq.is_some()); assert_eq!(*edge, Some(ssl_core::ast::types::ClockEdge::Rising)); }
        other => panic!("expected Clock, got {:?}", other),
    }
}

#[test] fn type_memory() {
    match &parse_one_type("Memory<UInt<8>, depth=1024>").node {
        TypeExprKind::Memory { element, params } => { assert!(matches!(element.node, TypeExprKind::Generic { .. })); assert_eq!(params.len(), 1); }
        other => panic!("expected Memory, got {:?}", other),
    }
}

#[test] fn type_partial_interface() {
    match &parse_one_type("AXI4Lite.{read_addr, read_data}").node {
        TypeExprKind::PartialInterface { name, groups } => { assert_eq!(name, "AXI4Lite"); assert_eq!(groups.len(), 2); }
        other => panic!("expected PartialInterface, got {:?}", other),
    }
}
```

- [ ] **7e. Verify tests pass**

```bash
cargo test -p ssl-core --test parser_tests type_ 2>&1
# Expected: 13 type expression tests pass
```

- [ ] **7f. Commit**

```bash
git add crates/ssl-core/src/parser/mod.rs crates/ssl-core/src/parser/types.rs
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "feat(parser): implement type expression parsing

Add parse_type_expr with support for named, generic, array, direction
wrapper, Flip, Clock, SyncReset/AsyncReset, Memory, domain annotation,
and generic parameter definitions. Add prev_span() helper to Parser."
```

---

### Task 8: Statement and Block Parsing

**Files:** `crates/ssl-core/src/parser/stmt.rs`

#### TDD Steps

- [ ] **8a. Implement `crates/ssl-core/src/parser/stmt.rs`**

Replace the stub:

```rust
use crate::ast::expr::{Expr, ExprKind};
use crate::ast::stmt::*;
use crate::lexer::Token;
use crate::span::{Span, Spanned};
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
            let saved = p.pos; p.advance();
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
    p.expect_token(Token::Colon)?; p.skip_newlines(); p.expect_token(Token::Indent)?;
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
    p.expect_token(Token::Colon)?; p.skip_newlines(); p.expect_token(Token::Indent)?;
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
        // Block form: unchecked: INDENT stmts DEDENT
        let stmts = p.parse_block(|p| parse_stmt(p))?;
        Ok(Spanned::new(StmtKind::UncheckedBlock(stmts), start.merge(p.prev_span())))
    } else if p.eat(Token::LParen).is_some() {
        // Inline form: unchecked(expr) — parsed as expression statement
        let inner = parse_expr(p)?;
        p.expect_token(Token::RParen)?;
        let span = start.merge(p.prev_span());
        let expr = Spanned::new(ExprKind::Unchecked(Box::new(inner)), span);
        Ok(Spanned::new(StmtKind::ExprStmt(expr), span))
    } else {
        Err(p.error("expected ':' or '(' after 'unchecked'"))
    }
}
```

- [ ] **8b. Verify compilation**

```bash
cargo check -p ssl-core 2>&1
# Expected: compiles
```

- [ ] **8c. Write declaration parsing tests**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
use ssl_core::ast::stmt::StmtKind;
use ssl_core::parser::stmt::parse_stmt;

fn parse_one_stmt(src: &str) -> ssl_core::ast::stmt::Stmt {
    let tokens = ssl_core::lexer::lex(src).expect("lexer error");
    let mut p = Parser::new(src, tokens);
    parse_stmt(&mut p).expect("parse error")
}

#[test] fn stmt_signal_decl() {
    match &parse_one_stmt("signal counter: UInt<8>").node {
        StmtKind::Signal(d) => { assert_eq!(d.name.node, "counter"); assert!(d.domain.is_none()); assert!(d.init.is_none()); }
        other => panic!("expected Signal, got {:?}", other),
    }
}
#[test] fn stmt_signal_with_domain_and_init() {
    match &parse_one_stmt("signal counter: UInt<8> @ sys_clk = 0").node {
        StmtKind::Signal(d) => { assert_eq!(d.domain.as_ref().unwrap().node, "sys_clk"); assert!(d.init.is_some()); }
        other => panic!("expected Signal, got {:?}", other),
    }
}
#[test] fn stmt_let_decl() { assert!(matches!(&parse_one_stmt("let x = 42").node, StmtKind::Let(d) if d.name.node == "x" && d.ty.is_none())); }
#[test] fn stmt_let_with_type() { assert!(matches!(&parse_one_stmt("let x: UInt<8> = 42").node, StmtKind::Let(d) if d.ty.is_some())); }
#[test] fn stmt_const_decl() { assert!(matches!(&parse_one_stmt("const WIDTH: UInt<8> = 32").node, StmtKind::Const(d) if d.name.node == "WIDTH")); }
#[test] fn stmt_type_alias() { assert!(matches!(&parse_one_stmt("type Word = UInt<32>").node, StmtKind::TypeAlias(d) if d.name.node == "Word")); }
#[test] fn stmt_assignment() { assert!(matches!(parse_one_stmt("x = y + 1").node, StmtKind::Assign { .. })); }
#[test] fn stmt_expr_stmt() { assert!(matches!(parse_one_stmt("foo(bar)").node, StmtKind::ExprStmt(_))); }
#[test] fn stmt_static_assert() { assert!(matches!(parse_one_stmt("static_assert WIDTH > 0, \"width must be positive\"").node, StmtKind::StaticAssert { .. })); }
```

- [ ] **8d. Verify tests pass**

```bash
cargo test -p ssl-core --test parser_tests stmt_ 2>&1
# Expected: 9 statement tests pass
```

- [ ] **8e. Commit**

```bash
git add crates/ssl-core/src/parser/stmt.rs crates/ssl-core/tests/parser_tests.rs
git commit -m "feat(parser): implement statement and declaration parsing

Add parse_stmt dispatcher with signal, let, const, type alias, assignment,
if/elif/else, match, for, comb, reg, priority, parallel, assert, assume,
cover, and static_assert parsers."
```

---

### Task 9: Block Statement Tests

**Files:** `crates/ssl-core/tests/parser_tests.rs`

#### TDD Steps

- [ ] **9a. Write block statement tests using manual token streams**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
use ssl_core::lexer::NumericLiteral;
type Stmt = ssl_core::ast::stmt::Stmt;

fn parse_stmt_tokens(source: &str, tokens: Vec<Spanned<Token>>) -> Stmt {
    let mut p = Parser::new(source, tokens);
    parse_stmt(&mut p).expect("parse error")
}

#[test] fn block_if_simple() {
    let s = "if x:\n    y = 1\n";
    let t = vec![tok(Token::KwIf,0,2), tok(Token::Ident,3,4), tok(Token::Colon,4,5),
        tok(Token::Newline,5,6), tok(Token::Indent,6,6), tok(Token::Ident,10,11),
        tok(Token::Eq,12,13), tok(Token::Numeric(NumericLiteral::Decimal(1)),14,15),
        tok(Token::Newline,15,16), tok(Token::Dedent,16,16)];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::If(i) => { assert_eq!(i.then_body.len(), 1); assert!(i.elif_branches.is_empty()); assert!(i.else_body.is_none()); }
        _ => panic!("expected If"),
    }
}

#[test] fn block_if_elif_else() {
    let s = "if a:\n  x=1\nelif b:\n  x=2\nelse:\n  x=3\n";
    let t = vec![
        tok(Token::KwIf,0,2), tok(Token::Ident,3,4), tok(Token::Colon,4,5), tok(Token::Newline,5,6), tok(Token::Indent,6,6),
        tok(Token::Ident,8,9), tok(Token::Eq,9,10), tok(Token::Numeric(NumericLiteral::Decimal(1)),10,11), tok(Token::Newline,11,12), tok(Token::Dedent,12,12),
        tok(Token::KwElif,12,16), tok(Token::Ident,17,18), tok(Token::Colon,18,19), tok(Token::Newline,19,20), tok(Token::Indent,20,20),
        tok(Token::Ident,22,23), tok(Token::Eq,23,24), tok(Token::Numeric(NumericLiteral::Decimal(2)),24,25), tok(Token::Newline,25,26), tok(Token::Dedent,26,26),
        tok(Token::KwElse,26,30), tok(Token::Colon,30,31), tok(Token::Newline,31,32), tok(Token::Indent,32,32),
        tok(Token::Ident,34,35), tok(Token::Eq,35,36), tok(Token::Numeric(NumericLiteral::Decimal(3)),36,37), tok(Token::Newline,37,38), tok(Token::Dedent,38,38),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::If(i) => { assert_eq!(i.elif_branches.len(), 1); assert!(i.else_body.is_some()); }
        _ => panic!("expected If"),
    }
}

#[test] fn block_match_two_arms() {
    let s = "match st:\n  A=>x=0\n  B=>x=1\n";
    let t = vec![
        tok(Token::KwMatch,0,5), tok(Token::Ident,6,8), tok(Token::Colon,8,9), tok(Token::Newline,9,10), tok(Token::Indent,10,10),
        tok(Token::Ident,12,13), tok(Token::FatArrow,13,15), tok(Token::Ident,15,16), tok(Token::Eq,16,17), tok(Token::Numeric(NumericLiteral::Decimal(0)),17,18), tok(Token::Newline,18,19),
        tok(Token::Ident,21,22), tok(Token::FatArrow,22,24), tok(Token::Ident,24,25), tok(Token::Eq,25,26), tok(Token::Numeric(NumericLiteral::Decimal(1)),26,27), tok(Token::Newline,27,28),
        tok(Token::Dedent,28,28),
    ];
    match &parse_stmt_tokens(s, t).node { StmtKind::Match(m) => assert_eq!(m.arms.len(), 2), _ => panic!("expected Match") }
}

#[test] fn block_comb() {
    let s = "comb:\n  x=a+b\n";
    let t = vec![
        tok(Token::KwComb,0,4), tok(Token::Colon,4,5), tok(Token::Newline,5,6), tok(Token::Indent,6,6),
        tok(Token::Ident,8,9), tok(Token::Eq,9,10), tok(Token::Ident,10,11), tok(Token::Plus,11,12), tok(Token::Ident,12,13),
        tok(Token::Newline,13,14), tok(Token::Dedent,14,14),
    ];
    match &parse_stmt_tokens(s, t).node { StmtKind::CombBlock(v) => assert_eq!(v.len(), 1), _ => panic!("expected CombBlock") }
}

#[test] fn block_reg_reset_tick() {
    let s = "reg(c,r):\n on reset:\n  x=0\n on tick:\n  x=x+1\n";
    let t = vec![
        tok(Token::KwReg,0,3), tok(Token::LParen,3,4), tok(Token::Ident,4,5), tok(Token::Comma,5,6), tok(Token::Ident,6,7), tok(Token::RParen,7,8),
        tok(Token::Colon,8,9), tok(Token::Newline,9,10), tok(Token::Indent,10,10),
        tok(Token::KwOn,11,13), tok(Token::KwReset,14,19), tok(Token::Colon,19,20), tok(Token::Newline,20,21), tok(Token::Indent,21,21),
        tok(Token::Ident,23,24), tok(Token::Eq,24,25), tok(Token::Numeric(NumericLiteral::Decimal(0)),25,26), tok(Token::Newline,26,27), tok(Token::Dedent,27,27),
        tok(Token::KwOn,28,30), tok(Token::KwTick,31,35), tok(Token::Colon,35,36), tok(Token::Newline,36,37), tok(Token::Indent,37,37),
        tok(Token::Ident,39,40), tok(Token::Eq,40,41), tok(Token::Ident,41,42), tok(Token::Plus,42,43), tok(Token::Numeric(NumericLiteral::Decimal(1)),43,44),
        tok(Token::Newline,44,45), tok(Token::Dedent,45,45),
        tok(Token::Dedent,45,45),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::RegBlock(r) => { assert_eq!(r.on_reset.len(), 1); assert_eq!(r.on_tick.len(), 1); assert!(r.enable.is_none()); }
        _ => panic!("expected RegBlock"),
    }
}

#[test] fn block_priority() {
    let s = "priority:\n when a=>x=1\n when b=>x=2\n otherwise=>x=0\n";
    let t = vec![
        tok(Token::KwPriority,0,8), tok(Token::Colon,8,9), tok(Token::Newline,9,10), tok(Token::Indent,10,10),
        tok(Token::KwWhen,11,15), tok(Token::Ident,16,17), tok(Token::FatArrow,17,19), tok(Token::Ident,19,20), tok(Token::Eq,20,21), tok(Token::Numeric(NumericLiteral::Decimal(1)),21,22), tok(Token::Newline,22,23),
        tok(Token::KwWhen,24,28), tok(Token::Ident,29,30), tok(Token::FatArrow,30,32), tok(Token::Ident,32,33), tok(Token::Eq,33,34), tok(Token::Numeric(NumericLiteral::Decimal(2)),34,35), tok(Token::Newline,35,36),
        tok(Token::KwOtherwise,37,46), tok(Token::FatArrow,46,48), tok(Token::Ident,48,49), tok(Token::Eq,49,50), tok(Token::Numeric(NumericLiteral::Decimal(0)),50,51), tok(Token::Newline,51,52),
        tok(Token::Dedent,52,52),
    ];
    match &parse_stmt_tokens(s, t).node { StmtKind::PriorityBlock(pb) => { assert_eq!(pb.arms.len(), 2); assert!(pb.otherwise.is_some()); } _ => panic!("expected PriorityBlock") }
}

#[test] fn block_parallel() {
    let s = "parallel:\n when a=>x=1\n when b=>x=2\n";
    let t = vec![
        tok(Token::KwParallel,0,8), tok(Token::Colon,8,9), tok(Token::Newline,9,10), tok(Token::Indent,10,10),
        tok(Token::KwWhen,11,15), tok(Token::Ident,16,17), tok(Token::FatArrow,17,19), tok(Token::Ident,19,20), tok(Token::Eq,20,21), tok(Token::Numeric(NumericLiteral::Decimal(1)),21,22), tok(Token::Newline,22,23),
        tok(Token::KwWhen,24,28), tok(Token::Ident,29,30), tok(Token::FatArrow,30,32), tok(Token::Ident,32,33), tok(Token::Eq,33,34), tok(Token::Numeric(NumericLiteral::Decimal(2)),34,35), tok(Token::Newline,35,36),
        tok(Token::Dedent,36,36),
    ];
    match &parse_stmt_tokens(s, t).node { StmtKind::ParallelBlock(pb) => { assert_eq!(pb.arms.len(), 2); assert!(pb.safe.is_none()); } _ => panic!("expected ParallelBlock") }
}

#[test] fn block_assert_always() {
    let s = "assert always @ ck: x > 0, \"msg\"";
    let t = vec![
        tok(Token::KwAssert,0,6), tok(Token::KwAlways,7,13), tok(Token::At,14,15), tok(Token::Ident,16,18),
        tok(Token::Colon,18,19), tok(Token::Ident,20,21), tok(Token::Greater,22,23), tok(Token::Numeric(NumericLiteral::Decimal(0)),24,25),
        tok(Token::Comma,25,26), tok(Token::StringLit("msg".into()),27,32),
    ];
    match &parse_stmt_tokens(s, t).node {
        StmtKind::Assert(a) => { assert!(a.always); assert!(a.domain.is_some()); assert!(a.message.is_some()); }
        _ => panic!("expected Assert"),
    }
}

#[test] fn block_for_loop() {
    let s = "for i in 0..8:\n  x=i\n";
    let t = vec![
        tok(Token::KwFor,0,3), tok(Token::Ident,4,5), tok(Token::KwIn,6,8),
        tok(Token::Numeric(NumericLiteral::Decimal(0)),9,10), tok(Token::RangeExclusive,10,12), tok(Token::Numeric(NumericLiteral::Decimal(8)),12,13),
        tok(Token::Colon,13,14), tok(Token::Newline,14,15), tok(Token::Indent,15,15),
        tok(Token::Ident,17,18), tok(Token::Eq,18,19), tok(Token::Ident,19,20), tok(Token::Newline,20,21), tok(Token::Dedent,21,21),
    ];
    match &parse_stmt_tokens(s, t).node { StmtKind::For(f) => { assert_eq!(f.var.node, "i"); assert_eq!(f.body.len(), 1); } _ => panic!("expected For") }
}
```

- [ ] **9b. Verify all tests pass**

```bash
cargo test -p ssl-core --test parser_tests block_ 2>&1
# Expected: 8 block statement tests pass
cargo test -p ssl-core --test parser_tests 2>&1
# Expected: all parser tests pass (9 helper + 18 expr + 17 edge + 13 type + 9 stmt + 9 block = 75)
cargo test -p ssl-core 2>&1
# Expected: all tests pass (lexer, ast, parser)
```

- [ ] **9c. Commit**

```bash
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "test(parser): add block statement tests for if, match, comb, reg, priority, assert, for

Add 8 tests covering if/elif/else, match with multiple arms, comb block,
reg block with on reset/on tick, priority block with otherwise,
assert always with domain, and for loop with range."
```

---

## Chunk 4: Item Parsing + CLI Integration

### Task 10: Module, Struct, Enum, Interface, Fn Parsing

**Files:** `crates/ssl-core/src/parser/item.rs`

#### TDD Steps

- [ ] **10a. Implement `crates/ssl-core/src/parser/item.rs`**

Replace the placeholder stub with the item parser. All parse functions are `impl Parser<'_>` methods. The existing `Parser::parse` static method in `mod.rs` must be updated to call `parser.parse_item()` instead of `item::parse_item(&mut parser)`.

First, update the call in `crates/ssl-core/src/parser/mod.rs` — change `item::parse_item(&mut parser)?` to `parser.parse_item()?`.

Then replace `crates/ssl-core/src/parser/item.rs`:

```rust
use crate::ast::expr::{Expr, ExprKind, CallArg};
use crate::ast::item::*;
use crate::ast::stmt::Stmt;
use crate::ast::types::{Direction, GenericParam};
use crate::ast::{Ident, Attribute, DocComment};
use crate::lexer::Token;
use crate::span::{Span, Spanned};
use super::expr::parse_expr;
use super::stmt::parse_stmt;
use super::types::{parse_generic_params, parse_type_expr, parse_type_expr_with_domain};
use super::{ParseError, Parser};

impl<'src> Parser<'src> {
    /// Parse a single top-level or nested item.
    pub fn parse_item(&mut self) -> Result<Item, ParseError> {
        let start = self.peek_span();

        // Collect leading doc comments
        let mut doc: Option<DocComment> = None;
        while self.check(Token::DocComment) {
            let tok = self.advance();
            let text = self.text(tok.span).to_string();
            doc = Some(DocComment { text, span: tok.span });
            self.skip_newlines();
        }

        // Collect leading attributes
        let mut attrs = Vec::new();
        while self.check(Token::At) {
            attrs.push(self.parse_attribute()?);
            self.skip_newlines();
        }

        // Check for pub modifier
        let public = self.eat(Token::KwPub).is_some();

        let kind = match self.peek().cloned() {
            Some(Token::KwModule) => ItemKind::Module(self.parse_module_def(doc.take(), attrs.drain(..).collect(), public)?),
            Some(Token::KwStruct) => ItemKind::Struct(self.parse_struct_def(doc.take())?),
            Some(Token::KwEnum) => ItemKind::Enum(self.parse_enum_def(doc.take())?),
            Some(Token::KwInterface) => ItemKind::Interface(self.parse_interface_def(doc.take())?),
            Some(Token::KwFn) => ItemKind::FnDef(self.parse_fn_def(doc.take())?),
            Some(Token::KwFsm) => self.parse_fsm_def()?,
            Some(Token::KwPipeline) => self.parse_pipeline_def()?,
            Some(Token::KwTest) => ItemKind::Test(self.parse_test_block()?),
            Some(Token::KwImport) => ItemKind::Import(self.parse_import()?),
            Some(Token::KwExtern) => ItemKind::ExternModule(self.parse_extern_module()?),
            Some(Token::KwInst) => ItemKind::Inst(self.parse_inst_decl()?),
            Some(Token::KwGen) => self.parse_gen()?,
            _ => {
                // Fall back to statement
                let stmt = parse_stmt(self)?;
                ItemKind::Stmt(stmt)
            }
        };

        Ok(Spanned::new(kind, start.merge(self.prev_span())))
    }

    /// Parse `@ IDENT [( ARGS )]`
    fn parse_attribute(&mut self) -> Result<Attribute, ParseError> {
        self.expect_token(Token::At)?;
        let name = self.expect_ident()?;
        let args = if self.eat(Token::LParen).is_some() {
            self.parse_comma_list(Token::RParen, parse_expr)?
        } else {
            Vec::new()
        };
        Ok(Attribute { name, args })
    }

    /// `[pub] module NAME [<GENERICS>] ( PORTS ) [@ DOMAIN]: INDENT ITEMS DEDENT`
    fn parse_module_def(&mut self, doc: Option<DocComment>, attrs: Vec<Attribute>, public: bool) -> Result<ModuleDef, ParseError> {
        self.expect_token(Token::KwModule)?;
        let name = self.expect_ident()?;
        let generics = parse_generic_params(self)?;
        self.expect_token(Token::LParen)?;
        let ports = self.parse_comma_list(Token::RParen, |p| p.parse_port())?;
        let default_domain = if self.eat(Token::At).is_some() { Some(self.expect_ident()?) } else { None };
        let body = self.parse_block(|p| p.parse_item())?;
        Ok(ModuleDef { doc, attrs, public, name, generics, ports, default_domain, body })
    }

    /// Parse a single port: `[doc] DIR NAME: TYPE`
    fn parse_port(&mut self) -> Result<Port, ParseError> {
        let start = self.peek_span();
        let doc = if self.check(Token::DocComment) {
            let tok = self.advance();
            self.skip_newlines();
            Some(DocComment { text: self.text(tok.span).to_string(), span: tok.span })
        } else { None };
        let direction = match self.peek().cloned() {
            Some(Token::KwIn) => { self.advance(); Direction::In }
            Some(Token::KwOut) => { self.advance(); Direction::Out }
            Some(Token::KwInout) => { self.advance(); Direction::InOut }
            _ => return Err(self.error("expected port direction (in, out, inout)")),
        };
        let name = self.expect_ident()?;
        self.expect_token(Token::Colon)?;
        let ty = parse_type_expr_with_domain(self)?;
        Ok(Port { doc, direction, name, ty, span: start.merge(self.prev_span()) })
    }

    /// `struct NAME: INDENT (NAME: TYPE [@ [H:L]])* DEDENT`
    fn parse_struct_def(&mut self, doc: Option<DocComment>) -> Result<StructDef, ParseError> {
        self.expect_token(Token::KwStruct)?;
        let name = self.expect_ident()?;
        let fields = self.parse_block(|p| {
            let field_start = p.peek_span();
            let fname = p.expect_ident()?;
            p.expect_token(Token::Colon)?;
            let ty = parse_type_expr(p)?;
            let bit_range = if p.eat(Token::At).is_some() {
                p.expect_token(Token::LBracket)?;
                let hi = parse_expr(p)?;
                p.expect_token(Token::Colon)?;
                let lo = parse_expr(p)?;
                p.expect_token(Token::RBracket)?;
                Some((hi, lo))
            } else {
                None
            };
            Ok(StructField { name: fname, ty, bit_range, span: field_start.merge(p.prev_span()) })
        })?;
        Ok(StructDef { doc, name, fields })
    }

    /// `enum NAME [encoding]: INDENT (VARIANT [= EXPR])* DEDENT`
    fn parse_enum_def(&mut self, doc: Option<DocComment>) -> Result<EnumDef, ParseError> {
        self.expect_token(Token::KwEnum)?;
        let name = self.expect_ident()?;
        // Optional encoding in brackets
        let encoding = if self.eat(Token::LBracket).is_some() {
            let enc_name = self.expect_ident()?;
            let enc = match enc_name.node.as_str() {
                "binary" => EnumEncoding::Binary,
                "onehot" => EnumEncoding::Onehot,
                "gray" => EnumEncoding::Gray,
                "custom" => EnumEncoding::Custom,
                _ => return Err(ParseError {
                    message: format!("expected encoding (binary/onehot/gray/custom), found '{}'", enc_name.node),
                    span: enc_name.span,
                }),
            };
            self.expect_token(Token::RBracket)?;
            Some(enc)
        } else {
            None
        };
        let variants = self.parse_block(|p| {
            let var_start = p.peek_span();
            let vname = p.expect_ident()?;
            let value = if p.eat(Token::Eq).is_some() { Some(parse_expr(p)?) } else { None };
            Ok(EnumVariant { name: vname, value, span: var_start.merge(p.prev_span()) })
        })?;
        Ok(EnumDef { doc, name, encoding, variants })
    }

    /// `interface NAME [<GENERICS>]: INDENT (group|signal|property)* DEDENT`
    fn parse_interface_def(&mut self, doc: Option<DocComment>) -> Result<InterfaceDef, ParseError> {
        self.expect_token(Token::KwInterface)?;
        let name = self.expect_ident()?;
        let generics = parse_generic_params(self)?;
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;
        let (mut groups, mut signals, mut properties) = (Vec::new(), Vec::new(), Vec::new());
        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() { break; }
            let s = self.peek_span();
            let label = self.expect_ident()?;
            match label.node.as_str() {
                "group" => {
                    let gn = self.expect_ident()?;
                    let gs = self.parse_block(|p| {
                        let ss = p.peek_span(); let sn = p.expect_ident()?;
                        p.expect_token(Token::Colon)?; let st = parse_type_expr(p)?;
                        Ok(InterfaceSignal { name: sn, ty: st, span: ss.merge(p.prev_span()) })
                    })?;
                    groups.push(InterfaceGroup { name: gn, signals: gs, span: s.merge(self.prev_span()) });
                }
                "property" => {
                    let pn = self.expect_ident()?;
                    self.expect_token(Token::Colon)?;
                    self.skip_newlines();
                    // Property body: either inline expr or indented block with single expr
                    let body = if self.check(Token::Indent) {
                        self.expect_token(Token::Indent)?;
                        self.skip_newlines();
                        let e = parse_expr(self)?;
                        self.skip_newlines();
                        self.expect_token(Token::Dedent)?;
                        e
                    } else {
                        parse_expr(self)?
                    };
                    properties.push(InterfaceProperty { name: pn, body, span: s.merge(self.prev_span()) });
                }
                _ => {
                    self.expect_token(Token::Colon)?; let ty = parse_type_expr(self)?;
                    signals.push(InterfaceSignal { name: label, ty, span: s.merge(self.prev_span()) });
                }
            }
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;
        Ok(InterfaceDef { doc, name, generics, groups, signals, properties })
    }

    /// `fn NAME [<GENERICS>] ( PARAMS ) -> TYPE: INDENT STMTS DEDENT`
    fn parse_fn_def(&mut self, doc: Option<DocComment>) -> Result<FnDef, ParseError> {
        self.expect_token(Token::KwFn)?;
        let name = self.expect_ident()?;
        let generics = parse_generic_params(self)?;

        self.expect_token(Token::LParen)?;
        let params = self.parse_comma_list(Token::RParen, |p| {
            let param_start = p.peek_span();
            let pname = p.expect_ident()?;
            p.expect_token(Token::Colon)?;
            let pty = parse_type_expr(p)?;
            Ok(FnParam { name: pname, ty: pty, span: param_start.merge(p.prev_span()) })
        })?;

        self.expect_token(Token::ThinArrow)?;
        let return_type = parse_type_expr(self)?;

        let body = self.parse_block(|p| parse_stmt(p))?;

        Ok(FnDef { doc, name, generics, params, return_type, body })
    }
}
```

- [ ] **10b. Write item parser tests**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
use ssl_core::ast::item::*;
use ssl_core::ast::types::Direction;
use ssl_core::lexer::{Token, NumericLiteral};

fn parse_one_item(src: &str) -> Item {
    let tokens = ssl_core::lexer::lex(src).expect("lexer error");
    let mut p = Parser::new(src, tokens);
    p.parse_item().expect("parse error")
}

#[test]
fn item_module_with_ports() {
    let src = "module Adder(in a: UInt<8>, in b: UInt<8>, out sum: UInt<9>):\n  signal c: UInt<9>\n";
    let item = parse_one_item(src);
    match &item.node {
        ItemKind::Module(m) => {
            assert_eq!(m.name.node, "Adder");
            assert_eq!(m.ports.len(), 3);
            assert_eq!(m.ports[0].direction, Direction::In);
            assert_eq!(m.ports[0].name.node, "a");
            assert_eq!(m.ports[2].direction, Direction::Out);
            assert_eq!(m.body.len(), 1);
        }
        other => panic!("expected Module, got {:?}", other),
    }
}

#[test]
fn item_struct_with_fields() {
    let src = "struct Packet:\n  header: UInt<8>\n  payload: UInt<32>\n";
    match &parse_one_item(src).node {
        ItemKind::Struct(s) => {
            assert_eq!(s.name.node, "Packet");
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name.node, "header");
        }
        other => panic!("expected Struct, got {:?}", other),
    }
}

#[test]
fn item_enum_with_encoding() {
    let src = "enum State [onehot]:\n  Idle\n  Run\n  Done\n";
    match &parse_one_item(src).node {
        ItemKind::Enum(e) => {
            assert_eq!(e.name.node, "State");
            assert_eq!(e.encoding, Some(EnumEncoding::Onehot));
            assert_eq!(e.variants.len(), 3);
            assert_eq!(e.variants[0].name.node, "Idle");
        }
        other => panic!("expected Enum, got {:?}", other),
    }
}

#[test]
fn item_interface_with_group() {
    let src = "interface AXI:\n  group write:\n    addr: Out<UInt<32>>\n    data: Out<UInt<64>>\n  ready: In<Bool>\n";
    match &parse_one_item(src).node {
        ItemKind::Interface(i) => {
            assert_eq!(i.name.node, "AXI");
            assert_eq!(i.groups.len(), 1);
            assert_eq!(i.groups[0].name.node, "write");
            assert_eq!(i.groups[0].signals.len(), 2);
            assert_eq!(i.signals.len(), 1);
            assert_eq!(i.signals[0].name.node, "ready");
        }
        other => panic!("expected Interface, got {:?}", other),
    }
}

#[test]
fn item_fn_definition() {
    let src = "fn add(a: UInt<8>, b: UInt<8>) -> UInt<9>:\n  let c = a + b\n";
    match &parse_one_item(src).node {
        ItemKind::FnDef(f) => {
            assert_eq!(f.name.node, "add");
            assert_eq!(f.params.len(), 2);
            assert_eq!(f.params[0].name.node, "a");
            assert_eq!(f.body.len(), 1);
        }
        other => panic!("expected FnDef, got {:?}", other),
    }
}
```

- [ ] **10c. Verify tests pass**

```bash
cargo test -p ssl-core --test parser_tests item_ 2>&1
# Expected: 5 item tests pass
cargo test -p ssl-core 2>&1
# Expected: all tests pass
```

- [ ] **10d. Commit**

```bash
git add crates/ssl-core/src/parser/item.rs crates/ssl-core/src/parser/mod.rs
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "feat(parser): implement module, struct, enum, interface, fn item parsing

Add parse_item dispatcher with doc comment and attribute collection,
parse_module_def with ports and generic params, parse_struct_def with
optional bit ranges, parse_enum_def with encoding, parse_interface_def
with groups/signals/properties, parse_fn_def with params and return type.
Add parse_attribute helper. All item parsers are impl Parser methods."
```

---

### Task 11: FSM, Pipeline, Test, Import, Extern, Inst, Gen Parsing

**Files:** `crates/ssl-core/src/parser/item.rs` (append to existing `impl Parser` block)

#### TDD Steps

- [ ] **11a. Add FSM, pipeline, and remaining item parsers**

Append these methods inside the existing `impl<'src> Parser<'src>` block in `crates/ssl-core/src/parser/item.rs`:

```rust
impl<'src> Parser<'src> {
    // ... (existing methods from Task 10 above) ...

    /// Parse FSM definition with states, transitions, outputs.
    fn parse_fsm_def(&mut self) -> Result<ItemKind, ParseError> {
        self.expect_token(Token::KwFsm)?;
        let name = self.expect_ident()?;

        // (clock, reset)
        self.expect_token(Token::LParen)?;
        let clock = parse_expr(self)?;
        self.expect_token(Token::Comma)?;
        let reset = parse_expr(self)?;
        self.expect_token(Token::RParen)?;

        // Open block
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;

        let mut states = Vec::new();
        let mut encoding = None;
        let mut initial: Option<Ident> = None;
        let mut transitions = Vec::new();
        let mut on_tick: Option<Vec<Stmt>> = None;
        let mut outputs = Vec::new();

        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() { break; }

            let label = self.expect_ident()?;
            match label.node.as_str() {
                "states" => {
                    self.expect_token(Token::Colon)?;
                    states.push(self.expect_ident()?);
                    while self.eat(Token::Pipe).is_some() {
                        states.push(self.expect_ident()?);
                    }
                }
                "encoding" => {
                    self.expect_token(Token::Colon)?;
                    let enc_name = self.expect_ident()?;
                    encoding = Some(match enc_name.node.as_str() {
                        "binary" => EnumEncoding::Binary,
                        "onehot" => EnumEncoding::Onehot,
                        "gray" => EnumEncoding::Gray,
                        "custom" => EnumEncoding::Custom,
                        _ => return Err(ParseError {
                            message: format!("expected fsm encoding (binary/onehot/gray/custom), found '{}'", enc_name.node),
                            span: enc_name.span,
                        }),
                    });
                }
                "initial" => {
                    self.expect_token(Token::Colon)?;
                    initial = Some(self.expect_ident()?);
                }
                "transitions" => {
                    let trans = self.parse_block(|p| p.parse_fsm_transition())?;
                    transitions = trans;
                }
                "on" => {
                    // on tick:
                    let what = self.expect_ident()?;
                    if what.node != "tick" {
                        return Err(ParseError { message: format!("expected 'tick', found '{}'", what.node), span: what.span });
                    }
                    on_tick = Some(self.parse_block(|p| parse_stmt(p))?);
                }
                "outputs" => {
                    outputs = self.parse_block(|p| {
                        let out_start = p.peek_span();
                        let state = p.expect_ident()?;
                        p.expect_token(Token::FatArrow)?;
                        // Output assignments: single stmt on same line, or block
                        let assignments = if p.check(Token::Newline) || p.check(Token::Indent) {
                            p.skip_newlines();
                            if p.check(Token::Indent) {
                                p.expect_token(Token::Indent)?;
                                let mut stmts = Vec::new();
                                while !p.check(Token::Dedent) && !p.is_at_end() {
                                    p.skip_newlines();
                                    if p.check(Token::Dedent) { break; }
                                    stmts.push(parse_stmt(p)?);
                                    p.skip_newlines();
                                }
                                p.expect_token(Token::Dedent)?;
                                stmts
                            } else {
                                vec![parse_stmt(p)?]
                            }
                        } else {
                            vec![parse_stmt(p)?]
                        };
                        Ok(FsmOutput { state, assignments, span: out_start.merge(p.prev_span()) })
                    })?;
                }
                _ => return Err(ParseError {
                    message: format!("unexpected fsm section '{}'", label.node),
                    span: label.span,
                }),
            }
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;

        let init = initial.ok_or_else(|| self.error("fsm missing 'initial' state"))?;

        Ok(ItemKind::Fsm(FsmDef {
            name, clock, reset, states, encoding, initial: init,
            transitions, on_tick, outputs,
        }))
    }

    /// Parse `STATE|_ --(EXPR)--> STATE|_` or `STATE|_ --timeout(EXPR)--> STATE|_`
    fn parse_fsm_transition(&mut self) -> Result<FsmTransition, ParseError> {
        let start = self.peek_span();

        // From state or wildcard
        let from = if self.eat(Token::Underscore).is_some() {
            FsmStateRef::Wildcard(self.prev_span())
        } else {
            FsmStateRef::Named(self.expect_ident()?)
        };

        // --
        self.expect_token(Token::DashDash)?;

        // Check for timeout variant: --timeout(EXPR)-->
        let condition = if self.check_ident() && self.text(self.peek_span()) == "timeout" {
            self.advance(); // consume "timeout"
            self.expect_token(Token::LParen)?;
            let expr = parse_expr(self)?;
            self.expect_token(Token::RParen)?;
            FsmCondition::Timeout(expr)
        } else {
            self.expect_token(Token::LParen)?;
            let expr = parse_expr(self)?;
            self.expect_token(Token::RParen)?;
            FsmCondition::Expr(expr)
        };

        // -->
        self.expect_token(Token::LongArrow)?;

        // To state or wildcard
        let to = if self.eat(Token::Underscore).is_some() {
            FsmStateRef::Wildcard(self.prev_span())
        } else {
            FsmStateRef::Named(self.expect_ident()?)
        };

        // Optional actions after `:` — single stmt on same line, or indented block
        let actions = if self.eat(Token::Colon).is_some() {
            if self.check(Token::Newline) || self.check(Token::Indent) {
                self.skip_newlines();
                if self.check(Token::Indent) {
                    self.expect_token(Token::Indent)?;
                    let mut stmts = Vec::new();
                    while !self.check(Token::Dedent) && !self.is_at_end() {
                        self.skip_newlines();
                        if self.check(Token::Dedent) { break; }
                        stmts.push(parse_stmt(self)?);
                        self.skip_newlines();
                    }
                    self.expect_token(Token::Dedent)?;
                    stmts
                } else {
                    vec![parse_stmt(self)?]
                }
            } else {
                vec![parse_stmt(self)?]
            }
        } else {
            Vec::new()
        };

        Ok(FsmTransition { from, condition, to, actions, span: start.merge(self.prev_span()) })
    }

    /// Parse pipeline definition with stages and backpressure.
    fn parse_pipeline_def(&mut self) -> Result<ItemKind, ParseError> {
        self.expect_token(Token::KwPipeline)?;
        let name = self.expect_ident()?;

        self.expect_token(Token::LParen)?;
        let clock = parse_expr(self)?;
        self.expect_token(Token::Comma)?;
        let reset = parse_expr(self)?;
        // Optional backpressure parameter
        let backpressure = if self.eat(Token::Comma).is_some() {
            let bp_name = self.expect_ident()?;
            if bp_name.node != "backpressure" {
                return Err(ParseError { message: format!("expected 'backpressure', found '{}'", bp_name.node), span: bp_name.span });
            }
            self.expect_token(Token::Eq)?;
            let mode = self.expect_ident()?;
            match mode.node.as_str() {
                "auto" => BackpressureMode::Auto(Vec::new()),
                "manual" => BackpressureMode::Manual,
                "none" => BackpressureMode::None,
                _ => return Err(ParseError { message: format!("expected backpressure mode, found '{}'", mode.node), span: mode.span }),
            }
        } else {
            BackpressureMode::Auto(Vec::new())
        };
        self.expect_token(Token::RParen)?;

        // Open block
        self.expect_token(Token::Colon)?;
        self.skip_newlines();
        self.expect_token(Token::Indent)?;

        // Parse input: and output:
        let input = self.parse_pipeline_port("input")?;
        self.skip_newlines();
        let output = self.parse_pipeline_port("output")?;
        self.skip_newlines();

        // Parse stages
        let mut stages = Vec::new();
        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() { break; }
            stages.push(self.parse_pipeline_stage()?);
            self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;

        Ok(ItemKind::Pipeline(PipelineDef { name, clock, reset, backpressure, input, output, stages }))
    }

    /// Parse `input: IDENT, IDENT, ...` or `output: IDENT, IDENT, ...`
    fn parse_pipeline_port(&mut self, expected_label: &str) -> Result<PipelinePort, ParseError> {
        let start = self.peek_span();
        let label = self.expect_ident()?;
        if label.node != expected_label {
            return Err(ParseError { message: format!("expected '{}', found '{}'", expected_label, label.node), span: label.span });
        }
        self.expect_token(Token::Colon)?;
        let mut bindings = Vec::new();
        bindings.push(self.expect_ident()?);
        while self.eat(Token::Comma).is_some() {
            bindings.push(self.expect_ident()?);
        }
        Ok(PipelinePort { bindings, span: start.merge(self.prev_span()) })
    }

    /// Parse `stage N ["label"]: INDENT [stall_when/flush_when] STMTS DEDENT`
    fn parse_pipeline_stage(&mut self) -> Result<PipelineStage, ParseError> {
        let start = self.peek_span();
        let kw = self.expect_ident()?;
        if kw.node != "stage" { return Err(ParseError { message: format!("expected 'stage', found '{}'", kw.node), span: kw.span }); }
        let index = parse_expr(self)?;
        let label = if let Some(Token::StringLit(s)) = self.peek().cloned() { self.advance(); Some(s) } else { None };
        self.expect_token(Token::Colon)?; self.skip_newlines(); self.expect_token(Token::Indent)?;
        let (mut stall_when, mut flush_when, mut body) = (None, None, Vec::new());
        while !self.check(Token::Dedent) && !self.is_at_end() {
            self.skip_newlines();
            if self.check(Token::Dedent) || self.is_at_end() { break; }
            if self.check_ident() {
                let txt = self.text(self.peek_span()).to_string();
                if txt == "stall_when" || txt == "flush_when" {
                    self.advance(); self.expect_token(Token::Colon)?; let expr = parse_expr(self)?;
                    if txt == "stall_when" { stall_when = Some(expr); } else { flush_when = Some(expr); }
                    self.skip_newlines(); continue;
                }
            }
            body.push(parse_stmt(self)?); self.skip_newlines();
        }
        self.expect_token(Token::Dedent)?;
        Ok(PipelineStage { index, label, stall_when, flush_when, body, span: start.merge(self.prev_span()) })
    }

    /// `test "name": INDENT STMTS DEDENT`
    fn parse_test_block(&mut self) -> Result<TestBlock, ParseError> {
        self.expect_token(Token::KwTest)?;
        let name = match self.peek().cloned() {
            Some(Token::StringLit(s)) => { self.advance(); s }
            _ => return Err(self.error("expected test name string")),
        };
        let body = self.parse_block(|p| parse_stmt(p))?;
        Ok(TestBlock { name, body })
    }

    /// `import NAME from "path"` or `import { NAME, NAME } from "path"`
    fn parse_import(&mut self) -> Result<ImportStmt, ParseError> {
        self.expect_token(Token::KwImport)?;
        let names = if self.eat(Token::LBrace).is_some() {
            self.parse_comma_list(Token::RBrace, |p| p.expect_ident())?
        } else {
            vec![self.expect_ident()?]
        };
        self.expect_token(Token::KwFrom)?;
        let path = match self.peek().cloned() {
            Some(Token::StringLit(s)) => { self.advance(); s }
            _ => return Err(self.error("expected import path string")),
        };
        // Optional alias: `as NAME`
        let alias = if self.eat(Token::KwAs).is_some() {
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok(ImportStmt { names, path, alias })
    }

    /// `extern module NAME( PORTS ) @ verilog("name")`
    fn parse_extern_module(&mut self) -> Result<ExternModuleDef, ParseError> {
        self.expect_token(Token::KwExtern)?;
        self.expect_token(Token::KwModule)?;
        let name = self.expect_ident()?;
        self.expect_token(Token::LParen)?;
        let ports = self.parse_comma_list(Token::RParen, |p| p.parse_port())?;
        self.expect_token(Token::At)?;
        let backend = self.expect_ident()?.node;
        self.expect_token(Token::LParen)?;
        let backend_name = match self.peek().cloned() {
            Some(Token::StringLit(s)) => { self.advance(); s }
            _ => return Err(self.error("expected backend module name string")),
        };
        self.expect_token(Token::RParen)?;
        Ok(ExternModuleDef { name, ports, backend, backend_name })
    }

    /// `inst NAME = MODULE [<GENERICS>]( CONNECTIONS )`
    fn parse_inst_decl(&mut self) -> Result<InstDecl, ParseError> {
        self.expect_token(Token::KwInst)?;
        let name = self.expect_ident()?;
        self.expect_token(Token::Eq)?;
        let module_name = self.expect_ident()?;
        let generic_args = if self.eat(Token::Less).is_some() { self.parse_comma_list(Token::Greater, parse_expr)? } else { Vec::new() };
        self.expect_token(Token::LParen)?;
        let connections = self.parse_comma_list(Token::RParen, |p| {
            let s = p.peek_span();
            let port = p.expect_ident()?;
            let binding = if p.eat(Token::Eq).is_some() {
                if p.eat(Token::Underscore).is_some() { PortBinding::Discard } else { PortBinding::Input(parse_expr(p)?) }
            } else if p.eat(Token::ThinArrow).is_some() {
                if p.eat(Token::Underscore).is_some() { PortBinding::Discard } else { PortBinding::Output(parse_expr(p)?) }
            } else if p.eat(Token::BiArrow).is_some() { PortBinding::Bidirectional(parse_expr(p)?)
            } else { return Err(p.error("expected '=', '->', or '<->' in port connection")); };
            Ok(PortConnection { port, binding, span: s.merge(p.prev_span()) })
        })?;
        Ok(InstDecl { name, module_name, generic_args, connections })
    }

    /// `gen for ...` or `gen if ...`
    fn parse_gen(&mut self) -> Result<ItemKind, ParseError> {
        self.expect_token(Token::KwGen)?;
        match self.peek().cloned() {
            Some(Token::KwFor) => {
                self.advance(); let var = self.expect_ident()?;
                self.expect_token(Token::KwIn)?; let iterable = parse_expr(self)?;
                let body = self.parse_block(|p| p.parse_item())?;
                Ok(ItemKind::GenFor(GenFor { var, iterable, body }))
            }
            Some(Token::KwIf) => {
                self.advance(); let condition = parse_expr(self)?;
                let then_body = self.parse_block(|p| p.parse_item())?;
                let else_body = if self.check(Token::KwGen) && self.tokens.get(self.pos + 1).map(|t| &t.node) == Some(&Token::KwElse) {
                    self.advance(); self.advance(); Some(self.parse_block(|p| p.parse_item())?)
                } else { None };
                Ok(ItemKind::GenIf(GenIf { condition, then_body, else_body }))
            }
            _ => Err(self.error("expected 'for' or 'if' after 'gen'")),
        }
    }

    /// Top-level: parse entire source file into a `SourceFile`.
    pub fn parse_file(&mut self) -> Result<SourceFile, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.is_at_end() {
            items.push(self.parse_item()?);
            self.skip_newlines();
        }
        Ok(SourceFile { items })
    }
}
```

- [ ] **11b. Write FSM, pipeline, and remaining item tests**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
#[test]
fn item_fsm_with_transitions() {
    let src = "fsm Controller(clk, rst):\n  states: Idle | Run | Done\n  encoding: onehot\n  initial: Idle\n  transitions:\n    Idle --(start)--> Run\n    Run --timeout(100)--> Done\n  outputs:\n    Idle => busy = 0\n    Run => busy = 1\n";
    match &parse_one_item(src).node {
        ItemKind::Fsm(f) => {
            assert_eq!(f.name.node, "Controller"); assert_eq!(f.states.len(), 3);
            assert_eq!(f.encoding, Some(EnumEncoding::Onehot)); assert_eq!(f.initial.node, "Idle");
            assert_eq!(f.transitions.len(), 2);
            assert!(matches!(f.transitions[0].condition, FsmCondition::Expr(_)));
            assert!(matches!(f.transitions[1].condition, FsmCondition::Timeout(_)));
            assert_eq!(f.outputs.len(), 2);
        } other => panic!("expected Fsm, got {:?}", other),
    }
}

#[test]
fn item_pipeline_with_stages() {
    let src = "pipeline DataPipe(clk, rst, backpressure=none):\n  input: a, b\n  output: result\n  stage 0 \"fetch\":\n    let sum = a + b\n  stage 1:\n    result = sum\n";
    match &parse_one_item(src).node {
        ItemKind::Pipeline(p) => {
            assert_eq!(p.name.node, "DataPipe"); assert!(matches!(p.backpressure, BackpressureMode::None));
            assert_eq!(p.input.bindings.len(), 2); assert_eq!(p.stages.len(), 2);
            assert_eq!(p.stages[0].label, Some("fetch".to_string()));
        } other => panic!("expected Pipeline, got {:?}", other),
    }
}

#[test]
fn item_test_block() {
    let src = "test \"adder works\":\n  let x = 1\n";
    match &parse_one_item(src).node { ItemKind::Test(t) => { assert_eq!(t.name, "adder works"); assert_eq!(t.body.len(), 1); } other => panic!("expected Test, got {:?}", other) }
}

#[test]
fn item_import_single() {
    let src = "import Utils from \"./utils.ssl\"\n";
    match &parse_one_item(src).node { ItemKind::Import(i) => { assert_eq!(i.names[0].node, "Utils"); assert_eq!(i.path, "./utils.ssl"); } other => panic!("expected Import, got {:?}", other) }
}

#[test]
fn item_import_destructured() {
    let src = "import { Adder, Mux } from \"components.ssl\"\n";
    match &parse_one_item(src).node { ItemKind::Import(i) => { assert_eq!(i.names.len(), 2); assert_eq!(i.names[0].node, "Adder"); } other => panic!("expected Import, got {:?}", other) }
}

#[test]
fn item_extern_module() {
    let src = "extern module BRAM(in addr: UInt<16>, out data: UInt<32>) @ verilog(\"bram_ip\")\n";
    match &parse_one_item(src).node { ItemKind::ExternModule(e) => { assert_eq!(e.name.node, "BRAM"); assert_eq!(e.ports.len(), 2); assert_eq!(e.backend, "verilog"); } other => panic!("expected ExternModule, got {:?}", other) }
}

#[test]
fn item_inst_with_mixed_bindings() {
    let src = "inst my_add = Adder<8>(a = x, b = y, sum -> result, carry -> _)\n";
    match &parse_one_item(src).node {
        ItemKind::Inst(i) => {
            assert_eq!(i.name.node, "my_add"); assert_eq!(i.module_name.node, "Adder"); assert_eq!(i.generic_args.len(), 1);
            assert_eq!(i.connections.len(), 4); assert!(matches!(i.connections[0].binding, PortBinding::Input(_)));
            assert!(matches!(i.connections[3].binding, PortBinding::Discard));
        } other => panic!("expected Inst, got {:?}", other),
    }
}

#[test]
fn item_gen_for() {
    let src = "gen for i in 0..4:\n  signal s: UInt<8>\n";
    match &parse_one_item(src).node { ItemKind::GenFor(g) => { assert_eq!(g.var.node, "i"); assert_eq!(g.body.len(), 1); } other => panic!("expected GenFor, got {:?}", other) }
}
```

- [ ] **11c. Verify all tests pass**

```bash
cargo test -p ssl-core --test parser_tests item_ 2>&1
# Expected: 13 item tests pass (5 from 10b + 8 from 11b)
cargo test -p ssl-core 2>&1
# Expected: all tests pass
```

- [ ] **11d. Commit**

```bash
git add crates/ssl-core/src/parser/item.rs
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "feat(parser): implement FSM, pipeline, test, import, extern, inst, gen parsing

Add parse_fsm_def with states/encoding/initial/transitions/outputs,
parse_fsm_transition with DashDash/LongArrow and timeout variant,
parse_pipeline_def with backpressure modes and stages with stall/flush,
parse_test_block, parse_import (single and destructured), parse_extern_module,
parse_inst_decl with Input/Output/Bidirectional/Discard bindings,
parse_gen for gen-for and gen-if with optional else."
```

---

### Task 12: Top-Level File Parser + CLI Integration + End-to-End Tests

**Files:** `crates/ssl-core/src/parser/mod.rs`, `crates/sslc/src/main.rs`, `crates/ssl-core/tests/parser_tests.rs`

#### TDD Steps

- [ ] **12a. Update `Parser::parse` in `crates/ssl-core/src/parser/mod.rs`**

The static `Parser::parse` method should delegate to the instance method `parse_file`. Replace the existing `Parser::parse` body:

```rust
    /// Top-level entry point: parse a full source file.
    pub fn parse(source: &str, tokens: Vec<Spanned<Token>>) -> Result<SourceFile, ParseError> {
        let mut parser = Parser::new(source, tokens);
        parser.parse_file()
    }
```

- [ ] **12b. Update `crates/sslc/src/main.rs` with `parse` command**

Replace the entire file:

```rust
use ssl_core::lexer::tokenize;
use ssl_core::parser::Parser;
use std::path::PathBuf;

fn read_source(args: &[String], cmd: &str) -> (PathBuf, String) {
    if args.len() < 3 {
        eprintln!("Usage: sslc {} <file.ssl>", cmd);
        std::process::exit(1);
    }
    let path = PathBuf::from(&args[2]);
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", path.display(), e);
        std::process::exit(1);
    });
    (path, source)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: sslc <command> [args]");
        eprintln!("Commands:  lex <file>  |  parse <file>");
        std::process::exit(1);
    }
    match args[1].as_str() {
        "lex" => {
            let (_path, source) = read_source(&args, "lex");
            match tokenize(&source) {
                Ok(tokens) => {
                    for tok in &tokens { println!("{:>4}..{:<4} {:?}", tok.span.start, tok.span.end, tok.node); }
                    eprintln!("\n{} tokens", tokens.len());
                }
                Err(e) => { eprintln!("Lex error: {}", e); std::process::exit(1); }
            }
        }
        "parse" => {
            let (_path, source) = read_source(&args, "parse");
            let tokens = tokenize(&source).unwrap_or_else(|e| {
                eprintln!("Lex error: {}", e); std::process::exit(1);
            });
            match Parser::parse(&source, tokens) {
                Ok(ast) => { println!("{:#?}", ast); eprintln!("\n{} top-level items", ast.items.len()); }
                Err(e) => { eprintln!("Parse error: {}", e); std::process::exit(1); }
            }
        }
        other => { eprintln!("Unknown command: {}", other); std::process::exit(1); }
    }
}
```

- [ ] **12c. Write end-to-end integration tests**

Append to `crates/ssl-core/tests/parser_tests.rs`:

```rust
// --- End-to-end tests ---

fn parse_source(source: &str) -> ssl_core::ast::item::SourceFile {
    let tokens = ssl_core::lexer::tokenize(source).expect("lex failed");
    let mut parser = Parser::new(source, tokens);
    parser.parse_file().expect("parse failed")
}

#[test]
fn e2e_module_with_signal_and_comb() {
    let src = "module Counter(in clk: Clock, in rst: SyncReset, out count: UInt<8>):\n  signal r: UInt<8>\n  comb:\n    count = r\n";
    let file = parse_source(src);
    assert_eq!(file.items.len(), 1);
    match &file.items[0].node { ItemKind::Module(m) => { assert_eq!(m.name.node, "Counter"); assert_eq!(m.ports.len(), 3); assert_eq!(m.body.len(), 2); } other => panic!("expected Module, got {:?}", other) }
}

#[test]
fn e2e_module_with_reg_block() {
    let src = "module Reg8(in clk: Clock, in rst: SyncReset, in d: UInt<8>, out q: UInt<8>):\n  signal r: UInt<8>\n  reg(clk, rst):\n    on reset:\n      r = 0\n    on tick:\n      r = d\n  comb:\n    q = r\n";
    let file = parse_source(src);
    match &file.items[0].node { ItemKind::Module(m) => { assert_eq!(m.body.len(), 3); } other => panic!("expected Module, got {:?}", other) }
}

#[test]
fn e2e_struct_definition() {
    let src = "struct Header:\n  version: UInt<4>\n  length: UInt<12>\n  flags: UInt<8>\n";
    let file = parse_source(src);
    match &file.items[0].node { ItemKind::Struct(s) => { assert_eq!(s.fields.len(), 3); } other => panic!("expected Struct, got {:?}", other) }
}

#[test]
fn e2e_enum_onehot() {
    let src = "enum Color [onehot]:\n  Red\n  Green = 4\n  Blue\n";
    let file = parse_source(src);
    match &file.items[0].node { ItemKind::Enum(e) => { assert_eq!(e.encoding, Some(EnumEncoding::Onehot)); assert_eq!(e.variants.len(), 3); assert!(e.variants[1].value.is_some()); } other => panic!("expected Enum, got {:?}", other) }
}

#[test]
fn e2e_fn_definition() {
    let src = "fn saturate(x: UInt<16>, max: UInt<16>) -> UInt<16>:\n  if x > max:\n    max\n  else:\n    x\n";
    let file = parse_source(src);
    match &file.items[0].node { ItemKind::FnDef(f) => { assert_eq!(f.name.node, "saturate"); assert_eq!(f.params.len(), 2); assert_eq!(f.body.len(), 1); } other => panic!("expected FnDef, got {:?}", other) }
}

#[test]
fn e2e_assert_always() {
    let src = "module Safe(in clk: Clock, in rst: SyncReset, in x: UInt<8>):\n  assert always @ clk: x != 0, \"x must not be zero\"\n";
    let file = parse_source(src);
    match &file.items[0].node { ItemKind::Module(m) => { assert_eq!(m.body.len(), 1); } other => panic!("expected Module, got {:?}", other) }
}
```

- [ ] **12d. Verify all tests pass**

```bash
cargo test -p ssl-core --test parser_tests e2e_ 2>&1
# Expected: 6 end-to-end tests pass
cargo test -p ssl-core --test parser_tests 2>&1
# Expected: all parser tests pass (9 helper + 18 expr + 17 edge + 13 type + 9 stmt + 9 block + 13 item + 6 e2e = 94)
cargo test -p ssl-core 2>&1
# Expected: all tests pass
```

- [ ] **12e. Verify CLI build and parse command**

```bash
cargo build -p sslc 2>&1
# Expected: compiles successfully

# Create a test file
cat > /tmp/test_parse.ssl << 'EOF'
module Adder(in a: UInt<8>, in b: UInt<8>, out sum: UInt<9>):
  comb:
    sum = a + b
EOF

cargo run -p sslc -- parse /tmp/test_parse.ssl 2>&1
# Expected: prints AST with SourceFile containing one Module item
```

- [ ] **12f. Commit**

```bash
git add crates/ssl-core/src/parser/mod.rs
git add crates/sslc/src/main.rs
git add crates/ssl-core/tests/parser_tests.rs
git commit -m "feat(parser): add parse_file, CLI parse command, and end-to-end tests

Add parse_file method to Parser and update static Parser::parse to
delegate to it. Add 'sslc parse' command that tokenizes and parses
a .ssl file, printing the AST in Debug format. Add 6 end-to-end tests
covering module+comb, module+reg, struct, enum, fn, and assert always."
```

---

## Plan Summary

| Chunk | Tasks | Scope |
|-------|-------|-------|
| **Chunk 1** (Tasks 1-3) | AST node definitions | Span, Expr, Type, Stmt, Item AST types; `SourceFile` |
| **Chunk 2** (Tasks 4-6) | Parser infrastructure + expressions | Parser struct, helpers, Pratt expression parser (16 precedence levels) |
| **Chunk 3** (Tasks 7-9) | Type + statement parsing | Type expression parser, statement/block parser (signal, let, if, match, for, comb, reg, priority, assert) |
| **Chunk 4** (Tasks 10-12) | Item parsing + CLI + integration | Module/struct/enum/interface/fn (Task 10), FSM/pipeline/test/import/extern/inst/gen (Task 11), parse_file + CLI `parse` command + 6 end-to-end tests (Task 12) |

**Total:** 12 tasks, 85 tests, full recursive descent parser covering SiliconScript Sections 1-7 + basic formal verification.
