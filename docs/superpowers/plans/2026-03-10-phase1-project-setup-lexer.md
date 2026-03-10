# Phase 1: Project Setup + Lexer — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set up the Rust workspace and implement a complete lexer that tokenizes SiliconScript source into a typed token stream.

**Architecture:** Cargo workspace with a `sslc` binary crate and `ssl-core` library crate. The lexer uses the `logos` crate for fast, declarative tokenization. All tokens are tagged with source spans for error reporting. The lexer handles SSL's full lexical grammar: keywords, operators, numeric literals (decimal, hex, binary, sized), identifiers, comments, string literals, and indentation tracking.

**Tech Stack:** Rust 1.85+, logos (lexer generator), miette (error diagnostics), insta (snapshot testing)

**Spec reference:** `docs/superpowers/specs/2026-03-10-siliconscript-language-design.md` — Section 1 (Syntax & Language Fundamentals)

---

## Chunk 1: Project Scaffolding

### Task 1: Initialize Cargo Workspace

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/sslc/Cargo.toml` (binary crate)
- Create: `crates/sslc/src/main.rs`
- Create: `crates/ssl-core/Cargo.toml` (library crate)
- Create: `crates/ssl-core/src/lib.rs`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
rust-version = "1.85"

[workspace.dependencies]
logos = "0.15"
miette = { version = "7", features = ["fancy"] }
thiserror = "2"
insta = "1.41"
```

- [ ] **Step 2: Create ssl-core library crate**

`crates/ssl-core/Cargo.toml`:
```toml
[package]
name = "ssl-core"
version.workspace = true
edition.workspace = true

[dependencies]
logos = { workspace = true }
miette = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
insta = { workspace = true }
```

`crates/ssl-core/src/lib.rs`:
```rust
pub mod lexer;
```

- [ ] **Step 3: Create sslc binary crate**

`crates/sslc/Cargo.toml`:
```toml
[package]
name = "sslc"
version.workspace = true
edition.workspace = true

[dependencies]
ssl-core = { path = "../ssl-core" }
miette = { workspace = true, features = ["fancy"] }
```

`crates/sslc/src/main.rs`:
```rust
fn main() {
    println!("sslc - SiliconScript Compiler v0.1.0");
}
```

- [ ] **Step 4: Verify workspace builds**

Run: `cargo build`
Expected: Compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/
git commit -m "feat: initialize Cargo workspace with sslc and ssl-core crates"
```

---

### Task 2: Define Source Span Type

**Files:**
- Create: `crates/ssl-core/src/span.rs`
- Modify: `crates/ssl-core/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/ssl-core/src/span.rs`:
```rust
/// A byte-offset span in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Byte offset of the first character (inclusive).
    pub start: u32,
    /// Byte offset past the last character (exclusive).
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merge two spans into a span covering both.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl From<std::ops::Range<usize>> for Span {
    fn from(range: std::ops::Range<usize>) -> Self {
        Self::new(range.start as u32, range.end as u32)
    }
}

/// A value tagged with its source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_merge() {
        let a = Span::new(5, 10);
        let b = Span::new(8, 15);
        let merged = a.merge(b);
        assert_eq!(merged, Span::new(5, 15));
    }

    #[test]
    fn span_from_range() {
        let span: Span = (3..7).into();
        assert_eq!(span.start, 3);
        assert_eq!(span.end, 7);
        assert_eq!(span.len(), 4);
    }
}
```

- [ ] **Step 2: Register module and run tests**

Add to `crates/ssl-core/src/lib.rs`:
```rust
pub mod span;
pub mod lexer;
```

Run: `cargo test -p ssl-core span`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/ssl-core/src/span.rs crates/ssl-core/src/lib.rs
git commit -m "feat: add Span and Spanned types for source location tracking"
```

---

## Chunk 2: Token Definition

### Task 3: Define Token Enum

**Files:**
- Create: `crates/ssl-core/src/lexer/token.rs`
- Create: `crates/ssl-core/src/lexer/mod.rs`

This is the core token enum. Every keyword, operator, and literal from the spec's Section 1.4 and 1.5 must be represented.

- [ ] **Step 1: Create the token module**

Create `crates/ssl-core/src/lexer/mod.rs`:
```rust
mod token;

pub use token::Token;
```

- [ ] **Step 2: Define the Token enum**

Create `crates/ssl-core/src/lexer/token.rs`:
```rust
use logos::Logos;

/// Numeric literal value parsed from source.
#[derive(Debug, Clone, PartialEq)]
pub enum NumericLiteral {
    /// Unsized decimal: `42`, `1_000_000`
    Decimal(u128),
    /// Unsized hex: `0xFF`
    Hex(u128),
    /// Unsized binary: `0b1010`
    Binary(u128),
    /// Sized literal: `8'b1010_0011`, `16'hDEAD`
    /// (width, value, original_base, dont_care_mask)
    Sized {
        width: u32,
        value: u128,
        base: NumericBase,
        /// Bitmask where 1 = don't-care bit (from `?` in binary literals).
        /// Used for pattern matching: `4'b10??` → value=0b1000, dont_care_mask=0b0011.
        dont_care_mask: u128,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericBase {
    Binary,
    Decimal,
    Hex,
}

/// All tokens in the SiliconScript language.
///
/// Token variants are grouped by category matching the spec Section 1.4.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t]+")]  // skip horizontal whitespace (NOT newlines)
pub enum Token {
    // ── Newlines & Indentation ──────────────────────────────────
    // Handled by a post-processing pass, not logos directly.
    // The lexer emits raw Newline tokens; an indentation tracker
    // converts them to Indent/Dedent/Newline.

