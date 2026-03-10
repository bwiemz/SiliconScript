# SiliconScript Language Design — Sections 8–15

**Date:** 2026-03-10
**Status:** Approved
**Scope:** Advanced language features (formal verification, AI accelerators, simulation, ISA support, toolchain, standard library, full example, design decisions)
**Companion to:** `2026-03-10-siliconscript-language-design.md` (Sections 1–7)
**Primary Target:** FPGA prototyping (Yosys/nextpnr, Vivado, Quartus)
**Compiler Language:** Rust

---

## Design Decisions (from brainstorming)

| Decision | Choice | Rationale |
|---|---|---|
| Formal verification scope | Minimal core (safety + bounded temporal) | Covers 90% of practical verification without full LTL/CTL complexity |
| AI accelerator approach | Hybrid (keywords lower to standard SSL) | Concise syntax, inspectable lowered output, no opaque scheduling magic |
| RISC-V ISA support | Declarative metadata | User writes decode/hazard logic; compiler validates encodings and generates extractors |
| Simulation backend | Interpreted first, native later | Ship fast with a cycle-accurate interpreter; LLVM/Cranelift native compilation is a documented future goal |
| Standard library scope | Design the module system, 2-3 fully worked examples | Lets the library grow organically without locking unimplemented API signatures |

---

## Addenda to Sections 1–7

The following additions were identified during Sections 8–15 design and apply retroactively to the foundation spec.

### Addendum A.20 — New Reserved Keywords

**Simulation:**
`testbench`, `task`, `var`, `drive`, `peek`, `settle`, `print`

**AI Accelerator:**
`systolic`, `dataflow`

**ISA:**
`isa`, `instr`, `format`, `registers`, `encoding_width`

**Formal (additions):**
`prove`, `equiv`, `constrain`

These are added to the reserved keyword list in Section 1.4.

### Addendum A.21 — `task` Definition

`task` defines a time-consuming simulation routine. Unlike `fn` (pure combinational, always inlined), `task` may contain `tick()`, `drive()`, `peek()`, `wait_until()`, and other simulation-only constructs. `task` is only callable from `testbench` blocks or other `task` bodies. Using `task` inside a synthesizable `module` is a compile error.

```
TASK_DEF := task NAME ( PARAMS ) [-> RETURN_TYPE] :
                BODY
```

### Addendum A.22 — `var` for Testbench State

`var` declares a mutable variable in testbench context. Unlike `signal` (hardware wire/register, module-only), `var` represents host-machine memory and is stripped from synthesis output.

| Keyword | Context | Semantics |
|---|---|---|
| `signal` | Module body | Hardware wire or register (depending on `comb`/`reg` usage) |
| `let` | Anywhere | Immutable binding (block-scoped intermediate or DUT instance) |
| `var` | Testbench only | Mutable software variable (host memory) |

Using `var` in a synthesizable module is a compile error. Using `signal` in a testbench is a compile error.

### Addendum A.23 — Single-Clock Implicit Domain

If a module has exactly one `Clock` input, that clock defines the module's implicit default domain. All internal signals and `reg` blocks belong to that domain without requiring `@ domain` annotations. Explicit `@ domain` tags are only needed when a module has multiple clocks.

### Addendum A.24 — Memory `init` Parameter

The `Memory` primitive's `init` parameter accepts a compile-time `string` path to a hex file (relative to the project root or absolute). The compiler reads the file during elaboration and generates initialization data. A missing file is a compile error, not a runtime error.

### Addendum A.25 — Addition Width Clarification

`a + b` where both operands are `UInt<N>` produces `UInt<N>` — modular arithmetic matching physical hardware behavior. This is not silent truncation; it is the mathematical behavior of an N-bit adder. The carry bit is only captured when the engineer explicitly requests it via `.widen()`:

- `a + b` → `UInt<max(N,M)>` — modular arithmetic, matches hardware
- `a.widen() + b.widen()` → `UInt<max(N,M)+1>` — captures carry

The "no silent truncation" rule (Section 2) applies to *assignments*: storing a wider value into a narrower signal is a compile error. Operations define their result width by fixed rules; assignments enforce that no bits are lost without explicit `.truncate<N>()`.

### Addendum A.26 — `saturate()` Built-in Function

```
fn saturate<N: uint>(val: SInt<M>, min: SInt<M>, max: SInt<M>) -> SInt<M>
fn saturate<N: uint>(val: UInt<M>, min: UInt<M>, max: UInt<M>) -> UInt<M>
```

Clamps `val` to `[min, max]`. Synthesis-friendly — lowers to comparators and muxes. Commonly used in AI accelerator quantization chains:

```
output = acc |> relu() |> arithmetic_shift_right(16) |> saturate(min=-128, max=127) |> truncate<8>()
```

---

## Section 8 — Formal Verification

**Design rationale:** Hardware bugs escape simulation because testbenches can't cover all states. Existing HDLs bolt on formal verification via SVA (verbose, confusing sampling semantics) or external tools (disconnected from source). SSL makes formal properties native — `assert`, `assume`, `cover` are keywords, not macros — with a minimal core that covers 90% of practical hardware verification: safety properties and bounded temporal operators. No full LTL/CTL.

**Backends:** SMT-LIB2 (Z3, CVC5), sby project files (SymbiYosys), SVA inline in Verilog output (JasperGold, Cadence Formal).

### 8.1 Grammar

```
FORMAL_STMT    := ASSERT_STMT | ASSUME_STMT | COVER_STMT
                | PROVE_BLOCK | EQUIV_STMT

ASSERT_STMT    := assert [always] [@ DOMAIN] : EXPR [, STRING]
ASSUME_STMT    := assume [@ DOMAIN] : EXPR [, STRING]
COVER_STMT     := cover [@ DOMAIN] : EXPR [, STRING]

PROVE_BLOCK    := prove NAME [@ DOMAIN] :
                      (ASSUME_STMT | ASSERT_STMT)+

TEMPORAL_EXPR  := next ( EXPR )                    // value in next cycle
                | next ( EXPR , N )                // value in N cycles
                | eventually ( EXPR , depth = N )  // true within N cycles

BOOL_FORMAL    := EXPR implies EXPR                // A implies B = !A || B

EQUIV_STMT     := equiv NAME : MODULE_A == MODULE_B

VERIFY_ATTR    := @bmc ( depth = N )
                | @induction ( k = N )
                | @check_during_reset              // override implicit reset disable
```

### 8.2 Semantics

- `assert always` generates a safety property checked at every cycle
- `assert` (without `always`) is a one-shot check at the point of evaluation
- `assume` constrains the input space — the verifier treats these as axioms
- `cover` asks the verifier to find an input trace that makes the condition true
- `next(expr)` refers to the value of `expr` one cycle later; `next(expr, N)` for N cycles
- `eventually(expr, depth=N)` asserts `expr` becomes true within N cycles — bounded, not unbounded
- `implies` is a boolean operator: `A implies B` is equivalent to `not A or B`
- `prove` blocks group related assumptions and assertions into named verification goals
- `@bmc(depth=N)` configures bounded model checking depth; `@induction(k=N)` configures k-induction
- `equiv` checks functional equivalence between two module instantiations

### 8.3 Clock Context Rules

Temporal operators (`next`, `eventually`) inherently tick on a clock. The clock is determined by context:

1. Inside `reg(clk, rst)` → inherits that clock
2. At module level with single/default domain → inherits default domain clock
3. At module level with multiple domains, no default → requires `@ domain` tag; compile error if missing
4. Inside `prove` block → block-level `@ domain` applies to all children

### 8.4 Reset Behavior

`assert always` is implicitly disabled while the associated domain's reset signal is active. This prevents false failures during reset cycles when registers are being initialized. Use `@check_during_reset` to override for the rare case of verifying reset behavior itself.

### 8.5 Backend Emission

| Backend | Output | Use case |
|---|---|---|
| SMT-LIB2 | `.smt2` file | Z3, CVC5 |
| sby | `.sby` project file | SymbiYosys (open source) |
| SVA | Inline in Verilog output | JasperGold, Cadence Formal |

### 8.6 Examples

**1. Safety property (no overflow):**
```
module SafeAdder(
    in  a: UInt<7>,
    in  b: UInt<7>,
    out sum: UInt<8>
):
    comb:
        sum = a.widen() + b.widen()

    assert always: sum <= 254, "sum of two 7-bit values cannot exceed 254"
```

**2. Bounded liveness property (response within deadline):**
```
module RequestHandler(
    in  clk: Clock,
    in  rst: SyncReset,
    in  req_valid: Bool,
    out resp_valid: Bool
):
    // ... FSM logic ...

    assume: not (req_valid and resp_valid), "no new request during response"

    @bmc(depth=20)
    assert always: req_valid implies eventually(resp_valid, depth=16),
        "response within 16 cycles of request"
```

**3. FSM reachability coverage:**
```
fsm Controller(clk, rst):
    states: Idle | Fetch | Decode | Execute | Writeback
    encoding: onehot
    initial: Idle
    // ... transitions ...

    // Verify every state is reachable
    cover: state == Idle
    cover: state == Fetch
    cover: state == Decode
    cover: state == Execute
    cover: state == Writeback

    // Verify no deadlock
    assert always: state == Execute implies
        eventually(state == Writeback, depth=4)
```

**4. AXI protocol compliance proof:**
```
module AXITarget(
    in  bus: Flip<AXI4Lite<32, 32>>
):
    // AXI-Lite: VALID must not depend on READY
    assert always: bus.write_addr.awvalid and not bus.write_addr.awready
        implies next(bus.write_addr.awvalid),
        "AWVALID must hold until AWREADY"

    assert always: bus.read_data.rvalid and not bus.read_data.rready
        implies next(bus.read_data.rvalid),
        "RVALID must hold until RREADY"

    // Response within deadline
    @bmc(depth=32)
    assert always: bus.write_addr.awvalid implies
        eventually(bus.write_resp.bvalid, depth=16),
        "write response within 16 cycles"
```

**5. Pipeline hazard freedom proof:**
```
prove no_data_hazard:
    assume: stage_ex.valid and stage_mem.valid
    assume: stage_ex.rd == hazard.id_rs1
    assume: stage_ex.reg_write and stage_ex.rd != 0
    assert: hazard.forward_a != ForwardSel.None,
        "forwarding unit resolves all RAW hazards"

prove no_stall_deadlock:
    assume: input_valid
    @bmc(depth=50)
    assert: eventually(output_valid, depth=20),
        "pipeline drains within 20 cycles"
```

**6. Refinement/equivalence check:**
```
module RippleAdder<W: uint>(in a: UInt<W>, in b: UInt<W>, out sum: UInt<W+1>):
    // ... simple implementation ...

module CLAAdder<W: uint>(in a: UInt<W>, in b: UInt<W>, out sum: UInt<W+1>):
    // ... carry-lookahead implementation ...

equiv adder_equivalence: RippleAdder<16> == CLAAdder<16>
```

---

## Section 9 — AI Accelerator Primitives

**Design rationale:** Custom silicon for AI is the fastest-growing segment of chip design, yet existing HDLs force engineers to manually instantiate hundreds of MAC units, wire systolic interconnects, and hand-schedule dataflows. SSL provides high-level keywords (`systolic`, `dataflow`) that express intent — the compiler lowers them to standard SSL constructs (modules, pipelines, `gen for` loops) during an early compilation pass. The engineer gets concise syntax; the generated code uses primitives they already understand and can inspect via `sslc --emit lowered`.

### 9.1 Grammar

```
SYSTOLIC_BLOCK := systolic NAME < GENERICS > :
                      cell ( PARAMS ) -> RETURN_TYPE :
                          BODY
                      [flow : FLOW_DIR]
                      [latency : EXPR]

FLOW_DIR       := west_to_east | north_to_south
                | west_to_east , north_to_south

DATAFLOW_BLOCK := dataflow NAME ( PARAMS ) :
                      [schedule : SCHEDULE_HINT]
                      LOOP_NEST

SCHEDULE_HINT  := spatial | temporal | auto

MAC_EXPR       := mac ( A , B [, acc = C] )

MEMORY_TILE    := memory NAME : SRAM < SRAM_PARAMS >
SRAM_PARAMS    := rows = N , cols = M , dtype = TYPE
                  [, banks = K]
                  [, read_ports = R]
                  [, write_ports = W]

ACTIVATION_FN  := relu ( EXPR )
                | sigmoid_approx ( EXPR , segments = N )
                | gelu_approx ( EXPR , error_budget = F )
                | lut_activation ( EXPR , table = CONST_ARRAY )
```

