# SiliconScript Language Design — Sections 1–6

**Date:** 2026-03-10
**Status:** Approved
**Scope:** Foundation language design (syntax, types, modules, clocking, combinational logic, sequential logic)
**Primary Target:** FPGA prototyping (Yosys/nextpnr, Vivado, Quartus)
**Compiler Language:** Rust
**Remaining Sections:** 7+ (to be designed in follow-up sessions)

---

## Design Decisions (from brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Primary target | FPGA prototyping | Synthesis-friendly semantics, fast iteration |
| Compiler language | Rust | Strong type system, compiler ecosystem (logos, chumsky, cranelift) |
| Syntax style | Hybrid (indentation blocks + inline pragmatics) | Clean blocks for logic, parenthesized port lists, angle-bracket generics |
| Verilog interop | FFI layer (`extern module`) | Type-checked at boundary, no raw Verilog escape hatches |
| Constraints | Both (attributes + separate files) | Design hints in code, board-level in `.ssc` files |
| Simulation | Inline `test` blocks (first-class) | Tests live next to design, can't get out of sync |
| Formal verification | Full property language in sections 1–6 | Temporal logic, protocol assertions, parametric verification |
| Architecture | Hardware-first with modern ergonomics | Hardware concepts map 1:1 to language constructs |

---

## Section 1 — Syntax & Language Fundamentals

### 1.1 Lexical Rules

**Comments:**
- `//` — line comment
- `/* */` — block comment (nestable)

**Numeric literals:**
- Decimal: `42`, `1_000_000` (underscores for readability)
- Hex: `0xFF`, `0xDEAD_BEEF`
- Binary: `0b1010_0011`
- Sized: `8'b1010_0011`, `16'hDEAD` (width-prefixed)
- Don't-care: `?` in bit literals (`4'b10??`)

**String literals:** Double-quoted, sim/test only (`"hello"`)

**Don't-care values:** `_` in pattern match context

**Statement termination:** Newlines. No semicolons. Line continuation with trailing `|>`, operator, or `\`.

### 1.2 Scoping Rules

- **Indentation-scoped:** module bodies, `comb`, `reg`, `fsm`, `pipeline`, `gen`, `test`, `match` blocks
- **Inline (parenthesized/bracketed):** port lists, generic parameters, function arguments, struct fields with `@[range]`

### 1.3 Naming Conventions

- `PascalCase` — types, modules, enums, interfaces, structs
- `snake_case` — signals, ports, instances, functions
- `UPPER_SNAKE` — constants

### 1.4 Reserved Keywords

**Hardware constructs:**
`module`, `signal`, `reg`, `comb`, `in`, `out`, `inout`, `inst`, `extern`, `domain`

**Type constructs:**
`struct`, `enum`, `interface`, `type`, `const`, `let`, `fn`, `group`

**Sequential constructs:**
`fsm`, `pipeline`, `stage`, `on`, `reset`, `tick`

**Control flow:**
`match`, `if`, `elif`, `else`, `then`, `for`, `gen`, `when`, `priority`, `parallel`, `otherwise`

**Formal verification:**
`assert`, `assume`, `cover`, `property`, `sequence`, `always`, `eventually`, `until`, `implies`, `verify`, `forall`, `next`

**Literals and logic:**
`true`, `false`, `and`, `or`, `not`

**Module system:**
`import`, `from`, `as`, `pub`

**Safety:**
`unchecked`, `static_assert`

**Test:**
`test`

### 1.5 Annotated Syntax Examples

**Example 1 — Signal declaration with bit width and clock domain:**
```
signal counter: UInt<8>                      // 8-bit unsigned, implicit clock domain
signal offset: SInt<16> @ sys_clk            // 16-bit signed, explicit clock domain
signal raw_data: Bits<32>                    // uninterpreted 32 bits
signal enable: Bool                          // single bit boolean
signal weight: Fixed<8, 8>                   // fixed-point: 8 int, 8 frac bits
```

**Example 2 — Module definition with typed port list:**
```
module ALU(
    in  a:      UInt<32>,
    in  b:      UInt<32>,
    in  opcode: Bits<4>,
    out result: UInt<32>,
    out zero:   Bool
):
    // module body here
```

**Example 3 — Combinational logic block:**
```
comb:
    match opcode:
        ADD => result = a + b
        SUB => result = a - b
        AND => result = a & b
        OR  => result = a | b
    zero = result == 0
```

**Example 4 — Synchronous sequential block (flip-flop inference):**
```
reg(clk, rst):
    on reset:
        counter = 0
    on tick:
        if enable:
            counter = counter + 1
