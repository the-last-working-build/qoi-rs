# Optimization 004 — Pixel hash and index update

## Hypothesis

Post-optimization-003 source profiling attributed approximately 13.72% of
noise-RGBA decode samples to `Pixel::hash`. The working hypothesis was broader:
the combined channel arithmetic, modulo/mask, index conversion, scaled address,
and four-byte index-table update might contain one safe, narrowly removable
cost. Source attribution alone was not treated as proof that the hash
arithmetic was responsible.

The investigation was restricted to this path. Opcode and operand loading,
output emission, RUN handling, allocation, cursor representation, public APIs,
and encoder behavior were not optimization targets.

## Baseline profile

The clean baseline was merge commit
`60341df081620a6154b51a98a6b6248c8cca17fe`. `./scripts/verify.sh` passed before
measurement. Five complete allocation-inclusive paired benchmark sessions are
stored in `/tmp/qoi-hash-baseline/`.

Matched 3,000-decode `perf record` captures contained 24,264 Rust samples and
14,333 C samples with zero losses. Rust placed 76.12% of samples in
`Decoder::next_pixel`; C placed 99.68% in `qoi_decode`.

Rust inline attribution placed 13.53% in `Pixel::hash`. Source lines assigned
another 10.85% to `self.index[pixel.hash()] = pixel`, but that number includes
4.06 percentage points sampled on the following result-discriminant `xor`.
Using instruction addresses instead, hash preparation/arithmetic accounts for
17.7441% of total sampled period and the three index stores for 2.8080%. The
exact assembly-bounded combined share is **20.5521%**. This is an attribution,
not an available speedup; sampling skid can still occur inside the range.

Full commands, reports, annotated periods, and disassembly are described in
`bench/profiles/analysis-004-hash-index.md` and retained under
`/tmp/qoi-hash-baseline/profile/`.

## Pixel representation

Nightly `-Zprint-type-sizes` reports `Pixel` as 4 bytes with alignment 1.
LLVM does not carry the dominant RGBA pixel as a single packed 32-bit register:
it carries `r` in `%al`, `g` in `%cl`, and adjacent `b`/`a` in `%dx`. It does,
however, exploit that partial packing for the hash and the index store.

The inlined `Pixel::hash()` reloads no fields from memory. All inputs are
already live in registers. Source `usize` arithmetic is emitted as 32-bit
arithmetic. `% 64` is already one `and $0x3f`, and the final conversion to
`usize` is free because a 32-bit register write zero-extends to 64 bits.

The index address is folded into `base + index * 4 + field offset`. The index
write is two byte stores plus one 16-bit store, not one 32-bit store and not
four byte stores. `previous` uses the same three-store shape. Pixel return
packing is a separate shift/OR sequence for the private
`Result<Pixel, DecodeError>` ABI.

## Rust generated code

The common state-update block performs the exact QOI hash with 14 register
preparation/arithmetic instructions and writes the index entry with three
stores. There are no multiply, division, remainder, conversion, component
reload, bounds-check, or standalone index-address instructions.

LLVM combines `b` and `a` through the packed `%dx` register. It computes a
term equivalent to `7 * (b + 256a) + 11a`; modulo 64, the alpha coefficient is
`7 * 256 + 11`, which is congruent to 11. This preserves
`r*3 + g*5 + b*7 + a*11 (mod 64)` while avoiding a separate `b` extraction.

The relevant listing and per-instruction sample periods are in the profile
analysis. The 17-instruction hash/index region consists of 14 arithmetic/input
preparation instructions and the three required index stores.

## C generated code

GCC keeps all four channels in separate registers. It also uses 32-bit LEA
arithmetic, one `and $0x3f`, and folded scaled addressing. Its corresponding
path is 18 instructions: 14 arithmetic/input-preparation instructions followed
by four byte stores. Rust is already one store instruction shorter.

C does not separately assign `previous` or return a Rust-style `Result`; its
current pixel and output loop live in one function. Those state-machine and
return-representation differences are out of scope. There is no cheaper C
hash/index mechanism available to port.

## Candidate experiments

- Candidate A, narrow arithmetic width, was rejected at the code-generation
  gate. The current `usize` source already emits 32-bit arithmetic and a free
  final widening, including a representation-assisted packed-channel
  optimization. A source rewrite cannot make those operations narrower.
- Candidate B, weighted wrapping-byte arithmetic, was not justified. It would
  make source reasoning less direct while LLVM already applies the useful
  modulo-64 congruence automatically.
