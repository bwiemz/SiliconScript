# Phase 4: Verilog Code Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Emit synthesizable Verilog 2005 from validated SSL programs, producing a working `sslc build --target verilog <file>` command that transforms `blinker.ssl` into correct, readable Verilog.

**Architecture:** Direct AST-to-Verilog emission (no intermediate IR). A `VerilogEmitter` walks the AST with access to the `SymbolTable` and `ScopeMap` from Phase 3's `analyze()`. Signal classification (reg vs wire) is determined by usage analysis before emission. Each module produces a self-contained Verilog `module ... endmodule` block.

**Tech Stack:** Rust (edition 2024). No new dependencies — string building uses `std::fmt::Write`. Output is Verilog 2005 compatible (no SystemVerilog features).

**Deferred to Phase 5+:**
- Generic module monomorphization (requires elaboration pass)
- `gen for`/`gen if` expansion (requires elaboration pass)
- FSM block lowering (state register + case statement generation)
- Pipeline block lowering (stage register insertion)
- CDC analysis and constraint generation
- AsyncReset sensitivity list (`always @(posedge clk or posedge rst)`) — Phase 4 only emits SyncReset pattern
- RTLIL / FIRRTL / SMT-LIB2 backends
- Optimization passes (constant folding, dead signal elimination)
- `ssl.toml` project configuration
- Multi-file compilation

**Important API notes for implementers:**
- `ssl_core::sema::analyze(file)` returns `(SymbolTable, Vec<SemaError>)`. The resolver internally builds a `ScopeMap` but the current `analyze()` does not expose it. The codegen module does NOT need `ScopeMap` — it uses the `SymbolTable` for type lookups and resolves types from AST type expressions for ports/signals. Pass `&SymbolTable` to `emit_module` for type information.
- `ssl_core::lexer::tokenize(src)` returns `Result` — always `.expect()` in tests.
- `Ty` has `bit_width() -> Option<u64>` for Verilog width declarations. Guard against `bit_width() == Some(0)` — treat as 1-bit.
- `Symbol.direction` is `Option<Direction>` — set for ports, `None` for signals.
- All operators in `BinOp` and `UnaryOp` have direct Verilog equivalents except `Implies` (rewrite as `!a || b`), `Pow` (compile-time only — emit error/comment in Verilog), and `Concat` (use `{a, b}` syntax).
- Module body items are `Vec<Item>` where `Item = Spanned<ItemKind>`. To find `CombBlock`/`RegBlock`, unwrap: `ItemKind::Stmt(stmt)` then `stmt.node` is `StmtKind::CombBlock(...)` etc.
- Ports are comma-separated with NO trailing comma on the last port (Verilog 2005 syntax error).
- Assignment target classification must extract root ident from composite targets: `Index { array, .. }` → recurse on `array`; `FieldAccess { object, .. }` → recurse on `object`.
- `BinOp::Pow` is NOT valid Verilog 2005 (`**` is SystemVerilog only). Emit `/* unsupported: pow */` comment.

---

## File Structure

```
crates/ssl-core/src/
  codegen/                       ← NEW module
    mod.rs                       Entry point: emit_verilog(), re-exports
    expr.rs                      Expression → Verilog string
    stmt.rs                      Statement → Verilog in procedural/continuous context
    module.rs                    Module emission: ports, signals, always blocks, assigns
  lib.rs                         Add `pub mod codegen`

crates/ssl-core/tests/
  codegen_tests.rs               Integration tests for Verilog emission

crates/sslc/src/
  main.rs                        Add `build` subcommand

examples/
  alu.ssl                        ALU example for testing
```

---

## Chunk 1: Expression Emission (Tasks 1–3)

### Task 1: Codegen Module + Writer Utility

**Files:**
- Create: `crates/ssl-core/src/codegen/mod.rs`
- Modify: `crates/ssl-core/src/lib.rs`
- Create: `crates/ssl-core/tests/codegen_tests.rs`

The writer utility manages indentation and line output for readable Verilog.

- [ ] **Step 1: Write failing test**

```rust
// crates/ssl-core/tests/codegen_tests.rs
use ssl_core::codegen::VerilogWriter;

#[test]
fn writer_basic_line() {
    let mut w = VerilogWriter::new();
    w.line("assign a = b;");
    assert_eq!(w.finish(), "assign a = b;\n");
}

#[test]
fn writer_indentation() {
    let mut w = VerilogWriter::new();
    w.line("module test;");
    w.indent();
    w.line("wire a;");
    w.dedent();
    w.line("endmodule");
    let out = w.finish();
    assert!(out.contains("module test;"));
    assert!(out.contains("    wire a;"));
    assert!(out.contains("endmodule"));
}

#[test]
fn writer_blank_line() {
    let mut w = VerilogWriter::new();
    w.line("wire a;");
    w.blank();
    w.line("wire b;");
    let out = w.finish();
    assert!(out.contains("wire a;\n\nwire b;"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test codegen_tests 2>&1`
Expected: FAIL — module `codegen` doesn't exist

- [ ] **Step 3: Write implementation**

```rust
// crates/ssl-core/src/codegen/mod.rs
pub mod expr;
pub mod stmt;
pub mod module;

/// Utility for building indented Verilog output.
pub struct VerilogWriter {
    buf: String,
    indent_level: usize,
}

impl VerilogWriter {
    pub fn new() -> Self {
        Self { buf: String::new(), indent_level: 0 }
    }

    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn dedent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    pub fn line(&mut self, text: &str) {
        for _ in 0..self.indent_level {
            self.buf.push_str("    ");
        }
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    /// Write a line using format args.
    pub fn line_fmt(&mut self, args: std::fmt::Arguments<'_>) {
        use std::fmt::Write;
        for _ in 0..self.indent_level {
            self.buf.push_str("    ");
        }
        let _ = self.buf.write_fmt(args);
        self.buf.push('\n');
    }

    pub fn finish(self) -> String {
        self.buf
    }
}

impl Default for VerilogWriter {
    fn default() -> Self {
        Self::new()
    }
}
```