```

**Example 5 — Parameterized/generic module:**
```
module ShiftReg<W: uint, DEPTH: uint>(
    in  din:  UInt<W>,
    out dout: UInt<W>
):
    signal stages: UInt<W>[DEPTH]

    reg(clk, rst):
        on reset:
            for i in 0..DEPTH:
                stages[i] = 0
        on tick:
            stages[0] = din
            for i in 1..DEPTH:
                stages[i] = stages[i - 1]

    comb:
        dout = stages[DEPTH - 1]
```

**Example 6 — Generate construct (loop and conditional):**
```
module AdderTree<W: uint, N: uint>(
    in  inputs: UInt<W>[N],
    out total:  UInt<W + clog2(N)>
):
    gen for i in 0..N / 2:
        signal partial_{i}: UInt<W + 1>
        comb:
            partial_{i} = inputs[2*i].widen() + inputs[2*i + 1].widen()

    gen if N > 2:
        inst sub = AdderTree<W + 1, N / 2>(inputs=partials, total=total)
    gen else:
        comb:
            total = partial_0
```

**Example 7 — Struct type for grouped signals:**
```
struct Pixel:
    r: UInt<8>
    g: UInt<8>
    b: UInt<8>
    a: UInt<8>

signal px: Pixel
px.r = 0xFF
let flat: Bits<32> = px.pack()           // flatten to bit vector
let px2 = Pixel.unpack(flat)             // reconstruct from bits
```

**Example 8 — Enum type for FSM state encoding:**
```
enum State [onehot]:
    Idle
    Fetch
    Decode
    Execute
    Writeback

enum AluOp [binary]:
    Add = 0b0000
    Sub = 0b0001
    And = 0b0010
    Or  = 0b0011
```

**Example 9 — Pipe operator for signal datapath chaining:**
```
comb:
    result = raw_adc
        |> sign_extend(to=16)
        |> scale(factor=3)
        |> saturate(min=-128, max=127)
        |> truncate(to=8)
```

**Example 10 — Compile-time const expressions:**
```
const DATA_W: uint = 32
const ADDR_W: uint = clog2(1024)          // = 10
const DEPTH:  uint = 2 ** ADDR_W          // = 1024
const NUM_BANKS: uint = if DATA_W > 16 then 4 else 2

type Word = UInt<DATA_W>
type Addr = UInt<ADDR_W>

static_assert DATA_W >= 8, "DATA_W must be at least 8 bits"
static_assert is_power_of_2(DEPTH), "DEPTH must be a power of 2"
```

---

## Section 2 — Type System

### 2.1 Type Hierarchy

```
HardwareType
 ├─ Bits<N>                 // uninterpreted N-bit vector (root numeric type)
 │   ├─ UInt<N>             // unsigned integer, N bits
 │   ├─ SInt<N>             // signed integer (2's complement), N bits
 │   └─ Fixed<I, F>         // fixed-point: I integer + F fractional = I+F bits
 ├─ Bool                    // single bit (alias: UInt<1>)
 ├─ Clock<freq>             // clock signal with frequency annotation
 ├─ Reset
 │   ├─ SyncReset           // synchronous reset
 │   └─ AsyncReset          // asynchronous reset
 ├─ Enum                    // user-defined with encoding
 ├─ Struct                  // user-defined record (flattens to Bits)
 └─ Array<T, N>             // fixed-size array of hardware type

MetaType (compile-time only)
 ├─ uint                    // compile-time unsigned integer
 ├─ int                     // compile-time signed integer
 ├─ bool                    // compile-time boolean
 ├─ string                  // compile-time string
 └─ type                    // type-of-type (for generic type params)

DirectionWrapper (port context only)
 ├─ In<T>                   // input — read-only inside module
 ├─ Out<T>                  // output — write-only inside module
 └─ InOut<T>                // bidirectional — tristate required

ClockDomain<clk, edge>      // type qualifier via @ operator
```

### 2.2 Width Inference Rules

| Operation | Inputs | Result | Rule |
|---|---|---|---|
| `a + b` | UInt\<N\>, UInt\<M\> | UInt\<max(N,M)\> | No implicit widening |
| `a.widen() + b.widen()` | UInt\<N\>, UInt\<M\> | UInt\<max(N,M)+1\> | Explicit widen captures carry |
| `a * b` | UInt\<N\>, UInt\<M\> | UInt\<N+M\> | Full-width multiply |
| `a << K` (const) | UInt\<N\>, const K | UInt\<N+K\> | Static shift widens |
| `a << b` (dynamic) | UInt\<N\>, UInt\<M\> | UInt\<N\> | Dynamic shift preserves width |
| `a ++ b` | Bits\<N\>, Bits\<M\> | Bits\<N+M\> | Concatenation (a=MSB) |
| `a[H:L]` | Bits\<N\> | Bits\<H-L+1\> | Static slice, bounds checked |
| `.truncate<M>()` | UInt\<N\> (N≥M) | UInt\<M\> | Explicit narrowing |
| `.zero_extend<M>()` | UInt\<N\> (M≥N) | UInt\<M\> | Explicit widening |

**Core principle:** Width mismatches are always compile errors. No silent truncation.

### 2.3 Deterministic Struct Bit Layout

Default: MSB-first packing (first field = most significant bits).

Optional explicit bit-range annotations with `@ [H:L]`:

```
struct RiscVInstruction:
    opcode: Bits<7>  @ [6:0]
    rd:     UInt<5>  @ [11:7]
    funct3: Bits<3>  @ [14:12]
    rs1:    UInt<5>  @ [19:15]
    rs2:    UInt<5>  @ [24:20]
    funct7: Bits<7>  @ [31:25]