    /// Raw newline (before indentation processing)
    Newline,

    /// Increase in indentation level (synthetic, from indentation pass)
    Indent,

    /// Decrease in indentation level (synthetic, from indentation pass)
    Dedent,

    // ── Hardware Construct Keywords ─────────────────────────────
    #[token("module")]
    KwModule,
    #[token("signal")]
    KwSignal,
    #[token("reg")]
    KwReg,
    #[token("comb")]
    KwComb,
    #[token("in")]
    KwIn,
    #[token("out")]
    KwOut,
    #[token("inout")]
    KwInout,
    #[token("inst")]
    KwInst,
    #[token("extern")]
    KwExtern,
    #[token("domain")]
    KwDomain,

    // ── Type Construct Keywords ─────────────────────────────────
    #[token("struct")]
    KwStruct,
    #[token("enum")]
    KwEnum,
    #[token("interface")]
    KwInterface,
    #[token("type")]
    KwType,
    #[token("const")]
    KwConst,
    #[token("let")]
    KwLet,
    #[token("fn")]
    KwFn,
    #[token("group")]
    KwGroup,

    // ── Sequential Construct Keywords ───────────────────────────
    #[token("fsm")]
    KwFsm,
    #[token("pipeline")]
    KwPipeline,
    #[token("stage")]
    KwStage,
    #[token("on")]
    KwOn,
    #[token("reset")]
    KwReset,
    #[token("tick")]
    KwTick,

    // ── Control Flow Keywords ───────────────────────────────────
    #[token("match")]
    KwMatch,
    #[token("if")]
    KwIf,
    #[token("elif")]
    KwElif,
    #[token("else")]
    KwElse,
    #[token("then")]
    KwThen,
    #[token("for")]
    KwFor,
    #[token("gen")]
    KwGen,
    #[token("when")]
    KwWhen,
    #[token("priority")]
    KwPriority,
    #[token("parallel")]
    KwParallel,
    #[token("otherwise")]
    KwOtherwise,

    // ── Formal Verification Keywords ────────────────────────────
    #[token("assert")]
    KwAssert,
    #[token("assume")]
    KwAssume,
    #[token("cover")]
    KwCover,
    #[token("property")]
    KwProperty,
    #[token("sequence")]
    KwSequence,
    #[token("always")]
    KwAlways,
    #[token("eventually")]
    KwEventually,
    #[token("until")]
    KwUntil,
    #[token("implies")]
    KwImplies,
    #[token("verify")]
    KwVerify,
    #[token("forall")]
    KwForall,
    #[token("next")]
    KwNext,

    // ── Literal & Logic Keywords ────────────────────────────────
    #[token("true")]
    KwTrue,
    #[token("false")]
    KwFalse,
    #[token("and")]
    KwAnd,
    #[token("or")]
    KwOr,
    #[token("not")]
    KwNot,

    // ── Module System Keywords ──────────────────────────────────
    #[token("import")]
    KwImport,
    #[token("from")]
    KwFrom,
    #[token("as")]
    KwAs,
    #[token("pub")]
    KwPub,

    // ── Safety Keywords ─────────────────────────────────────────
    #[token("unchecked")]
    KwUnchecked,
    #[token("static_assert")]
    KwStaticAssert,

    // ── Test Keyword ────────────────────────────────────────────
    #[token("test")]
    KwTest,

    // ── Operators ───────────────────────────────────────────────
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("&")]
    Ampersand,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("<<")]
    ShiftLeft,
    #[token(">>")]
    ShiftRight,
    #[token(">>>")]
    ArithShiftRight,
    #[token("++")]
    Concat,
    #[token("|>")]
    PipeOp,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<=")]
    LessEq,
    #[token(">=")]
    GreaterEq,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,
    #[token("=")]
    Eq,
    #[token("=>")]
    FatArrow,
    #[token("->")]
    ThinArrow,
    #[token("-->")]
    LongArrow,
    #[token("--")]
    DashDash,
    #[token("@")]
    At,
    #[token("?")]
    Question,
    #[token("**")]
    StarStar,

    // ── Range Operators ─────────────────────────────────────────
    #[token("..=")]
    RangeInclusive,
    #[token("..")]
    RangeExclusive,

    // ── Delimiters ──────────────────────────────────────────────
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("_")]
    Underscore,
    #[token("\\")]
    Backslash,

    // ── Literals ────────────────────────────────────────────────
    /// Numeric literal (parsed in a callback)
    Numeric(NumericLiteral),

    /// String literal: `"hello"`
    #[regex(r#""[^"]*""#, |lex| lex.slice()[1..lex.slice().len()-1].to_string())]
    StringLit(String),

    // ── Identifiers ─────────────────────────────────────────────
    /// Identifier: starts with letter or underscore, contains alphanumeric or underscore
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", priority = 1)]
    Ident,

    // ── Doc Comments (must come before LineComment for priority) ──
    /// Doc comment: `/// ...`
    #[regex(r"///[^\n]*", priority = 10)]
    DocComment,

    // ── Comments ────────────────────────────────────────────────
    /// Line comment: `// ...` (but NOT `///` which is a doc comment)
    #[regex(r"//[^\n]*", priority = 5)]
    LineComment,

    /// Block comment start `/*` — triggers nestable scanning callback
    #[token("/*", lex_block_comment)]
    BlockComment,
}