Add to `crates/ssl-core/src/lib.rs`:
```rust
pub mod codegen;
```

Create stub files so the module compiles:
- `crates/ssl-core/src/codegen/expr.rs` — empty (or `// Expression emission`)
- `crates/ssl-core/src/codegen/stmt.rs` — empty
- `crates/ssl-core/src/codegen/module.rs` — empty

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test codegen_tests 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/codegen/ crates/ssl-core/src/lib.rs crates/ssl-core/tests/codegen_tests.rs
git commit -m "feat(codegen): add Verilog writer utility and codegen module skeleton"
```

---

### Task 2: Expression Emission

**Files:**
- Create: `crates/ssl-core/src/codegen/expr.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Map every `ExprKind` variant to a Verilog expression string.

- [ ] **Step 1: Write failing test**

```rust
use ssl_core::codegen::expr::emit_expr;
use ssl_core::ast::expr::{ExprKind, BinOp, UnaryOp};
use ssl_core::lexer::NumericLiteral;
use ssl_core::span::{Span, Spanned};

fn make_ident(name: &str) -> ssl_core::ast::expr::Expr {
    Spanned::new(ExprKind::Ident(name.to_string()), Span::new(0, 1))
}

fn make_int(val: u128) -> ssl_core::ast::expr::Expr {
    Spanned::new(ExprKind::IntLiteral(NumericLiteral::Decimal(val)), Span::new(0, 1))
}

#[test]
fn emit_expr_ident() {
    assert_eq!(emit_expr(&make_ident("counter")), "counter");
}

#[test]
fn emit_expr_decimal_literal() {
    assert_eq!(emit_expr(&make_int(42)), "42");
}

#[test]
fn emit_expr_bool_true() {
    let e = Spanned::new(ExprKind::BoolLiteral(true), Span::new(0, 4));
    assert_eq!(emit_expr(&e), "1'b1");
}

#[test]
fn emit_expr_bool_false() {
    let e = Spanned::new(ExprKind::BoolLiteral(false), Span::new(0, 5));
    assert_eq!(emit_expr(&e), "1'b0");
}

#[test]
fn emit_expr_binary_add() {
    let e = Spanned::new(ExprKind::Binary {
        op: BinOp::Add,
        lhs: Box::new(make_ident("a")),
        rhs: Box::new(make_ident("b")),
    }, Span::new(0, 5));
    assert_eq!(emit_expr(&e), "(a + b)");
}

#[test]
fn emit_expr_binary_and_logical() {
    let e = Spanned::new(ExprKind::Binary {
        op: BinOp::And,
        lhs: Box::new(make_ident("x")),
        rhs: Box::new(make_ident("y")),
    }, Span::new(0, 5));
    assert_eq!(emit_expr(&e), "(x && y)");
}

#[test]
fn emit_expr_bitwise_and() {
    let e = Spanned::new(ExprKind::Binary {
        op: BinOp::BitAnd,
        lhs: Box::new(make_ident("a")),
        rhs: Box::new(make_ident("b")),
    }, Span::new(0, 5));
    assert_eq!(emit_expr(&e), "(a & b)");
}

#[test]
fn emit_expr_concat() {
    let e = Spanned::new(ExprKind::Binary {
        op: BinOp::Concat,
        lhs: Box::new(make_ident("a")),
        rhs: Box::new(make_ident("b")),
    }, Span::new(0, 5));
    assert_eq!(emit_expr(&e), "{a, b}");
}

#[test]
fn emit_expr_unary_not() {
    let e = Spanned::new(ExprKind::Unary {
        op: UnaryOp::LogicalNot,
        operand: Box::new(make_ident("x")),
    }, Span::new(0, 3));
    assert_eq!(emit_expr(&e), "(!x)");
}

#[test]
fn emit_expr_unary_bitnot() {
    let e = Spanned::new(ExprKind::Unary {
        op: UnaryOp::BitNot,
        operand: Box::new(make_ident("x")),
    }, Span::new(0, 3));
    assert_eq!(emit_expr(&e), "(~x)");
}

#[test]
fn emit_expr_index() {
    let e = Spanned::new(ExprKind::Index {
        array: Box::new(make_ident("mem")),
        index: Box::new(make_int(3)),
    }, Span::new(0, 6));
    assert_eq!(emit_expr(&e), "mem[3]");
}

#[test]
fn emit_expr_bit_slice() {
    let e = Spanned::new(ExprKind::BitSlice {
        value: Box::new(make_ident("data")),
        high: Box::new(make_int(7)),
        low: Box::new(make_int(0)),
    }, Span::new(0, 10));
    assert_eq!(emit_expr(&e), "data[7:0]");
}

#[test]
fn emit_expr_if_ternary() {
    let e = Spanned::new(ExprKind::IfExpr {
        condition: Box::new(make_ident("sel")),
        then_expr: Box::new(make_ident("a")),
        else_expr: Box::new(make_ident("b")),
    }, Span::new(0, 10));
    assert_eq!(emit_expr(&e), "(sel ? a : b)");
}

#[test]
fn emit_expr_paren() {
    let e = Spanned::new(ExprKind::Paren(Box::new(make_ident("x"))), Span::new(0, 3));
    assert_eq!(emit_expr(&e), "(x)");
}

#[test]
fn emit_expr_implies() {
    // A implies B → (!A || B)
    let e = Spanned::new(ExprKind::Binary {
        op: BinOp::Implies,
        lhs: Box::new(make_ident("req")),
        rhs: Box::new(make_ident("ack")),
    }, Span::new(0, 10));
    assert_eq!(emit_expr(&e), "(!req || ack)");
}

#[test]
fn emit_expr_sized_literal() {
    let e = Spanned::new(ExprKind::IntLiteral(NumericLiteral::Sized {
        width: 8,
        value: 0xFF,
        base: ssl_core::lexer::NumericBase::Hex,
        dont_care_mask: 0,
    }), Span::new(0, 5));
    assert_eq!(emit_expr(&e), "8'hff");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test codegen_tests -- emit_expr 2>&1`