```

Compiler verifies: contiguous ranges, no overlaps, field width matches range width, total coverage equals struct width. Mixing modes (some fields annotated, some not) is a compile error.

### 2.4 Clock Domain Type Algebra

1. Signals in the same domain combine freely
2. Cross-domain assignment is a compile error
3. Explicit `cdc()` required for crossing
4. Unqualified signals inherit module's default domain
5. Combinational logic is domain-transparent (inherits from inputs; mixed domains = error)
6. Constants and literals are domain-free

### 2.5 Built-in CDC Synchronizer Library

Five compiler intrinsics:

| Method | Use Case | Latency | Constraint |
|---|---|---|---|
| `two_ff_sync` | Single-bit signals | 2 cycles | Multi-bit = compile error |
| `gray_code` | Monotonic counters | 2 cycles | Non-monotonic = warning |
| `async_fifo<depth>` | Streaming data | Variable | Generates full FIFO |
| `handshake` | Multi-bit infrequent | ~4 cycles | High-throughput = warning |
| `pulse_sync` | Single-cycle pulses | 2 cycles | For edge events |

### 2.6 Scoped Type Relaxation (`unchecked` blocks)

```
unchecked:
    result = a + b                     // implicit truncation
    addr_low = full_addr               // implicit truncation
    extended = narrow_val              // implicit zero-extend

// Single-expression form:
result = unchecked(a + b)
```

Rules:
- Implicit truncation: keeps low bits
- Implicit extension: zero-extend for UInt, sign-extend for SInt
- Emits ONE summary warning per block listing all implicit conversions
- Does NOT relax clock domain checks (never relaxable)
- Does NOT relax direction checks (driving inputs still an error)
- Does NOT relax struct ↔ Bits (still requires `.pack()`/`.unpack()`)

### 2.7 Implicit Conversion Rules

**Always allowed:**
- Integer literal → any numeric type (if value fits at compile time)
- Bool ↔ UInt\<1\>
- Enum variant → underlying Bits type (read-only context)

**Requires explicit conversion (outside `unchecked`):**
- UInt ↔ SInt — `.as_signed()` / `.as_unsigned()`
- Wider → narrower — `.truncate<N>()`
- Narrower → wider — `.zero_extend<N>()` / `.sign_extend<N>()`
- Bits ↔ UInt/SInt — `.as_uint()` / `.as_bits()`
- Cross clock domain — `cdc()` primitive
- Struct ↔ Bits — `.pack()` / `.unpack()`

---

## Section 3 — Module System

### 3.1 Module Grammar

```
module NAME [<GENERICS>] ( PORT_LIST ) [@ CLOCK_DOMAIN] :
    BODY

PORT_LIST  := PORT (, PORT)*
PORT       := DIRECTION NAME : TYPE
DIRECTION  := in | out | inout
GENERICS   := GENERIC (, GENERIC)*
GENERIC    := NAME : KIND [= DEFAULT]
KIND       := uint | int | bool | type
```

Body contains: `signal`, `const`, `inst`, `comb`, `reg`, `fsm`, `pipeline`, `gen for/if`, `assert`/`assume`/`cover`, `test` blocks.

### 3.2 Instantiation Syntax

```
inst alu = ALU<32>(
    a      = alu_a,           // = for input connections
    b      = alu_b,
    result -> alu_result,     // -> for output connections
    zero   -> _               // _ discards unused outputs
)
```

All ports must be connected — unconnected port = compile error.

### 3.3 Interface Types

```
interface Stream<T: type>:
    data:  Out<T>
    valid: Out<Bool>
    ready: In<Bool>