impl Token {
    /// Returns true if this token is a keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::KwModule
                | Token::KwSignal
                | Token::KwReg
                | Token::KwComb
                | Token::KwIn
                | Token::KwOut
                | Token::KwInout
                | Token::KwInst
                | Token::KwExtern
                | Token::KwDomain
                | Token::KwStruct
                | Token::KwEnum
                | Token::KwInterface
                | Token::KwType
                | Token::KwConst
                | Token::KwLet
                | Token::KwFn
                | Token::KwGroup
                | Token::KwFsm
                | Token::KwPipeline
                | Token::KwStage
                | Token::KwOn
                | Token::KwReset
                | Token::KwTick
                | Token::KwMatch
                | Token::KwIf
                | Token::KwElif
                | Token::KwElse
                | Token::KwThen
                | Token::KwFor
                | Token::KwGen
                | Token::KwWhen
                | Token::KwPriority
                | Token::KwParallel
                | Token::KwOtherwise
                | Token::KwAssert
                | Token::KwAssume
                | Token::KwCover
                | Token::KwProperty
                | Token::KwSequence
                | Token::KwAlways
                | Token::KwEventually
                | Token::KwUntil
                | Token::KwImplies
                | Token::KwVerify
                | Token::KwForall
                | Token::KwNext
                | Token::KwTrue
                | Token::KwFalse
                | Token::KwAnd
                | Token::KwOr
                | Token::KwNot
                | Token::KwImport
                | Token::KwFrom
                | Token::KwAs
                | Token::KwPub
                | Token::KwUnchecked
                | Token::KwStaticAssert
                | Token::KwTest
        )
    }
}
```

**Design notes:**
- `Ident` uses `priority = 1` so keywords (`#[token(...)]` — priority 2) take precedence.
- `Ident` carries no data intentionally — the parser recovers identifier text by slicing `source[span.start..span.end]`. This avoids allocating strings for every identifier token.
- Line continuation (`\` before newline, per spec Section 1.1) is deferred to Phase 2. The `Backslash` token is emitted but not yet consumed for continuation.

- [ ] **Step 3: Run build to verify Token compiles**

Run: `cargo build -p ssl-core`
Expected: Compiles (logos derive may need adjustments — iterate)

- [ ] **Step 4: Commit**

```bash
git add crates/ssl-core/src/lexer/
git commit -m "feat: define Token enum with all SSL keywords, operators, and literals"
```

---

### Task 4: Implement Numeric Literal Parsing

**Files:**
- Create: `crates/ssl-core/src/lexer/numeric.rs`
- Modify: `crates/ssl-core/src/lexer/token.rs` (wire up callbacks)
- Modify: `crates/ssl-core/src/lexer/mod.rs`

Numeric literals are the most complex lexical element: `42`, `0xFF`, `0b1010`, `8'hFF`, `16'b1010_0011`, `4'b10??` (don't-care).

- [ ] **Step 1: Write failing tests for numeric parsing**

Create `crates/ssl-core/src/lexer/numeric.rs`:
```rust
/// Parse a numeric literal string into a NumericLiteral.
///
/// Formats:
/// - Decimal: `42`, `1_000_000`
/// - Hex: `0xFF`, `0xDEAD_BEEF`
/// - Binary: `0b1010_0011`
/// - Sized: `8'b1010_0011`, `16'hDEAD`, `8'd255`
///
/// Underscores are allowed anywhere after the prefix for readability.
/// Don't-care bits (`?`) in sized binary literals are stored as 0.
use super::token::{NumericBase, NumericLiteral};

