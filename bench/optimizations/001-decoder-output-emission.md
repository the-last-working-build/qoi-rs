# Optimization 001 - Specialized decoder output emission

## Hypothesis

The decoder spent a large share of its time making three or four separate
`Vec::push` calls per output pixel and checking the selected output channel
count inside the pixel loop. Selecting RGB or RGBA once and extending the
output with one fixed-size slice per pixel should reduce instructions and
branches without changing QOI parsing.

## Profile evidence

The pre-change profiles are documented in `bench/profiles/analysis.md`.
`perf report --no-children` attributed 99.89% of flat RGBA decode samples to
the Rust decoder. The leading inlined frame was `Vec::push`'s `push_mut` at
47.38%; other disjoint push internals added 17.93%. The corresponding output
path accounted for 36.82% of noise RGBA decode samples.

For 500 flat decode operations, Rust retired 29.334B instructions and 4.272B
branches versus C's 5.732B instructions and 1.143B branches. This was a larger
and more specific verified cost than decoder allocation or RUN continuation.

Post-change sampling used the same symbolized release build and 2,000-iteration
profile commands. The former multi-push frames disappeared. Flat decode moved
`Decoder::next_pixel` to the leading symbol at 52.99%. Noise decode attributed
80.07% to `Decoder::next_pixel` and only 19.71% to the outer decode/output path.

## Change

`decode` now matches `Channels::Rgb` or `Channels::Rgba` once before decoding
pixels. Each specialized loop appends `[r, g, b]` or `[r, g, b, a]` with one
`extend_from_slice` call. The old per-pixel `write_pixel` helper and its
individual pushes were removed.

This is one optimization category: decoder output specialization. No encoder,
opcode parser, allocation bound, or benchmark timing behavior changed.

## Correctness impact

Output byte order and requested-channel behavior are unchanged. The change is
after `Decoder::next_pixel`, so chunk parsing, run accounting, cursor checks,
trailer validation, and malformed-input errors retain their existing paths.

- `./scripts/verify.sh`: passed (42 unit tests, deterministic C/Rust
  differential test, doc tests, reference integrity checks, and release builds)
- `cargo +nightly fuzz run differential -- -max_total_time=60`: 280,258 runs
  in 61 seconds, no crash
- `cargo +nightly fuzz run decode_arbitrary -- -max_total_time=60`: 293,515
  runs in 61 seconds, no crash
- Production remains under `#![forbid(unsafe_code)]`
- Public API and production dependency set are unchanged

## Benchmark environment