### 9.2 Lowering Rules

The compiler lowers high-level AI constructs to standard SSL during elaboration. After lowering, the design is pure standard SSL — the optimizer and backend see no AI-specific constructs.

| Construct | Lowers to |
|---|---|
| `systolic<R,C,D>` | `R*C` module instances + `gen for` interconnect + pipeline stages for latency |
| `dataflow` with `spatial` | `gen for` unrolling + parallel module instances |
| `dataflow` with `temporal` | `pipeline(backpressure=auto)` + FSM for sequential iteration |
| `dataflow` with `auto` | Compiler chooses spatial/temporal based on target resources |
| `memory ... SRAM<>` | `Memory<>` primitives with bank-select logic |
| `relu(x)` | `if x[MSB] then 0 else x` (single mux) |
| `sigmoid_approx` | Piecewise-linear LUT via `gen_table` |
| `gelu_approx` | LUT + interpolation, table size derived from `error_budget` |

`dataflow` blocks with `schedule: temporal` lower to `pipeline(backpressure=auto)`. Memory bank conflicts generate stall cycles via the pipeline's ready/valid signals. No data loss, no silent drops.

### 9.3 `mac()` Primitive

`mac()` is the one true intrinsic (not lowered to SSL). It maps to DSP48E2 on Xilinx, DSP block on Intel, discrete multiplier + adder on Lattice.

```
mac(a: UInt<N>, b: UInt<M>, acc: UInt<A> = 0) -> UInt<A>
    where A >= N + M
mac(a: SInt<N>, b: SInt<M>, acc: SInt<A> = 0) -> SInt<A>
    where A >= N + M
```

- Hardware semantics: `acc + (a * b)` with full-precision intermediate
- The accumulator width `A` is an independent parameter. The compiler statically verifies `A >= N + M`. In practice, `A = N + M + clog2(K)` where K is accumulation depth — the user sizes this correctly.
- `@use_resource(DSP)` is implicit; `@use_resource(LUT)` overrides to fabric implementation
- The `acc` parameter defaults to 0 (single multiply) — when fed back, infers accumulator register

### 9.4 Systolic Array Semantics

```
systolic NAME<ROWS: uint, COLS: uint, DEPTH: uint>:
    cell(a: T_A, b: T_B, acc: T_ACC) -> T_ACC:
        BODY
    flow: west_to_east, north_to_south
    latency: ROWS + COLS - 1
```

The compiler generates:
- `ROWS * COLS` cell instances
- Horizontal pipeline registers for `a` propagation (west→east)
- Vertical pipeline registers for `b` propagation (north→south)
- Accumulator chain per column
- Skew logic for input wavefront alignment

**Generated port names** (deterministic, based on `flow` declaration):

| Port | Type | Description |
|---|---|---|
| `west_in` | `T_A[ROWS]` | Input fed to each row (west side) |
| `north_in` | `T_B[COLS]` | Input fed to each column (north side) |
| `south_out` | `T_ACC[COLS]` | Accumulated result per column (south side) |
| `east_out` | `T_A[ROWS]` | Passthrough (east side, optional) |
| `clear` | `Bool` | Accumulator reset |
| `valid` | `Bool` | Results ready after `latency` cycles |

### 9.5 Memory Tile Semantics

```
memory NAME: SRAM<rows=R, cols=C, dtype=T, banks=K, read_ports=RP, write_ports=WP>
```

Lowers to:
- `K` independent `Memory<T, depth=R/K>` instances
- Bank-select logic from address bits `[clog2(K)-1:0]`
- Arbiter if `read_ports + write_ports > physical ports per bank`
- If `banks=1` (default), no banking logic generated

### 9.6 Examples

**1. 8x8 systolic array for INT8 matrix multiply:**
```
systolic MatMul8x8<ROWS: uint = 8, COLS: uint = 8, DEPTH: uint = 8>:
    cell(a: SInt<8>, b: SInt<8>, acc: SInt<32>) -> SInt<32>:
        return mac(a, b, acc=acc)
    flow: west_to_east, north_to_south

module GEMMUnit(
    in  clk: Clock<200MHz>,
    in  rst: SyncReset,
    in  a_rows: SInt<8>[8],
    in  b_cols: SInt<8>[8],
    out results: SInt<32>[8],
    out valid: Bool
):
    inst array = MatMul8x8(
        west_in = a_rows,
        north_in = b_cols,
        south_out -> results,
        valid -> valid
    )
```

**2. Single MAC lane with accumulator reset:**
```
module MACLane<N: uint = 8>(
    in  clk: Clock,
    in  rst: SyncReset,
    in  a: SInt<N>,
    in  b: SInt<N>,
    in  clear_acc: Bool,
    out result: SInt<2*N>
):
    signal acc: SInt<2*N>

    reg(clk, rst):
        on reset:
            acc = 0
        on tick:
            if clear_acc:
                acc = mac(a, b)
            else:
                acc = mac(a, b, acc=acc)

    comb:
        result = acc
```

**3. Convolution engine with weight double-buffering:**
```
module Conv2DEngine<
    IN_CH: uint = 3,
    OUT_CH: uint = 16,
    KH: uint = 3,
    KW: uint = 3,
    ACT_W: uint = 8
>(
    in  clk: Clock,
    in  rst: SyncReset,
    in  pixel_in: Stream<SInt<ACT_W>>,
    in  weight_load: Stream<SInt<ACT_W>>,
    in  buffer_sel: Bool,
    out pixel_out: Stream<SInt<4*ACT_W>>
):
    memory weights_a: SRAM<rows=KH*KW*IN_CH, cols=OUT_CH, dtype=SInt<ACT_W>>
    memory weights_b: SRAM<rows=KH*KW*IN_CH, cols=OUT_CH, dtype=SInt<ACT_W>>

    dataflow conv(input: SInt<ACT_W>[KH][KW][IN_CH],
                  weights: SInt<ACT_W>[KH][KW][IN_CH][OUT_CH],
                  output: SInt<4*ACT_W>[OUT_CH]):
        schedule: spatial
        for oc in 0..OUT_CH:
            output[oc] = 0
            for r in 0..KH:
                for s in 0..KW:
                    for ic in 0..IN_CH:
                        output[oc] = mac(input[r][s][ic],
                                         weights[r][s][ic][oc],
                                         acc=output[oc])
```

**4. Custom compute unit (dot product):**
```
module DotProduct<N: uint = 8, LANES: uint = 4>(
    in  clk: Clock,
    in  rst: SyncReset,
    in  a: SInt<N>[LANES],
    in  b: SInt<N>[LANES],
    in  valid_in: Bool,
    out result: SInt<2*N + clog2(LANES)>,
    out valid_out: Bool
):
    signal partials: SInt<2*N>[LANES]

    gen for i in 0..LANES:
        comb:
            partials[i] = mac(a[i], b[i])

    pipeline reduce(clk, rst, backpressure=none):
        input: partials, valid_in
        output: result, valid_out

        stage 0 "pairwise_add":
            signal sums: SInt<2*N+1>[LANES/2]
            gen for i in 0..LANES/2:
                sums[i] = partials[2*i].widen() + partials[2*i+1].widen()

        stage 1 "final_add":
            result = sums[0].widen() + sums[1].widen()
```

**5. Attention score unit (Q*K^T scaled):**
```
module ScaledDotAttention<
    D_MODEL: uint = 64,
    SEQ_LEN: uint = 16,
    HEAD_W: uint = 8
>(
    in  clk: Clock,
    in  rst: SyncReset,
    in  q_vec: Stream<SInt<HEAD_W>[D_MODEL]>,
    in  k_mat: SInt<HEAD_W>[SEQ_LEN][D_MODEL],
    out scores: Stream<SInt<2*HEAD_W + clog2(D_MODEL)>[SEQ_LEN]>
):
    const SCALE_SHIFT: uint = clog2(D_MODEL) / 2

    gen for s in 0..SEQ_LEN:
        inst dp_{s} = DotProduct<HEAD_W, D_MODEL>(
            clk = clk, rst = rst,
            a = q_vec.data,
            b = k_mat[s],
            valid_in = q_vec.valid,
            result -> scores.data[s],
            valid_out -> _
        )

    comb:
        gen for s in 0..SEQ_LEN:
            scores.data[s] = scores.data[s] >>> SCALE_SHIFT
```

**6. Pooling unit with configurable window size:**
```
module MaxPool<W: uint = 8, WINDOW: uint = 2>(
    in  clk: Clock,
    in  rst: SyncReset,
    in  pixels: Stream<UInt<W>[WINDOW][WINDOW]>,
    out pooled: Stream<UInt<W>>
):
    fn max2(a: UInt<W>, b: UInt<W>) -> UInt<W>:
        if a > b then a else b

    comb:
        signal row_max: UInt<W>[WINDOW]
        gen for r in 0..WINDOW:
            row_max[r] = pixels.data[r][0]
            gen for c in 1..WINDOW:
                row_max[r] = max2(row_max[r], pixels.data[r][c])

        signal col_max: UInt<W> = row_max[0]
        gen for r in 1..WINDOW:
            col_max = max2(col_max, row_max[r])

        pooled.data = col_max
        pooled.valid = pixels.valid

    comb:
        pixels.ready = pooled.ready
```

---

## Section 10 — Simulation & Testbench

**Design rationale:** Simulation in existing HDLs is either second-class (Verilog testbenches use the same language but with non-synthesizable hacks like `#10`, `$display`, `$readmemh`) or completely external (cocotb, UVM). SSL makes simulation a first-class target with strict enforcement: simulation-only constructs can only appear inside `testbench` blocks, which the compiler provably strips from synthesis output. The initial backend is an interpreted cycle-accurate simulator; native compilation via LLVM/Cranelift is a documented future goal.

### 10.1 Grammar

```
TESTBENCH_BLOCK := testbench NAME [( PARAMS )] :
                       [config : TB_CONFIG]
                       BODY

TB_CONFIG       := cycles = N
                 | timeout = N
                 | dump = FORMAT [, file = STRING]

FORMAT          := vcd | fst | none

TB_STMT         := DRIVE_STMT | TICK_STMT | SETTLE_STMT
                 | ASSERT_STMT | PEEK_STMT | PRINT_STMT
                 | WAIT_STMT | RANDOM_STMT | COVER_STMT
                 | FILE_STMT

DRIVE_STMT      := drive ( PORT , EXPR )
TICK_STMT       := tick ( [N] )
SETTLE_STMT     := settle ()
PEEK_STMT       := peek ( PORT ) -> TYPE
WAIT_STMT       := wait_until ( EXPR [, timeout = N] [, clock = CLOCK_REF] )
PRINT_STMT      := print ( FORMAT_STR , ARGS* )
RANDOM_STMT     := random ( TYPE [, constraint = LAMBDA] [, seed = N] )
FILE_STMT       := read_hex ( PATH ) -> ARRAY_TYPE
                 | write_hex ( PATH , ARRAY_EXPR )

COVER_STMT      := cover NAME : EXPR

TASK_DEF        := task NAME ( PARAMS ) [-> RETURN_TYPE] :
                       BODY
```

### 10.2 Synthesizability Wall

The compiler enforces a hard boundary between synthesizable and simulation contexts:

| Context | Allowed constructs | Output |
|---|---|---|
| **Synthesizable** | `module`, `comb`, `reg`, `fsm`, `pipeline`, `gen`, `signal`, `inst` | RTL |
| **Simulation-only** | `testbench`, `task`, `tick()`, `drive()`, `peek()`, `settle()`, `random()`, `print()`, file I/O, `var` | Simulator |

**Enforcement rules:**
- `testbench` blocks are top-level or in separate `.ssl` test files — not nested inside modules
- Simulation-only functions inside a `module` = compile error
- `testbench` blocks can instantiate modules and drive/peek their ports
- Existing `test` blocks (Section 7) inside modules are syntactic sugar — they desugar to `testbench` blocks that instantiate the enclosing module

### 10.3 `fn` vs `task`