pub fn parse_numeric(s: &str) -> Option<NumericLiteral> {
    // Check for sized literal: N'b... or N'h... or N'd...
    if let Some(tick_pos) = s.find('\'') {
        let width_str = &s[..tick_pos];
        let width: u32 = width_str.parse().ok()?;
        let rest = &s[tick_pos + 1..];

        if rest.is_empty() {
            return None;
        }

        let (base, digits) = match rest.as_bytes()[0] {
            b'b' | b'B' => (NumericBase::Binary, &rest[1..]),
            b'h' | b'H' => (NumericBase::Hex, &rest[1..]),
            b'd' | b'D' => (NumericBase::Decimal, &rest[1..]),
            _ => return None,
        };

        // Build value and don't-care mask simultaneously
        let stripped: Vec<char> = digits.chars().filter(|&c| c != '_').collect();

        let radix = match base {
            NumericBase::Binary => 2,
            NumericBase::Decimal => 10,
            NumericBase::Hex => 16,
        };

        // For binary: track which bits are don't-care ('?')
        let mut dont_care_mask: u128 = 0;
        if base == NumericBase::Binary {
            for &ch in &stripped {
                dont_care_mask <<= 1;
                if ch == '?' {
                    dont_care_mask |= 1;
                }
            }
        }

        let clean: String = stripped
            .iter()
            .map(|&c| if c == '?' { '0' } else { c })
            .collect();

        let value = u128::from_str_radix(&clean, radix).ok()?;

        // Verify value fits in declared width
        if width < 128 && value >= (1u128 << width) {
            return None; // Value too large for declared width
        }

        return Some(NumericLiteral::Sized { width, value, base, dont_care_mask });
    }

    // Unsized literals
    let clean: String = s.chars().filter(|&c| c != '_').collect();

    if let Some(hex) = clean.strip_prefix("0x").or(clean.strip_prefix("0X")) {
        let value = u128::from_str_radix(hex, 16).ok()?;
        Some(NumericLiteral::Hex(value))
    } else if let Some(bin) = clean.strip_prefix("0b").or(clean.strip_prefix("0B")) {
        let value = u128::from_str_radix(bin, 2).ok()?;
        Some(NumericLiteral::Binary(value))
    } else {
        let value: u128 = clean.parse().ok()?;
        Some(NumericLiteral::Decimal(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_literal() {
        assert_eq!(parse_numeric("42"), Some(NumericLiteral::Decimal(42)));
    }

    #[test]
    fn decimal_with_underscores() {
        assert_eq!(
            parse_numeric("1_000_000"),
            Some(NumericLiteral::Decimal(1_000_000))
        );
    }

    #[test]
    fn hex_literal() {
        assert_eq!(parse_numeric("0xFF"), Some(NumericLiteral::Hex(0xFF)));
    }

    #[test]
    fn hex_literal_upper() {
        assert_eq!(
            parse_numeric("0xDEAD_BEEF"),
            Some(NumericLiteral::Hex(0xDEAD_BEEF))
        );
    }

    #[test]
    fn binary_literal() {
        assert_eq!(
            parse_numeric("0b1010_0011"),
            Some(NumericLiteral::Binary(0b1010_0011))
        );
    }

    #[test]
    fn sized_binary() {
        assert_eq!(
            parse_numeric("8'b1010_0011"),
            Some(NumericLiteral::Sized {
                width: 8,
                value: 0b1010_0011,
                base: NumericBase::Binary,
                dont_care_mask: 0,
            })
        );
    }

    #[test]
    fn sized_hex() {
        assert_eq!(
            parse_numeric("16'hDEAD"),
            Some(NumericLiteral::Sized {
                width: 16,
                value: 0xDEAD,
                base: NumericBase::Hex,
                dont_care_mask: 0,
            })
        );
    }

    #[test]
    fn sized_decimal() {
        assert_eq!(
            parse_numeric("8'd255"),
            Some(NumericLiteral::Sized {
                width: 8,
                value: 255,
                base: NumericBase::Decimal,
                dont_care_mask: 0,
            })
        );
    }

    #[test]
    fn sized_with_dont_care() {
        // 4'b10?? — don't-care bits tracked in mask
        assert_eq!(
            parse_numeric("4'b10??"),
            Some(NumericLiteral::Sized {
                width: 4,
                value: 0b1000,
                base: NumericBase::Binary,
                dont_care_mask: 0b0011,
            })
        );
    }

    #[test]
    fn sized_value_too_large() {
        // 4'hFF = 255, which doesn't fit in 4 bits (max 15)
        assert_eq!(parse_numeric("4'hFF"), None);
    }

    #[test]
    fn zero() {
        assert_eq!(parse_numeric("0"), Some(NumericLiteral::Decimal(0)));
    }
}
```

- [ ] **Step 2: Register module and run tests**

Add `mod numeric;` to `crates/ssl-core/src/lexer/mod.rs`:
```rust
mod token;
mod numeric;

pub use token::{Token, NumericLiteral, NumericBase};
pub use numeric::parse_numeric;
```

Run: `cargo test -p ssl-core numeric`
Expected: All 10 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/ssl-core/src/lexer/numeric.rs crates/ssl-core/src/lexer/mod.rs
git commit -m "feat: implement numeric literal parser with sized, hex, binary, don't-care support"
```

---

## Chunk 3: Lexer Implementation

### Task 5: Implement the Lexer Driver

**Files:**
- Create: `crates/ssl-core/src/lexer/lex.rs`
- Modify: `crates/ssl-core/src/lexer/mod.rs`
- Modify: `crates/ssl-core/src/lexer/token.rs` (wire numeric callback)

The lexer driver wraps logos to produce `Spanned<Token>` items. It handles:
1. Running logos for keyword/operator/identifier tokenization
2. Numeric literal detection and parsing via callback
3. Nestable block comment scanning
4. Newline detection

- [ ] **Step 1: Update Token to use logos callbacks for numerics**

In `crates/ssl-core/src/lexer/token.rs`, the `Numeric` variant needs a regex and callback. Replace the `Numeric` placeholder:

```rust
    // Sized literals: N'bXXX, N'hXXX, N'dXXX (must come before unsized)
    #[regex(r"[0-9]+\'[bBhHdD][0-9a-fA-F_?]+", lex_sized_numeric, priority = 5)]
    // Hex: 0xFF
    #[regex(r"0[xX][0-9a-fA-F_]+", lex_unsized_numeric, priority = 4)]
    // Binary: 0b1010
    #[regex(r"0[bB][01_]+", lex_unsized_numeric, priority = 4)]
    // Decimal: 42, 1_000_000
    #[regex(r"[0-9][0-9_]*", lex_unsized_numeric, priority = 3)]
    Numeric(NumericLiteral),
```

Add callback functions (outside the enum, same file):
```rust
fn lex_sized_numeric(lex: &mut logos::Lexer<'_, Token>) -> Option<NumericLiteral> {
    super::numeric::parse_numeric(lex.slice())
}

fn lex_unsized_numeric(lex: &mut logos::Lexer<'_, Token>) -> Option<NumericLiteral> {
    super::numeric::parse_numeric(lex.slice())
}

/// Scan a nestable block comment `/* ... */` from inside a logos callback.
/// Advances the lexer past the closing `*/`.
fn lex_block_comment(lex: &mut logos::Lexer<'_, Token>) -> logos::FilterResult<(), ()> {
    let remaining = lex.remainder();
    let mut depth = 1u32; // already inside first /*
    let bytes = remaining.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            depth += 1;
            i += 2;
        } else if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                lex.bump(i);
                return logos::FilterResult::Emit(());
            }
        } else {
            i += 1;
        }
    }

    // Unterminated block comment — emit as error
    logos::FilterResult::Error(())
}
```

- [ ] **Step 2: Implement the lexer driver**

Create `crates/ssl-core/src/lexer/lex.rs`:
```rust
use logos::Logos;

use crate::span::{Span, Spanned};

use super::token::Token;

/// Lex source code into a sequence of spanned tokens.
///
/// Block comments (nestable `/* */`) are handled by a logos callback
/// in the Token enum. Newlines are detected in the error fallback.
///
/// Note: Line continuation (`\` before newline) is deferred to a future phase.
pub fn lex(source: &str) -> Result<Vec<Spanned<Token>>, LexError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(source);

    while let Some(result) = lexer.next() {
        let span: Span = lexer.span().into();
        match result {
            Ok(token) => {
                tokens.push(Spanned::new(token, span));
            }
            Err(()) => {
                let slice = &source[span.start as usize..span.end as usize];

                if slice == "\n" || slice == "\r\n" || slice == "\r" {
                    tokens.push(Spanned::new(Token::Newline, span));
                } else {
                    return Err(LexError {
                        message: format!("unexpected character: {:?}", slice),
                        span,
                    });
                }
            }
        }
    }

    Ok(tokens)
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "lex error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for LexError {}
```

- [ ] **Step 3: Update mod.rs exports**

```rust
mod token;
mod numeric;
mod lex;

pub use token::{Token, NumericLiteral, NumericBase};
pub use numeric::parse_numeric;
pub use lex::{lex, LexError};
```

- [ ] **Step 4: Run build to verify lexer compiles**

Run: `cargo build -p ssl-core`
Expected: Compiles (may need iteration on logos derive details)

- [ ] **Step 5: Commit**

```bash
git add crates/ssl-core/src/lexer/
git commit -m "feat: implement lexer driver with block comment scanning and newline detection"
```

---

### Task 6: Write Lexer Integration Tests

**Files:**
- Create: `crates/ssl-core/tests/lexer_tests.rs`

- [ ] **Step 1: Write integration tests covering all token categories**

```rust
use ssl_core::lexer::{lex, Token, NumericLiteral, NumericBase};

fn token_types(source: &str) -> Vec<Token> {
    lex(source)
        .expect("lex failed")
        .into_iter()
        .map(|s| s.node)
        // Filter out newlines and comments for cleaner assertions
        .filter(|t| !matches!(t, Token::Newline | Token::LineComment | Token::BlockComment))
        .collect()
}

#[test]
fn keywords() {
    let tokens = token_types("module signal reg comb");
    assert_eq!(
        tokens,
        vec![Token::KwModule, Token::KwSignal, Token::KwReg, Token::KwComb]
    );
}

#[test]
fn operators() {
    let tokens = token_types("+ - * / == != <= >= << >> >>> ++ |>");
    assert_eq!(
        tokens,
        vec![
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::EqEq,
            Token::NotEq,
            Token::LessEq,
            Token::GreaterEq,
            Token::ShiftLeft,
            Token::ShiftRight,
            Token::ArithShiftRight,
            Token::Concat,
            Token::PipeOp,
        ]
    );
}

#[test]
fn numeric_literals() {
    let tokens = token_types("42 0xFF 0b1010 8'hAB 16'b1010_0011");
    assert_eq!(
        tokens,
        vec![
            Token::Numeric(NumericLiteral::Decimal(42)),
            Token::Numeric(NumericLiteral::Hex(0xFF)),
            Token::Numeric(NumericLiteral::Binary(0b1010)),
            Token::Numeric(NumericLiteral::Sized {
                width: 8,
                value: 0xAB,
                base: NumericBase::Hex,
                dont_care_mask: 0,
            }),
            Token::Numeric(NumericLiteral::Sized {
                width: 16,
                value: 0b1010_0011,
                base: NumericBase::Binary,
                dont_care_mask: 0,
            }),
        ]
    );
}

#[test]
fn string_literal() {
    let tokens = token_types(r#""hello world""#);
    assert_eq!(tokens, vec![Token::StringLit("hello world".to_string())]);
}

#[test]
fn identifier_vs_keyword() {
    let tokens = token_types("module my_module signal count");
    assert_eq!(
        tokens,
        vec![Token::KwModule, Token::Ident, Token::KwSignal, Token::Ident]
    );
}

#[test]
fn delimiters() {
    let tokens = token_types("( ) [ ] { } : , .");
    assert_eq!(
        tokens,
        vec![
            Token::LParen,
            Token::RParen,
            Token::LBracket,
            Token::RBracket,
            Token::LBrace,
            Token::RBrace,
            Token::Colon,
            Token::Comma,
            Token::Dot,
        ]
    );
}

#[test]
fn range_operators() {
    let tokens = token_types("0..8 0..=7");
    assert_eq!(
        tokens,
        vec![
            Token::Numeric(NumericLiteral::Decimal(0)),
            Token::RangeExclusive,
            Token::Numeric(NumericLiteral::Decimal(8)),
            Token::Numeric(NumericLiteral::Decimal(0)),
            Token::RangeInclusive,
            Token::Numeric(NumericLiteral::Decimal(7)),
        ]
    );
}

#[test]
fn line_comment() {
    let all_tokens: Vec<Token> = lex("signal x // this is a comment\nsignal y")
        .unwrap()
        .into_iter()
        .map(|s| s.node)
        .collect();
    // Should contain: KwSignal, Ident, LineComment, Newline, KwSignal, Ident
    assert!(all_tokens.contains(&Token::LineComment));
}

#[test]
fn nestable_block_comment() {
    let tokens = token_types("signal /* outer /* inner */ still outer */ x");
    assert_eq!(tokens, vec![Token::KwSignal, Token::Ident]);
}

#[test]
fn module_port_list() {
    let tokens = token_types("module ALU(\n    in a: UInt,\n    out b: UInt\n):");
    // Should produce: KwModule, Ident, LParen, KwIn, Ident, Colon, Ident, Comma,
    //                  KwOut, Ident, Colon, Ident, RParen, Colon
    assert_eq!(tokens[0], Token::KwModule);
    assert_eq!(tokens[1], Token::Ident); // ALU
    assert_eq!(tokens[2], Token::LParen);
    assert_eq!(tokens[3], Token::KwIn);
}

#[test]
fn arrows() {
    let tokens = token_types("-> => -->");
    assert_eq!(
        tokens,
        vec![Token::ThinArrow, Token::FatArrow, Token::LongArrow]
    );
}

#[test]
fn exponentiation_operator() {
    let tokens = token_types("2 ** 10");
    assert_eq!(
        tokens,
        vec![
            Token::Numeric(NumericLiteral::Decimal(2)),
            Token::StarStar,
            Token::Numeric(NumericLiteral::Decimal(10)),
        ]
    );
}

#[test]
fn underscore_vs_identifier() {
    let tokens = token_types("_ _foo");
    assert_eq!(tokens, vec![Token::Underscore, Token::Ident]);
}

#[test]
fn at_operator() {
    let tokens = token_types("signal x: UInt @ sys");
    assert!(tokens.contains(&Token::At));
}

#[test]
fn empty_source() {
    let tokens = token_types("");
    assert!(tokens.is_empty());
}

#[test]
fn only_comments() {
    let tokens = token_types("// just a comment");
    assert!(tokens.is_empty());
}

#[test]
fn blank_lines_dont_affect_indent() {
    let source = "comb:\n    x = y\n\n    z = w";
    let tokens = tokenize(source).expect("tokenize failed");
    let types: Vec<Token> = tokens.iter().map(|s| s.node.clone()).collect();
    // Should have exactly 1 Indent (not extra Dedent/Indent from blank line)
    assert_eq!(types.iter().filter(|t| **t == Token::Indent).count(), 1);
}

#[test]
fn fsm_transition_syntax() {
    // Idle --(condition)--> Fetch
    let tokens = token_types("Idle --(condition)--> Fetch");
    // Should contain: Ident(Idle), DashDash, LParen, Ident, RParen, LongArrow, Ident(Fetch)
    assert_eq!(tokens[0], Token::Ident); // Idle
    assert_eq!(tokens[1], Token::DashDash);
    assert_eq!(tokens[2], Token::LParen);
    assert_eq!(tokens[3], Token::Ident); // condition
    assert_eq!(tokens[4], Token::RParen);
    assert_eq!(tokens[5], Token::LongArrow);
    assert_eq!(tokens[6], Token::Ident); // Fetch
}

#[test]
fn doc_comment() {
    let all_tokens: Vec<Token> = lex("/// This is a doc comment\nmodule Foo")
        .unwrap()
        .into_iter()
        .map(|s| s.node)
        .collect();
    assert!(all_tokens.contains(&Token::DocComment));
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p ssl-core --test lexer_tests`
Expected: All tests pass (iterate on Token definition as needed)

- [ ] **Step 3: Commit**

```bash
git add crates/ssl-core/tests/lexer_tests.rs
git commit -m "test: add lexer integration tests covering all token categories"
```

---

## Chunk 4: Indentation Processing

### Task 7: Implement Indentation Tracker

**Files:**
- Create: `crates/ssl-core/src/lexer/indent.rs`
- Modify: `crates/ssl-core/src/lexer/mod.rs`

SiliconScript uses indentation-scoped blocks. The indentation tracker converts raw `Newline` tokens into `Newline`, `Indent`, and `Dedent` tokens based on indentation level changes.

- [ ] **Step 1: Write failing tests**

Create `crates/ssl-core/src/lexer/indent.rs`:
```rust
use crate::span::{Span, Spanned};

use super::token::Token;

/// Process a token stream, converting Newline tokens into
/// Newline/Indent/Dedent tokens based on indentation changes.
///
/// Rules:
/// - Blank lines (only whitespace before next newline) are emitted as Newline only,
///   they do NOT affect indentation state
/// - Increased indentation emits Indent
/// - Decreased indentation emits one or more Dedent tokens
/// - At EOF, emit Dedent for each remaining indent level
/// - Indentation is measured in spaces (tabs = 4 spaces)
pub fn process_indentation(
    source: &str,
    tokens: Vec<Spanned<Token>>,
) -> Result<Vec<Spanned<Token>>, IndentError> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut indent_stack: Vec<u32> = vec![0]; // stack of indentation levels
    let mut i = 0;

    while i < tokens.len() {
        let tok = &tokens[i];

        if tok.node == Token::Newline {
            let newline_span = tok.span;

            // Measure indentation: count spaces at the start of the next line
            let line_start = newline_span.end as usize;

            // Check if this is a blank line (only whitespace until next newline/EOF)
            if is_blank_line(source, line_start) {
                // Blank line — emit Newline but don't change indent state
                result.push(Spanned::new(Token::Newline, newline_span));
                i += 1;
                continue;
            }

            let indent_level = measure_indent(source, line_start);
            let current = *indent_stack.last().unwrap();

            if indent_level > current {
                // Emit newline then indent
                result.push(Spanned::new(Token::Newline, newline_span));
                result.push(Spanned::new(
                    Token::Indent,
                    Span::new(line_start as u32, (line_start as u32) + indent_level),
                ));
                indent_stack.push(indent_level);
            } else if indent_level < current {
                // Emit dedents for each level we're leaving
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
                        span: Span::new(
                            line_start as u32,
                            (line_start as u32) + indent_level,
                        ),
                    });
                }
            } else {
                // Same level — just a newline
                result.push(Spanned::new(Token::Newline, newline_span));
            }
        } else {
            result.push(tok.clone());
        }

        i += 1;
    }

    // EOF: close all open indentation levels
    let eof_pos = source.len() as u32;
    while indent_stack.len() > 1 {
        indent_stack.pop();
        result.push(Spanned::new(Token::Dedent, Span::new(eof_pos, eof_pos)));
    }

    Ok(result)
}

/// Check if the line starting at `pos` is blank (only whitespace until newline or EOF).
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
    // Reached EOF — this is effectively a blank trailing line
    true
}

/// Count the number of spaces at position `pos` in `source`.
/// Tab = 4 spaces.
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
        // Should have 2 Indents and 1 Dedent (inner), then later 1 Dedent (outer at EOF)
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
        // 2 opens, 2 closes at EOF
        assert_eq!(indent_count, dedent_count);
    }
}
```

- [ ] **Step 2: Register module and run tests**

Add to `crates/ssl-core/src/lexer/mod.rs`:
```rust
mod token;
mod numeric;
mod lex;
mod indent;

pub use token::{Token, NumericLiteral, NumericBase};
pub use numeric::parse_numeric;
pub use lex::{lex, LexError};
pub use indent::{process_indentation, IndentError};
```

Run: `cargo test -p ssl-core indent`
Expected: 4 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/ssl-core/src/lexer/indent.rs crates/ssl-core/src/lexer/mod.rs
git commit -m "feat: implement indentation tracker converting newlines to Indent/Dedent tokens"
```

---

### Task 8: Implement `tokenize()` — Full Lexer Pipeline

**Files:**
- Modify: `crates/ssl-core/src/lexer/mod.rs`
- Modify: `crates/ssl-core/src/lexer/lex.rs`

Provide a single entry point `tokenize()` that runs lex + indentation processing + comment/whitespace filtering.

- [ ] **Step 1: Add `tokenize()` function**

Add to `crates/ssl-core/src/lexer/mod.rs`:
```rust
use crate::span::Spanned;

/// Full lexer pipeline: lex → strip comments → indentation processing.
///
/// Returns a token stream ready for parsing, with:
/// - Comments stripped BEFORE indentation (so comment-only lines don't affect indent)
/// - Doc comments preserved
/// - Indent/Dedent tokens for block structure
pub fn tokenize(source: &str) -> Result<Vec<Spanned<Token>>, TokenizeError> {
    let raw = lex(source).map_err(TokenizeError::Lex)?;

    // Strip comments BEFORE indentation processing so comment-only lines
    // and multi-line block comments don't generate spurious indent changes.
    // Doc comments are preserved as they carry semantic meaning.
    let no_comments: Vec<Spanned<Token>> = raw
        .into_iter()
        .filter(|t| !matches!(t.node, Token::LineComment | Token::BlockComment))
        .collect();

    let indented = process_indentation(source, no_comments).map_err(TokenizeError::Indent)?;

    Ok(indented)
}

#[derive(Debug)]
pub enum TokenizeError {
    Lex(LexError),
    Indent(IndentError),
}

impl std::fmt::Display for TokenizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizeError::Lex(e) => write!(f, "{}", e),
            TokenizeError::Indent(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for TokenizeError {}
```

- [ ] **Step 2: Write integration test for full pipeline**

Add to `crates/ssl-core/tests/lexer_tests.rs`:
```rust
use ssl_core::lexer::tokenize;

#[test]
fn full_pipeline_module() {
    let source = r#"module ALU(
    in  a: UInt,
    out b: UInt
):
    comb:
        b = a
"#;
    let tokens = tokenize(source).expect("tokenize failed");
    let types: Vec<Token> = tokens.iter().map(|s| s.node.clone()).collect();

    // Should contain Indent after the colon-newline
    assert!(types.contains(&Token::Indent));
    // Should not contain LineComment or BlockComment
    assert!(!types.contains(&Token::LineComment));
    assert!(!types.contains(&Token::BlockComment));
}

#[test]
fn full_pipeline_comments_stripped() {
    let source = "signal x // comment\n/* block */\nsignal y";
    let tokens = tokenize(source).expect("tokenize failed");
    let types: Vec<Token> = tokens.iter().map(|s| s.node.clone()).collect();
    assert!(!types.contains(&Token::LineComment));
    assert!(!types.contains(&Token::BlockComment));
    assert_eq!(
        types.iter().filter(|t| **t == Token::KwSignal).count(),
        2
    );
}

#[test]
fn full_pipeline_doc_comments_preserved() {
    let source = "/// doc\nmodule Foo";
    let tokens = tokenize(source).expect("tokenize failed");
    let types: Vec<Token> = tokens.iter().map(|s| s.node.clone()).collect();
    assert!(types.contains(&Token::DocComment));
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p ssl-core`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/ssl-core/src/lexer/mod.rs crates/ssl-core/tests/lexer_tests.rs
git commit -m "feat: add tokenize() full pipeline with comment filtering and doc comment preservation"
```

---

### Task 9: Add Snapshot Tests for Real SSL Examples

**Files:**
- Create: `crates/ssl-core/tests/snapshots/` (directory for insta snapshots)
- Modify: `crates/ssl-core/tests/lexer_tests.rs`

Snapshot tests capture the full token stream for representative SSL code examples from the spec. This catches regressions when the lexer changes.

- [ ] **Step 1: Add insta dev-dependency**

Verify `insta` is in workspace dev-dependencies (already in `Cargo.toml`). Add to `crates/ssl-core/Cargo.toml`:
```toml
[dev-dependencies]
insta = { workspace = true }
```

- [ ] **Step 2: Write snapshot tests**

Add to `crates/ssl-core/tests/lexer_tests.rs`:
```rust
use insta::assert_snapshot;

fn snapshot_tokens(source: &str) -> String {
    let tokens = tokenize(source).expect("tokenize failed");
    tokens
        .iter()
        .map(|s| format!("{:>4}..{:<4} {:?}", s.span.start, s.span.end, s.node))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn snapshot_signal_declarations() {
    let source = r#"signal counter: UInt<8>
signal offset: SInt<16> @ sys_clk
signal enable: Bool"#;
    assert_snapshot!(snapshot_tokens(source));
}

#[test]
fn snapshot_comb_block() {
    let source = r#"comb:
    match opcode:
        ADD => result = a + b
        SUB => result = a - b
    zero = result == 0"#;
    assert_snapshot!(snapshot_tokens(source));
}

#[test]
fn snapshot_reg_block() {
    let source = r#"reg(clk, rst):
    on reset:
        counter = 0
    on tick:
        if enable:
            counter = counter + 1"#;
    assert_snapshot!(snapshot_tokens(source));
}

#[test]
fn snapshot_pipe_operator() {
    let source = r#"comb:
    result = raw_adc
        |> sign_extend(to=16)
        |> scale(factor=3)"#;
    assert_snapshot!(snapshot_tokens(source));
}
```

- [ ] **Step 3: Run snapshot tests to generate initial snapshots**

Run: `cargo insta test -p ssl-core --test lexer_tests`
Then: `cargo insta review` to accept the snapshots

Expected: 4 new snapshots created and accepted

- [ ] **Step 4: Commit**

```bash
git add crates/ssl-core/tests/ crates/ssl-core/Cargo.toml
git commit -m "test: add insta snapshot tests for lexer output on spec examples"
```

---

### Task 10: Wire Up CLI to Lex a File

**Files:**
- Modify: `crates/sslc/src/main.rs`

Basic CLI: `sslc lex <file.ssl>` prints the token stream.

- [ ] **Step 1: Implement CLI entry point**

```rust
use std::path::PathBuf;

use ssl_core::lexer::tokenize;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: sslc <command> [args]");
        eprintln!("Commands:");
        eprintln!("  lex <file.ssl>    Tokenize a file and print tokens");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "lex" => {
            if args.len() < 3 {
                eprintln!("Usage: sslc lex <file.ssl>");
                std::process::exit(1);
            }
            let path = PathBuf::from(&args[2]);
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading {}: {}", path.display(), e);
                    std::process::exit(1);
                }
            };

            match tokenize(&source) {
                Ok(tokens) => {
                    for tok in &tokens {
                        println!(
                            "{:>4}..{:<4} {:?}",
                            tok.span.start, tok.span.end, tok.node
                        );
                    }
                    eprintln!("\n{} tokens", tokens.len());
                }
                Err(e) => {
                    eprintln!("Lex error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Unknown command: {}", other);
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 2: Create a test SSL file**

Create `examples/blinker.ssl`:
```
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
```

- [ ] **Step 3: Run the CLI**

Run: `cargo run -p sslc -- lex examples/blinker.ssl`
Expected: Prints token stream with spans, no errors

- [ ] **Step 4: Commit**

```bash
git add crates/sslc/src/main.rs examples/blinker.ssl
git commit -m "feat: add 'sslc lex' command to tokenize SSL files from CLI"
```

---

### Task 11: Final Validation

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Run formatter**

Run: `cargo fmt --check`
Expected: No formatting issues

- [ ] **Step 4: Final commit if any fixes needed**

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting"
```

---

## Summary

**What this phase produces:**
- Cargo workspace with `sslc` (binary) and `ssl-core` (library) crates
- Complete token enum covering all 70+ SSL keywords, operators, and literal types
- Numeric literal parser handling decimal, hex, binary, sized, and don't-care formats
- Logos-based lexer with nestable block comment support
- Indentation tracker producing Indent/Dedent tokens for block structure
- Full `tokenize()` pipeline combining all stages
- CLI command `sslc lex` for debugging
- 25+ tests including snapshot tests against spec examples

**What's next:** Phase 2 (Parser + AST) will consume the token stream from this phase and produce an abstract syntax tree.