Expected: FAIL

- [ ] **Step 3: Write implementation**

Implement `pub fn emit_expr(expr: &Expr) -> String` in `codegen/expr.rs`.

**Operator mapping:**

| SSL BinOp | Verilog | Format |
|---|---|---|
| `Add` | `+` | `(lhs + rhs)` |
| `Sub` | `-` | `(lhs - rhs)` |
| `Mul` | `*` | `(lhs * rhs)` |
| `Div` | `/` | `(lhs / rhs)` |
| `Mod` | `%` | `(lhs % rhs)` |
| `Pow` | N/A | `/* unsupported: pow is compile-time only */` |
| `BitAnd` | `&` | `(lhs & rhs)` |
| `BitOr` | `\|` | `(lhs \| rhs)` |
| `BitXor` | `^` | `(lhs ^ rhs)` |
| `Shl` | `<<` | `(lhs << rhs)` |
| `Shr` | `>>` | `(lhs >> rhs)` |
| `ArithShr` | `>>>` | `(lhs >>> rhs)` |
| `Eq` | `==` | `(lhs == rhs)` |
| `Ne` | `!=` | `(lhs != rhs)` |
| `Lt` | `<` | `(lhs < rhs)` |
| `Gt` | `>` | `(lhs > rhs)` |
| `Le` | `<=` | `(lhs <= rhs)` |
| `Ge` | `>=` | `(lhs >= rhs)` |
| `And` | `&&` | `(lhs && rhs)` |
| `Or` | `\|\|` | `(lhs \|\| rhs)` |
| `Implies` | — | `(!lhs \|\| rhs)` |
| `Concat` | `{,}` | `{lhs, rhs}` |

| SSL UnaryOp | Verilog | Format |
|---|---|---|
| `Neg` | `-` | `(-operand)` |
| `BitNot` | `~` | `(~operand)` |
| `LogicalNot` | `!` | `(!operand)` |

**Literal emission:**
- `Decimal(n)` → `"n"` (plain number)
- `Hex(n)` → `format!("'h{n:x}")`
- `Binary(n)` → `format!("'b{n:b}")`
- `Sized { width, value, base: Hex, .. }` → `format!("{width}'h{value:x}")`
- `Sized { width, value, base: Binary, .. }` → `format!("{width}'b{value:b}")`
- `Sized { width, value, base: Decimal, .. }` → `format!("{width}'d{value}")`
- `BoolLiteral(true)` → `"1'b1"`
- `BoolLiteral(false)` → `"1'b0"`

**Other expressions:**
- `Ident(name)` → `name`
- `Paren(inner)` → `(emit_expr(inner))`
- `Index { array, index }` → `emit_expr(array)[emit_expr(index)]`
- `BitSlice { value, high, low }` → `emit_expr(value)[emit_expr(high):emit_expr(low)]`
- `IfExpr { cond, then, else }` → `(emit_expr(cond) ? emit_expr(then) : emit_expr(else))`
- `FieldAccess { object, field }` → `emit_expr(object).field` (for future struct support)
- `Call/MethodCall/Pipe/Range/StructLiteral/ArrayLiteral` → `/* unsupported */` (placeholder)
- `Next/Eventually/TypeCast/Unchecked` → `/* unsupported */`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test codegen_tests -- emit_expr 2>&1`
Expected: PASS (all 15 expression tests)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/codegen/expr.rs crates/ssl-core/tests/codegen_tests.rs
git commit -m "feat(codegen): add expression emission for Verilog"
```

---

### Task 3: Signal Classification

**Files:**
- Modify: `crates/ssl-core/src/codegen/module.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Before emitting a module, we must classify each signal as `reg` or `wire`. A signal is `reg` if it is assigned inside an `always` block (reg block in SSL). Otherwise it's `wire`.

- [ ] **Step 1: Write failing test**

```rust
use ssl_core::codegen::module::classify_signals;
use ssl_core::ast::item::SourceFile;

fn parse_module(src: &str) -> SourceFile {
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    ssl_core::parser::Parser::parse(src, tokens).expect("parse failed")
}