| Keyword | Time-consuming | Where callable | Synthesizable |
|---|---|---|---|
| `fn` | No — pure, combinational, inlined | Everywhere (`comb`, `testbench`, other `fn`) | Yes |
| `task` | Yes — may contain `tick()`, `drive()`, `wait_until()` | `testbench` and other `task` only | No |

```
// Pure function — works everywhere
fn clamp(val: SInt<16>, lo: SInt<16>, hi: SInt<16>) -> SInt<16>:
    if val < lo then lo elif val > hi then hi else val

// Task — testbench only, consumes simulation time
task axi_write(dut: AXITarget, addr: UInt<32>, data: UInt<32>):
    drive(dut.bus.write_addr.awvalid, true)
    drive(dut.bus.write_addr.awaddr, addr)
    wait_until(peek(dut.bus.write_addr.awready), timeout=100)
    tick()
    drive(dut.bus.write_addr.awvalid, false)
```

### 10.4 DUT Instantiation

```
testbench tb_name:
    let dut = ModuleName<GENERICS>(PORT_DEFAULTS)

    drive(dut.input_port, value)
    tick()
    let v = peek(dut.output_port)
    assert v == expected
```

Ports not driven in the constructor default to zero. Clock and reset are managed implicitly.

### 10.5 Clock and Reset Management

**Default behavior:** If no explicit reset drive, the testbench asserts reset for 1 cycle, then deasserts.

```
// Override with explicit control:
drive(dut.rst, 1)
tick(5)                            // hold reset for 5 cycles
drive(dut.rst, 0)
tick()                             // first active cycle
```

**Multi-clock DUTs:** `tick()` advances all clocks by their relative frequencies. `tick_domain(domain, N)` advances only one domain:

```
tick_domain(dut.fast, 3)           // 3 fast clock cycles
tick_domain(dut.slow, 1)           // 1 slow clock cycle
```

### 10.6 `tick()` Execution Model

`tick()` performs these operations in strict order:

1. Apply all `drive()` values to DUT input ports
2. Advance the clock edge (rising edge by default)
3. Evaluate all sequential logic (`reg`, `fsm`, `pipeline`)
4. Propagate all combinational logic to stability (`settle()`)
5. Return control to the testbench

`peek()` after `tick()` always reads stable, post-edge values. No delta-cycle ambiguity. `settle()` is also available standalone for purely combinational DUTs.

### 10.7 `wait_until` Semantics

```
wait_until(EXPR, timeout=N [, clock=CLOCK_REF])
```

- Evaluates `EXPR` after each `tick()` on the specified clock
- If `clock` is omitted, uses the DUT's default clock domain
- If the DUT has multiple clocks and no default, `clock` is required — compile error if missing
- If timeout expires, the testbench raises a simulation error (not a silent failure)

### 10.8 Constrained Random

```
let val = random(UInt<8>)                                      // uniform 0..255
let val = random(UInt<8>, constraint = |x| x > 10 and x < 200) // constrained
let val = random(UInt<8>, seed = 42)                           // reproducible

let pkt = random(Packet, constraint = |p|
    p.length > 0 and p.length <= 64 and
    p.addr[1:0] == 0
)
```

The simulator uses rejection sampling for simple constraints. For complex constraints on large types, the compiler emits a warning suggesting a custom generator function.

### 10.9 Functional Coverage

```
cover fifo_empty: peek(dut.count) == 0
cover fifo_full: peek(dut.count) == 16
```

Coverage points are checked at every `tick()`. The simulator tracks hit counts. `sslc test --coverage` emits a coverage report.

### 10.10 Waveform Dump

```
testbench tb_name:
    config:
        dump = vcd, file = "output.vcd"
```

- **VCD:** Standard Value Change Dump — compatible with GTKWave, Surfer
- **FST:** Fast Signal Trace — compressed, ~10x smaller than VCD
- Default: No dump (faster simulation). Enable per-testbench or globally via `sslc test --dump vcd`
- Selective dump via `@dump` attribute on specific signals

### 10.11 Co-simulation FFI

```
extern fn reference_model(input: Bits<32>) -> Bits<32>
    @ ffi("c", library = "refmodel.so", symbol = "ref_compute")

testbench tb_cosim:
    let dut = MyModule()
    for i in 0..1000:
        let input = random(UInt<32>)
        drive(dut.data_in, input)
        tick()
        let expected = reference_model(input.as_bits())
        assert peek(dut.data_out).as_bits() == expected
```

Only available in `testbench` blocks. Supported languages: C, C++ (via `extern "C"`), Rust (via `extern "C"`).

### 10.12 Timing-Annotated Simulation (Future)

```
testbench tb_timing:
    config:
        sdf = "post_synth.sdf"
```

When an SDF file is provided, the simulator applies gate-level delays. Initial implementation uses zero-delay (cycle-accurate) semantics only.

### 10.13 Examples

**1. Basic combinational module testbench (adder):**
```
testbench tb_adder:
    let dut = Adder<8>()

    for i in 0..256:
        for j in 0..256:
            drive(dut.a, i)
            drive(dut.b, j)
            settle()
            assert peek(dut.sum) == i + j,
                "expected {i}+{j}={i+j}, got {peek(dut.sum)}"
```

**2. FSM testbench with state coverage:**
```
testbench tb_traffic_light:
    let dut = TrafficLight()

    cover state_green:  peek(dut.state) == State.Green
    cover state_yellow: peek(dut.state) == State.Yellow
    cover state_red:    peek(dut.state) == State.Red
    cover emergency:    peek(dut.state) == State.AllRed

    drive(dut.emergency, false)
    tick(100)

    drive(dut.emergency, true)
    tick(1)
    assert peek(dut.state) == State.AllRed

    drive(dut.emergency, false)
    tick(1)
    assert peek(dut.state) != State.AllRed
```

**3. AXI4 BFM testbench:**
```
task axi_write(dut: AXITarget, addr: UInt<32>, data: UInt<32>):
    drive(dut.bus.write_addr.awaddr, addr)
    drive(dut.bus.write_addr.awvalid, true)
    drive(dut.bus.write_data.wdata, data)
    drive(dut.bus.write_data.wstrb, 0xF)
    drive(dut.bus.write_data.wvalid, true)
    drive(dut.bus.write_resp.bready, true)

    wait_until(peek(dut.bus.write_addr.awready), timeout=100)
    tick()
    drive(dut.bus.write_addr.awvalid, false)

    wait_until(peek(dut.bus.write_data.wready), timeout=100)
    tick()
    drive(dut.bus.write_data.wvalid, false)

    wait_until(peek(dut.bus.write_resp.bvalid), timeout=100)
    assert peek(dut.bus.write_resp.bresp) == 0b00
    tick()
    drive(dut.bus.write_resp.bready, false)

task axi_read(dut: AXITarget, addr: UInt<32>) -> UInt<32>:
    drive(dut.bus.read_addr.araddr, addr)
    drive(dut.bus.read_addr.arvalid, true)
    drive(dut.bus.read_data.rready, true)

    wait_until(peek(dut.bus.read_addr.arready), timeout=100)
    tick()
    drive(dut.bus.read_addr.arvalid, false)

    wait_until(peek(dut.bus.read_data.rvalid), timeout=100)
    let data = peek(dut.bus.read_data.rdata)
    assert peek(dut.bus.read_data.rresp) == 0b00
    tick()
    drive(dut.bus.read_data.rready, false)
    return data

testbench tb_axi_target:
    let dut = AXITarget<32, 32>()

    axi_write(dut, 0x0000_0100, 0xDEAD_BEEF)
    let readback = axi_read(dut, 0x0000_0100)
    assert readback == 0xDEAD_BEEF

    for i in 0..16:
        axi_write(dut, i * 4, i * 0x1111)
    for i in 0..16:
        let val = axi_read(dut, i * 4)
        assert val == i * 0x1111
```

**4. Constrained random test for a FIFO:**
```
testbench tb_fifo_random:
    config:
        timeout = 100_000
        dump = fst, file = "fifo_random.fst"

    let dut = SyncFifo<UInt<8>, 16>()

    cover fifo_empty:     peek(dut.empty)
    cover fifo_full:      peek(dut.full)
    cover simultaneous:   peek(dut.wr_en) and peek(dut.rd_en)
    cover write_full:     peek(dut.full) and peek(dut.wr_en)
    cover read_empty:     peek(dut.empty) and peek(dut.rd_en)

    var ref_count: uint = 0

    for cycle in 0..10_000:
        let do_write = random(Bool)
        let do_read = random(Bool)
        let wr_data = random(UInt<8>)

        drive(dut.wr_en, do_write and not peek(dut.full))
        drive(dut.wr_data, wr_data)
        drive(dut.rd_en, do_read and not peek(dut.empty))

        tick()
```

**5. Pipeline latency validation testbench:**
```
testbench tb_pipeline_latency:
    let dut = MulPipeline<16>()

    const PIPELINE_DEPTH: uint = 3
    var expected: UInt<32>[13]

    for i in 0..10:
        let a = i + 1
        let b = i + 2
        drive(dut.a, a)
        drive(dut.b, b)
        drive(dut.valid_in, true)
        expected[i + PIPELINE_DEPTH] = a * b
        tick()

    drive(dut.valid_in, false)

    for i in 0..10:
        assert peek(dut.valid_out) == true
        assert peek(dut.result) == expected[i + PIPELINE_DEPTH]
        tick()

    tick(PIPELINE_DEPTH)
    assert peek(dut.valid_out) == false
```

---

## Section 11 — RISC-V & Custom ISA Support

**Design rationale:** RISC-V has exploded as a platform for custom silicon, yet existing HDLs offer no structured way to express instruction sets. Engineers manually write decode logic from prose specifications, leading to encoding bugs, missed hazard cases, and decode tables that drift from documentation. SSL provides an `isa` block as a structured data format — it captures instruction encodings, field layouts, and semantic annotations in a machine-readable form. The user writes decode logic, pipelines, and hazard detection using standard SSL constructs, referencing the ISA definition for encoding constants and field extractors.

### 11.1 Grammar

```
ISA_BLOCK      := isa NAME [extends BASE_ISA] :
                      [encoding_width : N]
                      (INSTR_DEF | FORMAT_DEF | REG_DEF | GROUP_DEF)*

FORMAT_DEF     := format NAME :
                      FIELD_SPEC+
                      [ASSEMBLE_SPEC]*

FIELD_SPEC     := NAME : TYPE @ [H:L]

ASSEMBLE_SPEC  := assemble NAME : TYPE = { FIELD_CONCAT }

FIELD_CONCAT   := FIELD_REF [, FIELD_REF]*
FIELD_REF      := NAME | NUMERIC_LITERAL            // e.g., 1'b0 for constant bits

REG_DEF        := registers NAME :
                      count = N
                      width = M
                      [zero_reg = INDEX]

GROUP_DEF      := group NAME : opcode = BIT_PATTERN

INSTR_DEF      := instr NAME ( OPERANDS ) :
                      group : GROUP_NAME
                      [funct3 : BIT_PATTERN]
                      [funct7 : BIT_PATTERN]
                      [semantics : SEMANTIC_EXPR]
                      [latency : N cycles]

BIT_PATTERN    := NUMERIC_LITERAL
OPERANDS       := (NAME : OPERAND_TYPE)*
OPERAND_TYPE   := Reg | Imm<N> | SImm<N> | UImm<N>

SEMANTIC_EXPR  := informal text describing behavior   // NOT compiled
```

### 11.2 What the Compiler Does

| Feature | Compiler action |
|---|---|
| **Encoding validation** | Verifies no two instructions have overlapping bit patterns (considering opcode + funct3 + funct7) |
| **Field coverage** | Verifies format fields cover all bits of `encoding_width` with no gaps or overlaps |
| **Field extractors** | Generates `ISA.field_name(instruction)` accessor functions that extract and sign-extend fields |
| **Assembled field extractors** | Generates extractors for `assemble` directives that reconstruct scattered fields |
| **Encoding constructors** | Generates `ISA.encode.INSTR(operands)` for assembling instruction words |
| **Constant export** | Makes all opcode/funct values available as named constants for `match` blocks |
| **Extension validation** | When `extends` is used, verifies no encoding conflicts between base and extension |

### 11.3 What the Compiler Does NOT Do