- Candidate C, a representation-assisted helper at opcode sites, was not
  justified. Channels are already in registers and the helper is fully
  inlined; moving it would duplicate the single hash/index point across
  branches.
- Candidate D, a local index variable or rearranged assignment, was rejected
  at the code-generation gate. The masked index is already held in a register,
  its widening is free, scaled addressing is folded into the stores, and no
  bounds check remains.

No candidate was changed in production or benchmarked. The evidence identified
material mandatory work, not a narrowly removable cost. A decoder-wide packed
`u32` representation might encourage a single store, but it is explicitly
forbidden and would disturb several out-of-scope mechanisms for one remaining
instruction pair.

## Selected change or rejection

The production optimization was rejected. The only retained Rust changes are
tests. They make the existing representation and exact hash/index semantics
more explicit without changing non-test code.

## Hash equivalence

For any `u8` channel, the maximum unmasked weighted sum is
`255 * (3 + 5 + 7 + 11) = 6,630`, so it is exactly representable in both
`u32` and `usize`. Because 64 is a power of two, `sum % 64 == sum & 63` for
these non-negative unsigned values. Converting the final 0..63 result to
`usize` is exact.

The focused test compares the existing implementation, the literal QOI
formula, and the narrow candidate formula over the full Cartesian product of
the required edge set
`0, 1, 2, 15, 31, 63, 127, 128, 254, 255` (10,000 pixels), then over 2,000,000
pixels from a deterministic xorshift64* sequence. Known slots include
transparent black 0, opaque black 53, `[1,2,3,4]` 14, and opaque white 38.
`[0,0,0,0]` and `[5,1,0,4]` explicitly prove a slot-0 collision.

## Index semantics

The common decoder assignment remains untouched and therefore still executes
after every consumed opcode, including RGB, RGBA, INDEX, DIFF, LUMA, and RUN.
RUN continuations still take the existing early return and do not consume an
opcode; their pixel value and table state are unchanged.

Focused tests now cover retrieval after RGB and RGBA, replacement on a hash
collision, the otherwise-observable INDEX-to-hash-slot update, retrieval after
wrapping DIFF and LUMA channel arithmetic, opaque-black slot 53, and RUN/INDEX
interaction. Existing malformed-input and cursor tests remain unchanged.

## Tests

`./scripts/verify.sh` passes with 55 unit tests, the deterministic C/Rust
differential suite, doc tests, reference-integrity checks, and release builds.

No public API, encoder implementation, production dependency, unsafe-code
policy, allocation, decoder state-machine, or malformed-input behavior changed.

## Fuzzing

Both required libFuzzer targets completed their 300-second limits with no
crash. The checkout was based on exact commit
`60341df081620a6154b51a98a6b6248c8cca17fe`; the worktree contained only the
documented test and documentation changes, and all non-test production code was
identical to that commit.

- Nightly Rust: `rustc 1.99.0-nightly (73dc9167f 2026-08-01)`
- Nightly Cargo: `cargo 1.99.0-nightly (7c83d4cc0 2026-07-29)`
- cargo-fuzz: `0.13.2`
- `differential`: 1,534,908 executions in 301 seconds; final active corpus
  124 inputs / 928 bytes; coverage 210, feature set 863; 0 crashes
- `decode_arbitrary`: 1,359,351 executions in 301 seconds; final active corpus
  77 inputs / 6,867 bytes; coverage 117, feature set 420; 0 crashes

Raw logs are `/tmp/qoi-hash-baseline/fuzz-differential.txt` and
`/tmp/qoi-hash-baseline/fuzz-decode-arbitrary.txt`.

## Counters before

Ten runs of 500 allocation-inclusive noise-RGBA decodes produced:

| Implementation | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses | Process time |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust | 12,213,036,707 | 44,455,684,403 | 5,810,714,893 | 2,091,168 | 177,579,610 | 119,256,665 | 4.209810 s |
| C | 7,077,464,590 | 27,664,457,874 | 3,715,562,901 | 2,059,662 | 177,031,322 | 111,402,459 | 2.381604 s |

Rust retires 1.607x the C instructions, 1.564x the branches, and 1.726x the
cycles for this allocation-inclusive command. The isolated Rust hash/index
instruction sequence is nevertheless already one instruction shorter than C;
the broader gap lies elsewhere.

## Counters after