- Baseline implementation: `111b9a0` (`v0.1.0`), clean tree
- Optimized implementation: `0ba41ef1364a00385113f5c60e82d07d8c86eab8`, clean tree
- OS: Arch Linux under WSL2, Linux `6.18.33.2-microsoft-standard-WSL2`
- CPU: Intel Core i7-10750H at 2.60 GHz, 3 cores / 6 logical CPUs exposed
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`
- C: `cc (GCC) 16.1.1 20260430`
- Rust build: Cargo release profile
- C build: `-O3 -DNDEBUG` through the existing `cc` build script

Each side used five independent normal benchmark runs. Every run used 10
warm-ups and 100 measured samples per fixture/operation. C and Rust alternated
paired order, and each timed operation included allocation, codec work,
constant-time observation, and deallocation.

The baseline raw files retained under `/tmp/qoi-baseline` contain an old
compile-time banner (`c2e0daa`, `feat/submission`) because Cargo reused the
existing benchmark build. The actual checkout was verified clean at `111b9a0`
before the runs. There is no `src` or `bench` diff between `c2e0daa` and
`111b9a0`, so the timed implementation and harness are the release baseline.
The optimized run used a clean build and records `0ba41ef` correctly.

## Before

Medians across five runs:

| Fixture | Operation | C median | Rust median | Rust/C |
| --- | --- | ---: | ---: | ---: |
| Flat RGBA | Encode | 998,300 ns | 3,910,800 ns | 3.917x |
| Flat RGBA | Decode | 1,363,900 ns | 5,642,501 ns | 4.137x |
| Gradient RGB | Encode | 6,629,800 ns | 8,537,399 ns | 1.288x |
| Gradient RGB | Decode | 3,908,100 ns | 8,053,901 ns | 2.061x |
| Noise RGBA | Encode | 4,286,802 ns | 8,712,304 ns | 2.032x |
| Noise RGBA | Decode | 3,777,701 ns | 8,569,903 ns | 2.269x |

## After

Medians across five runs:

| Fixture | Operation | C median | Rust median | Rust/C |
| --- | --- | ---: | ---: | ---: |
| Flat RGBA | Encode | 981,297 ns | 3,825,302 ns | 3.898x |
| Flat RGBA | Decode | 1,357,300 ns | 3,149,300 ns | 2.320x |
| Gradient RGB | Encode | 6,813,704 ns | 8,901,006 ns | 1.306x |
| Gradient RGB | Decode | 3,937,000 ns | 8,205,599 ns | 2.084x |
| Noise RGBA | Encode | 4,366,001 ns | 9,209,102 ns | 2.109x |
| Noise RGBA | Decode | 3,706,100 ns | 6,265,200 ns | 1.691x |

## Result

| Fixture | Operation | Rust change | Rust/C change |
| --- | --- | ---: | ---: |
| Flat RGBA | Encode | 2.19% faster | 3.917x to 3.898x |
| Flat RGBA | Decode | **44.19% faster** | **4.137x to 2.320x** |
| Gradient RGB | Encode | 4.26% slower | 1.288x to 1.306x |
| Gradient RGB | Decode | 1.88% slower | 2.061x to 2.084x |
| Noise RGBA | Encode | 5.70% slower | 2.032x to 2.109x |
| Noise RGBA | Decode | **26.89% faster** | **2.269x to 1.691x** |

The intended RGBA decode workloads improve well beyond the 5% keep threshold.
Gradient RGB decode is 1.88% slower in absolute Rust time and 1.12% worse by
the paired ratio. Its before and after run ranges overlap, so this is treated as
noise or a small possible regression rather than hidden. Encoder code did not
change; its absolute shifts track same-direction C shifts and broad run ranges.

Rust run-to-run ranges (minimum to maximum median) were:

| Fixture | Operation | Before range | After range |
| --- | --- | ---: | ---: |
| Flat RGBA | Encode | 3,863,501-4,079,001 ns | 3,744,100-4,430,701 ns |
| Flat RGBA | Decode | 5,393,601-5,809,803 ns | 3,099,103-3,342,290 ns |
| Gradient RGB | Encode | 8,439,002-9,383,304 ns | 8,601,506-9,146,773 ns |
| Gradient RGB | Decode | 7,905,000-8,076,703 ns | 7,979,805-8,688,974 ns |
| Noise RGBA | Encode | 8,465,001-9,437,800 ns | 8,971,402-9,821,200 ns |
| Noise RGBA | Decode | 8,201,704-8,718,699 ns | 6,236,902-7,590,502 ns |

The slow fourth optimized noise-decode run affected both C and Rust. The
five-run median remains close to the other four Rust samples and the paired
ratio improvement is repeatable.

## Rejected alternatives

- Batch RUN decoding was not attempted. Pre-change `next_pixel` accounted for
  only 16.23% of flat decode, while output emission was about 65%, and batching
  would change more decoder state logic.
- Encoder channel specialization was not attempted because the initial profile
  did not identify the channel decision as the dominant encoder frame.
- Batched encoder chunk writes were not combined with this change; they are a
  separate optimization category and need their own measurement.

## Remaining bottlenecks

After output specialization, `Decoder::next_pixel` dominates both sampled
decode fixtures: 52.99% for flat RGBA and 80.07% for noise RGBA. A separately
profiled follow-up should examine opcode dispatch and safe operand reads on
noise, and RUN continuation overhead on flat, before choosing between them.

The encoder remains 1.3x to 3.9x slower than C. Its earlier profiles show
different behavior by fixture, so its next change should be selected from a
focused encoder experiment rather than included here.
