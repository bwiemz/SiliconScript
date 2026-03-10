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
    Sized {
        width: u32,
        value: u128,
        base: NumericBase,
        /// Bitmask where 1 = don't-care bit (from `?` in binary literals).
        dont_care_mask: u128,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericBase {
    Binary,
    Decimal,
    Hex,
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t]+")]
pub enum Token {
    // Synthetic tokens (not matched by logos)
    Newline,
    Indent,
    Dedent,

    // Hardware Construct Keywords
    #[token("module")] KwModule,
    #[token("signal")] KwSignal,
    #[token("reg")] KwReg,
    #[token("comb")] KwComb,
    #[token("in")] KwIn,
    #[token("out")] KwOut,
    #[token("inout")] KwInout,
    #[token("inst")] KwInst,
    #[token("extern")] KwExtern,
    #[token("domain")] KwDomain,

    // Type Construct Keywords
    #[token("struct")] KwStruct,
    #[token("enum")] KwEnum,
    #[token("interface")] KwInterface,
    #[token("type")] KwType,
    #[token("const")] KwConst,
    #[token("let")] KwLet,
    #[token("fn")] KwFn,
    #[token("group")] KwGroup,

    // Sequential Construct Keywords
    #[token("fsm")] KwFsm,
    #[token("pipeline")] KwPipeline,
    #[token("stage")] KwStage,
    #[token("on")] KwOn,
    #[token("reset")] KwReset,
    #[token("tick")] KwTick,

    // Control Flow Keywords
    #[token("match")] KwMatch,
    #[token("if")] KwIf,
    #[token("elif")] KwElif,
    #[token("else")] KwElse,
    #[token("then")] KwThen,
    #[token("for")] KwFor,
    #[token("gen")] KwGen,
    #[token("when")] KwWhen,
    #[token("priority")] KwPriority,
    #[token("parallel")] KwParallel,
    #[token("otherwise")] KwOtherwise,

    // Formal Verification Keywords
    #[token("assert")] KwAssert,
    #[token("assume")] KwAssume,
    #[token("cover")] KwCover,
    #[token("property")] KwProperty,
    #[token("sequence")] KwSequence,
    #[token("always")] KwAlways,
    #[token("eventually")] KwEventually,
    #[token("until")] KwUntil,
    #[token("implies")] KwImplies,
    #[token("verify")] KwVerify,
    #[token("forall")] KwForall,
    #[token("next")] KwNext,

    // Literal & Logic Keywords
    #[token("true")] KwTrue,
    #[token("false")] KwFalse,
    #[token("and")] KwAnd,
    #[token("or")] KwOr,
    #[token("not")] KwNot,

    // Module System Keywords
    #[token("import")] KwImport,
    #[token("from")] KwFrom,
    #[token("as")] KwAs,
    #[token("pub")] KwPub,

    // Safety Keywords
    #[token("unchecked")] KwUnchecked,
    #[token("static_assert")] KwStaticAssert,

    // Test Keyword
    #[token("test")] KwTest,

    // Operators (multi-char before single-char for correct matching)
    #[token(">>>")] ArithShiftRight,
    #[token("-->")] LongArrow,
    #[token("**")] StarStar,
    #[token("++")] Concat,
    #[token("|>")] PipeOp,
    #[token("==")] EqEq,
    #[token("!=")] NotEq,
    #[token("<=")] LessEq,
    #[token(">=")] GreaterEq,
    #[token("<<")] ShiftLeft,
    #[token(">>")] ShiftRight,
    #[token("=>")] FatArrow,
    #[token("->")] ThinArrow,
    #[token("--")] DashDash,
    #[token("..=")] RangeInclusive,
    #[token("..")] RangeExclusive,
    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Star,
    #[token("/")] Slash,
    #[token("%")] Percent,
    #[token("&")] Ampersand,
    #[token("|")] Pipe,
    #[token("^")] Caret,
    #[token("~")] Tilde,
    #[token("<")] Less,
    #[token(">")] Greater,
    #[token("=")] Eq,
    #[token("@")] At,
    #[token("?")] Question,

    // Delimiters
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("[")] LBracket,
    #[token("]")] RBracket,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token(":")] Colon,
    #[token(",")] Comma,
    #[token(".")] Dot,
    #[token("_")] Underscore,
    #[token("\\")] Backslash,

    // Literals — numeric uses callbacks
    // Sized literals: N'bXXX, N'hXXX, N'dXXX
    #[regex(r"[0-9]+'[bBhHdD][0-9a-fA-F_?]+", lex_sized_numeric, priority = 5)]
    // Hex: 0xFF
    #[regex(r"0[xX][0-9a-fA-F_]+", lex_unsized_numeric, priority = 4)]
    // Binary: 0b1010
    #[regex(r"0[bB][01_]+", lex_unsized_numeric, priority = 4)]
    // Decimal: 42, 1_000_000
    #[regex(r"[0-9][0-9_]*", lex_unsized_numeric, priority = 3)]
    Numeric(NumericLiteral),

    // String literal
    #[regex(r#""[^"]*""#, |lex| lex.slice()[1..lex.slice().len()-1].to_string())]
    StringLit(String),

    // Identifiers (priority 1 so keywords win)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", priority = 1)]
    Ident,

    // Doc comments (priority 10 — must beat line comments)
    #[regex(r"///[^\n]*", priority = 10)]
    DocComment,

    // Line comments (priority 5)
    #[regex(r"//[^\n]*", priority = 5)]
    LineComment,

    // Block comment — uses callback for nestable scanning
    #[token("/*", lex_block_comment)]
    BlockComment,
}

/// Scan a nestable block comment from inside a logos callback.
fn lex_block_comment(lex: &mut logos::Lexer<'_, Token>) -> logos::FilterResult<(), ()> {
    let remaining = lex.remainder();
    let mut depth = 1u32;
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

    logos::FilterResult::Error(())
}

fn lex_sized_numeric(lex: &mut logos::Lexer<'_, Token>) -> Option<NumericLiteral> {
    super::numeric::parse_numeric(lex.slice())
}

fn lex_unsized_numeric(lex: &mut logos::Lexer<'_, Token>) -> Option<NumericLiteral> {
    super::numeric::parse_numeric(lex.slice())
}

impl Token {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::KwModule | Token::KwSignal | Token::KwReg | Token::KwComb
                | Token::KwIn | Token::KwOut | Token::KwInout | Token::KwInst
                | Token::KwExtern | Token::KwDomain | Token::KwStruct | Token::KwEnum
                | Token::KwInterface | Token::KwType | Token::KwConst | Token::KwLet
                | Token::KwFn | Token::KwGroup | Token::KwFsm | Token::KwPipeline
                | Token::KwStage | Token::KwOn | Token::KwReset | Token::KwTick
                | Token::KwMatch | Token::KwIf | Token::KwElif | Token::KwElse
                | Token::KwThen | Token::KwFor | Token::KwGen | Token::KwWhen
                | Token::KwPriority | Token::KwParallel | Token::KwOtherwise
                | Token::KwAssert | Token::KwAssume | Token::KwCover | Token::KwProperty
                | Token::KwSequence | Token::KwAlways | Token::KwEventually | Token::KwUntil
                | Token::KwImplies | Token::KwVerify | Token::KwForall | Token::KwNext
                | Token::KwTrue | Token::KwFalse | Token::KwAnd | Token::KwOr | Token::KwNot
                | Token::KwImport | Token::KwFrom | Token::KwAs | Token::KwPub
                | Token::KwUnchecked | Token::KwStaticAssert | Token::KwTest
        )
    }
}
