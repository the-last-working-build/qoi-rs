# Analysis 003 — Safe RGBA operand loading

## Environment

- Production implementation: `395fbf8ac00fad46046e29d5188f295914543d40`
  (merged optimization-002 record on `main`)
- Production tree state before measurement: clean
- Analysis-only change during profiling: the untimed `inspect` subcommand in
  `bench/src/main.rs`; there were no `src/` changes
- Host: WSL2, Linux `6.18.33.2-microsoft-standard-WSL2`
- Distribution: Arch Linux
- CPU: Intel Core i7-10750H at 2.60 GHz, 3 cores / 6 logical CPUs exposed
- Architecture: x86_64
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`, LLVM 22.1.2
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`
- C: `cc (GCC) 16.1.1 20260430`
- Profiler: `perf version 7.0.10-1`

The five normal baseline sessions were collected after a clean benchmark
rebuild. Every output banner records commit `395fbf8`, branch
`perf/rgba-operand-loading`, and a clean tree.

## Commands

Opcode distribution was inspected outside all timed loops:

```sh
cargo run --release --manifest-path bench/Cargo.toml -- inspect noise-rgba
cargo run --release --manifest-path bench/Cargo.toml -- inspect flat-rgba
cargo run --release --manifest-path bench/Cargo.toml -- inspect gradient-rgb
```

The symbolized release build used frame pointers for Rust and C:

```sh
cargo clean --manifest-path bench/Cargo.toml

CFLAGS="-g -fno-omit-frame-pointer" \
CARGO_PROFILE_RELEASE_DEBUG=1 \
RUSTFLAGS="-C force-frame-pointers=yes" \
cargo build --release --manifest-path bench/Cargo.toml
```

Matched Rust and C profiles used:

```sh
perf record -F 999 -g --call-graph fp \
  -o /tmp/rust-noise-rgba-read.data -- \
  ./bench/target/release/qoi-rs-bench \
  profile rust decode noise-rgba 3000

perf report --stdio --no-children \
  -i /tmp/rust-noise-rgba-read.data \
  > /tmp/rust-noise-rgba-read.txt

perf record -F 999 -g --call-graph fp \
  -o /tmp/c-noise-rgba-read.data -- \
  ./bench/target/release/qoi-rs-bench \
  profile c decode noise-rgba 3000

perf report --stdio --no-children \
  -i /tmp/c-noise-rgba-read.data \
  > /tmp/c-noise-rgba-read.txt
```

Counters used ten independent processes of 500 decodes:

```sh
perf stat -r 10 \
  -e cycles,instructions,branches,branch-misses,cache-references,cache-misses \
  -- ./bench/target/release/qoi-rs-bench \
  profile <rust|c> decode noise-rgba 500
```

Instruction-level analysis used:

```sh
perf annotate --stdio --no-source \
  -i /tmp/rust-noise-rgba-read.data \
  'qoi_rs::decode::Decoder::next_pixel'

perf annotate --stdio --no-source \
  -i /tmp/c-noise-rgba-read.data qoi_decode

objdump -Cd --demangle bench/target/release/qoi-rs-bench
```

No samples were lost. The Rust capture contained 23,639 samples, including
18,977 local samples in `Decoder::next_pixel`; the C capture contained 13,911
samples, including 13,829 local samples in `qoi_decode`.

## Opcode distribution

The inspector parses the logical chunk bytes from the C-produced QOI stream,
skips RGB as 4 bytes, RGBA as 5, LUMA as 2, and the remaining opcode families
as 1, and accounts for `RUN + 1` emitted pixels.

| Fixture | RGB | RGBA | INDEX | DIFF | LUMA | RUN | Total chunks | Emitted pixels | Chunk bytes | RGBA chunk/pixel share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Noise RGBA | 3,997 | 1,044,574 | 0 | 0 | 5 | 0 | 1,048,576 | 1,048,576 | 5,238,868 | 99.6183% |
| Flat RGBA | 1 | 0 | 0 | 0 | 0 | 16,913 | 16,914 | 1,048,576 | 16,917 | 0% |
| Gradient RGB | 0 | 0 | 0 | 0 | 1,048,575 | 1 | 1,048,576 | 1,048,576 | 2,097,151 | 0% |

Noise RGBA directly exercises the four-byte reader for 99.6183% of decoded
pixels and consumed chunks. A synthetic profiling fixture is unnecessary.

## Function and source-line sampling