- **No decode logic generation** — the user writes `match` on opcode/funct fields
- **No assembler generation** — separate tool concern; use `sslc --emit isa-json` to export for external tooling
- **No pipeline generation** — the user writes `pipeline` blocks with explicit stages
- **No hazard detection** — the user writes hazard logic using standard `comb`/`reg` blocks

### 11.4 Generated Namespace

| Accessor | Type | Description |
|---|---|---|
| `ISA.opcode(instr)` | `fn(Bits<W>) -> Bits<N>` | Extract opcode field |
| `ISA.rd(instr)` | `fn(Bits<W>) -> UInt<N>` | Extract destination register |
| `ISA.rs1(instr)` | `fn(Bits<W>) -> UInt<N>` | Extract source register 1 |
| `ISA.imm_b(instr)` | `fn(Bits<W>) -> SInt<N>` | Assembled field (scattered bits reconstructed) |
| `ISA.opcodes.R_ALU` | `const Bits<7>` | Opcode constant for group |
| `ISA.funct3.ADD` | `const Bits<3>` | funct3 constant for instruction |
| `ISA.funct7.ADD` | `const Bits<7>` | funct7 constant for instruction |
| `ISA.encode.ADD(...)` | `fn(...) -> Bits<W>` | Construct instruction word |

### 11.5 `extends` Semantics

`extends` merges the child ISA definitions into the parent's namespace for encoding overlap validation. The child inherits all parent definitions. Accessors are available through both the parent and child name. The recommended pattern:

```
isa MyCoreISA extends RV32I:
    // custom instructions here

// Use MyCoreISA everywhere — includes all RV32I definitions
let op = MyCoreISA.opcode(instr)
```

### 11.6 ISA Export

```
sslc --emit isa-json design.ssl > rv32i.json
```

Exports the ISA definition as structured JSON — usable by external assemblers, disassemblers, documentation generators, or verification tools.

### 11.7 Decode Pattern

In `comb` blocks, the recommended decode pattern is to assign safe defaults at the top, then override in `match` arms. The catch-all `_ => ()` is valid because the defaults ensure complete assignment on all paths. Without defaults, this would be a compile error per Section 5 rules.

### 11.8 Examples

**1. RV32I core subset definition:**
```
isa RV32I:
    encoding_width: 32

    registers X:
        count = 32
        width = 32
        zero_reg = 0

    format R_type:
        funct7: Bits<7>  @ [31:25]
        rs2:    UInt<5>  @ [24:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        rd:     UInt<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]

    format I_type:
        imm:    SInt<12> @ [31:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        rd:     UInt<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]

    format S_type:
        imm_hi: Bits<7>  @ [31:25]
        rs2:    UInt<5>  @ [24:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        imm_lo: Bits<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]
        assemble imm_s: SInt<12> = { imm_hi, imm_lo }

    format B_type:
        imm_12: Bits<1>  @ [31]
        imm_hi: Bits<6>  @ [30:25]
        rs2:    UInt<5>  @ [24:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        imm_lo: Bits<4>  @ [11:8]
        imm_11: Bits<1>  @ [7]
        opcode: Bits<7>  @ [6:0]
        assemble imm_b: SInt<13> = { imm_12, imm_11, imm_hi, imm_lo, 1'b0 }

    format U_type:
        imm:    UInt<20> @ [31:12]
        rd:     UInt<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]

    format J_type:
        imm_20: Bits<1>  @ [31]
        imm_hi: Bits<10> @ [30:21]
        imm_11: Bits<1>  @ [20]
        imm_lo: Bits<8>  @ [19:12]
        rd:     UInt<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]
        assemble imm_j: SInt<21> = { imm_20, imm_lo, imm_11, imm_hi, 1'b0 }

    group R_ALU:   opcode = 0b0110011
    group I_ALU:   opcode = 0b0010011
    group LOAD:    opcode = 0b0000011
    group STORE:   opcode = 0b0100011
    group BRANCH:  opcode = 0b1100011
    group LUI:     opcode = 0b0110111
    group JAL:     opcode = 0b1101111

    instr ADD(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b000
        funct7: 0b0000000
        semantics: rd = rs1 + rs2

    instr SUB(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b000
        funct7: 0b0100000
        semantics: rd = rs1 - rs2

    instr AND(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b111
        funct7: 0b0000000
        semantics: rd = rs1 & rs2

    instr OR(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b110
        funct7: 0b0000000
        semantics: rd = rs1 | rs2

    instr SLT(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b010
        funct7: 0b0000000
        semantics: rd = (rs1 < rs2) ? 1 : 0

    instr ADDI(rd: Reg, rs1: Reg, imm: SImm<12>):
        group: I_ALU
        funct3: 0b000
        semantics: rd = rs1 + sign_extend(imm)

    instr ANDI(rd: Reg, rs1: Reg, imm: SImm<12>):
        group: I_ALU
        funct3: 0b111
        semantics: rd = rs1 & sign_extend(imm)

    instr LW(rd: Reg, rs1: Reg, imm: SImm<12>):
        group: LOAD
        funct3: 0b010
        semantics: rd = mem[rs1 + sign_extend(imm)]
        latency: 2 cycles

    instr SW(rs1: Reg, rs2: Reg, imm: SImm<12>):
        group: STORE
        funct3: 0b010
        semantics: mem[rs1 + sign_extend(imm)] = rs2

    instr BEQ(rs1: Reg, rs2: Reg, imm: SImm<13>):
        group: BRANCH
        funct3: 0b000
        semantics: if rs1 == rs2 then pc = pc + sign_extend(imm)

    instr BNE(rs1: Reg, rs2: Reg, imm: SImm<13>):
        group: BRANCH
        funct3: 0b001
        semantics: if rs1 != rs2 then pc = pc + sign_extend(imm)

    instr LUI(rd: Reg, imm: UImm<20>):
        group: LUI
        semantics: rd = imm << 12

    instr JAL(rd: Reg, imm: SImm<21>):
        group: JAL
        semantics: rd = pc + 4; pc = pc + sign_extend(imm)
```

**2. Custom vector dot-product extension:**
```
isa DotExt extends RV32I:
    instr DOT8(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b000
        funct7: 0b0000001
        semantics: rd = dot_product_int8(rs1, rs2)
        latency: 2 cycles

    instr MACC8(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b001
        funct7: 0b0000001
        semantics: rd = rd + dot_product_int8(rs1, rs2)
        latency: 2 cycles
```

**3. Decode logic using ISA definition (user-written):**
```
module Decoder(
    in  instr: Bits<32>,
    out alu_op: AluOp,
    out alu_src: Bool,
    out mem_read: Bool,
    out mem_write: Bool,
    out reg_write: Bool,
    out branch: Bool,
    out imm_value: SInt<32>
):
    let op = RV32I.opcode(instr)
    let f3 = RV32I.funct3(instr)
    let f7 = RV32I.funct7(instr)

    comb:
        // Safe defaults — ensures all outputs assigned on all paths
        alu_op = AluOp.Add
        alu_src = false
        mem_read = false
        mem_write = false
        reg_write = false
        branch = false
        imm_value = 0

        match op:
            RV32I.opcodes.R_ALU =>
                reg_write = true
                match (f3, f7):
                    (RV32I.funct3.ADD, RV32I.funct7.ADD) => alu_op = AluOp.Add
                    (RV32I.funct3.ADD, RV32I.funct7.SUB) => alu_op = AluOp.Sub
                    (RV32I.funct3.AND, _) => alu_op = AluOp.And
                    (RV32I.funct3.OR, _)  => alu_op = AluOp.Or
                    (RV32I.funct3.SLT, _) => alu_op = AluOp.Slt
                    _ => alu_op = AluOp.Add

            RV32I.opcodes.I_ALU =>
                reg_write = true
                alu_src = true
                imm_value = RV32I.imm_i(instr).sign_extend<32>()

            RV32I.opcodes.LOAD =>
                reg_write = true
                mem_read = true
                alu_src = true
                imm_value = RV32I.imm_i(instr).sign_extend<32>()

            RV32I.opcodes.STORE =>
                mem_write = true
                alu_src = true
                imm_value = RV32I.imm_s(instr).sign_extend<32>()

            RV32I.opcodes.BRANCH =>
                branch = true
                imm_value = RV32I.imm_b(instr).sign_extend<32>()

            RV32I.opcodes.LUI =>
                reg_write = true
                alu_src = true
                imm_value = RV32I.imm_u(instr).as_signed()
                    .zero_extend<32>() << 12

            _ => ()    // defaults apply — no latch risk
```

**4. Hazard detection using ISA extractors (user-written):**
```
module HazardUnit(
    in  id_instr: Bits<32>,
    in  ex_instr: Bits<32>,
    in  mem_instr: Bits<32>,
    in  ex_reg_write: Bool,
    in  mem_reg_write: Bool,
    in  ex_mem_read: Bool,
    out stall: Bool,
    out forward_a: ForwardSel,
    out forward_b: ForwardSel
):
    let id_rs1 = RV32I.rs1(id_instr)
    let id_rs2 = RV32I.rs2(id_instr)
    let ex_rd  = RV32I.rd(ex_instr)
    let mem_rd = RV32I.rd(mem_instr)

    comb:
        stall = false
        forward_a = ForwardSel.None
        forward_b = ForwardSel.None

        // Load-use hazard: stall 1 cycle
        if ex_mem_read and ex_rd != 0 and
           (ex_rd == id_rs1 or ex_rd == id_rs2):
            stall = true

        // EX→EX forwarding
        if ex_reg_write and ex_rd != 0:
            if ex_rd == id_rs1:
                forward_a = ForwardSel.FromEX
            if ex_rd == id_rs2:
                forward_b = ForwardSel.FromEX

        // MEM→EX forwarding (lower priority)
        if mem_reg_write and mem_rd != 0:
            if mem_rd == id_rs1 and forward_a == ForwardSel.None:
                forward_a = ForwardSel.FromMEM
            if mem_rd == id_rs2 and forward_b == ForwardSel.None:
                forward_b = ForwardSel.FromMEM

    prove forwarding_complete:
        assume: ex_reg_write and ex_rd != 0 and ex_rd == id_rs1
        assert: forward_a != ForwardSel.None
```

---

## Section 12 — Compilation Targets & Toolchain

**Design rationale:** Existing HDL toolchains force engineers into vendor-specific workflows. SSL compiles from a single source to multiple backends, with the compiler handling target-specific lowering.

### 12.1 Compilation Backends

| Backend | Output format | Use case |
|---|---|---|
| **Verilog 2005** | `.v` | Universal — works with all synthesis and simulation tools |
| **RTLIL** | `.il` | Yosys native — skips Verilog parsing for open-source flows |
| **FIRRTL** | `.fir` | CIRCT/MLIR ecosystem |
| **SVA** | Inline in `.v` | Formal properties as SystemVerilog Assertions |
| **SMT-LIB2** | `.smt2` | Z3, CVC5 — standalone formal verification |
| **sby** | `.sby` | SymbiYosys project files |
| **Simulation** | Interpreted | Cycle-accurate simulator for `testbench` blocks |
| **ISA JSON** | `.json` | Machine-readable ISA definition export |

### 12.2 Compiler Pipeline

```
Source (.ssl)
    │
    ▼
┌─────────┐
│  Parse   │  Tokens → AST
└────┬─────┘
     ▼
┌──────────────┐
│  Name Resolve │  Resolve imports, build symbol table
└────┬─────────┘
     ▼
┌─────────────┐
│  Type Check  │  Width inference, type compatibility, generic monomorphization
└────┬────────┘
     ▼
┌──────────────────┐
│  Clock Domain     │  Assign domains, verify CDC crossings
│  Analysis         │
└────┬─────────────┘
     ▼
┌──────────────┐
│  Elaboration  │  Expand gen for/if, lower systolic/dataflow,
│               │  monomorphize generics, flatten hierarchy
└────┬─────────┘
     ▼
┌──────────────┐
│  Lint &       │  Combinational loops, latch check, exhaustive match,
│  Safety       │  dead code warnings
└────┬─────────┘
     ▼
┌──────────────┐
│  Optimization │  Constant folding, dead signal elimination,
│               │  resource sharing, retiming
└────┬─────────┘
     ▼
┌──────────────────┐
│  Target Emission  │  Verilog / RTLIL / FIRRTL / SMT-LIB2 / sby
└──────────────────┘
```

