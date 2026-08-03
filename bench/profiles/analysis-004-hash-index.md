# Analysis 004 — Pixel hash and index update

## Environment

- Baseline production commit: `60341df081620a6154b51a98a6b6248c8cca17fe`
  (merge commit for optimization 003)
- Branch: `perf/hash-index-update`
- Tree state before verification, benchmarking, and profiling: clean
- Host: WSL2, Linux `6.18.33.2-microsoft-standard-WSL2`
- Architecture: x86_64
- CPU: Intel Core i7-10750H at 2.60 GHz, 3 cores / 6 logical CPUs exposed
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`, LLVM 22.1.2
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`
- C: `cc (GCC) 16.1.1 20260430`
- Profiler: `perf version 7.0.10-1`

`./scripts/verify.sh` passed at the clean baseline with 48 unit tests, the
deterministic C/Rust differential suite, doc tests, reference-integrity checks,
and release builds.

## Commands

The five normal allocation-inclusive paired benchmark sessions were collected
after a clean benchmark rebuild:

```sh
cargo clean --manifest-path bench/Cargo.toml
cargo build --release --manifest-path bench/Cargo.toml

for run in 1 2 3 4 5; do
  ./bench/target/release/qoi-rs-bench \
    | tee "/tmp/qoi-hash-baseline/run-${run}.txt"
done
```

The symbolized release build kept frame pointers in both implementations:

```sh
cargo clean --manifest-path bench/Cargo.toml

CFLAGS="-g -fno-omit-frame-pointer" \
CARGO_PROFILE_RELEASE_DEBUG=1 \
RUSTFLAGS="-C force-frame-pointers=yes" \
cargo build --release --manifest-path bench/Cargo.toml
```

Matched Rust and C profiles used 3,000 allocation-inclusive noise-RGBA
decodes:

```sh
perf record -F 999 -g --call-graph fp \
  -o /tmp/qoi-hash-baseline/profile/rust-noise-rgba.data -- \
  ./bench/target/release/qoi-rs-bench \
  profile rust decode noise-rgba 3000

perf record -F 999 -g --call-graph fp \
  -o /tmp/qoi-hash-baseline/profile/c-noise-rgba.data -- \
  ./bench/target/release/qoi-rs-bench \
  profile c decode noise-rgba 3000
```

Reports and instruction annotations used:

```sh
perf report --stdio --no-children --inline \
  -i /tmp/qoi-hash-baseline/profile/rust-noise-rgba.data
perf report --stdio --no-children --inline \
  -i /tmp/qoi-hash-baseline/profile/c-noise-rgba.data

perf annotate --stdio --no-source --show-total-period \
  -i /tmp/qoi-hash-baseline/profile/rust-noise-rgba.data \
  'qoi_rs::decode::Decoder::next_pixel'
perf annotate --stdio --no-source --show-total-period \
  -i /tmp/qoi-hash-baseline/profile/c-noise-rgba.data qoi_decode

objdump -d --demangle=rust \
  --disassemble='qoi_rs::decode::Decoder::next_pixel' \
  bench/target/release/qoi-rs-bench
objdump -d --demangle --disassemble=qoi_decode \
  bench/target/release/qoi-rs-bench
```

Baseline counters used ten independent processes of 500 decodes:

```sh
perf stat -r 10 \
  -e cycles,instructions,branches,branch-misses,cache-references,cache-misses \
  -- ./bench/target/release/qoi-rs-bench \
  profile <rust|c> decode noise-rgba 500
```

Nightly `-Zprint-type-sizes` independently reported the private representation:

```sh
RUSTFLAGS='-Zprint-type-sizes' \
cargo +nightly rustc --lib --release -- -Zprint-type-sizes
```

## Baseline profile

No samples were lost. The Rust capture contained 24,264 samples and
73,012,619,368 approximate sampled cycles. It placed 76.12% of total samples
in `Decoder::next_pixel` and 23.69% in the surrounding `decode` loop. The C
capture contained 14,333 samples and 42,508,709,726 approximate sampled cycles;
99.68% landed in `qoi_decode`.

Rust inline-frame attribution assigned 13.53% of total samples to `Pixel::hash`.
Source-line attribution assigned 13.53% across the four hash-expression lines,
10.85% to the combined index assignment, 5.15% to `previous`, and 13.54% to
the closing line where the `Result<Pixel, DecodeError>` return is packed.

Those source percentages are not instruction boundaries. In particular, the
10.85% index-assignment line includes 4.06 percentage points sampled on the
following `xor %esi,%esi`, which begins result packing. Summing source lines
would therefore overstate the hash/index path.

Using the annotated instruction boundaries instead, the register preparation
and hash arithmetic from `movzwl %dx,%esi` through `and $0x3f,%r11d` account
for 12,955,422,457 sampled cycles, or 17.7441% of total sampled period. The
three index stores account for 2,050,193,247 sampled cycles, or 2.8080%.
The assembly-bounded combined path is therefore exactly 15,005,615,704 sampled
cycles, or **20.5521%** of the captured total period. This is an attribution,
not a speedup estimate: skid is still possible within the address range, and
most of the instructions are mandatory arithmetic, addressing, and stores.

## Pixel representation

Nightly layout output reports:

```text
print-type-size type: `types::Pixel`: 4 bytes, alignment: 1 bytes
```

The optimized Rust decoder does not carry one packed `Pixel` value through the
hot common path. For the dominant RGBA path it keeps `r` in `%al`, `g` in
`%cl`, and the adjacent `b`/`a` pair in `%dx`. This is still representation-
aware code generation: LLVM uses the packed `%dx` pair to reduce hash work and
uses a 16-bit store for `b` plus `a`.