```

- `Flip<T>` inverts all In ↔ Out directions
- Dot notation for access: `bus.awvalid`
- Interfaces can nest sub-interfaces

### 3.4 Interface Channel Groups & Partial Binding

```
interface AXI4Lite<ADDR_W: uint, DATA_W: uint>:
    group write_addr:
        awaddr:  Out<UInt<ADDR_W>>
        awvalid: Out<Bool>
        awready: In<Bool>
    group read_data:
        rdata:  In<UInt<DATA_W>>
        rresp:  In<Bits<2>>
        rvalid: In<Bool>
        rready: Out<Bool>
    // ... more groups

// Partial binding:
in bus: Flip<AXI4Lite.{read_addr, read_data}>

// Vectorized interfaces:
in masters: Flip<AXI4Lite>[N_MASTERS]
```

### 3.5 Protocol Assertions on Interfaces

```
interface Stream<T: type>:
    data:  Out<T>
    valid: Out<Bool>
    ready: In<Bool>

    property valid_stability:
        valid and not ready implies next(valid)

    property data_stability:
        valid and not ready implies next(data) == data
```

Any module using the interface automatically inherits these formal checks.

### 3.6 Parametric Formal Verification

```
verify "adder correctness across widths":
    forall W: uint in 1..64:
        inst dut = Adder<W>(...)
        assert dut.sum == (dut.a + dut.b).truncate<W>()
```

### 3.7 Extern Module FFI

```
extern module MMCME2_BASE(
    in  CLKIN1:  Clock,
    in  RST:     Bool,
    out CLKOUT0: Clock,
    out LOCKED:  Bool
) @ verilog("MMCME2_BASE")
```

Generic params map to Verilog `#(.PARAM(val))`. Type-checked at SSL boundary, opaque inside.

### 3.8 Memory Primitives

```
signal mem: Memory<UInt<8>, depth=1024>

signal protected: Memory<UInt<32>, depth=4096,
    resource = BRAM,
    ecc      = secded
>

signal rom: Memory<UInt<32>, depth=1024,
    init     = "bootrom.hex",
    writable = false                    // ROM — compile error if written
>

signal dp: DualPortMemory<UInt<32>, depth=256,
    port_a = read_write,
    port_b = read_only
>
```

Access via `.read(addr=...)` / `.write(addr=..., data=..., enable=...)`. ECC memories expose `.error` and `.uncorrectable` flags.

### 3.9 Conditional Compilation

```
gen if TARGET == "sim":
    // simulation model
gen else:
    // synthesis implementation
```

`TARGET` is a built-in compile-time constant set by `sslc --target sim|synth`.

### 3.10 Visibility & Imports

- Modules private by default within a file
- `pub module` exports for other files
- `import ModuleName from "path/file.ssl"`
- `import { Foo, Bar } from "lib.ssl"`

### 3.11 Attribute System & Doc Comments

**Built-in attributes:** `@use_resource(BRAM)`, `@keep_hierarchy`, `@max_fanout(32)`, `@dont_touch`

**Vendor passthrough:** `@synth("ram_style", "ultra")`, `@synth("loc", "SLICE_X1Y1")`

**Doc comments:** `///` prefix on modules, ports, interfaces. Export to JSON manifest via `sslc --emit docs`.

---

## Section 4 — Clocking & Reset

### 4.1 Clock & Reset Types

```
Clock                                  // unparameterized
Clock<100MHz>                          // with frequency
Clock<100MHz, rising>                  // explicit edge (default: rising)
Clock<100MHz, falling>                 // falling-edge
Clock<100MHz, dual>                    // DDR — both edges

SyncReset                              // synchronous (active high default)
SyncReset<active_low>
AsyncReset                             // asynchronous (active high)
AsyncReset<active_low>
```

### 4.2 Named Clock Domains

```
domain sys  = (sys_clk, sys_rst)
domain fast = (fast_clk, fast_rst)

signal a: UInt<8> @ sys                // belongs to sys domain
signal b: UInt<8> @ fast               // belongs to fast domain
signal c: UInt<8>                      // inherits module's default domain
```

### 4.3 CDC Crossing Rules

1. Cross-domain signal use without `cdc()` = compile error
2. `cdc(signal, from=domain_a, to=domain_b, method=...)` is the only crossing mechanism
3. CDC method must be appropriate for the signal (e.g., `two_ff_sync` on multi-bit = error)

### 4.4 Reset Synchronizer

```
let rst_synced: SyncReset = reset_sync(rst_n, clk,
    stages   = 2,
    polarity = active_high
)
```

Built-in primitive: async assert, synchronous de-assert. Type converts `AsyncReset` → `SyncReset`. Connecting `AsyncReset` to a `SyncReset` port without `reset_sync()` = compile error.

### 4.5 Clock Enable vs Gated Clock