`perf report --no-children` attributed 80.31% of all Rust noise-decode samples
to `Decoder::next_pixel` and 19.53% to the surrounding decode/output loop. Its
inlined-frame breakdown attributed 14.88% of total samples to
`read_operands::<4>`, 13.19% to hashing, 9.33% to the opcode-byte read, and
4.56% to the safe `get` used by that byte read.

The 14.88% source attribution is not treated as four-byte loading alone. It
contains cursor arithmetic, availability validation, cursor update, the load,
and fixed-array unpacking, and sampled instructions can skid. It is an upper
bound that justifies examining the generated basic block.

## Relevant Rust assembly

After the `0xff` dispatch comparison, LLVM emits this RGBA block (register
`rcx` is the cursor before the opcode byte, `r9 = rcx + 1` is the operand
start, and `r10` is the logical chunk length):

```text
cmp    $0xfffffffffffffffa,%rcx  # cursor + opcode + four operands overflow?
ja     error
add    $0x5,%rcx                 # cursor after opcode and operands
cmp    %r10,%rcx                 # four operands available?
ja     error
mov    %rcx,0x10(%rdi)           # one cursor store
mov    (%r8,%r9,1),%eax          # one four-byte load
mov    %eax,%ecx
shr    $0x8,%ecx
mov    %eax,%edx
shr    $0x10,%edx
jmp    common_pixel_path
```

The generic helper is completely inlined. There is one combined four-byte
bounds check, one cursor update, one 32-bit load, and no call, temporary slice,
copy loop, or residual `TryInto`/array-conversion machinery. There are not four
independent bounds checks and the decoder fields are already held in registers.
The remaining reader-specific difference is the checked-add overflow compare
and branch in addition to the required availability compare and branch.

## Relevant C assembly

The pinned C decoder validates a broad padded-input condition in its surrounding
loop. Its hot RGBA block therefore performs no per-operand availability check:

```text
movslq %r14d,%rcx
lea    0x3(%rdi),%esi
movzbl (%rbx,%rcx,1),%r10d
lea    0x2(%rdi),%ecx
movslq %esi,%rsi
movslq %ecx,%rcx
movzbl (%rbx,%rsi,1),%esi
movzbl (%rbx,%rcx,1),%r11d
lea    0x4(%rdi),%ecx
add    $0x5,%edi
movslq %ecx,%rcx
movzbl (%rbx,%rcx,1),%ecx
jmp    common_pixel_path
```

GCC uses four byte loads and several 32-to-64-bit index extensions, whereas
Rust uses one 32-bit load. C avoids both Rust safety branches here because its
reference validation model is different. That difference cannot simply be
copied: Rust must preserve exact `TruncatedChunk` behavior for zero through
three remaining operands.

## Hardware counters

Counts are `perf stat -r 10` means for 500 allocation-inclusive decodes.

| Implementation | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust | 12,408,731,582 | 46,025,678,886 | 6,334,046,378 | 2,081,270 | 180,137,107 | 121,018,068 |
| C | 7,140,166,821 | 27,667,591,520 | 3,716,607,449 | 2,058,636 | 178,440,706 | 114,474,256 |

Rust retires 1.66x the instructions, 1.70x the branches, and 1.74x the cycles
of C. Cache references are close, so this remains primarily an instruction and
control-flow investigation rather than a cache hypothesis.

## Estimated operand-loading share

Source attribution gives 14.88% of total cycles to the inlined helper. The
annotated RGBA block confirms that this includes the overflow check, bounds
check, cursor store, one load, unpacking, and the jump to common pixel work.
Because of skid and unavoidable validation/load work, 14.88% is an upper bound,
not a predicted speedup. The removable part is narrower: one overflow compare
and branch per RGBA opcode if a safe tail-based formulation lets LLVM infer
that advancing four bytes cannot overflow.

This is sufficiently frequent and locally isolated to compare one narrow safe
formulation. A candidate must still emit one availability decision, one cursor
advance, and the same 32-bit operand load; otherwise it is not preserving the
same validation semantics or LLVM has failed to improve the mechanism.

## Decision

**The safe RGBA operand-loading path is sufficiently material to justify an experiment.**

The experiment is justified by 99.6183% opcode coverage, 14.88% inlined-helper
sampling, and an identified extra overflow branch—not by assigning all of
`Decoder::next_pixel` to operand reading. If candidate generated code does not
remove meaningful work or matched counters and runtime do not improve, the
production change will be reverted and recorded as rejected.