`Pixel::hash()` is fully inlined and reloads no channel from memory. It
zero-extends the already-live component registers. The source-level `usize`
arithmetic is optimized to 32-bit operations. `% 64` is compiled as exactly
one `and $0x3f`; there is no division or remainder instruction. The final
32-bit write to `%r11d` zero-extends into `%r11`, so conversion to `usize`
requires no instruction.

The index address calculation is folded into the three stores as
`base + index * 4 + field_offset`. The four-byte `Pixel` write is emitted as
two byte stores and one 16-bit store, not as one 32-bit store and not as four
byte stores. `previous` is likewise assigned with two byte stores and one
16-bit store. Pixel return packing is separate: LLVM shifts and ORs the live
channels into the five-byte `Result<Pixel, DecodeError>` ABI value.

## Rust generated code

The dominant RGBA path enters the following common state-update block with the
pixel already in registers:

```text
mov    %al,0x118(%rdi)              # previous.r
mov    %cl,0x119(%rdi)              # previous.g
mov    %dx,0x11a(%rdi)              # previous.b + previous.a
movzwl %dx,%esi                     # retain packed b/a for hash and store
movzbl %al,%r8d
lea    (%r8,%r8,2),%r9d             # r * 3
movzbl %cl,%r10d
lea    (%r10,%r10,4),%r11d          # g * 5
mov    %esi,%ebx
shr    $0x8,%ebx                    # a
lea    (%rbx,%rbx,4),%r14d          # a * 5
lea    (%rbx,%r14,2),%ebx           # a * 11
lea    (%r11,%rdx,8),%r11d
sub    %edx,%r11d                   # 7 * packed(b,a)
add    %ebx,%r11d
add    %r9d,%r11d
and    $0x3f,%r11d
mov    %r8b,0x18(%rdi,%r11,4)
mov    %r10b,0x19(%rdi,%r11,4)
mov    %si,0x1a(%rdi,%r11,4)
```

The apparently unusual `7 * packed(b,a) + 11 * a` is exact modulo 64:
the packed high byte contributes `7 * 256 * a`, and `7 * 256 + 11` is
congruent to 11 modulo 64. LLVM has therefore found a narrower representation-
assisted formulation while preserving the exact QOI slot.

There are no component reloads, multiply instructions, remainder instructions,
index conversions, or standalone address-calculation instructions in this
block. The remaining 17 instructions are 14 register-preparation/arithmetic
instructions plus three required stores.

## C generated code

GCC keeps the four channels in separate byte registers. Its corresponding
hash/index block uses 14 register-preparation/arithmetic instructions and four
byte stores:

```text
movzbl %r10b,%r8d
lea    (%r8,%r8,2),%r15d
movzbl %r11b,%r8d
lea    (%r8,%r8,4),%r14d
lea    (%r15,%r14,1),%r8d
movzbl %sil,%r15d
lea    (%r8,%r15,8),%r8d
sub    %r15d,%r8d
mov    %r8d,%r15d
movzbl %cl,%r8d
lea    (%r8,%r8,4),%r14d
lea    (%r8,%r14,2),%r14d
lea    (%r15,%r14,1),%r8d
and    $0x3f,%r8d
mov    %r10b,-0x140(%rbp,%r8,4)
mov    %r11b,-0x13f(%rbp,%r8,4)
mov    %sil,-0x13e(%rbp,%r8,4)
mov    %cl,-0x13d(%rbp,%r8,4)
```

Both implementations use 32-bit arithmetic, one mask, and folded scaled
addressing. Rust is already one instruction shorter because its 16-bit
`b`/`a` store replaces two C byte stores. C has no distinct `previous`
assignment or Rust `Result` return packing because its decoder loop owns the
current pixel and output emission in the same function; those broader state-
machine differences are outside this experiment.

The C arithmetic instructions account for 26.0980% of total sampled C period
and its four index stores for 11.2819%, a combined 37.3799%. Cross-binary
sample percentages are not directly comparable runtime costs, but the
instruction listing provides no cheaper C hash/index mechanism to copy.

## Candidate experiments

Candidate A, narrower source arithmetic, is screened out by the generated
code: LLVM already proves the channel bounds, performs every arithmetic step
as `u32`, folds the final conversion to `usize`, and exploits the packed
`b`/`a` register modulo 64. Rewriting the source operands to `u32` cannot make
the hot arithmetic narrower than the emitted instructions.

Candidate B is not justified. Weighted wrapping-byte arithmetic would need
more difficult source-level equivalence reasoning while LLVM already obtains
the useful modulo-64 narrowing and packed-channel congruence automatically.

Candidate C is not justified. The components are already live in registers and
the inlined helper reloads nothing. Moving hashing into opcode branches would
duplicate work sites and weaken the single, auditable index-update point.

Candidate D is not justified. The existing assignment already holds the index
in `%r11`, folds `index * 4` into each store address, and emits no bounds check
because the mask proves the 0..63 range. A local source variable has no
remaining address or conversion instruction to remove.

No evidence-backed candidate exposes a safe, narrow, removable instruction.
In particular, changing `Pixel` into a decoder-wide `u32` might enable one
32-bit store but is explicitly out of scope, would disturb previous/return/
opcode representation, and is disproportionate to the three-store sequence.

## Conclusion

**The current hash/index path is already adequately optimized; no production
change was attempted.**

The assembly-bounded 20.5521% share is material work, but not material
*removable* work under the allowed categories. Rust already uses 32-bit LEA
arithmetic, a single mask, a free final index conversion, folded scaled
addressing, no component reloads, and fewer index-store instructions than C.
The final change will therefore be tests and documentation only; no candidate
will be timed as though unchanged generated code were an optimization.
