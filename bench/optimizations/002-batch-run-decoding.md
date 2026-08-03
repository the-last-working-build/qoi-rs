# Optimization 002 — Batch RUN decoding

## Hypothesis

After optimization 001, flat RGBA decode still invoked the full
`Decoder::next_pixel` state machine once per output pixel. Returning a complete
RUN chunk as one pixel span should avoid the continuation decrement, previous
pixel return, result packing, and function call for up to 61 continuation
pixels. The experiment was expected to reduce flat-decode instructions and
branches without materially changing gradient or noise decode.

## Pre-change profile evidence

The full pre-change investigation is in
`bench/profiles/analysis-002-run-decoding.md`. It used clean current-main commit
`dadc968941fe37ef3cdee490b568caf19e5a4c51`, a symbolized release build, and
3,000 selected operations per sample profile.

`Decoder::next_pixel` accounted for 51.46% of flat RGBA samples. Annotated
assembly separated the entry-through-early-return continuation path from chunk
parsing. The direct path was 44.16% of local `next_pixel` samples, and common
return packing was another 45.48%. Because approximately 98.4% of flat calls
were RUN continuations, about 89% of local `next_pixel` samples, or about 46%
of all flat-decode samples, were attributable to calls that span decoding
could avoid.

Noise RGBA isolated the control path: `next_pixel` accounted for 80.74%, but
the continuation decrement and return body were effectively unsampled. Noise
time was instead in opcode dispatch, safe chunk and operand reads, hashing,
and index-table updates.

RUN continuation is sufficiently material to justify an experiment.

## State-machine change

Experimental commits `3a42e7c` and `286eafd` replaced the private
`next_pixel` operation and `run_remaining` field with:

```rust
struct PixelSpan {
    pixel: Pixel,
    count: u8,
}
```

`next_span` returned `count: 1` for RGB, RGBA, INDEX, DIFF, and LUMA. RUN
returned the current previous pixel with `count: encoded_run_field + 1`, in
the range `1..=62`. The specialized RGB and RGBA loops tracked remaining
declared pixels, rejected a span larger than that remainder, and emitted the
span with a straightforward repeated `extend_from_slice` loop.

No stack buffer, repeated-slice fill, output preinitialization, opcode-dispatch
rewrite, operand-read rewrite, allocation change, unchecked indexing, SIMD,
unsafe Rust, public API change, dependency, or encoder change was included.

The experiment was later reverted because the low-RUN controls regressed. Its
implementation and tests remain visible in branch history.

## Compatibility reasoning

- A RUN span used the current `previous` pixel.
- Consuming a RUN opcode updated its hash slot exactly once, matching the old
  opcode-consumption call; repeated span emission did not update the index.
- A RUN span consumed exactly one encoded byte. Emission consumed no input.
- A span larger than the remaining declared pixel count returned
  `DecodeError::TooManyPixels`.
- Once the declared pixel count reached zero, any unconsumed chunk bytes still
  returned `DecodeError::TrailingData`.
- RGB, RGBA, and LUMA still used the existing checked operand reads and
  returned `DecodeError::TruncatedChunk` on truncation.
- Header, exact eight-byte end marker, output-size, cursor, and chunk-bound
  validation were unchanged.

The public API and production `#![forbid(unsafe_code)]` declaration were
untouched. `./scripts/verify.sh` passed on the experimental commit.

## Test additions

The experimental test commit retained all existing RUN tests and added six
focused cases:

- a RUN followed by a different opcode;
- multiple adjacent RUN chunks;
- RUN output converted to requested RGB;
- RUN output converted to requested RGBA;
- cursor invariance while a returned RUN span is emitted; and
- unused chunk data following an exactly completing RUN span.

Together with retained tests, coverage included RUN lengths 1 and 62, a RUN
exceeding the declared remainder, initial opaque black, index-slot update,
truncation, and trailing data. Unit-test count increased from 42 to 48 during
the experiment. All 48 unit tests and the deterministic C/Rust differential
test passed.

## Fuzzing