Not applicable. No production candidate passed the generated-code gate, so
there is no changed decoder binary whose counters could demonstrate a
mechanism-sized reduction. Repeating counters on identical production code
would measure only host drift.

## Benchmarks before

Every run used 10 warm-ups and 100 measured iterations per fixture/operation,
alternating paired C/Rust order and including allocation and deallocation.
Each entry is `C ns / Rust ns / exact Rust/C`.

| Fixture | Operation | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Flat RGBA | Encode | 1,026,000 / 3,968,100 / 3.867544 | 1,123,600 / 4,318,500 / 3.843450 | 1,298,700 / 5,217,400 / 4.017402 | 1,194,500 / 4,575,000 / 3.830054 | 1,085,800 / 4,055,000 / 3.734574 |
| Flat RGBA | Decode | 1,487,800 / 3,461,599 / 2.326656 | 1,596,100 / 3,686,600 / 2.309755 | 1,407,500 / 3,226,400 / 2.292291 | 1,688,600 / 3,825,101 / 2.265250 | 1,845,900 / 4,318,401 / 2.339456 |
| Gradient RGB | Encode | 7,920,299 / 10,140,599 / 1.280330 | 7,149,800 / 9,254,500 / 1.294372 | 7,694,000 / 9,891,500 / 1.285612 | 7,459,600 / 9,666,401 / 1.295834 | 9,315,500 / 11,642,300 / 1.249777 |
| Gradient RGB | Decode | 4,281,599 / 8,794,900 / 2.054116 | 4,100,100 / 8,636,201 / 2.106339 | 4,226,500 / 8,803,500 / 2.082929 | 4,196,501 / 8,774,300 / 2.090861 | 4,210,300 / 8,582,500 / 2.038453 |
| Noise RGBA | Encode | 4,671,099 / 9,930,700 / 2.125988 | 4,517,500 / 9,293,400 / 2.057200 | 4,605,401 / 9,497,400 / 2.062231 | 4,344,300 / 9,081,901 / 2.090533 | 4,623,300 / 9,551,400 / 2.065927 |
| Noise RGBA | Decode | 3,951,800 / 6,427,000 / 1.626347 | 4,621,401 / 7,241,200 / 1.566884 | 4,052,901 / 6,608,700 / 1.630610 | 4,217,500 / 6,871,401 / 1.629259 | 4,135,001 / 6,445,403 / 1.558743 |

Five-run medians are:

| Fixture | Operation | C median | Rust median | Paired Rust/C median |
| --- | --- | ---: | ---: | ---: |
| Flat RGBA | Encode | 1,123,600 ns | 4,318,500 ns | 3.843450x |
| Flat RGBA | Decode | 1,596,100 ns | 3,686,600 ns | 2.309755x |
| Gradient RGB | Encode | 7,694,000 ns | 9,891,500 ns | 1.285612x |
| Gradient RGB | Decode | 4,210,300 ns | 8,774,300 ns | 2.082929x |
| Noise RGBA | Encode | 4,605,401 ns | 9,497,400 ns | 2.065927x |
| Noise RGBA | Decode | 4,135,001 ns | 6,608,700 ns | 1.626347x |

## Benchmarks after

Not applicable. No production change was selected. The five baseline sessions
are the complete benchmark result for the final production implementation,
which is byte-for-byte unchanged. Running a nominal “after” set on the same
binary would not test an optimization category.

## Decision

**The current hash/index path is already adequately optimized; no production
change was attempted.**

The sampled region is material, but LLVM has already removed or folded every
allowed candidate cost: natural-width arithmetic, multiplication lowering,
modulo lowering, final widening, field reloads, bounds checks, and standalone
index addressing. The remaining three stores are fewer than C and follow from
the live split-channel representation. Retaining a source-only rewrite would
violate the reject criteria because it has no mechanism-sized generated-code
change.

## Regressions

No production code changed, so there is no performance or compatibility
regression. The new work is test-only and documentation-only. The structured
2,010,000-pixel hash test adds approximately 0.15 seconds to the local debug
unit-test run.

## Remaining bottlenecks

The next narrow investigation should isolate private pixel return packing and
the caller-side `Result<Pixel, DecodeError>` unpacking. The closing
`next_pixel` source line receives 13.54% of total samples and the assembly shows
multiple shifts/ORs to build the five-byte result value. This is only a
hypothesis: it must be separated from sampling skid and from the already-
retained specialized output emission before any representation or control-flow
experiment.