#[test]
fn classify_reg_signal() {
    let file = parse_module("\
module M(in clk: Clock, in rst: SyncReset):
    signal counter: UInt<8>
    reg(clk, rst):
        on reset:
            counter = 0
        on tick:
            counter = counter + 1
");
    let classified = classify_signals(&file.items[0]);
    assert!(classified.contains("counter"), "counter should be classified");
    assert_eq!(classified["counter"], ssl_core::codegen::module::SignalKind::Reg);
}

#[test]
fn classify_wire_signal() {
    let file = parse_module("\
module M(in a: UInt<8>, out b: UInt<8>):
    signal temp: UInt<8>
    comb:
        temp = a
        b = temp
");
    let classified = classify_signals(&file.items[0]);
    // temp is only assigned in comb (wire context)
    assert_eq!(classified.get("temp"), Some(&ssl_core::codegen::module::SignalKind::Wire));
}

#[test]
fn classify_output_port_comb() {
    let file = parse_module("\
module M(in a: UInt<8>, out b: UInt<8>):
    comb:
        b = a
");
    let classified = classify_signals(&file.items[0]);
    // output port 'b' assigned only in comb → wire
    assert_eq!(classified.get("b"), Some(&ssl_core::codegen::module::SignalKind::Wire));
}

#[test]
fn classify_output_port_reg() {
    let file = parse_module("\
module M(in clk: Clock, in rst: SyncReset, out q: UInt<8>):
    reg(clk, rst):
        on reset:
            q = 0
        on tick:
            q = q + 1
");
    let classified = classify_signals(&file.items[0]);
    assert_eq!(classified.get("q"), Some(&ssl_core::codegen::module::SignalKind::Reg));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ssl-core --test codegen_tests -- classify_ 2>&1`
Expected: FAIL

- [ ] **Step 3: Write implementation**

```rust
// crates/ssl-core/src/codegen/module.rs
use std::collections::HashMap;
use crate::ast::item::{Item, ItemKind};
use crate::ast::stmt::{Stmt, StmtKind};
use crate::ast::expr::ExprKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    Reg,
    Wire,
}

/// Classify all signals/output-ports in a module as Reg or Wire.
///
/// A signal is Reg if it is assigned inside a `reg()` block's
/// `on_reset` or `on_tick`. Otherwise it is Wire.
pub fn classify_signals(item: &Item) -> HashMap<String, SignalKind> {
    let mut result = HashMap::new();

    if let ItemKind::Module(ref module_def) = item.node {
        // First pass: collect all signals assigned in reg blocks → Reg
        let mut reg_signals = std::collections::HashSet::new();
        collect_reg_assignments(&module_def.body, &mut reg_signals);

        // Second pass: collect ALL assigned signals
        let mut all_assigned = std::collections::HashSet::new();
        collect_all_assignments(&module_def.body, &mut all_assigned);

        // Classify: if in reg_signals → Reg, else → Wire
        for name in &all_assigned {
            if reg_signals.contains(name) {
                result.insert(name.clone(), SignalKind::Reg);
            } else {
                result.insert(name.clone(), SignalKind::Wire);
            }
        }
    }

    result
}
```

Implement `collect_reg_assignments` that walks body items. Module body is `Vec<Item>` — each item must be unwrapped: `for item in body { if let ItemKind::Stmt(stmt) = &item.node { if let StmtKind::RegBlock(reg) = &stmt.node { /* collect from on_reset/on_tick */ } } }`. Collect assignment target idents — for composite targets like `arr[i]` or `obj.field`, extract the root ident by recursing through `Index { array, .. }` and `FieldAccess { object, .. }`.

Implement `collect_all_assignments` that walks all items/statements recursively and collects all assignment target root idents (same root-extraction logic).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ssl-core --test codegen_tests -- classify_ 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/codegen/module.rs crates/ssl-core/tests/codegen_tests.rs
git commit -m "feat(codegen): add signal classification (reg vs wire)"
```

---

## Chunk 2: Statement and Block Emission (Tasks 4–6)

### Task 4: Continuous Assignment Emission (Comb Blocks)

**Files:**
- Modify: `crates/ssl-core/src/codegen/stmt.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Comb blocks with simple assignments become `assign` statements. Comb blocks with control flow (if/match) become `always @(*)` blocks.

- [ ] **Step 1: Write failing test**

```rust
use ssl_core::codegen::stmt::{emit_comb_block, CombStyle};

#[test]
fn emit_comb_simple_assign() {
    let file = parse_module("\
module M(in a: UInt<8>, out b: UInt<8>):
    comb:
        b = a
");
    if let ItemKind::Module(ref m) = file.items[0].node {
        if let ItemKind::Stmt(ref stmt) = m.body[0].node {
            if let StmtKind::CombBlock(ref stmts) = stmt.node {
                let mut w = VerilogWriter::new();
                emit_comb_block(&mut w, stmts);
                let out = w.finish();
                assert!(out.contains("assign b = a;"), "got: {out}");
            }
        }
    }
}

#[test]
fn emit_comb_with_if() {
    let file = parse_module("\
module M(in sel: Bool, in a: UInt<8>, in b: UInt<8>, out y: UInt<8>):
    comb:
        if sel:
            y = a
        else:
            y = b
");
    if let ItemKind::Module(ref m) = file.items[0].node {
        if let ItemKind::Stmt(ref stmt) = m.body[0].node {
            if let StmtKind::CombBlock(ref stmts) = stmt.node {
                let mut w = VerilogWriter::new();
                emit_comb_block(&mut w, stmts);
                let out = w.finish();
                assert!(out.contains("always @(*)"), "got: {out}");
                assert!(out.contains("if (sel)"), "got: {out}");
            }
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

- [ ] **Step 3: Write implementation**

Implement `emit_comb_block(w: &mut VerilogWriter, stmts: &[Stmt])`:

1. Analyze statements: if ALL are simple `Assign { target: Ident, value }`, emit as `assign` statements.
2. If ANY statement is `If`, `Match`, `PriorityBlock`, `ParallelBlock`, emit the whole block as `always @(*) begin ... end`.

For simple assign: `w.line_fmt(format_args!("assign {} = {};", emit_expr(target), emit_expr(value)));`

For procedural context, call `emit_procedural_stmt(w, stmt)` for each statement.

Also implement `emit_procedural_stmt(w: &mut VerilogWriter, stmt: &Stmt)`:
- `Assign { target, value }` → `target = value;` (blocking assignment in always @(*))
- `If { condition, then_body, elif_branches, else_body }` →
  ```verilog
  if (condition) begin
      ...
  end else if (...) begin
      ...
  end else begin
      ...
  end
  ```
- `Match { scrutinee, arms }` →
  ```verilog
  case (scrutinee)
      pattern: begin ... end
      default: begin ... end
  endcase
  ```
- Other → skip or emit as comment

- [ ] **Step 4: Run test to verify it passes**

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/codegen/stmt.rs crates/ssl-core/tests/codegen_tests.rs
git commit -m "feat(codegen): add comb block emission (assign + always @(*))"
```

---

### Task 5: Sequential Block Emission (Reg Blocks)

**Files:**
- Modify: `crates/ssl-core/src/codegen/stmt.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Reg blocks become `always @(posedge clk)` with synchronous reset pattern.

- [ ] **Step 1: Write failing test**

```rust
use ssl_core::codegen::stmt::emit_reg_block;
use ssl_core::ast::stmt::RegBlock;

#[test]
fn emit_reg_basic() {
    let file = parse_module("\
module M(in clk: Clock, in rst: SyncReset, out q: UInt<8>):
    reg(clk, rst):
        on reset:
            q = 0
        on tick:
            q = q + 1
");
    if let ItemKind::Module(ref m) = file.items[0].node {
        if let ItemKind::Stmt(ref stmt) = m.body[0].node {
            if let StmtKind::RegBlock(ref reg) = stmt.node {
                let mut w = VerilogWriter::new();
                emit_reg_block(&mut w, reg);
                let out = w.finish();
                assert!(out.contains("always @(posedge"), "got: {out}");
                assert!(out.contains("if (rst)"), "got: {out}");
                assert!(out.contains("<= "), "should use non-blocking: {out}");
            }
        }
    }
}

#[test]
fn emit_reg_with_enable() {
    let file = parse_module("\
module M(in clk: Clock, in rst: SyncReset, in en: Bool, out q: UInt<8>):
    reg(clk, rst, enable = en):
        on reset:
            q = 0
        on tick:
            q = q + 1
");
    if let ItemKind::Module(ref m) = file.items[0].node {
        if let ItemKind::Stmt(ref stmt) = m.body[0].node {
            if let StmtKind::RegBlock(ref reg) = stmt.node {
                let mut w = VerilogWriter::new();
                emit_reg_block(&mut w, reg);
                let out = w.finish();
                assert!(out.contains("if (en)"), "should check enable: {out}");
            }
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

- [ ] **Step 3: Write implementation**

Implement `emit_reg_block(w: &mut VerilogWriter, reg: &RegBlock)`:

```verilog
always @(posedge clk) begin
    if (rst) begin
        // on_reset assignments (non-blocking <=)
    end else begin
        // optional: if (enable) begin ... end
        // on_tick assignments (non-blocking <=)
    end
end
```

Key: `reg` block assignments use `<=` (non-blocking) while `comb` block assignments use `=` (blocking). Implement a helper `emit_sequential_stmt(w, stmt)` that uses `<=` for assignments.

The clock expression is emitted as `posedge emit_expr(clock)`. The reset expression is emitted as `emit_expr(reset)`.

- [ ] **Step 4: Run test to verify it passes**

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/codegen/stmt.rs crates/ssl-core/tests/codegen_tests.rs
git commit -m "feat(codegen): add reg block emission (always @(posedge) with reset)"
```

---

### Task 6: Priority and Match Statement Emission

**Files:**
- Modify: `crates/ssl-core/src/codegen/stmt.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Priority blocks emit as if/else-if chains. Match blocks emit as case statements.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn emit_priority_block() {
    let file = parse_module("\
module M(in sel: UInt<2>, out y: UInt<8>):
    comb:
        priority:
            when sel == 0 => y = 10
            when sel == 1 => y = 20
            otherwise => y = 0
");
    // Extract and emit — check for if/else if/else pattern
    let mut w = VerilogWriter::new();
    // ... extract comb block, emit
    // Verify output contains if/else if/else
}

#[test]
fn emit_match_in_comb() {
    let file = parse_module("\
module M(in opcode: UInt<2>, out y: UInt<8>):
    comb:
        y = 0
        match opcode:
            0 => y = 10
            1 => y = 20
            _ => y = 0
");
    // Check for case statement emission
}
```

- [ ] **Step 2-5: Implement, test, commit**

Implement `emit_procedural_priority(w, priority_block)`:
```verilog
if (condition_a) begin
    ...
end else if (condition_b) begin
    ...
end else begin
    ...  // otherwise
end
```

Implement `emit_procedural_match(w, match_stmt)`:
```verilog
case (scrutinee)
    value1: begin ... end
    value2: begin ... end
    default: begin ... end
endcase
```

Note: Match arm pattern `_` (wildcard) maps to Verilog `default`.

```bash
git commit -m "feat(codegen): add priority and match statement emission"
```

---

## Chunk 3: Module Assembly (Tasks 7–9)

### Task 7: Type-to-Verilog Declaration

**Files:**
- Modify: `crates/ssl-core/src/codegen/module.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Map `Ty` to Verilog wire/reg declarations.

- [ ] **Step 1: Write failing test**

```rust
use ssl_core::codegen::module::{ty_to_verilog_width, ty_to_port_type};
use ssl_core::sema::types::Ty;

#[test]
fn ty_width_uint8() {
    assert_eq!(ty_to_verilog_width(&Ty::UInt(8)), "[7:0]");
}

#[test]
fn ty_width_uint1() {
    assert_eq!(ty_to_verilog_width(&Ty::UInt(1)), "");
}

#[test]
fn ty_width_bool() {
    assert_eq!(ty_to_verilog_width(&Ty::Bool), "");
}

#[test]
fn ty_width_bits32() {
    assert_eq!(ty_to_verilog_width(&Ty::Bits(32)), "[31:0]");
}

#[test]
fn ty_width_clock() {
    assert_eq!(ty_to_verilog_width(&Ty::Clock { freq: None }), "");
}
```

- [ ] **Step 2-5: Implement, test, commit**

```rust
/// Return Verilog width specifier for a type.
/// Returns "" for 1-bit types, "[N-1:0]" for multi-bit.
pub fn ty_to_verilog_width(ty: &Ty) -> String {
    match ty.bit_width() {
        Some(0) | Some(1) | None => String::new(),
        Some(w) => format!("[{}:0]", w - 1),
    }
}
```

```bash
git commit -m "feat(codegen): add type-to-Verilog width conversion"
```

---

### Task 8: Module Emission — Port List and Signal Declarations

**Files:**
- Modify: `crates/ssl-core/src/codegen/module.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Emit a complete Verilog module with ports and signal declarations.

- [ ] **Step 1: Write failing test**

```rust
use ssl_core::codegen::module::emit_module;

#[test]
fn emit_module_blinker() {
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
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse failed");
    let (table, errors) = ssl_core::sema::analyze(&file);
    assert!(errors.is_empty(), "should pass analysis: {errors:?}");

    let verilog = emit_module(&file.items[0], src, &table);
    assert!(verilog.contains("module Blinker("), "got: {verilog}");
    assert!(verilog.contains("input wire clk"), "got: {verilog}");
    assert!(verilog.contains("input wire rst"), "got: {verilog}");
    assert!(verilog.contains("output wire led") || verilog.contains("output reg led"),
        "got: {verilog}");
    assert!(verilog.contains("reg [23:0] counter"), "got: {verilog}");
    assert!(verilog.contains("always @(posedge"), "got: {verilog}");
    assert!(verilog.contains("assign led"), "got: {verilog}");
    assert!(verilog.contains("endmodule"), "got: {verilog}");
}

#[test]
fn emit_module_simple_comb() {
    let src = "\
module Adder(
    in a: UInt<8>,
    in b: UInt<8>,
    out sum: UInt<8>
):
    comb:
        sum = a + b
";
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse failed");
    let (table, _errors) = ssl_core::sema::analyze(&file);

    let verilog = emit_module(&file.items[0], src, &table);
    assert!(verilog.contains("module Adder("), "got: {verilog}");
    assert!(verilog.contains("input wire [7:0] a"), "got: {verilog}");
    assert!(verilog.contains("output wire [7:0] sum"), "got: {verilog}");
    assert!(verilog.contains("assign sum = (a + b)"), "got: {verilog}");
    assert!(verilog.contains("endmodule"), "got: {verilog}");
}
```

- [ ] **Step 2: Run test to verify it fails**

- [ ] **Step 3: Write implementation**

Implement `pub fn emit_module(item: &Item, source: &str, table: &SymbolTable) -> String`:

1. Extract `ModuleDef` from `ItemKind::Module`
2. Classify signals (reg vs wire)
3. Resolve types for ports and signals using source text + parser (re-parse type annotations to get Ty) OR walk the AST type expressions and use a simple resolver

**Actually, simpler approach:** Since we don't have the SymbolTable available in the module emitter (it's constructed by `analyze()`), we need to either:
- Pass the SymbolTable to `emit_module`
- OR resolve types locally from AST type expressions

**Best approach:** Add a top-level `emit_verilog(file: &SourceFile, source: &str, table: &SymbolTable) -> String` function in `codegen/mod.rs` that passes context through. The module emitter gets `(&ModuleDef, source, &SymbolTable, &ScopeMap)`.

But to keep Task 8 simpler, use a local type resolver that handles common cases (UInt<N>, SInt<N>, Bits<N>, Bool, Clock, SyncReset, AsyncReset) from AST type expressions. This avoids coupling to the full sema pipeline for codegen tests.

Implement a helper `resolve_type_simple(ty: &TypeExpr, source: &str) -> Ty` that handles the basic cases by reading the source text for the type name and evaluating simple numeric generic args.

**Module emission structure:**

```verilog
module NAME(
    input wire [W-1:0] port_a,
    input wire [W-1:0] port_b,
    output wire [W-1:0] port_c,
    output reg [W-1:0] port_d
);

    // Internal signals
    reg [W-1:0] sig_reg;
    wire [W-1:0] sig_wire;

    // Combinational logic
    assign port_c = expr;

    // Sequential logic
    always @(posedge clk) begin
        if (rst) begin
            sig_reg <= 0;
        end else begin
            sig_reg <= expr;
        end
    end

endmodule
```

Port direction mapping:
- `in` → `input wire`
- `out` + wire-classified → `output wire`
- `out` + reg-classified → `output reg`
- `inout` → `inout wire`

Walk body items:
- `ItemKind::Stmt(CombBlock(stmts))` → call `emit_comb_block`
- `ItemKind::Stmt(RegBlock(reg))` → call `emit_reg_block`
- `ItemKind::Stmt(Signal(decl))` → emit signal declaration (`wire`/`reg [W-1:0] name;`)
- `ItemKind::Stmt(Const(decl))` → emit `localparam name = value;`
- Other → skip or comment

- [ ] **Step 4: Run test to verify it passes**

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/codegen/ crates/ssl-core/tests/codegen_tests.rs
git commit -m "feat(codegen): add full module emission with ports, signals, and body"
```

---

### Task 9: Module Instantiation Emission

**Files:**
- Modify: `crates/ssl-core/src/codegen/module.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Emit `inst` declarations as Verilog module instantiations.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn emit_inst_basic() {
    let src = "\
module Inner(in x: UInt<8>, out y: UInt<8>):
    comb:
        y = x

module Outer(in a: UInt<8>, out b: UInt<8>):
    inst i = Inner(x = a, y -> b)
";
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse failed");
    let (table, _errors) = ssl_core::sema::analyze(&file);

    // Emit only the Outer module
    let verilog = emit_module(&file.items[1], src, &table);
    assert!(verilog.contains("Inner i("), "got: {verilog}");
    assert!(verilog.contains(".x(a)"), "got: {verilog}");
    assert!(verilog.contains(".y(b)"), "got: {verilog}");
}
```

- [ ] **Step 2-5: Implement, test, commit**

Walk `ItemKind::Inst(inst_decl)` in module body:

```verilog
ModuleName instance_name(
    .port_a(signal_a),
    .port_b(signal_b)
);
```

Port connections: `PortBinding::Input(expr)` and `PortBinding::Output(expr)` both emit as `.port(expr)`. `PortBinding::Discard` emits as `.port()` (unconnected).

```bash
git commit -m "feat(codegen): add module instantiation emission"
```

---

## Chunk 4: Top-Level API + CLI (Tasks 10–12)

### Task 10: Top-Level emit_verilog API

**Files:**
- Modify: `crates/ssl-core/src/codegen/mod.rs`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

Wire everything together: `emit_verilog(file, source) -> String` processes all top-level modules.

- [ ] **Step 1: Write failing test**

```rust
use ssl_core::codegen::emit_verilog;

#[test]
fn emit_verilog_full_file() {
    let src = std::fs::read_to_string(
        format!("{}/../../examples/blinker.ssl", env!("CARGO_MANIFEST_DIR"))
    ).expect("read blinker.ssl");
    let tokens = ssl_core::lexer::tokenize(&src).expect("tokenize failed");
    let file = ssl_core::parser::Parser::parse(&src, tokens).expect("parse failed");
    let (table, errors) = ssl_core::sema::analyze(&file);
    assert!(errors.is_empty(), "should pass analysis: {errors:?}");

    let verilog = emit_verilog(&file, &src, &table);
    assert!(verilog.contains("module Blinker"), "got: {verilog}");
    assert!(verilog.contains("endmodule"), "got: {verilog}");
    // Should be valid-looking Verilog
    assert!(!verilog.contains("unsupported"), "should not have unsupported markers: {verilog}");
}
```

- [ ] **Step 2-5: Implement, test, commit**

```rust
// crates/ssl-core/src/codegen/mod.rs
pub fn emit_verilog(file: &SourceFile, source: &str, table: &crate::sema::scope::SymbolTable) -> String {
    let mut output = String::new();
    for item in &file.items {
        if matches!(item.node, ItemKind::Module(_)) {
            output.push_str(&module::emit_module(item, source, table));
            output.push('\n');
        }
    }
    output
}
```

```bash
git commit -m "feat(codegen): add top-level emit_verilog API"
```

---

### Task 11: CLI `build` Command

**Files:**
- Modify: `crates/sslc/src/main.rs`

- [ ] **Step 1: Add `build` command**

```rust
"build" => {
    // Parse --target flag (default: verilog)
    let mut target = "verilog";
    let mut file_path = None;
    let mut output_path = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => { i += 1; target = &args[i]; }
            "--output" | "-o" => { i += 1; output_path = Some(args[i].clone()); }
            _ => { file_path = Some(args[i].clone()); }
        }
        i += 1;
    }

    let path = file_path.unwrap_or_else(|| {
        eprintln!("Usage: sslc build [--target verilog] <file>");
        std::process::exit(1);
    });

    let source = read_source(&path);
    let tokens = match ssl_core::lexer::tokenize(&source) { ... };
    let file = match ssl_core::parser::Parser::parse(&source, tokens) { ... };
    let (table, errors) = ssl_core::sema::analyze(&file);

    if !errors.is_empty() {
        for err in &errors { eprintln!("Error: {err}"); }
        eprintln!("\n{} error(s) found", errors.len());
        std::process::exit(1);
    }

    match target {
        "verilog" => {
            let verilog = ssl_core::codegen::emit_verilog(&file, &source, &table);
            if let Some(out) = output_path {
                std::fs::write(&out, &verilog).expect("failed to write output");
                println!("Wrote {}", out);
            } else {
                print!("{verilog}");
            }
        }
        _ => {
            eprintln!("Unsupported target: {target}");
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 2: Test manually**

Run: `cargo run -p sslc -- build examples/blinker.ssl 2>&1`
Expected: Verilog output to stdout

Run: `cargo run -p sslc -- build --target verilog examples/blinker.ssl -o build/blinker.v 2>&1`
Expected: "Wrote build/blinker.v"

- [ ] **Step 3: Commit**

```bash
git add crates/sslc/src/main.rs
git commit -m "feat(cli): add sslc build command for Verilog code generation"
```

---

### Task 12: End-to-End Integration Tests

**Files:**
- Create: `examples/alu.ssl`
- Modify: `crates/ssl-core/tests/codegen_tests.rs`

- [ ] **Step 1: Create examples/alu.ssl**

```
module ALU(
    in  a:      UInt<8>,
    in  b:      UInt<8>,
    in  opcode: UInt<2>,
    out result: UInt<8>,
    out zero:   Bool
):
    comb:
        match opcode:
            0 => result = a + b
            1 => result = a - b
            2 => result = a & b
            _ => result = a | b
        zero = result == 0
```

- [ ] **Step 2: Write integration tests**

```rust
#[test]
fn e2e_blinker_verilog() {
    let src = std::fs::read_to_string(
        format!("{}/../../examples/blinker.ssl", env!("CARGO_MANIFEST_DIR"))
    ).expect("read blinker.ssl");
    let tokens = ssl_core::lexer::tokenize(&src).expect("tokenize");
    let file = ssl_core::parser::Parser::parse(&src, tokens).expect("parse");
    let (table, _errors) = ssl_core::sema::analyze(&file);

    let verilog = ssl_core::codegen::emit_verilog(&file, &src, &table);

    // Structural checks
    assert!(verilog.contains("module Blinker("));
    assert!(verilog.contains("input wire clk"));
    assert!(verilog.contains("input wire rst"));
    assert!(verilog.contains("output"));
    assert!(verilog.contains("reg [23:0] counter"));
    assert!(verilog.contains("always @(posedge clk)"));
    assert!(verilog.contains("counter <= "));
    assert!(verilog.contains("endmodule"));
    // No unsupported markers
    assert!(!verilog.contains("unsupported"));
    assert!(!verilog.contains("TODO"));
}

#[test]
fn e2e_alu_verilog() {
    let src = std::fs::read_to_string(
        format!("{}/../../examples/alu.ssl", env!("CARGO_MANIFEST_DIR"))
    ).expect("read alu.ssl");
    let tokens = ssl_core::lexer::tokenize(&src).expect("tokenize");
    let file = ssl_core::parser::Parser::parse(&src, tokens).expect("parse");
    let (table, _errors) = ssl_core::sema::analyze(&file);

    let verilog = ssl_core::codegen::emit_verilog(&file, &src, &table);

    assert!(verilog.contains("module ALU("));
    assert!(verilog.contains("input wire [7:0] a"));
    assert!(verilog.contains("output"));
    assert!(verilog.contains("case"), "ALU match should use case: {verilog}");
    assert!(verilog.contains("endmodule"));
}

#[test]
fn e2e_adder_verilog() {
    let src = "\
module Adder(
    in a: UInt<16>,
    in b: UInt<16>,
    out sum: UInt<16>
):
    comb:
        sum = a + b
";
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse");
    let (table, _) = ssl_core::sema::analyze(&file);
    let verilog = ssl_core::codegen::emit_verilog(&file, src, &table);

    assert!(verilog.contains("assign sum = (a + b)"), "got: {verilog}");
}

#[test]
fn e2e_multi_module() {
    let src = "\
module A(in x: UInt<8>, out y: UInt<8>):
    comb:
        y = x

module B(in a: UInt<8>, out b: UInt<8>):
    comb:
        b = a
";
    let tokens = ssl_core::lexer::tokenize(src).expect("tokenize");
    let file = ssl_core::parser::Parser::parse(src, tokens).expect("parse");
    let (table, _) = ssl_core::sema::analyze(&file);
    let verilog = ssl_core::codegen::emit_verilog(&file, src, &table);

    // Both modules should appear
    assert!(verilog.contains("module A("));
    assert!(verilog.contains("module B("));
    assert_eq!(verilog.matches("endmodule").count(), 2);
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p ssl-core 2>&1`
Expected: ALL tests pass

Run: `cargo clippy --workspace 2>&1`
Expected: Clean

- [ ] **Step 4: Commit**

```bash
git add examples/alu.ssl crates/ssl-core/tests/codegen_tests.rs
git commit -m "test(codegen): add end-to-end Verilog generation tests"
```

---

## Post-Implementation

After all 12 tasks are complete:

1. Run `cargo test` — all tests must pass
2. Run `cargo clippy` — no warnings
3. Run `cargo run -p sslc -- build examples/blinker.ssl` — must produce valid Verilog
4. Manually inspect generated Verilog for readability
5. Dispatch final code reviewer via `superpowers:requesting-code-review`
6. Use `superpowers:finishing-a-development-branch` for merge/commit strategy

## Summary

| Task | Description | Tests | Cumulative |
|------|-------------|-------|------------|
| 1 | Writer utility + codegen skeleton | 3 | 3 |
| 2 | Expression emission | 15 | 18 |
| 3 | Signal classification (reg vs wire) | 4 | 22 |
| 4 | Comb block emission (assign + always @(*)) | 2 | 24 |
| 5 | Reg block emission (always @(posedge)) | 2 | 26 |
| 6 | Priority and match statement emission | 2 | 28 |
| 7 | Type-to-Verilog width conversion | 5 | 33 |
| 8 | Full module emission (ports, signals, body) | 2 | 35 |
| 9 | Module instantiation emission | 1 | 36 |
| 10 | Top-level emit_verilog API | 1 | 37 |
| 11 | CLI `build` command | 2 (manual) | 39 |
| 12 | End-to-end integration tests | 4 | 43 |

**Total: 12 tasks, ~43 tests, 5 new files, 1 new example**