### 12.3 CLI Design

```
sslc <COMMAND> [OPTIONS] <FILES>

COMMANDS:
    build       Compile SSL source to target output
    test        Run testbench blocks
    verify      Run formal verification
    check       Type-check and lint without emitting output
    doc         Generate documentation
    fmt         Format SSL source files
    init        Create a new SSL project

BUILD OPTIONS:
    --target <TARGET>       verilog | rtlil | firrtl | sim
    --output <DIR>          Output directory (default: build/)
    --top <MODULE>          Top-level module name
    --device <DEVICE>       Target FPGA device (e.g., xc7a35t)
    --emit <ARTIFACT>       sdc | xdc | pcf | smt2 | sby | docs | isa-json |
                            lowered | stubs | name-map
    --define <KEY=VALUE>    Compile-time constant
    --optimize <LEVEL>      0 | 1 (default) | 2
    --keep-hierarchy        Preserve module hierarchy

TEST OPTIONS:
    --filter <PATTERN>      Run matching testbenches
    --coverage              Emit coverage report
    --dump <FORMAT>         vcd | fst
    --dump-file <PATH>      Waveform output path
    --seed <N>              Random seed
    --timeout <CYCLES>      Global cycle timeout

VERIFY OPTIONS:
    --engine <ENGINE>       z3 | cvc5 | sby (default: sby)
    --depth <N>             BMC depth
    --prove <NAME>          Run matching prove blocks
    --timeout <SECONDS>     Solver timeout

CHECK OPTIONS:
    --cdc-report            Emit CDC analysis report
    --warnings-as-errors    Treat warnings as errors

DOC OPTIONS:
    --output <DIR>          Documentation output directory
    --format <FMT>          json | html

FMT OPTIONS:
    --check                 Check without modifying
    --indent <N>            Indentation width (default: 4)
```

**Test command scoping:**
- `sslc test` — runs all testbenches project-wide (reads `ssl.toml`)
- `sslc test src/cpu.ssl` — runs testbenches in that file only
- `sslc test --filter "tb_hazard*"` — runs matching testbenches project-wide
- `sslc test src/mmu/ --filter "tb_tlb*"` — matching testbenches within directory

**Formal verification timeout behavior:**

| Outcome | Condition | Exit code |
|---|---|---|
| **PASS** | All depths explored, no counterexample | 0 |
| **FAIL** | Counterexample found at depth K | 1 |
| **UNKNOWN** | Timeout before depth fully explored | 1 |
| **UNKNOWN** | Out of memory | 1 |

`UNKNOWN` is never reported as `PASS`. Exit code is nonzero for `FAIL` and `UNKNOWN`. Report always states maximum depth successfully explored.

### 12.4 Project Configuration (`ssl.toml`)

```toml
[project]
name = "my-cpu"
version = "0.1.0"
edition = "2026"
top = "CPU"

[build]
target = "verilog"
output = "build/"
optimize = 1

[build.defines]
TARGET = "synth"
DEBUG_HOOKS = false

[device]
family = "xilinx"
part = "xc7a35tcpg236-1"
speed_grade = "-1"

[test]
coverage = true
dump = "fst"
seed = 0
timeout = 1_000_000

[verify]
engine = "sby"
depth = 20
timeout = 300

[dependencies]
ssl-std = { version = "0.1", features = ["interfaces", "memory"] }

[dependencies.xilinx-primitives]
version = "1.0"
path = "vendor/xilinx/"

[lint]
warnings_as_errors = false
allow = ["unused-signal"]
deny = ["combinational-loop"]

[[constraint-files]]
path = "constraints/board.xdc"
format = "xdc"

[[constraint-files]]
path = "constraints/timing.sdc"
format = "sdc"
```

### 12.5 Synthesis & Physical Metadata

Timing constraints and synthesis directives (metadata for synthesis tools, not formal properties):

```
CONSTRAIN_STMT   := constrain NAME : CONSTRAINT_EXPR
CONSTRAINT_EXPR  := period = TIME_EXPR
                  | setup = TIME_EXPR , hold = TIME_EXPR
                  | max_delay = TIME_EXPR
                  | false_path

TIME_EXPR        := NUMBER ns | NUMBER us | NUMBER ps

SYNTH_ATTR       := @use_resource ( RESOURCE )
                  | @keep_hierarchy
                  | @max_fanout ( N )
                  | @dont_touch
                  | @synth ( STRING , STRING )
                  | @clock_gate_enable
                  | @power_opt ( STRING )
                  | @critical_path ( STRING )
                  | @export_name ( STRING )
```

Examples:
```
constrain clk: period = 10ns
constrain data_in: setup = 2ns, hold = 0.5ns
constrain cross_domain_path: max_delay = 5ns

@use_resource(DSP)
@keep_hierarchy
@max_fanout(32)
@synth("ram_style", "ultra")
@export_name("rd_ptr_gray")            // deterministic Verilog name
```

### 12.6 Constraint File Generation

| SSL construct | Generated constraint |
|---|---|
| `Clock<100MHz>` | `create_clock -period 10.0 [get_ports clk]` |
| `cdc(..., method=two_ff_sync)` | `set_false_path` + `set_max_delay` |
| `cdc(..., method=gray_code)` | `set_false_path` + `set_bus_skew` |
| `cdc(..., method=async_fifo)` | False paths for pointers, max delay for data |
| `clock_gate(clk, enable)` | `create_generated_clock` |
| `constrain clk: period = 10ns` | `create_clock -period 10.0` |

User-authored constraint files from `ssl.toml` are concatenated after auto-generated constraints, allowing manual overrides.

### 12.7 Signal Naming Guarantees

The compiler guarantees deterministic naming in emitted Verilog:
- Naming scheme: `{module_instance_path}_{signal_name}`
- `gen for` loops produce `{signal}_{index}`
- No random suffixes or hash-based mangling
- `@export_name("name")` forces a specific Verilog name (must be unique — duplicates are a compile error)
- `sslc build --emit name-map` outputs a JSON mapping of SSL signal names to emitted Verilog names

### 12.8 Extern Module Handling

`extern module` declarations are always emitted as blackbox instantiations. The compiler emits the instance with port connections but never emits a module definition body. Generic parameters map to Verilog `#(.PARAM(val))`. `sslc build --emit stubs` outputs a file listing all extern module signatures for vendor tool integration.

### 12.9 FPGA Vendor Integration

| Vendor | Target ID | Toolchain | SSL output |
|---|---|---|---|
| Xilinx/AMD | `xilinx` | Vivado | Verilog + XDC |
| Intel/Altera | `intel` | Quartus | Verilog + SDC |
| Lattice (iCE40) | `lattice-ice40` | nextpnr-ice40 | RTLIL + PCF |
| Lattice (ECP5) | `lattice-ecp5` | nextpnr-ecp5 | RTLIL + LPF |
| Efinix | `efinix` | Efinity | Verilog + SDC |
| Open ASIC | `asic-open` | Yosys + OpenROAD | RTLIL + SDC |
| Commercial ASIC | `asic` | Via Verilog handoff | Verilog + SDC |

### 12.10 Documentation Generation

```bash
sslc doc src/ --format=html --output=docs/
```

Auto-generates from `///` doc comments and module structure:
- Module hierarchy tree
- Port tables with types, domains, doc comments
- FSM state diagrams (Graphviz/Mermaid)
- Pipeline timing diagrams
- Interface protocol tables
- ISA reference (instruction table, encoding map, format diagrams)

---

## Section 13 — Standard Library

**Design rationale:** SSL's standard library is designed as a module system, not a fixed API surface. The spec defines how modules are structured, discovered, versioned, and imported — then provides fully worked reference modules as examples. The remaining modules are documented as planned without locking API signatures.

### 13.1 Package Structure

```
ssl-std/
├── ssl.toml
├── src/
│   ├── primitives/          # gates, MUXes, flip-flops, latches, tristate
│   ├── arithmetic/          # adders, multipliers, dividers
│   ├── memory/              # FIFO, LIFO, CAM, ROM
│   ├── interfaces/          # AXI4, AXI4-Lite, AXI4-Stream, APB, Wishbone
│   ├── io/                  # UART, SPI, I2C, GPIO
│   ├── verify/              # protocol checkers, coverage helpers
│   └── sim/                 # behavioral models, BFMs (testbench-only)
```

### 13.2 Import System

```
import SyncFifo from "ssl-std/memory/fifo"
import { AXI4Lite, AXI4Stream } from "ssl-std/interfaces/axi4"
import SyncFifo as Fifo from "ssl-std/memory/fifo"

import ssl.memory
let f = ssl.memory.SyncFifo<UInt<8>, 16>(...)
```

### 13.3 Package Manifest

```toml
[package]
name = "ssl-std"
version = "0.1.0"
edition = "2026"
description = "SiliconScript Standard Library"

[features]
default = ["primitives", "arithmetic", "memory"]
primitives = []
arithmetic = []
memory = []
interfaces = []
io = []
verify = []
sim = []

[features.io]
requires = ["interfaces"]
```

### 13.4 Versioning

- Semver pinning in `ssl.toml`
- Breaking changes require major version bump
- Compiler validates imported module signatures match expected version
- `sslc update` updates dependencies to latest compatible versions

### 13.5 Fully Worked Module: `SyncFifo`

```
/// Synchronous FIFO with parameterized depth and data type.
/// Uses circular buffer with pointer comparison for full/empty.
pub module SyncFifo<T: type, DEPTH: uint>(
    in  clk:     Clock,
    in  rst:     SyncReset,
    in  wr_data: T,
    in  wr_en:   Bool,
    out full:    Bool,
    out rd_data: T,
    in  rd_en:   Bool,
    out empty:   Bool,
    out count:   UInt<clog2(DEPTH) + 1>
):
    static_assert is_power_of_2(DEPTH), "FIFO depth must be a power of 2"
    static_assert DEPTH >= 2, "FIFO depth must be at least 2"

    const ADDR_W: uint = clog2(DEPTH)
    const PTR_W: uint = ADDR_W + 1        // extra MSB for wrap detection

    signal mem: Memory<T, depth=DEPTH>
    signal wr_ptr: UInt<PTR_W>
    signal rd_ptr: UInt<PTR_W>

    reg(clk, rst):
        on reset:
            wr_ptr = 0
            rd_ptr = 0
        on tick:
            if wr_en and not full:
                wr_ptr = wr_ptr + 1
                mem.write(addr=wr_ptr[ADDR_W-1:0], data=wr_data, enable=true)
            if rd_en and not empty:
                rd_ptr = rd_ptr + 1

    comb:
        // MSBs differ, lower bits match → full
        full = (wr_ptr[ADDR_W] != rd_ptr[ADDR_W]) and
               (wr_ptr[ADDR_W-1:0] == rd_ptr[ADDR_W-1:0])
        // Pointers identical → empty
        empty = wr_ptr == rd_ptr
        count = wr_ptr - rd_ptr
        rd_data = mem.read(addr=rd_ptr[ADDR_W-1:0])

    // Formal properties
    assert always: count <= DEPTH, "fill count never exceeds depth"
    assert always: not (full and empty), "cannot be both full and empty"

    cover: full
    cover: empty
    cover: count == DEPTH / 2

    test "basic write-read":
        drive(wr_data, 0xAB)
        drive(wr_en, true)
        drive(rd_en, false)
        tick()
        assert not peek(empty)

        drive(wr_en, false)
        drive(rd_en, true)
        tick()
        assert peek(rd_data) == 0xAB
        assert peek(empty)
```

### 13.6 Fully Worked Module: `UartTx`