**Clock enable** (preferred for FPGA):
```
reg(clk, rst, enable = not sleep):
    // FFs only update when enabled — same clock domain
```

**Gated clock** (for ASIC / explicit power control):
```
let gated_clk: Clock = clock_gate(clk, enable = not sleep)
// gated_clk IS A NEW DOMAIN — CDC required when mixing with clk
```

### 4.6 Automatic Constraint Generation

The compiler generates timing constraint files from CDC and clock information:

- `sslc --emit sdc design.ssl` → SDC format
- `sslc --emit xdc design.ssl` → Vivado XDC format

Generated constraints per CDC method:
- `two_ff_sync` → `set_false_path` + `set_max_delay`
- `gray_code` → `set_false_path` + `set_bus_skew`
- `async_fifo` → false paths for pointers, max delay for data
- `clock_gate` → `create_generated_clock`

Clock definitions auto-generated from `Clock<freq>` annotations (`create_clock -period ...`).

### 4.7 Compiler CDC Analysis

Built-in checks run automatically:

**Errors:** Cross-domain without `cdc()`, `AsyncReset` → `SyncReset` without `reset_sync()`, `two_ff_sync` on multi-bit bus, gated clock used as parent domain.

**Warnings:** `gray_code` on non-monotonic signal, `handshake` on high-throughput path, multiple related signals using separate CDC when they should share one mechanism.

**Diagnostics (`sslc --cdc-report`):** Full domain map, crossing inventory, latency estimates, visual signal flow between domains.

### 4.8 Clocking Examples

1. Single-clock synchronous design (Blinker)
2. Multi-clock with explicit CDC (ADC interface with `handshake` + `pulse_sync`)
3. Async reset with synchronous de-assert (`reset_sync()` built-in)
4. Clock enable vs gated clock (power-aware processor)
5. Gray-code CDC for counter (FIFO fill-level monitor)
6. Async FIFO CDC crossing (domain bridge with `async_fifo<depth=16>`)

---

## Section 5 — Combinational Logic

### 5.1 Comb Block Safety Guarantees

Enforced as **errors** (not warnings):
1. Every output signal assigned on every path through the block
2. No latch inference — ever
3. `match` must be exhaustive — missing cases = compile error
4. No combinational loops — compiler detects and reports the cycle
5. Sensitivity list is implicit and always complete

### 5.2 Operator Reference

| Category | Operator | Description | Result Width |
|---|---|---|---|
| Arithmetic | `+ - * / %` | Standard ops | See width rules |
| Bitwise | `& \| ^ ~` | AND, OR, XOR, NOT | max(N,M) |
| Shift | `<< >>` | Logical shift | Const: widens; Dynamic: preserves |
| Shift | `>>>` | Arithmetic right shift | Preserves (sign preserved) |
| Concat | `++` | Concatenation (a=MSB) | N+M |
| Slice | `[H:L]` `[i]` | Bit slice, single bit | H-L+1, Bool |
| Reduction | `.and_reduce()` `.or_reduce()` `.xor_reduce()` | Reduce all bits | Bool |
| Comparison | `== != < > <= >=` | Relational | Bool |
| Logical | `and or not` | Bool-only logical ops | Bool |
| Pipe | `\|>` | Pass LHS as first arg to RHS | Return type of RHS |

### 5.3 Encoding Constructs

**`priority:` block** — if/else-if chain, first match wins:
```
priority:
    when condition_a => output = value_a
    when condition_b => output = value_b
    otherwise => output = default
```

**`parallel:` block** — one-hot decode, exactly one match (compiler-verified):
```
parallel(safe = 0):                    // safe fallback if one-hot violated
    when sel[0] => out = inputs[0]
    when sel[1] => out = inputs[1]
```

The `safe` parameter defines behavior when the one-hot invariant is violated in physical hardware:
- `safe = 0` — force output to zero
- `safe = <expr>` — fall back to expression
- `safe = LAST` — hold last valid output (requires register)
- `safe = ERROR` — drive error flag
- Omitted — formal assertion only, no runtime guard

**`match` on enum** — exhaustive, adding a variant forces updates to all match blocks.

### 5.4 Combinational Examples

1. ALU with exhaustive match (RISC-V ALU, 10 operations)
2. Priority encoder (parameterized, `gen for` variant)
3. Barrel shifter (logarithmic stages with `gen for`)
4. Lookup table/ROM (`const` arrays, `gen_table()` for compile-time generation)
5. Mux tree (dynamic array indexing, `parallel:` one-hot mux with `safe` fallback)
6. Complex datapath (pixel pipeline with `|>` pipe operator, `fn` for pure combinational functions)

---

## Section 6 — Sequential Logic & State Machines

### 6.1 Reg Block Grammar

