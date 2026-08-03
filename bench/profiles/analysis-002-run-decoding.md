# RUN continuation profile

Profiles were collected from clean commit
`dadc968941fe37ef3cdee490b568caf19e5a4c51`, the merge commit for
optimization 001. This is the current `main` implementation, not `v0.1.0`.
The working tree was clean before verification, benchmarking, and profiling.

## Environment

- Host: WSL2, Linux `6.18.33.2-microsoft-standard-WSL2`
- CPU: Intel Core i7-10750H at 2.60 GHz, 3 cores / 6 logical CPUs exposed
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`
- C: `cc (GCC) 16.1.1 20260430`
- Profiler: `perf version 7.0.10-1`

`./scripts/verify.sh` passed before measurement: 42 unit tests, the
deterministic C/Rust differential test, release builds, formatting, Clippy,
reference-source hashes, and the safe-Rust checks all remained clean.

## Current-main benchmark baseline

Five independent benchmark processes used 10 warm-up and 100 measured
iterations for every fixture/operation. Each cell below is the median of the
five per-process medians. The range is the minimum and maximum per-process
median. `Paired Rust/C` is the median of the five within-process ratios; the
parenthetical value is the ratio of the five-run Rust and C medians.

| Fixture | Operation | C median (range) | Rust median (range) | Paired Rust/C range |
| --- | --- | ---: | ---: | ---: |
| Flat RGBA | Encode | 923,000 ns (896,600-974,801) | 3,628,600 ns (3,542,201-3,697,201) | 3.89x (3.77-3.95; medians 3.931x) |
| Flat RGBA | Decode | 1,348,100 ns (1,273,000-1,443,000) | 3,132,000 ns (2,927,801-3,276,800) | 2.32x (2.27-2.33; medians 2.323x) |
| Gradient RGB | Encode | 6,410,701 ns (6,365,300-6,708,801) | 8,451,402 ns (8,315,100-8,617,702) | 1.31x (1.28-1.32; medians 1.318x) |
| Gradient RGB | Decode | 3,799,101 ns (3,758,401-4,061,100) | 7,925,701 ns (7,686,602-8,448,901) | 2.08x (2.05-2.09; medians 2.086x) |
| Noise RGBA | Encode | 4,094,701 ns (4,062,600-4,156,100) | 8,571,202 ns (8,505,502-8,705,201) | 2.09x (2.07-2.10; medians 2.093x) |
| Noise RGBA | Decode | 3,617,700 ns (3,590,701-3,735,300) | 6,117,601 ns (6,091,701-6,289,301) | 1.69x (1.68-1.70; medians 1.691x) |

Raw outputs are in `/tmp/qoi-run-baseline/run-{1,2,3,4,5}.txt` and all have
checksum `0000000001a8c354`. Their compile-time banner is stale because Cargo
reused the optimization-001 benchmark binary: it reports `0ba41ef` and
`perf/first-profiled-optimization`. Git, rather than that cached banner, was
used as the provenance source. The production and benchmark sources at the
clean checkout are exactly `dadc968`.

## Profile build and commands

The profile build used release optimization, debug symbols, and frame pointers
for both implementations:

```sh
cargo clean --manifest-path bench/Cargo.toml