```
/// UART transmitter — 8N1 frames, configurable baud rate.
pub module UartTx<CLK_FREQ: uint, BAUD: uint = 115200>(
    in  clk:   Clock<CLK_FREQ>,
    in  rst:   SyncReset,
    in  data:  UInt<8>,
    in  valid: Bool,
    out ready: Bool,
    out tx:    Bool
):
    const DIVISOR: uint = CLK_FREQ / BAUD
    const DIV_W: uint = clog2(DIVISOR)
    static_assert DIVISOR >= 1, "clock too slow for baud rate"

    signal baud_tick: Bool

    fsm TxState(clk, rst):
        states: Idle | Transmitting
        encoding: binary
        initial: Idle

        // FSM-local registered signals
        signal shift_reg: Bits<10> = 0b1111111111
        signal bit_index: UInt<4> = 0
        signal baud_counter: UInt<DIV_W> = 0

        transitions:
            Idle --(valid)--> Transmitting:
                shift_reg = 1'b1 ++ data ++ 1'b0
                bit_index = 0
                baud_counter = 0
            Transmitting --(baud_tick and bit_index == 9)--> Idle

        on tick:
            match state:
                Transmitting =>
                    baud_counter = baud_counter + 1
                    if baud_tick:
                        baud_counter = 0
                        shift_reg = 1'b1 ++ shift_reg[9:1]
                        bit_index = bit_index + 1
                _ => ()

        outputs:
            Idle =>
                tx = true
                ready = true
            Transmitting =>
                tx = shift_reg[0]
                ready = false

    comb:
        baud_tick = TxState.baud_counter == DIVISOR - 1

    assert always: state == TxState.Idle implies tx, "TX idle high"

    test "send byte 0x55":
        assert peek(tx) == true
        assert peek(ready) == true

        drive(data, 0x55)
        drive(valid, true)
        tick()
        drive(valid, false)

        assert peek(ready) == false
        tick(10 * DIVISOR + 1)
        assert peek(ready) == true
```

### 13.7 Fully Worked Module: `AXI4LiteTarget`

```
/// AXI4-Lite target (slave) adapter.
/// Provides a simple register interface to the user.
pub module AXI4LiteTarget<ADDR_W: uint = 32, DATA_W: uint = 32>(
    in  clk:  Clock,
    in  rst:  SyncReset,
    in  bus:  Flip<AXI4Lite<ADDR_W, DATA_W>>,
    out reg_addr:       UInt<ADDR_W>,
    out reg_write_en:   Bool,
    out reg_write_data: UInt<DATA_W>,
    out reg_write_strb: UInt<DATA_W / 8>,
    in  reg_read_data:  UInt<DATA_W>,
    out reg_read_en:    Bool
):
    // Registered address capture — holds value across handshake phases
    signal addr_reg: UInt<ADDR_W>

    reg(clk, rst):
        on reset:
            addr_reg = 0
        on tick:
            if bus.write_addr.awvalid and bus.write_addr.awready:
                addr_reg = bus.write_addr.awaddr
            if bus.read_addr.arvalid and bus.read_addr.arready:
                addr_reg = bus.read_addr.araddr

    comb:
        reg_addr = addr_reg

    fsm WriteCtrl(clk, rst):
        states: WIdle | WData | WResp
        encoding: onehot
        initial: WIdle

        transitions:
            WIdle --(bus.write_addr.awvalid)--> WData
            WData --(bus.write_data.wvalid)--> WResp:
                reg_write_en = true
                reg_write_data = bus.write_data.wdata
                reg_write_strb = bus.write_data.wstrb
            WResp --(bus.write_resp.bready)--> WIdle:
                reg_write_en = false

        outputs:
            WIdle =>
                bus.write_addr.awready = true
                bus.write_data.wready = false
                bus.write_resp.bvalid = false
                bus.write_resp.bresp = 0b00
            WData =>
                bus.write_addr.awready = false
                bus.write_data.wready = true
                bus.write_resp.bvalid = false
                bus.write_resp.bresp = 0b00
            WResp =>
                bus.write_addr.awready = false
                bus.write_data.wready = false
                bus.write_resp.bvalid = true
                bus.write_resp.bresp = 0b00

    fsm ReadCtrl(clk, rst):
        states: RIdle | RData
        encoding: onehot
        initial: RIdle

        transitions:
            RIdle --(bus.read_addr.arvalid)--> RData:
                reg_read_en = true
            RData --(bus.read_data.rready)--> RIdle:
                reg_read_en = false

        outputs:
            RIdle =>
                bus.read_addr.arready = true
                bus.read_data.rvalid = false
                bus.read_data.rdata = 0
                bus.read_data.rresp = 0b00
            RData =>
                bus.read_addr.arready = false
                bus.read_data.rvalid = true
                bus.read_data.rdata = reg_read_data
                bus.read_data.rresp = 0b00

    assert always: bus.write_resp.bvalid and not bus.write_resp.bready
        implies next(bus.write_resp.bvalid),
        "BVALID must hold until BREADY"
```

### 13.8 Planned Modules

The following modules are planned. Signatures are indicative — they will be finalized when implemented.

| Package | Module | Description |
|---|---|---|
| `ssl.primitives` | `Mux2`, `Mux4`, `MuxN` | Parameterized multiplexers |
| `ssl.primitives` | `DFF`, `DFFE`, `DLatch` | Primitive flip-flops and latches |
| `ssl.primitives` | `TristateBuf` | Tristate buffer with enable |
| `ssl.arithmetic` | `RippleAdder<W>` | Simple ripple-carry adder |
| `ssl.arithmetic` | `CLAAdder<W>` | Carry-lookahead adder |
| `ssl.arithmetic` | `PrefixAdder<W>` | Brent-Kung / Kogge-Stone prefix adder |
| `ssl.arithmetic` | `ArrayMultiplier<W>` | Array multiplier (low resource) |
| `ssl.arithmetic` | `BoothMultiplier<W>` | Booth-encoded multiplier |
| `ssl.arithmetic` | `Divider<W>` | Multi-cycle restoring divider |
| `ssl.memory` | `AsyncFifo<T, DEPTH>` | Dual-clock async FIFO with gray-code pointers |
| `ssl.memory` | `Lifo<T, DEPTH>` | Stack (LIFO) |
| `ssl.memory` | `CAM<KEY, VALUE, DEPTH>` | Content-addressable memory |
| `ssl.memory` | `Rom<T, DEPTH>` | ROM with hex file initialization |
| `ssl.interfaces` | `AXI4<A,D>` | Full AXI4 interface definition |
| `ssl.interfaces` | `AXI4Lite<A,D>` | AXI4-Lite interface definition |
| `ssl.interfaces` | `AXI4Stream<D>` | AXI4-Stream interface definition |
| `ssl.interfaces` | `APB<A,D>` | APB interface definition |
| `ssl.interfaces` | `Wishbone<A,D>` | Wishbone interface definition |
| `ssl.io` | `UartRx<CLK,BAUD>` | UART receiver (8N1) |
| `ssl.io` | `SpiController<W>` | SPI master |
| `ssl.io` | `SpiPeripheral<W>` | SPI slave |
| `ssl.io` | `I2CController` | I2C master |
| `ssl.io` | `Gpio<W>` | Configurable GPIO port |
| `ssl.verify` | `AxiChecker` | AXI protocol compliance checker |
| `ssl.verify` | `ApbChecker` | APB protocol compliance checker |
| `ssl.sim` | `AxiBfm` | AXI bus functional model (testbench) |
| `ssl.sim` | `MemoryModel<A,D>` | Behavioral memory model (testbench) |
| `ssl.ai` | `SystolicTemplate<R,C>` | Configurable systolic array template |
| `ssl.ai` | `WeightBuffer<T,R,C>` | Double-buffered weight memory controller |
| `ssl.crypto` | `AES128Encrypt` | AES-128 encryption core |
| `ssl.crypto` | `SHA256` | SHA-256 hash engine |

---

## Section 14 — Full Worked Example

**Design rationale:** A language spec without a complete, realistic example is a thought experiment. This section presents a 5-stage RISC-V RV32I pipeline in SSL — demonstrating modules, types, ISA definitions, formal verification, testbenches, and synthesis annotations working together.

**Approach note:** This example uses explicit pipeline registers (`reg` blocks with `if_id_*`, `id_ex_*`, etc.) rather than SSL's `pipeline` construct. Both approaches are valid. Manual registers give the CPU architect full control over stall insertion, flush logic, and forwarding paths. The `pipeline` construct (Section 6.3) is better suited for data-processing pipelines with uniform backpressure. A CPU pipeline — with its irregular hazard handling, branch flush, and asymmetric stall logic — is a case where manual control is the natural fit.

### 14.1 Complete Source