```
reg(CLOCK, RESET [, enable = EXPR]):
    on reset:
        RESET_ASSIGNMENTS
    on tick:
        CLOCKED_LOGIC
```

Compiler verifies: every signal assigned in `on tick` has a corresponding reset value in `on reset`.

### 6.2 FSM Block Grammar

```
fsm NAME(CLOCK, RESET):
    states: VARIANT (| VARIANT)*
    encoding: binary | onehot | gray | custom
    initial: VARIANT

    transitions:
        STATE --(CONDITION)--> STATE [: ACTIONS]
        STATE --timeout(CYCLES)--> STATE [: ACTIONS]
        _ --(CONDITION)--> _           // _ = current state (self-loop)

    on tick:
        LOGIC_EVERY_CYCLE

    outputs:
        STATE => ASSIGNMENTS
```

**FSM Timeout:**
```
WaitResp --timeout(1000)--> Error:
    timeout_flag = true
```

Compiler generates a hidden counter per state with timeout. Counter resets on state entry. Counter width auto-sized from timeout value. Timeout has lowest priority — normal transitions always win. Compiler auto-generates bounded liveness property.

**Compiler verifications:**
- All states reachable from initial state
- All states have at least one exit transition
- All states have output assignments for every output signal
- Encoding matches state count
- No deadlocks (with timeout transitions)

### 6.3 Pipeline Block Grammar

```
pipeline NAME(CLOCK, RESET, backpressure = auto|manual|none):
    input: STREAM_PORT
    output: STREAM_PORT

    stage N ["label"]:
        [stall_when: EXPR]             // manual mode only
        [flush_when: EXPR]             // manual mode only
        LOGIC
```

**Backpressure modes:**

| Mode | Valid/Ready | Stall Logic | Use Case |
|---|---|---|---|
| `auto` | Compiler-generated | Automatic bubble collapse | Data processing, AXI |
| `manual` | Compiler-generated | User `stall_when`/`flush_when` | CPU pipelines with hazards |
| `none` | Valid propagation only | No stalling | Fixed-latency DSP |

Cross-stage signal references are automatically registered by the compiler.

### 6.4 Built-in Primitives

- `UpCounter<W>` / `DownCounter<W>` — basic counters with enable/clear
- `RingCounter<N>` — one-hot rotating counter
- `LFSR<W>` — linear feedback shift register
- `ShiftReg<W, DEPTH>` — parameterized shift register

### 6.5 Sequential Logic Examples

1. D flip-flop with async reset and clock enable
2. Synchronous FIFO using `Stream<T>` interface, generic over type T, with `cover` properties
3. Moore FSM (traffic light) — `fsm` block with `transitions:`, `outputs:`, `on tick:`
4. Mealy FSM (sync word detector) — transition actions for Mealy outputs, `_` wildcard
5. 3-stage multiply pipeline — `backpressure=auto`, auto-generated valid/ready
6. Parameterized counter — load/enable/clear/up-down with configurable max

---

## Section 7 — Test Block Semantics

Test blocks live inside modules and are stripped during synthesis:

```
test "descriptive name":
    drive(port, value)         // set input value
    settle()                   // propagate combinational logic (for comb modules)
    tick()                     // advance one clock cycle
    tick(N)                    // advance N clock cycles
    assert condition           // check condition, fail test if false
    assert condition, "msg"    // with error message
```

Compiled to native binary for fast execution (`sslc --test`). No external simulator required.

---

## Appendix A — Language Semantics Clarifications

This section resolves ambiguities identified during spec review.

### A.1 `Bool` is a Distinct Type with Implicit Coercion

`Bool` is NOT a type alias for `UInt<1>`. It is a distinct type with implicit bidirectional coercion to/from `UInt<1>`. This means:
- `Bool` values can be used where `UInt<1>` is expected, and vice versa
- `Bool` has logical semantics (`and`, `or`, `not`); `UInt<1>` has bitwise semantics (`&`, `|`, `~`)
- They are the same width (1 bit) but different types in the type checker

### A.2 Port Direction Syntax: `in`/`out` Keywords vs `In<T>`/`Out<T>` Wrappers

These are NOT interchangeable. They are used in different contexts:

- **`in`/`out`/`inout` keywords** — used ONLY in module port lists:
  ```
  module Foo(in a: UInt<8>, out b: UInt<8>):
  ```
- **`In<T>`/`Out<T>`/`InOut<T>` wrappers** — used ONLY in interface definitions:
  ```
  interface Stream<T: type>:
      data: Out<T>
      valid: Out<Bool>
      ready: In<Bool>
  ```

The keywords are syntactic sugar for readability in port lists. The wrapper types are required in interfaces because interface signals carry direction as part of their type (needed for `Flip<T>` inversion).