CFLAGS="-g -fno-omit-frame-pointer" \
CARGO_PROFILE_RELEASE_DEBUG=1 \
RUSTFLAGS="-C force-frame-pointers=yes" \
cargo build --release --manifest-path bench/Cargo.toml
```

Sampling used 3,000 selected decode operations for each implementation and
fixture:

```sh
perf record -F 999 -g --call-graph fp -o /tmp/rust-flat-run.data -- ./bench/target/release/qoi-rs-bench profile rust decode flat-rgba 3000
perf record -F 999 -g --call-graph fp -o /tmp/c-flat-run.data -- ./bench/target/release/qoi-rs-bench profile c decode flat-rgba 3000
perf record -F 999 -g --call-graph fp -o /tmp/rust-noise-run.data -- ./bench/target/release/qoi-rs-bench profile rust decode noise-rgba 3000
perf record -F 999 -g --call-graph fp -o /tmp/c-noise-run.data -- ./bench/target/release/qoi-rs-bench profile c decode noise-rgba 3000
```

No samples were lost. The captures contained 10,354 Rust-flat, 4,028 C-flat,
22,447 Rust-noise, and 12,176 C-noise samples. Full reports, source-line
reports, and Rust annotations are retained under `/tmp` with the corresponding
`*-run.txt`, `*-run-lines.txt`, and `*-run-annotate.txt` names.

## Function-level sampling

| Workload | Dominant function | Self samples |
| --- | --- | ---: |
| Rust flat RGBA decode | `Decoder::next_pixel` | 51.46% |
| C flat RGBA decode | `qoi_decode` | 99.66% |
| Rust noise RGBA decode | `Decoder::next_pixel` | 80.74% |
| C noise RGBA decode | `qoi_decode` | 99.69% |

Function-level attribution alone cannot distinguish a RUN continuation from
chunk parsing. The annotated Rust instructions provide that distinction.

## Isolating the RUN-continuation path

For flat RGBA, `perf annotate` attributed 5,320 samples locally within
`Decoder::next_pixel`. The assembly generated for the early-return block was
separate from the opcode path:

| Instruction group | Local `next_pixel` samples | Interpretation |
| --- | ---: | --- |
| Load/test `run_remaining` | 9.23% | Entry check; executed for every call |
| Decrement/store remaining count | 9.56% | RUN-continuation-only work |
| Load `previous` and jump to the return pack | 25.37% | RUN-continuation-only return path |
| Common `Pixel`/`Result` return packing and `ret` | 45.48% | Paid once for every `next_pixel` call |
| Chunk read, opcode dispatch, state update, and hashing | about 10.36% | Non-continuation path on the flat stream |

The entry-through-early-return instructions account for 44.16% of local
`next_pixel` samples before the common return packing. Flat RGBA has one
million pixels and RUN chunks encode up to 62 pixels, so approximately 98.4%
of `next_pixel` invocations are continuation invocations. Allocating the common
return pack by invocation count therefore puts about 89% of local
`next_pixel` samples, or approximately 46% of all flat-decode samples, on work
that span decoding can avoid. This estimate is intentionally narrower than
claiming that all 51.46% of `next_pixel` belongs to RUN continuation.

Noise RGBA is the control. `next_pixel` still accounts for 80.74% of its
samples, but the decrement, store, previous-pixel load, and continuation jump
are effectively unsampled. Its local work instead falls on safe chunk reads,
RGBA operand reads, opcode dispatch, `Pixel::hash`, and index-table updates.
The entry load/test of `run_remaining` accounts for about 5.21% of local
`next_pixel` samples, but that is only the always-executed check, not evidence
of useful RUN compression.

Outer fixed-channel emission accounts for 48.41% of flat samples and 19.05%
of noise samples. Allocation and destruction remain individually below the
sampling resolution. This experiment does not change any of those paths.

## Hardware-counter baseline

Each row is the mean of ten repetitions of 500 selected decodes. Instructions
and branches varied by less than 0.01%; cache misses were noisier and are
directional only.

| Implementation/workload | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses | Runtime |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust flat RGBA | 5.739B | 21.505B | 3.746B | 8.491M | 79.879M | 27.628M | 1.691 s |
| C flat RGBA | 2.445B | 5.717B | 1.142B | 0.036M | 78.674M | 10.200M | 0.652 s |
| Rust noise RGBA | 12.257B | 46.026B | 6.334B | 2.072M | 178.778M | 112.657M | 3.757 s |
| C noise RGBA | 7.105B | 27.668B | 3.717B | 2.055M | 177.032M | 110.352M | 2.262 s |

The flat instruction and branch gaps are consistent with the repeated
state-machine calls observed in annotated assembly. Noise provides a control
for opcode parsing and safe operand reads, where a RUN-span optimization should
have little direct benefit.

## Decision

RUN continuation is sufficiently material to justify an experiment.

The experiment will replace only the internal per-pixel decoder result with a
pixel span and use straightforward repeated fixed-width appends. Opcode
dispatch, operand reads, allocation, output-buffer preinitialization, bulk
slice filling, stack buffers, encoder code, and the public API remain out of
scope.

## Experimental outcome

The experiment confirmed the mechanism on flat RGBA but failed the control
criteria. Flat Rust instructions fell 74.23%, branches 68.30%, and benchmark
time 73.22%. Gradient RGB decode regressed 16.59%, noise RGBA decode regressed
14.83%, and their paired Rust/C ratio ranges did not overlap the baseline.
Noise instructions increased 5.71% and branches 24.88%, consistent with paying
the new remaining-count check and inner count loop for count-one spans.

The production and test changes were therefore reverted. Full results and the
compatibility/fuzzing record are in
`bench/optimizations/002-batch-run-decoding.md` and
`bench/results/optimization-002.txt`.