Both required sessions tested clean experimental commit
`286eafdcdec612c7ae91c4ee082a19a9db189386` with
`rustc 1.99.0-nightly (73dc9167f 2026-08-01)`,
`cargo 1.99.0-nightly (7c83d4cc0 2026-07-29)`, and cargo-fuzz 0.13.2.

| Target | Executions | Duration | Final active corpus | Coverage/features | Crashes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `differential` | 1,660,507 | 301 s | 125 inputs / 968 bytes | 206 / 839 | 0 |
| `decode_arbitrary` | 1,424,831 | 301 s | 86 inputs / 9,895 bytes | 114 / 391 | 0 |

The differential target verified byte-for-byte Rust/C encoding equality,
Rust decoding of C output, C decoding of Rust output, and Rust self-decoding.
The arbitrary target called the decoder in source, RGB-requested, and
RGBA-requested modes without a panic or sanitizer finding.

## Hardware counters before

Each row is the mean of ten repetitions of 500 selected decodes. The common
one-time setup is included.

| Implementation/workload | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses | Runtime |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust flat RGBA | 5.739B | 21.505B | 3.746B | 8.491M | 79.879M | 27.628M | 1.691 s |
| C flat RGBA | 2.445B | 5.717B | 1.142B | 0.036M | 78.674M | 10.200M | 0.652 s |
| Rust noise RGBA | 12.257B | 46.026B | 6.334B | 2.072M | 178.778M | 112.657M | 3.757 s |
| C noise RGBA | 7.105B | 27.668B | 3.717B | 2.055M | 177.032M | 110.352M | 2.262 s |

## Hardware counters after

| Implementation/workload | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses | Runtime |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust flat RGBA | 1.653B | 5.541B | 1.188B | 0.016M | 77.639M | 6.309M | 0.489 s |
| C flat RGBA | 2.462B | 5.685B | 1.137B | 0.021M | 78.731M | 13.579M | 0.716 s |
| Rust noise RGBA | 12.808B | 48.654B | 7.910B | 2.067M | 180.525M | 107.102M | 3.892 s |
| C noise RGBA | 6.922B | 27.673B | 3.720B | 2.051M | 177.534M | 87.864M | 2.120 s |

Flat Rust instructions fell 74.23%, branches 68.30%, and cycles 71.19%.
Sampling agreed: the same 3,000 flat decodes fell from 10,354 to 2,847 total
samples, while state handling fell from 51.46% in `next_pixel` to 15.54% in
`next_span`.

Noise moved the wrong way: Rust instructions increased 5.71%, branches
24.88%, cycles 4.49%, and counter runtime 3.60%. C noise instructions and
branches changed only +0.02% and +0.08%, so the Rust increase is not explained
by control drift. The new remaining-count checks and inner count loop were paid
for nearly every count-one noise span.

Cache-miss variability was high and cache misses are not used for the decision.

## Benchmark before

Five independent current-main runs used 10 warm-ups and 100 measurements for
each paired C/Rust operation. Values are medians across the five per-run
medians.

| Fixture | Operation | C median | Rust median | Paired Rust/C median |
| --- | --- | ---: | ---: | ---: |
| Flat RGBA | Encode | 923,000 ns | 3,628,600 ns | 3.89x |
| Flat RGBA | Decode | 1,348,100 ns | 3,132,000 ns | 2.32x |
| Gradient RGB | Encode | 6,410,701 ns | 8,451,402 ns | 1.31x |
| Gradient RGB | Decode | 3,799,101 ns | 7,925,701 ns | 2.08x |
| Noise RGBA | Encode | 4,094,701 ns | 8,571,202 ns | 2.09x |
| Noise RGBA | Decode | 3,617,700 ns | 6,117,601 ns | 1.69x |

## Benchmark after