### A.3 Array Syntax Sugar

`T[N]` is syntactic sugar for `Array<T, N>`. Both forms are valid:
```
signal a: UInt<8>[32]          // sugar form (preferred)
signal b: Array<UInt<8>, 32>   // explicit form (allowed)
```

### A.4 `cdc()` Formal Signature

```
cdc<T: type>(
    signal:  T @ DOMAIN_A,
    from:    Domain,
    to:      Domain,
    method:  CdcMethod
) -> T @ DOMAIN_B
```

Where `CdcMethod` is one of: `two_ff_sync`, `gray_code`, `async_fifo<depth=N>`, `handshake`, `pulse_sync`. These are enum-like values passed to the `method` parameter, not standalone functions.

For `async_fifo`, the input and output are `Stream<T>` types (valid/ready handshake is part of the FIFO).

### A.5 `fn` — Pure Combinational Functions

```
fn NAME [<GENERICS>] ( PARAMS ) -> RETURN_TYPE :
    BODY
```

Pure combinational functions:
- No side effects, no signal assignments, no register inference
- Always inlined by the compiler (no function call overhead in hardware)
- Can only be called from within `comb` blocks or other `fn` bodies
- Parameters and return types must be hardware types
- Body is a single expression or indented block of `let` bindings + final expression

```
fn clamp(val: SInt<16>, lo: SInt<16>, hi: SInt<16>) -> SInt<16>:
    if val < lo: lo
    elif val > hi: hi
    else: val
```

### A.6 `group` Grammar in Interfaces

```
interface NAME [<GENERICS>]:
    [group GROUP_NAME:
        SIGNAL_DECL+
    ]*
    [SIGNAL_DECL]*                    // ungrouped signals allowed
    [property PROP_NAME:
        ASSERTION
    ]*
```

Groups are optional organizational blocks within interfaces. Ungrouped signals and grouped signals can coexist. Groups enable partial binding via `.{group1, group2}` syntax.

### A.7 `gen_table()` — Compile-Time Table Generation

```
gen_table<T: type, N: uint>(generator: fn(uint) -> T) -> T[N]
```

Evaluates `generator(i)` for `i` in `0..N` at compile time, producing a constant array. The generator function runs in the compiler's const-evaluation engine and has access to compile-time math functions (`sin`, `cos`, `sqrt`, `log2`, etc.).

```
const TABLE: SInt<8>[256] = gen_table(i =>
    SInt<8>.from_float(sin(2.0 * PI * i / 256) * 127.0)
)
```

### A.8 `if`/`then`/`else` Expression Syntax

`then` is required ONLY in inline (expression) form of `if`:
```
// Expression form (inline ternary) — requires `then`
let x = if cond then value_a else value_b

// Block form (statement) — does NOT use `then`
if cond:
    x = value_a
elif other_cond:
    x = value_b
else:
    x = value_c
```

### A.9 Range Semantics

`a..b` is a **half-open range** — inclusive of `a`, exclusive of `b` (Rust convention):
```
for i in 0..8:     // i = 0, 1, 2, 3, 4, 5, 6, 7
```

`a..=b` is a **closed range** — inclusive of both ends:
```
for i in 0..=7:    // i = 0, 1, 2, 3, 4, 5, 6, 7  (same as 0..8)
```

Bit slices use inclusive syntax with colon: `signal[7:0]` = bits 7 down to 0 (both inclusive, 8 bits). This matches hardware convention.

### A.10 `let` vs `signal` Semantics

| Keyword | Context | Semantics |
|---|---|---|
| `signal` | Module body | Declares a named wire or register (depending on usage in `comb`/`reg`). Visible across the module. |
| `let` | Inside `comb`/`reg`/`fn` blocks | Declares a local intermediate value. Scoped to the block. Does not create a named net in the output netlist. |

`let` is for computation intermediates; `signal` is for module-level wires that may be connected to ports or sub-modules.

### A.11 `.widen()` Method

`.widen()` adds exactly 1 bit to the width:
```
UInt<N>.widen() -> UInt<N+1>    // zero-extends by 1 bit
SInt<N>.widen() -> SInt<N+1>    // sign-extends by 1 bit
```

This is specifically designed for arithmetic: `a.widen() + b.widen()` produces `UInt<max(N,M)+1>`, capturing the carry bit without requiring the user to specify the target width.

### A.12 Built-in Compile-Time Functions

