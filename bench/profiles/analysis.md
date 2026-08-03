# Initial codec profiles

Profiles were collected from clean commit `603af50` after adding only the
isolated profiling mode. Binary `perf.data` files and the full textual reports
are retained under `/tmp`; they are not committed.

## Environment

- Host: WSL2, Linux `6.18.33.2-microsoft-standard-WSL2`
- Distribution: Arch Linux (rolling)
- CPU: Intel Core i7-10750H at 2.60 GHz, 3 cores / 6 logical CPUs exposed
- Memory exposed to WSL: 12,249,204 KiB
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`, LLVM 22.1.2
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`
- C: `cc (GCC) 16.1.1 20260430`
- Profiler: `perf version 7.0.10-1`

The profile build used release optimization for both implementations, debug
symbols for attribution, and frame pointers for both Rust and C:

```sh
cargo clean --manifest-path bench/Cargo.toml

CFLAGS="-g -fno-omit-frame-pointer" \
CARGO_PROFILE_RELEASE_DEBUG=1 \
RUSTFLAGS="-C force-frame-pointers=yes" \
cargo build --release --manifest-path bench/Cargo.toml
```

## Sampling commands

Each command used 2,000 selected codec operations. Fixture construction,
encoded-input preparation, and one correctness check occurred before the
repeated loop.

```sh
perf record -F 999 -g --call-graph fp -o /tmp/rust-flat-encode.data -- ./bench/target/release/qoi-rs-bench profile rust encode flat-rgba 2000
perf record -F 999 -g --call-graph fp -o /tmp/c-flat-encode.data -- ./bench/target/release/qoi-rs-bench profile c encode flat-rgba 2000
perf record -F 999 -g --call-graph fp -o /tmp/rust-flat-decode.data -- ./bench/target/release/qoi-rs-bench profile rust decode flat-rgba 2000
perf record -F 999 -g --call-graph fp -o /tmp/c-flat-decode.data -- ./bench/target/release/qoi-rs-bench profile c decode flat-rgba 2000
perf record -F 999 -g --call-graph fp -o /tmp/rust-noise-encode.data -- ./bench/target/release/qoi-rs-bench profile rust encode noise-rgba 2000
perf record -F 999 -g --call-graph fp -o /tmp/c-noise-encode.data -- ./bench/target/release/qoi-rs-bench profile c encode noise-rgba 2000
perf record -F 999 -g --call-graph fp -o /tmp/rust-noise-decode.data -- ./bench/target/release/qoi-rs-bench profile rust decode noise-rgba 2000
perf record -F 999 -g --call-graph fp -o /tmp/c-noise-decode.data -- ./bench/target/release/qoi-rs-bench profile c decode noise-rgba 2000
```

Text reports were generated with:

```sh
perf report --stdio --no-children -i /tmp/<profile>.data > /tmp/<profile>.txt
```

No samples were lost in any of the eight captures. The captures ranged from
1,996 samples for C flat encode to 18,467 samples for Rust noise encode.

## Dominant functions

`perf report --no-children` attributed nearly all selected-operation samples
to the codec function, as expected from the isolated loop:

| Workload | Dominant Rust symbol | Rust samples | Dominant C symbol | C samples |
| --- | --- | ---: | --- | ---: |
| Flat RGBA encode | `qoi_rs::encode::encode` | 99.81% | `qoi_encode` | 99.11% |
| Flat RGBA decode | `qoi_rs::decode::decode` | 99.89% | `qoi_decode` | 99.48% |
| Noise RGBA encode | `qoi_rs::encode::encode` | 99.82% | `qoi_encode` | 99.56% |
| Noise RGBA decode | `qoi_rs::decode::decode` | 99.80% | `qoi_decode` | 99.51% |

The inlined Rust call paths provide the useful breakdown:

- Flat decode: `Vec::push`'s `push_mut` path was 47.38%. Other disjoint
  `Vec::push` internals (`write` and pointer access) were another 17.93%.
  `Decoder::next_pixel` was 16.23% and loop iteration was 10.30%.