| Fixture | Operation | C median | Rust median | Rust change | Paired Rust/C median | Interpretation |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| Flat RGBA | Encode | 1,042,400 ns | 3,933,700 ns | +8.41% | 3.83x | Encoder/control drift; unchanged code |
| Flat RGBA | Decode | 1,342,400 ns | 838,799 ns | **-73.22%** | **0.64x** | Intended RUN benefit |
| Gradient RGB | Encode | 6,939,200 ns | 9,032,600 ns | +6.88% | 1.30x | Encoder/control drift; unchanged code |
| Gradient RGB | Decode | 4,053,200 ns | 9,240,500 ns | **+16.59%** | **2.28x** | Repeatable material regression |
| Noise RGBA | Encode | 4,309,200 ns | 9,155,502 ns | +6.82% | 2.10x | Encoder/control drift; unchanged code |
| Noise RGBA | Decode | 3,855,600 ns | 7,024,801 ns | **+14.83%** | **1.82x** | Repeatable material regression |

The paired ratios separate implementation effects from the substantial
whole-machine drift in some rows. Flat improves from 2.32x to 0.64x. Gradient
worsens from 2.08x to 2.28x, and noise worsens from 1.69x to 1.82x. Every
post-change gradient ratio was 2.27x-2.38x versus 2.05x-2.09x before; every
post-change noise ratio was 1.81x-1.83x versus 1.68x-1.70x before.

Rust per-run median ranges were:

| Fixture | Operation | Before range | Experimental range |
| --- | --- | ---: | ---: |
| Flat RGBA | Encode | 3,542,201-3,697,201 ns | 3,745,400-4,207,599 ns |
| Flat RGBA | Decode | 2,927,801-3,276,800 ns | 828,100-1,027,501 ns |
| Gradient RGB | Encode | 8,315,100-8,617,702 ns | 8,824,100-9,340,801 ns |
| Gradient RGB | Decode | 7,686,602-8,448,901 ns | 9,072,301-9,649,100 ns |
| Noise RGBA | Encode | 8,505,502-8,705,201 ns | 9,014,901-9,319,400 ns |
| Noise RGBA | Decode | 6,091,701-6,289,301 ns | 6,892,001-7,992,300 ns |

The decode ranges do not overlap for flat, gradient, or noise. Raw files are
retained at `/tmp/qoi-run-baseline/run-{1,2,3,4,5}.txt` and
`/tmp/qoi-run-after/run-{1,2,3,4,5}.txt`; every run produced checksum
`0000000001a8c354`.

## Result

The hypothesis was correct for highly RUN-compressed data: batching removed
the sampled continuation path and dramatically reduced flat instructions,
branches, and time. The general replacement was not acceptable because
count-one spans made common gradient and noise inputs materially slower.

The experiment failed the explicit keep criteria and was reverted. Final
production decoder and test code on this branch are identical to `dadc968`;
only the profile and rejected-experiment documentation remain in the net diff.

## Regressions

- Gradient RGB decode: +16.59% Rust time and a paired-ratio regression from
  2.08x to 2.28x; before/after ranges do not overlap.
- Noise RGBA decode: +14.83% Rust time and a paired-ratio regression from
  1.69x to 1.82x; before/after ranges do not overlap.
- Noise control counters: +5.71% instructions and +24.88% branches, directly
  contradicting the no-regression gate.
- Encoder movements tracked C-side/whole-machine drift and are not attributed
  to the decoder experiment.

No correctness, safety, fuzzing, API, malformed-input, or encoder-compatibility
regression was found.

## Rejected alternatives

- A stack buffer, repeated-slice bulk fill, and output preinitialization were
  deliberately excluded; each would combine a second output optimization with
  the state-machine experiment.
- Special-casing `count == 1` in the output loop could reduce the measured
  control regression, but it would be a follow-up emission optimization and
  would no longer measure the requested straightforward span design alone.
- Opcode dispatch, safe operand reads, allocation, unchecked indexing, SIMD,
  and encoder changes were outside the experiment.

## Remaining bottlenecks

On noise RGBA, safe RGBA operand reads, opcode dispatch, `Pixel::hash`, and
index updates remain the dominant decoder work. The next optimization should
profile and isolate safe four-byte RGBA operand loading or index/hash work
without changing the output loop or batching RUNs.

RUN batching should not be retried as a universal `next_span` replacement
without a design that preserves the current count-one fast path and is measured
as a distinct experiment.