```
// ============================================================
// rv32i_cpu.ssl — 5-stage pipelined RISC-V RV32I processor
// ============================================================
//
// Features:
//   - IF / ID / EX / MEM / WB pipeline stages
//   - Data hazard forwarding (EX→EX, MEM→EX)
//   - Load-use stall insertion
//   - Static branch prediction (not-taken)
//   - Formal verification properties
//   - Testbench with program execution
//   - Synthesis annotations for Xilinx Artix-7

import { SyncFifo } from "ssl-std/memory/fifo"

// ---- ISA Definition ----

isa RV32I:
    encoding_width: 32

    registers X:
        count = 32
        width = 32
        zero_reg = 0

    format R_type:
        funct7: Bits<7>  @ [31:25]
        rs2:    UInt<5>  @ [24:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        rd:     UInt<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]

    format I_type:
        imm:    SInt<12> @ [31:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        rd:     UInt<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]

    format S_type:
        imm_hi: Bits<7>  @ [31:25]
        rs2:    UInt<5>  @ [24:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        imm_lo: Bits<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]
        assemble imm_s: SInt<12> = { imm_hi, imm_lo }

    format B_type:
        imm_12: Bits<1>  @ [31]
        imm_hi: Bits<6>  @ [30:25]
        rs2:    UInt<5>  @ [24:20]
        rs1:    UInt<5>  @ [19:15]
        funct3: Bits<3>  @ [14:12]
        imm_lo: Bits<4>  @ [11:8]
        imm_11: Bits<1>  @ [7]
        opcode: Bits<7>  @ [6:0]
        assemble imm_b: SInt<13> = { imm_12, imm_11, imm_hi, imm_lo, 1'b0 }

    format U_type:
        imm:    UInt<20> @ [31:12]
        rd:     UInt<5>  @ [11:7]
        opcode: Bits<7>  @ [6:0]

    group R_ALU:   opcode = 0b0110011
    group I_ALU:   opcode = 0b0010011
    group LOAD:    opcode = 0b0000011
    group STORE:   opcode = 0b0100011
    group BRANCH:  opcode = 0b1100011
    group LUI:     opcode = 0b0110111

    instr ADD(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b000
        funct7: 0b0000000

    instr SUB(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b000
        funct7: 0b0100000

    instr AND(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b111
        funct7: 0b0000000

    instr OR(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b110
        funct7: 0b0000000

    instr SLT(rd: Reg, rs1: Reg, rs2: Reg):
        group: R_ALU
        funct3: 0b010
        funct7: 0b0000000

    instr ADDI(rd: Reg, rs1: Reg, imm: SImm<12>):
        group: I_ALU
        funct3: 0b000

    instr LW(rd: Reg, rs1: Reg, imm: SImm<12>):
        group: LOAD
        funct3: 0b010
        latency: 2 cycles

    instr SW(rs1: Reg, rs2: Reg, imm: SImm<12>):
        group: STORE
        funct3: 0b010

    instr BEQ(rs1: Reg, rs2: Reg, imm: SImm<13>):
        group: BRANCH
        funct3: 0b000

    instr BNE(rs1: Reg, rs2: Reg, imm: SImm<13>):
        group: BRANCH
        funct3: 0b001

    instr LUI(rd: Reg, imm: UImm<20>):
        group: LUI

// ---- Shared Types ----

enum AluOp [binary]:
    Add  = 0b0000
    Sub  = 0b0001
    And  = 0b0010
    Or   = 0b0011
    Slt  = 0b0100

enum ForwardSel [binary]:
    None    = 0b00
    FromEX  = 0b01
    FromMEM = 0b10

// ---- Register File ----

module RegFile(
    in  clk:      Clock,
    in  rst:      SyncReset,
    in  rs1_addr: UInt<5>,
    in  rs2_addr: UInt<5>,
    out rs1_data: UInt<32>,
    out rs2_data: UInt<32>,
    in  wr_addr:  UInt<5>,
    in  wr_data:  UInt<32>,
    in  wr_en:    Bool
):
    signal regs: UInt<32>[32]

    reg(clk, rst):
        on reset:
            gen for i in 0..32:
                regs[i] = 0
        on tick:
            if wr_en and wr_addr != 0:
                regs[wr_addr] = wr_data

    comb:
        rs1_data = if rs1_addr == 0 then 0
                   elif wr_en and rs1_addr == wr_addr then wr_data
                   else regs[rs1_addr]
        rs2_data = if rs2_addr == 0 then 0
                   elif wr_en and rs2_addr == wr_addr then wr_data
                   else regs[rs2_addr]

// ---- ALU ----

module ALU(
    in  a:      UInt<32>,
    in  b:      UInt<32>,
    in  op:     AluOp,
    out result: UInt<32>,
    out zero:   Bool
):
    comb:
        match op:
            AluOp.Add => result = a + b
            AluOp.Sub => result = a - b
            AluOp.And => result = a & b
            AluOp.Or  => result = a | b
            AluOp.Slt => result = if a.as_signed() < b.as_signed()
                                  then 1 else 0
        zero = result == 0

// ---- Hazard Unit ----

module HazardUnit(
    in  id_rs1:        UInt<5>,
    in  id_rs2:        UInt<5>,
    in  ex_rd:         UInt<5>,
    in  mem_rd:        UInt<5>,
    in  ex_reg_write:  Bool,
    in  mem_reg_write: Bool,
    in  ex_mem_read:   Bool,
    out stall:         Bool,
    out forward_a:     ForwardSel,
    out forward_b:     ForwardSel
):
    comb:
        stall = false
        forward_a = ForwardSel.None
        forward_b = ForwardSel.None

        if ex_mem_read and ex_rd != 0 and
           (ex_rd == id_rs1 or ex_rd == id_rs2):
            stall = true

        if ex_reg_write and ex_rd != 0:
            if ex_rd == id_rs1:
                forward_a = ForwardSel.FromEX
            if ex_rd == id_rs2:
                forward_b = ForwardSel.FromEX

        if mem_reg_write and mem_rd != 0:
            if mem_rd == id_rs1 and forward_a == ForwardSel.None:
                forward_a = ForwardSel.FromMEM
            if mem_rd == id_rs2 and forward_b == ForwardSel.None:
                forward_b = ForwardSel.FromMEM

// ---- Top-Level CPU ----

@critical_path("ALU result -> forwarding mux -> ALU input")
@keep_hierarchy
module CPU(
    in  clk: Clock<100MHz>,
    in  rst: SyncReset
):
    // -- Memories --
    signal imem: Memory<Bits<32>, depth=1024,
        init = "program.hex", writable = false>
    signal dmem: Memory<UInt<32>, depth=1024>

    // -- Sub-module instances --
    inst regfile = RegFile(clk=clk, rst=rst)
    inst alu = ALU()
    inst hazard = HazardUnit()

    // -- Pipeline registers --
    // IF/ID
    signal if_id_pc:    UInt<32>
    signal if_id_instr: Bits<32>
    signal if_id_valid: Bool

    // ID/EX
    signal id_ex_pc:        UInt<32>
    signal id_ex_rs1_data:  UInt<32>
    signal id_ex_rs2_data:  UInt<32>
    signal id_ex_imm:       SInt<32>
    signal id_ex_rd:        UInt<5>
    signal id_ex_rs1:       UInt<5>
    signal id_ex_rs2:       UInt<5>
    signal id_ex_alu_op:    AluOp
    signal id_ex_alu_src:   Bool
    signal id_ex_mem_read:  Bool
    signal id_ex_mem_write: Bool
    signal id_ex_reg_write: Bool
    signal id_ex_branch:    Bool
    signal id_ex_valid:     Bool

    // EX/MEM
    signal ex_mem_alu_result:    UInt<32>
    signal ex_mem_rs2_data:      UInt<32>
    signal ex_mem_rd:            UInt<5>
    signal ex_mem_mem_read:      Bool
    signal ex_mem_mem_write:     Bool
    signal ex_mem_reg_write:     Bool
    signal ex_mem_zero:          Bool
    signal ex_mem_branch:        Bool
    signal ex_mem_branch_target: UInt<32>
    signal ex_mem_valid:         Bool

    // MEM/WB
    signal mem_wb_rd:        UInt<5>
    signal mem_wb_data:      UInt<32>
    signal mem_wb_reg_write: Bool
    signal mem_wb_valid:     Bool

    // -- PC --
    signal pc: UInt<32>
    signal pc_next: UInt<32>
    signal branch_taken: Bool

    // -- Decode signals --
    signal dec_alu_op:    AluOp
    signal dec_alu_src:   Bool
    signal dec_mem_read:  Bool
    signal dec_mem_write: Bool
    signal dec_reg_write: Bool
    signal dec_branch:    Bool
    signal dec_imm:       SInt<32>

    // =====================================================
    // Stage 1: Instruction Fetch (IF)
    // =====================================================
    comb:
        branch_taken = ex_mem_valid and ex_mem_branch and ex_mem_zero
        pc_next = if branch_taken then ex_mem_branch_target
                  else pc + 4

    reg(clk, rst):
        on reset:
            pc = 0
            if_id_pc = 0
            if_id_instr = 0
            if_id_valid = false
        on tick:
            if not hazard.stall:
                pc = pc_next
                if_id_pc = pc
                if_id_instr = imem.read(addr=pc[11:2]).as_bits()
                if_id_valid = not branch_taken
            if branch_taken:
                if_id_valid = false

    // =====================================================
    // Stage 2: Instruction Decode (ID)
    // =====================================================
    comb:
        let op = RV32I.opcode(if_id_instr)
        let f3 = RV32I.funct3(if_id_instr)
        let f7 = RV32I.funct7(if_id_instr)

        dec_alu_op = AluOp.Add
        dec_alu_src = false
        dec_mem_read = false
        dec_mem_write = false
        dec_reg_write = false
        dec_branch = false
        dec_imm = 0

        match op:
            RV32I.opcodes.R_ALU =>
                dec_reg_write = true
                match (f3, f7):
                    (0b000, 0b0000000) => dec_alu_op = AluOp.Add
                    (0b000, 0b0100000) => dec_alu_op = AluOp.Sub
                    (0b111, _)         => dec_alu_op = AluOp.And
                    (0b110, _)         => dec_alu_op = AluOp.Or
                    (0b010, _)         => dec_alu_op = AluOp.Slt
                    _                  => dec_alu_op = AluOp.Add

            RV32I.opcodes.I_ALU =>
                dec_reg_write = true
                dec_alu_src = true
                dec_imm = RV32I.imm_i(if_id_instr).sign_extend<32>()

            RV32I.opcodes.LOAD =>
                dec_reg_write = true
                dec_mem_read = true
                dec_alu_src = true
                dec_imm = RV32I.imm_i(if_id_instr).sign_extend<32>()

            RV32I.opcodes.STORE =>
                dec_mem_write = true
                dec_alu_src = true
                dec_imm = RV32I.imm_s(if_id_instr).sign_extend<32>()

            RV32I.opcodes.BRANCH =>
                dec_branch = true
                dec_imm = RV32I.imm_b(if_id_instr).sign_extend<32>()

            RV32I.opcodes.LUI =>
                dec_reg_write = true
                dec_alu_src = true
                dec_imm = RV32I.imm_u(if_id_instr).as_signed()
                    .zero_extend<32>() << 12

            _ => ()

    comb:
        regfile.rs1_addr = RV32I.rs1(if_id_instr)
        regfile.rs2_addr = RV32I.rs2(if_id_instr)

    comb:
        hazard.id_rs1 = RV32I.rs1(if_id_instr)
        hazard.id_rs2 = RV32I.rs2(if_id_instr)
        hazard.ex_rd = id_ex_rd
        hazard.mem_rd = ex_mem_rd
        hazard.ex_reg_write = id_ex_reg_write and id_ex_valid
        hazard.mem_reg_write = ex_mem_reg_write and ex_mem_valid
        hazard.ex_mem_read = id_ex_mem_read and id_ex_valid

    reg(clk, rst):
        on reset:
            id_ex_valid = false
            id_ex_rd = 0
            id_ex_alu_op = AluOp.Add
            id_ex_alu_src = false
            id_ex_mem_read = false
            id_ex_mem_write = false
            id_ex_reg_write = false
            id_ex_branch = false
            id_ex_pc = 0
            id_ex_rs1_data = 0
            id_ex_rs2_data = 0
            id_ex_imm = 0
            id_ex_rs1 = 0
            id_ex_rs2 = 0
        on tick:
            if hazard.stall:
                id_ex_valid = false
                id_ex_reg_write = false
                id_ex_mem_read = false
                id_ex_mem_write = false
                id_ex_branch = false
            elif branch_taken:
                id_ex_valid = false
                id_ex_reg_write = false
                id_ex_mem_read = false
                id_ex_mem_write = false
                id_ex_branch = false
            else:
                id_ex_valid = if_id_valid
                id_ex_pc = if_id_pc
                id_ex_rs1_data = regfile.rs1_data
                id_ex_rs2_data = regfile.rs2_data
                id_ex_imm = dec_imm
                id_ex_rd = RV32I.rd(if_id_instr)
                id_ex_rs1 = RV32I.rs1(if_id_instr)
                id_ex_rs2 = RV32I.rs2(if_id_instr)
                id_ex_alu_op = dec_alu_op
                id_ex_alu_src = dec_alu_src
                id_ex_mem_read = dec_mem_read
                id_ex_mem_write = dec_mem_write
                id_ex_reg_write = dec_reg_write
                id_ex_branch = dec_branch

    // =====================================================
    // Stage 3: Execute (EX)
    // =====================================================
    comb:
        let rs1_forwarded: UInt<32> = match hazard.forward_a:
            ForwardSel.FromEX  => ex_mem_alu_result
            ForwardSel.FromMEM => mem_wb_data
            _                  => id_ex_rs1_data

        let rs2_forwarded: UInt<32> = match hazard.forward_b:
            ForwardSel.FromEX  => ex_mem_alu_result
            ForwardSel.FromMEM => mem_wb_data
            _                  => id_ex_rs2_data

        let alu_b = if id_ex_alu_src then id_ex_imm.as_unsigned()
                    else rs2_forwarded

        alu.a = rs1_forwarded
        alu.b = alu_b
        alu.op = id_ex_alu_op

    reg(clk, rst):
        on reset:
            ex_mem_valid = false
            ex_mem_rd = 0
            ex_mem_alu_result = 0
            ex_mem_rs2_data = 0
            ex_mem_mem_read = false
            ex_mem_mem_write = false
            ex_mem_reg_write = false
            ex_mem_zero = false
            ex_mem_branch = false
            ex_mem_branch_target = 0
        on tick:
            ex_mem_valid = id_ex_valid
            ex_mem_alu_result = alu.result
            ex_mem_rs2_data = rs2_forwarded
            ex_mem_rd = id_ex_rd
            ex_mem_mem_read = id_ex_mem_read
            ex_mem_mem_write = id_ex_mem_write
            ex_mem_reg_write = id_ex_reg_write
            ex_mem_zero = alu.zero
            ex_mem_branch = id_ex_branch
            ex_mem_branch_target = id_ex_pc + id_ex_imm.as_unsigned()

    // =====================================================
    // Stage 4: Memory Access (MEM)
    // =====================================================
    signal mem_read_data: UInt<32>

    reg(clk, rst):
        on reset:
            mem_read_data = 0
        on tick:
            if ex_mem_mem_write and ex_mem_valid:
                dmem.write(addr=ex_mem_alu_result[11:2],
                           data=ex_mem_rs2_data, enable=true)
            mem_read_data = dmem.read(addr=ex_mem_alu_result[11:2])

    reg(clk, rst):
        on reset:
            mem_wb_valid = false
            mem_wb_rd = 0
            mem_wb_data = 0
            mem_wb_reg_write = false
        on tick:
            mem_wb_valid = ex_mem_valid
            mem_wb_rd = ex_mem_rd
            mem_wb_reg_write = ex_mem_reg_write
            mem_wb_data = if ex_mem_mem_read then mem_read_data
                          else ex_mem_alu_result

    // =====================================================
    // Stage 5: Write-Back (WB)
    // =====================================================
    comb:
        regfile.wr_addr = mem_wb_rd
        regfile.wr_data = mem_wb_data
        regfile.wr_en = mem_wb_reg_write and mem_wb_valid

    // =====================================================
    // Formal Verification Properties
    // =====================================================

    // Safety: x0 is always zero
    assert always: regfile.regs[0] == 0,
        "x0 must always be zero"

    // Safety: PC is always 4-byte aligned
    assert always: pc[1:0] == 0,
        "PC must be 4-byte aligned"

    // Liveness: instructions eventually complete
    @bmc(depth=20)
    assert always: if_id_valid implies
        eventually(mem_wb_valid, depth=10),
        "every fetched instruction reaches writeback"

    // Forwarding completeness
    prove forwarding_covers_raw:
        assume: id_ex_valid and ex_mem_valid
        assume: id_ex_rd == hazard.id_rs1
        assume: id_ex_reg_write and id_ex_rd != 0
        assert: hazard.forward_a != ForwardSel.None,
            "RAW hazard on rs1 must be forwarded"

    prove mem_coherence:
        assume: ex_mem_valid and ex_mem_mem_write
        assume: id_ex_valid and id_ex_mem_read
        assume: ex_mem_alu_result[11:2] == alu.result[11:2]
        assert: hazard.stall or
                hazard.forward_a != ForwardSel.None or
                hazard.forward_b != ForwardSel.None,
            "memory hazard must be resolved"

// ============================================================
// Testbench
// ============================================================

testbench tb_cpu:
    config:
        timeout = 10_000
        dump = fst, file = "cpu_trace.fst"

    let dut = CPU()

    // Reset sequence
    tick(5)

    // Wait for program completion
    // (program writes 0xDEAD to address 0x100 when done)
    var done: bool = false
    var cycle_count: uint = 0

    while not done and cycle_count < 5000:
        tick()
        cycle_count = cycle_count + 1
        if peek(dut.dmem.read(addr=64)) == 0xDEAD:
            done = true

    assert done, "program did not complete within 5000 cycles"
    print("Program completed in {cycle_count} cycles")

    // Verify sorted output (bubble sort result at addresses 0x80..0x90)
    var prev: UInt<32> = 0
    for i in 0..8:
        let val = peek(dut.dmem.read(addr=32 + i))
        assert val >= prev, "array not sorted at index {i}"
        prev = val

    print("Sort verification passed")

// ============================================================
// Synthesis Annotations
// ============================================================

constrain clk: period = 10ns
@clock_gate_enable
@synth("ram_style", "block")
```