- Noise decode: `push_mut` was 25.47%, with `write` and pointer access adding
  11.35%. `next_pixel` was 21.90%, `Pixel::hash` was 14.62%, and RGBA operand
  reading was 8.27%.
- Flat encode: the main encode path was 74.08% and pixel equality was 25.62%.
- Noise encode: the main path was 44.48%, pixel equality 16.30%, output
  pointer/push work 25.35%, `Pixel::hash` 10.16%, and byte writes 3.33%.

The C reference is compiled from its single-header implementation, so the
profiles attribute its inlined inner work to `qoi_encode` or `qoi_decode`.
The FFI wrappers, allocation entry points, observation, and deallocation were
individually below 1% in every C capture.

## Hardware counters

The following prefix was run for each row below, using 500 operations per
process and ten repetitions:

```sh
perf stat -r 10 \
  -e cycles,instructions,branches,branch-misses,cache-references,cache-misses \
  -- ./bench/target/release/qoi-rs-bench \
  profile <implementation> <operation> <fixture> 500
```

Counts are the mean reported by `perf stat`. `B` is billion and `M` is
million. The one-time common setup is included but is small relative to 500
codec operations.

| Implementation/workload | Cycles | Instructions | Branches | Branch misses | Cache refs | Cache misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Rust flat encode | 6.660B | 26.983B | 4.754B | 0.046M | 74.816M | 27.440M |
| C flat encode | 1.752B | 8.582B | 2.124B | 0.034M | 76.370M | 9.859M |
| Rust flat decode | 10.416B | 29.334B | 4.272B | 8.496M | 80.271M | 41.611M |
| C flat decode | 2.484B | 5.732B | 1.143B | 0.040M | 78.747M | 15.631M |
| Rust noise encode | 15.832B | 60.743B | 7.396B | 4.816M | 176.804M | 113.439M |
| C noise encode | 7.932B | 31.391B | 2.677B | 2.261M | 177.342M | 101.120M |
| Rust noise decode | 15.345B | 49.176B | 6.861B | 2.061M | 177.079M | 113.550M |
| C noise decode | 6.938B | 27.674B | 3.718B | 2.052M | 176.927M | 90.494M |

Instruction counts varied by less than 0.01% and branch counts by less than
0.01%. Cycle variability ranged from 0.11% to 1.30%. Cache-miss variability
was higher, reaching 18.48% for C flat encode and 15.93% for C flat decode, so
cache-miss differences are directional rather than decisive.

Flat decode is the clearest instruction and control-flow gap: Rust used 5.12x
the instructions, 3.74x the branches, and 4.19x the cycles of C. Noise decode
used 1.78x the instructions, 1.85x the branches, and 2.21x the cycles. Cache
reference counts were similar within each pair, which argues against allocation
or memory traffic being the primary explanation by itself.

## Selected bottleneck

The first optimization will specialize decoder output emission by output
channel count and emit each pixel as one fixed-size slice. The current hot loop
performs three or four separate `Vec::push` calls and tests the output channel
count for every pixel. That path accounts for about 65% of sampled flat-decode
cycles and 37% of noise-decode cycles when its disjoint inlined frames are
combined. It is also consistent with the elevated Rust instruction and branch
counts.

Moving the RGB/RGBA choice outside the pixel loop and using one
`extend_from_slice` per pixel should reduce capacity checks, pointer retrieval,
length updates, and the per-pixel channel branch. It does not alter opcode
parsing, malformed-input checks, public types, allocation bounds, or the safe
Rust constraint.

## Alternative hypotheses

- Batch RUN decoding could reduce `next_pixel` and loop overhead on flat input,
  but `next_pixel` accounted for only 16.23%, far below output emission. It is
  also a larger state-machine change with more malformed-input risk.
- Encoder channel specialization may address the large flat-encode gap, but the
  sample breakdown did not isolate the channel decision as the leading cost.
- Batched encoder chunk writes are plausible for noise input, where push-related
  frames are visible, but decode output emission is a larger verified share and
  applies to both compressible and incompressible streams.
- Allocation was considered because it is intentionally included in the fair
  benchmark. Allocation and deallocation symbols were below 1%, and paired
  cache-reference counts were similar, so it is not selected first.