| Function | Signature | Description |
|---|---|---|
| `clog2(n)` | `uint -> uint` | Ceiling log base 2 |
| `is_power_of_2(n)` | `uint -> bool` | True if n is a power of 2 |
| `max(a, b)` | `uint, uint -> uint` | Maximum |
| `min(a, b)` | `uint, uint -> uint` | Minimum |
| `sin(x)`, `cos(x)` | `float -> float` | Trig (for `gen_table` only) |
| `sqrt(x)` | `float -> float` | Square root (for `gen_table` only) |
| `log2(x)` | `float -> float` | Log base 2 (for `gen_table` only) |

Float functions are available only in compile-time const evaluation contexts (e.g., `gen_table` lambdas). They do NOT synthesize to hardware.

### A.13 `type` Alias Syntax

```
type NAME [<GENERICS>] = TYPE_EXPR
```

Creates a type alias. The alias is fully transparent — the compiler treats it as identical to the underlying type:
```
type Word = UInt<32>
type Memory<W: uint, D: uint> = UInt<W>[D]

signal a: Word              // identical to UInt<32>
signal b: Memory<8, 256>    // identical to UInt<8>[256]
```

### A.14 `Flip<T>` Semantics

`Flip<T>` recursively inverts all `In` ↔ `Out` directions within type `T`:

- `Flip<In<T>>` → `Out<T>`
- `Flip<Out<T>>` → `In<T>`
- `Flip<InOut<T>>` → `InOut<T>` (bidirectional unchanged)
- For structs/interfaces: `Flip` recurses into all fields
- Non-directional fields (e.g., `const` parameters) are unchanged
- `Flip<Flip<T>>` = `T` (double flip is identity)

### A.15 Generate-Loop Signal Naming

Signal names in `gen for` blocks use `_{LOOP_VAR}` suffix interpolation:
```
gen for i in 0..4:
    signal partial_{i}: UInt<8>    // creates: partial_0, partial_1, partial_2, partial_3
```

Rules:
- Only loop variable names can appear in `{}`
- Only simple identifiers, no expressions (e.g., `{i+1}` is NOT valid)
- Nested loops: `signal cell_{i}_{j}` is valid
- The interpolation is purely for naming — it creates distinct signals

### A.16 Memory Access Context

`Memory` primitives have two access patterns depending on context:

- **In `comb` block:** `.read(addr=...)` produces combinational (async) read — maps to distributed RAM or LUT
- **In `reg` block:** `.read(addr=...)` produces registered (sync) read — maps to BRAM (1-cycle latency)
- **`.write(addr=..., data=..., enable=...)` must always be in a `reg` block** — writes are always synchronous

The compiler uses the access context to determine whether to infer BRAM or distributed RAM, along with any `@use_resource` attribute.

### A.17 `tick` Keyword — Dual Role

`tick` has two distinct uses determined by context:
- **In `reg` blocks:** `on tick:` is a block label introducing clocked logic (keyword)
- **In `test` blocks:** `tick()` / `tick(N)` is a function call advancing simulation by N clock cycles

These are syntactically unambiguous because `on tick:` only appears after `on reset:` inside `reg` blocks, while `tick()` is a function call with parentheses.

### A.18 Naming: SSL Abbreviation

The abbreviation "SSL" is acknowledged to conflict with Secure Sockets Layer. In documentation, prefer the full name "SiliconScript" or the file extension `.ssl`. The compiler binary is `sslc` (SiliconScript Compiler). If the conflict proves problematic, the extension could be changed to `.sls` in a future revision.

### A.19 AXI4Lite Interface Groups (Corrected)

The complete AXI4Lite interface defines five channel groups:
```
interface AXI4Lite<ADDR_W: uint = 32, DATA_W: uint = 32>:
    group write_addr:    // awaddr, awvalid, awready
    group write_data:    // wdata, wstrb, wvalid, wready
    group write_resp:    // bresp, bvalid, bready
    group read_addr:     // araddr, arvalid, arready
    group read_data:     // rdata, rresp, rvalid, rready
```

The partial binding example `AXI4Lite.{read_addr, read_data}` references both `read_addr` and `read_data` groups.

---

## Appendix B — File Extensions & Tooling

| Artifact | Extension | Command |
|---|---|---|
| Source code | `.ssl` | `sslc compile design.ssl` |
| Constraint file | `.ssc` | Board-level pin/timing constraints |
| Test output | — | `sslc test design.ssl` |
| Verilog output | `.v` | `sslc --emit verilog design.ssl` |
| SDC constraints | `.sdc` | `sslc --emit sdc design.ssl` |
| XDC constraints | `.xdc` | `sslc --emit xdc design.ssl` |
| CDC report | — | `sslc --cdc-report design.ssl` |
| Doc manifest | `.docs.json` | `sslc --emit docs design.ssl` |
| SMT-LIB2 | `.smt2` | `sslc --emit smt2 design.ssl` |