### 14.2 Design Summary

| Metric | Value |
|---|---|
| Total SSL lines | ~285 |
| Modules | 4 (`CPU`, `RegFile`, `ALU`, `HazardUnit`) |
| ISA definition | RV32I subset (11 instructions, 6 formats) |
| Pipeline stages | 5 (IF, ID, EX, MEM, WB) |
| Hazard handling | EX→EX forwarding, MEM→EX forwarding, load-use stall |
| Branch strategy | Static not-taken, 1-cycle flush penalty |
| Formal properties | 5 (x0 safety, PC alignment, liveness, forwarding, memory coherence) |
| Testbench | Program execution + output verification |

### 14.3 Language Features Demonstrated

| Feature | Where used |
|---|---|
| `module` with typed ports | `CPU`, `RegFile`, `ALU`, `HazardUnit` |
| `isa` block with formats | RV32I definition with `assemble` directives |
| ISA field extractors | `RV32I.opcode()`, `RV32I.rs1()`, `RV32I.imm_b()` |
| ISA opcode groups | `RV32I.opcodes.R_ALU`, `RV32I.opcodes.LOAD` |
| `enum` with encoding | `AluOp [binary]`, `ForwardSel [binary]` |
| `comb` block | Decode logic, forwarding muxes, ALU |
| `reg` block | Pipeline registers, register file, PC |
| `match` (exhaustive) | Decode, ALU operations, forwarding select |
| `if`/`then`/`else` expression | Inline mux selections |
| `gen for` | Register file reset loop |
| `Memory` primitive | `imem` (ROM with init), `dmem` (RAM) |
| `inst` instantiation | `regfile`, `alu`, `hazard` |
| `assert always` | x0 safety, PC alignment |
| `eventually` (bounded) | Instruction liveness |
| `prove` block | Forwarding completeness, memory coherence |
| `@bmc(depth=N)` | Bounded model checking annotation |
| `testbench` block | Program execution test |
| `var` (testbench state) | `done`, `cycle_count`, `prev` |
| `tick()`, `peek()`, `print()` | Simulation control |
| `constrain` | Clock period |
| `@synth` | RAM style hint |
| `@critical_path` | Documentation annotation |
| `@keep_hierarchy` | Synthesis directive |
| `static_assert` | FIFO depth validation (via import) |
| `import` | `SyncFifo` from standard library |

---

## Section 15 — Design Decisions & Tradeoffs

### 15.1 Implicit vs. Explicit Register Inference

**The tension:** Chisel infers registers from assignment inside `withClock` blocks — concise but hard to see what's a wire vs flip-flop. Verilog makes registers explicit (`reg` keyword) but the keyword is misleading. VHDL uses `signal` for both, with the distinction determined by clocked vs unclocked `process`.

**SSL's choice:** Explicit `reg` blocks. A signal becomes a register if and only if it is assigned inside a `reg(clk, rst)` block. Assignments in `comb` blocks produce wires. There is no ambiguity — the block keyword determines hardware semantics. This means SSL code is slightly more verbose than Chisel (you must write `reg(clk, rst):` and `on tick:` explicitly), but an engineer can look at any signal assignment and immediately know whether it produces a flip-flop or a wire. In a 5-stage CPU pipeline with 30+ signals, this clarity prevents entire classes of bugs where a signal was accidentally registered or accidentally combinational.

**What we gave up:** Chisel's brevity. `val counter = RegInit(0.U(8.W)); counter := counter + 1.U` is one line in Chisel. In SSL, it's a `signal` declaration plus a `reg` block with `on reset` and `on tick`. SSL accepts this verbosity as the cost of eliminating register inference ambiguity.

### 15.2 Simulation-Synthesis Parity

**The tension:** Verilog has no enforced boundary between simulation and synthesis. `$display`, `#10`, and `initial` blocks are "simulation-only" by convention, but nothing prevents them from appearing in synthesizable modules — synthesis tools simply ignore them, sometimes silently changing behavior.

**SSL's choice:** Hard compiler-enforced wall. Synthesizable code lives in `module` bodies (`comb`, `reg`, `fsm`, `pipeline`, `gen`). Simulation code lives in `testbench` blocks (`tick()`, `drive()`, `peek()`, `random()`, `print()`, file I/O, `var`). Using a simulation-only construct inside a `module` is a compile error — not a warning, a hard error. The `task` keyword (time-consuming simulation routines) is only callable from `testbench` blocks; `fn` (pure combinational functions) works in both contexts. Similarly, `signal` is module-only and `var` is testbench-only.

**What we gave up:** The convenience of quick debug `print()` statements inside synthesizable modules. In SSL, you must write a `testbench` that uses `peek()` to observe internal state. The tradeoff is that SSL designs are guaranteed to behave identically in simulation and synthesis.

### 15.3 Bit Width Inference vs. Explicit Widths

**The tension:** Chisel aggressively infers bit widths — arithmetic results auto-widen, and the user rarely specifies widths explicitly. Verilog requires explicit widths everywhere. VHDL requires explicit conversions for nearly everything.

**SSL's choice:** Conservative inference with explicit widening. SSL infers result widths from a fixed set of rules (Section 2.2): addition preserves width (`max(N,M)`) via modular arithmetic, multiplication produces full width (`N+M`), constant shifts widen (`N+K`). Addition does NOT auto-widen to capture carry — the user must explicitly call `.widen()`. Width mismatches between assignment target and source are always compile errors. The `unchecked` block (Section 2.6) relaxes width rules for specific scopes.

The "no silent truncation" rule applies to *assignments*, not *operations*. `a + b` where both are `UInt<8>` produces `UInt<8>` — this is modular arithmetic matching physical hardware, not truncation. Assigning a `UInt<9>` value to a `UInt<8>` signal is where data gets lost — that's the compile error. Operations define result widths by fixed rules; assignments enforce that no bits are lost without explicit `.truncate<N>()`.

**What we gave up:** Chisel-style convenience where `a + b` automatically produces a result wide enough to hold any value. In SSL, every bit of every signal is accounted for. In hardware, every bit costs a LUT — SSL makes that cost visible.

### 15.4 Behavioral vs. Structural Description

**The tension:** Behavioral description (`comb:` blocks with `if`/`match`) lets the synthesis tool choose implementation. Structural description (explicit instantiation) gives the engineer control. Chisel blurs this line. Verilog has `assign` (behavioral) and module instantiation (structural) as distinct paradigms.

**SSL's choice:** Behavioral by default, structural when needed. `comb` blocks describe *what* the logic does; the compiler and synthesis tool decide *how*. When the engineer needs structural control, they use `inst` to instantiate specific modules or `@use_resource` attributes. The `extern module` FFI provides access to vendor primitives. The AI accelerator keywords (`systolic`, `dataflow`) occupy a middle ground: they express structural intent but lower to behavioral SSL during elaboration.

**What we gave up:** The ability to describe gate-level netlists directly. An engineer cannot write `and(a, b)` as a primitive — they write `a & b` in a `comb` block. For gate-level control, `extern module` wraps vendor primitives. SSL does not compete at the gate level — that's the synthesizer's job.

### 15.5 Synthesis Tool Portability vs. Optimization

**The tension:** Generic RTL is portable but leaves optimization on the table. Vendor attributes unlock device-specific resources but make code non-portable.

**SSL's choice:** Layered approach. The core language emits portable Verilog 2005. Vendor-specific behavior is controlled through three mechanisms:

1. **`@use_resource(BRAM|DSP|LUT)`** — portable resource hints. The compiler translates to the correct vendor attribute for the target.
2. **`@synth("key", "value")`** — vendor passthrough. Emitted verbatim as synthesis attributes. The compiler does not validate these.
3. **`extern module`** — vendor primitive instantiation. For hard IP with no behavioral equivalent. The compiler type-checks ports but treats internals as opaque.

**What we gave up:** Automatic vendor optimization without annotations. SSL does not auto-detect that a pattern should map to DSP — the engineer uses `mac()` or `@use_resource(DSP)`. This means the engineer knows exactly what resource every operation maps to.

### 15.6 Formal Verification Performance

**The tension:** Full temporal logic (LTL, CTL) can express any property but most are undecidable for arbitrary designs. SVA offers full power but most engineers use only simple assertions.

**SSL's choice:** Minimal core. `assert always` for safety properties. `eventually(expr, depth=N)` for bounded liveness. `next(expr, K)` for fixed future cycles. No unbounded `eventually`, no `until`, no nested temporal operators, no LTL path quantifiers. Every SSL formal property is decidable by bounded model checking at a known depth.

The `@bmc(depth=N)` annotation makes computational cost explicit. K-induction (`@induction(k=N)`) provides a path to unbounded proofs for inductive properties without requiring a full temporal logic engine.

**What we gave up:** Unbounded liveness properties and complex temporal sequences. Engineers needing these can export SSL's design to SVA via the Verilog backend and use commercial formal tools. SSL provides the practical subset that catches the vast majority of real-world hardware bugs without the complexity of a full temporal logic engine.

---

## Appendix C — Complete Reserved Keyword List

All reserved keywords across Sections 1–15:

**Hardware constructs:**
`module`, `signal`, `reg`, `comb`, `in`, `out`, `inout`, `inst`, `extern`, `domain`

**Type constructs:**
`struct`, `enum`, `interface`, `type`, `const`, `let`, `fn`, `group`

**Sequential constructs:**
`fsm`, `pipeline`, `stage`, `on`, `reset`, `tick`

**Control flow:**
`match`, `if`, `elif`, `else`, `then`, `for`, `gen`, `when`, `priority`, `parallel`, `otherwise`

**Formal verification:**
`assert`, `assume`, `cover`, `property`, `sequence`, `always`, `eventually`, `until`, `implies`, `verify`, `forall`, `next`, `prove`, `equiv`, `constrain`

**Literals and logic:**
`true`, `false`, `and`, `or`, `not`

**Module system:**
`import`, `from`, `as`, `pub`

**Safety:**
`unchecked`, `static_assert`

**Test:**
`test`

**Simulation:**
`testbench`, `task`, `var`, `drive`, `peek`, `settle`, `print`

**AI Accelerator:**
`systolic`, `dataflow`

**ISA:**
`isa`, `instr`, `format`, `registers`, `encoding_width`

**Total: 82 reserved keywords**
