# SiliconScript

A modern, statically-typed hardware description language (HDL) designed to replace Verilog and VHDL for FPGA prototyping and digital design.

## Overview

SiliconScript (SSL) brings modern programming language ergonomics to hardware description:

- **Python-like indentation syntax** -- no semicolons or `begin`/`end` blocks
- **Strong static typing** with generics (`UInt<8>`, `Flip<Stream<T>>`)
- **First-class HDL constructs** -- `reg`, `comb`, `signal`, clock domains, reset handling
- **Formal verification built in** -- `assert always`, `assume`, `cover`
- **Advanced abstractions** -- FSMs, pipelines with backpressure, interfaces, `gen for`/`gen if`

## Example

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

## Project Structure

```
crates/
  ssl-core/       Core library: lexer, parser, AST definitions
  sslc/           CLI compiler frontend
docs/
  specs/          Language specification (sections 1-15)
  superpowers/    Implementation plans
examples/         Example .ssl files
```

## Building

Requires Rust 1.85+ (edition 2024).

```bash
cargo build
cargo test
```

## CLI Usage

```bash
# Tokenize a file
cargo run -p sslc -- lex examples/blinker.ssl

# Parse a file and print the AST
cargo run -p sslc -- parse examples/blinker.ssl
```

## Current Status

**Phase 1 (Lexer)** -- Complete. Full tokenizer with keywords, operators, numeric literals (decimal, hex, binary, sized), string literals, indentation tracking (Indent/Dedent tokens), and doc comments.

**Phase 2 (Parser + AST)** -- Complete. Recursive descent parser with:
- Pratt expression parser (14 precedence levels)
- Type expressions with generic support and nested `>>` handling
- Statements: `signal`, `let`, `const`, `type`, `if`/`elif`/`else`, `match`, `for`, `comb`, `reg`, `priority`, `parallel`, `assert`, `assume`, `cover`, `unchecked`
- Items: `module`, `struct`, `enum`, `interface`, `fn`, `fsm`, `pipeline`, `test`, `import`, `extern module`, `inst`, `gen for`/`gen if`
- 95 parser tests + 26 lexer tests

**Phase 3 (Semantic Analysis)** -- Not yet started.

## License

Copyright 2026 Brandon Wiemer. Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
