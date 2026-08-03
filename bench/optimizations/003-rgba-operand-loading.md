# Optimization 003 — Safe RGBA operand loading

## Hypothesis

Noise RGBA decode spends measurable time validating and loading the four
operand bytes following `QOI_OP_RGBA`. A four-byte-specific safe read may let
LLVM remove redundant cursor-overflow work while preserving exact truncation
behavior. No change is justified if the generic const reader already lowers to
equally compact code.

This experiment is limited to RGBA operands. Opcode dispatch, RGB/LUMA reads,
RUN handling, output emission, hashing, index updates, allocation, public APIs,
and the encoder remain unchanged.

## Opcode distribution

An untimed `qoi-rs-bench inspect <fixture>` command parses the C-produced QOI
stream and accounts for variable chunk widths and RUN-emitted pixels.

| Fixture | RGB | RGBA | INDEX | DIFF | LUMA | RUN | Total chunks | Emitted pixels | Chunk bytes | RGBA share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Noise RGBA | 3,997 | 1,044,574 | 0 | 0 | 5 | 0 | 1,048,576 | 1,048,576 | 5,238,868 | 99.6183% |
| Flat RGBA | 1 | 0 | 0 | 0 | 0 | 16,913 | 16,914 | 1,048,576 | 16,917 | 0% |
| Gradient RGB | 0 | 0 | 0 | 0 | 1,048,575 | 1 | 1,048,576 | 1,048,576 | 2,097,151 | 0% |

Noise RGBA directly exercises the target path for 99.6183% of chunks and
decoded pixels. No profiling-only fixture was needed.

## Pre-change profile

The baseline production implementation was clean commit
`395fbf8ac00fad46046e29d5188f295914543d40`. Analysis-only inspector code was
present in the benchmark package during symbolized profiling, with no `src/`
change.

For 3,000 decodes, `perf report --no-children` attributed 80.31% of samples to
`Decoder::next_pixel`. Its inlined-frame breakdown assigned 14.88% of total
samples to `read_operands::<4>`, 13.19% to hashing, and 9.33% to the opcode-byte
read. The helper attribution includes validation, cursor work, loading,
unpacking, and sampling skid; it is not treated as pure operand-load time.

Ten 500-decode counter runs measured 46.026B Rust instructions, 6.334B
branches, and 12.409B cycles versus 27.668B instructions, 3.717B branches, and
7.140B cycles for C.

Full commands, environment, samples, C comparison, and assembly are in
`bench/profiles/analysis-003-rgba-operands.md`.

## Current generated code

The generic helper was already inlined and its slice-to-array conversion was
fully eliminated. LLVM generated one combined four-byte load—not four checked
loads—and one cursor store:

```text
cmp    $0xfffffffffffffffa,%rcx  # checked-add overflow
ja     error
add    $0x5,%rcx
cmp    %r10,%rcx                 # four-byte availability
ja     error
mov    %rcx,0x10(%rdi)
mov    (%r8,%r9,1),%eax          # one 32-bit load
mov    %eax,%ecx
shr    $0x8,%ecx
mov    %eax,%edx
shr    $0x10,%edx
jmp    common_pixel_path
```

The isolated opportunity was one overflow compare/branch in addition to the
required availability decision. C performs no per-RGBA bounds branch because
the pinned reference uses a broader padded-input loop check, but it performs
four byte loads. Rust's exact `TruncatedChunk` semantics preclude simply copying
that validation model.

## Candidate designs

- Candidate A added a four-byte helper but kept `checked_add(4)` and a checked
  cursor range. Its generated RGBA block was identical to the generic reader,
  so specialization by itself offered no mechanism and was rejected.
- Candidate B used a safe tail plus `first_chunk::<4>()`. Keeping the fixed
  array by reference until returning it generated one tail-length comparison,
  one conditional branch, three naturally packed loads, and one cursor store.
  It was selected.
- Candidate C matched a safe remaining tail as `[r, g, b, a, ..]`. It produced
  the same nine-instruction hot-block shape as Candidate B but was slightly more
  manual than the fixed-chunk expression, so it was not retained.

An eager `.copied()` variant of Candidate B retained a single 32-bit load and
unpacking shifts. It removed the branch but saved fewer instructions than
copying the fixed array at return, so it was not selected.

No candidate changed the generic reader, RGB parsing, attributes, or decoder
state representation.

## Selected experiment

`QOI_OP_RGBA` now calls a private four-byte helper:

```rust
fn read_rgba_operands(&mut self) -> Result<[u8; 4], DecodeError> {
    let operands = self
        .chunks
        .get(self.cursor..)
        .and_then(|tail| tail.first_chunk::<4>())
        .ok_or(DecodeError::TruncatedChunk)?;

    self.cursor += operands.len();

    Ok(*operands)
}
```

The successful tail check proves `cursor <= chunks.len() - 4`; advancing by
the fixed array length therefore cannot overflow. Failure occurs before the
cursor assignment. The generic helper remains unchanged for RGB.

The selected assembly is:

```text
sub    %r10,%rcx                 # remaining logical chunk bytes
cmp    $0x4,%rcx
jae    success
jmp    error
success:
lea    0x5(%r8),%rax             # opcode plus four operands
mov    %rax,0x10(%rdi)           # one cursor store
movzbl 0x1(%r9,%r8,1),%eax
movzbl 0x2(%r9,%r8,1),%ecx
movzwl 0x3(%r9,%r8,1),%edx
jmp    common_pixel_path
```

The hot successful block falls from 12 to 9 instructions and from two safety
branches to one. The expected 500-decode reductions are approximately three
instructions and one branch times 1,044,574 RGBA chunks times 500 operations;
the observed counter reductions closely match those values.

## Validation semantics

- Success reads exactly four bytes in `r, g, b, a` order and advances once by
  exactly four.
- Zero, one, two, or three available operand bytes return
  `DecodeError::TruncatedChunk`; the helper does not advance the operand cursor
  on failure.
- A four-byte sequence ending exactly at the logical chunk boundary succeeds.
- Overflow is structurally impossible after a successful four-byte tail match.
- The helper only sees the logical chunk slice, so it cannot consume the QOI
  end marker.
- Exact post-image cursor validation remains unchanged, including
  `DecodeError::TrailingData` for unused chunk bytes.
- Requested RGB output still discards alpha; requested RGBA output preserves
  it.

## Tests

Six focused tests were added, and four existing tests complete the requested
semantic matrix:

- New: zero, one, and two available operands each return `TruncatedChunk`.
- Existing: three operands return `TruncatedChunk`.
- Existing: all four operands succeed and preserve RGBA.
- New: an RGBA chunk after another opcode may end exactly at the chunk boundary.
- New: RGBA followed by another opcode leaves the cursor correctly positioned.
- New: RGBA completing the declared image still rejects unused chunk data as
  `TrailingData`.
- Existing: requested RGB output discards loaded alpha.
- Existing: requested RGBA output preserves loaded alpha.

`./scripts/verify.sh` passed first with 47 tests on the exact fuzzed/benchmarked
commit and again with the additional explicit boundary test, for 48 total unit
tests plus the deterministic C/Rust differential suite.

## Fuzzing

The production implementation and first five focused tests were fuzzed at
`263f524ea32cb4c18bba08fbbe19920ad63421d7`. The subsequent commit amendment
adds only the explicit boundary test; production code is identical.

- `rustc 1.99.0-nightly (73dc9167f 2026-08-01)`
- `cargo 1.99.0-nightly (7c83d4cc0 2026-07-29)`
- `cargo-fuzz 0.13.2`
- `differential`: 1,400,022 executions in 301 seconds; final active corpus 126
  inputs / 953 bytes; 0 crashes
- `decode_arbitrary`: 1,278,609 executions in 301 seconds; final active corpus
  75 inputs / 7,934 bytes; 0 crashes

Differential compatibility was preserved, arbitrary truncation did not panic,
strict validation remained unchanged, production still forbids unsafe Rust,
and no public API or dependency changed.

## Counters before

Ten runs of 500 allocation-inclusive noise-RGBA decodes:

| Implementation | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses | Process time |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust | 12,408,731,582 | 46,025,678,886 | 6,334,046,378 | 2,081,270 | 180,137,107 | 121,018,068 | 4.1105 s |
| C | 7,140,166,821 | 27,667,591,520 | 3,716,607,449 | 2,058,636 | 178,440,706 | 114,474,256 | 2.3604 s |

## Counters after

| Implementation | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses | Process time |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust | 12,165,300,177 | 44,455,684,377 | 5,810,714,847 | 2,084,370 | 181,292,787 | 116,941,406 | 3.9991 s |
| C | 7,113,288,774 | 27,664,457,890 | 3,715,562,923 | 2,056,340 | 179,609,736 | 107,564,513 | 2.3392 s |

Rust changed by −3.41% instructions, −8.26% branches, −1.96% cycles, and
−2.71% process time. Branch misses were effectively flat (+0.15%). C changed
by −0.01% instructions, −0.03% branches, and −0.38% cycles. Cache misses are
noisy and are not used for the decision.

The instruction reduction is 1.570B for 500 decodes, nearly the predicted
1.567B from three instructions per RGBA chunk. The branch reduction is 0.523B,
nearly the predicted 0.522B from one branch per RGBA chunk. This confirms the
operand-reader mechanism rather than unrelated elapsed-time movement.

Post-change sampling reduced the inlined helper attribution from 14.88% to
6.51% of total samples. `Decoder::next_pixel` moved from 80.31% to 76.83%.

## Benchmark before

Five-run medians of the per-run medians:

| Fixture | Operation | C median | Rust median | Median paired Rust/C |
| --- | --- | ---: | ---: | ---: |
| Flat RGBA | Encode | 1,051,400 ns | 4,032,200 ns | 3.824x |
| Flat RGBA | Decode | 1,457,600 ns | 3,323,500 ns | 2.300x |
| Gradient RGB | Encode | 6,990,600 ns | 9,099,500 ns | 1.300x |
| Gradient RGB | Decode | 4,167,500 ns | 8,645,300 ns | 2.061x |
| Noise RGBA | Encode | 4,547,300 ns | 9,330,000 ns | 2.078x |
| Noise RGBA | Decode | 3,964,900 ns | 6,727,700 ns | 1.691x |

## Benchmark after

| Fixture | Operation | C median | Rust median | Rust change | Median paired Rust/C | Interpretation |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| Flat RGBA | Encode | 1,042,800 ns | 4,090,600 ns | +1.45% | 3.893x | Encoder control drift |
| Flat RGBA | Decode | 1,526,200 ns | 3,515,300 ns | +5.77% | 2.323x | RUN-heavy control drift; ranges overlap |
| Gradient RGB | Encode | 7,045,800 ns | 9,074,799 ns | −0.27% | 1.284x | Encoder control drift |
| Gradient RGB | Decode | 4,267,000 ns | 8,706,700 ns | +0.71% | 2.070x | RGB control unchanged within overlap |
| Noise RGBA | Encode | 4,548,600 ns | 9,446,700 ns | +1.25% | 2.080x | Encoder control drift |
| Noise RGBA | Decode | 4,236,500 ns | 6,748,299 ns | +0.31% | **1.627x** | **Intended effect: paired ratio 3.78% better** |

Absolute post-change noise Rust time was flat because the median C control
slowed 6.85% across the same sessions. The paired ratio improved in all five
runs: baseline ratios ranged 1.688x–1.711x and post-change ratios ranged
1.591x–1.647x, with no overlap. The median paired improvement is 3.78%.

Every raw per-run median, five-run median, C/Rust range, paired ratio, checksum,
and exact commit is retained in `bench/results/optimization-003.txt`.

## Decision

**Retain Candidate B.**

The elapsed improvement is slightly below 5%, but it is repeatable in paired
ratios and accompanied by clear, mechanism-sized reductions of 3.41% in total
instructions and 8.26% in total branches. The code is short, entirely safe,
and makes the successful-read and failure-before-advance semantics at least as
easy to audit as the generic range reader.

The exact absolute Rust five-run median is not claimed as a speedup; it moved
by +0.31% amid measurable session drift. Retention rests on the non-overlapping
paired ratios plus assembly and counters.

## Regressions

- Flat RGBA decode moved +5.77% in absolute Rust time, but the fixture contains
  zero RGBA chunks, C moved +4.71%, the paired ratio moved only +1.03%, and
  before/after Rust ranges overlap. This is control drift, not an attributed
  decoder regression.
- Gradient RGB decode moved +0.71% in Rust time and +0.43% in paired ratio with
  overlapping ranges. It contains zero RGBA chunks.
- Encoder rows moved between −0.27% and +1.45% in Rust time. Encoder code is
  unchanged and these are controls.
- Branch misses were unchanged; cache-miss movements were noisy.

No material workload regression was isolated.

## Remaining bottlenecks

After this change, hashing is the largest separately attributed inner operation
at 13.72% of total noise-decode samples. A next experiment should isolate the
pixel hash and index-update path together, compare its arithmetic and stores to
the C loop, and only then test one narrow safe change. Opcode dispatch, output
emission, RUN handling, and operand loading should remain fixed during that
investigation.
