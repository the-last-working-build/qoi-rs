# qoi-rs

[![CI](https://github.com/the-last-working-build/qoi-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/the-last-working-build/qoi-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

`qoi-rs` is a minimal, auditable, safe Rust port of the pinned QOI reference
encoder and decoder. The production library exposes a small in-memory API for
encoding raw RGB/RGBA pixels to QOI bytes and decoding QOI bytes back to raw
pixels, with C-reference compatibility tests, fuzzing evidence and a
reproducible benchmark report.

## Highlights

- Entirely safe production Rust, enforced with `#![forbid(unsafe_code)]`.
- Byte-for-byte encoder agreement with the pinned C implementation across
  deterministic and fuzz-generated test inputs.
- Strict malformed-input validation for headers, chunk bounds, pending runs,
  trailing data and the exact end marker.
- Deterministic C/Rust differential tests plus differential and arbitrary-input
  fuzz targets.
- Reproducible benchmark report comparing the Rust codec with the pinned C
  implementation under equivalent in-memory workloads.

## Quick Start

```bash
git clone --recurse-submodules \
  https://github.com/the-last-working-build/qoi-rs.git
cd qoi-rs
./scripts/verify.sh
```

For normal development:

```bash
cargo build
cargo test
```

## API Example

```rust
use qoi_rs::{
    Channels, ColorSpace, ImageDesc, decode, encode,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pixels = vec![255, 0, 0, 255];

    let desc = ImageDesc {
        width: 1,
        height: 1,
        channels: Channels::Rgba,
        colorspace: ColorSpace::SrgbWithLinearAlpha,
    };

    let encoded = encode(&pixels, desc)?;
    let decoded = decode(&encoded, None)?;

    assert_eq!(decoded.pixels, pixels);
    assert_eq!(decoded.desc, desc);

    Ok(())
}
```

`decode(input, None)` returns the channel count declared in the QOI header.
Passing `Some(Channels::Rgb)` or `Some(Channels::Rgba)` requests that output
layout while preserving the original header metadata in `DecodedImage::desc`.

## Compatibility

The source of truth is the pinned upstream QOI repository:

```text
97bacc86a9c4abf5a2d452102dc26546c4c670b9
```

Pinned source hashes are recorded in [SOURCE_HASHES.txt](SOURCE_HASHES.txt) and
checked by `./scripts/verify.sh`.

For valid inputs covered by deterministic and fuzz testing:

- Rust encoding matches the pinned C encoder byte-for-byte.
- Rust decodes C-produced streams to the original pixels.
- C decodes Rust-produced streams to the original pixels.
- Rust decodes Rust-produced streams to the original pixels.
- Requested RGB/RGBA output agrees between C and Rust.

## Deliberate Differences From C

The Rust decoder is stricter for malformed streams. It rejects:

- Missing or incorrect eight-byte end markers.
- Multi-byte chunks whose operands would cross the logical chunk boundary.
- Final RUN chunks that describe more pixels than the header permits.
- Extra unused chunk bytes before the end marker.

These checks are intentional safety validations and do not change behavior for
valid QOI streams. See [DECISIONS.md](DECISIONS.md) and [SPEC.md](SPEC.md).

## Validation

Deterministic integration tests compile a small C reference executable from the
pinned `reference/qoi/qoi.h` and compare C and Rust codecs across RGB/RGBA
fixtures, repeated pixels, INDEX reuse, small and large deltas, alpha changes
and edge byte values.

Fuzzing evidence from [fuzz/log.txt](fuzz/log.txt):

- Differential fuzz target: 1,502,450 executions.
- Arbitrary Rust decoder target: 1,917,549 executions.
- Combined executions: 3,419,999.
- Crashes: 0.

The fuzz-only crate links the C implementation for differential testing. The
released Rust library does not link to C.

## Performance

Corrected benchmark summary from [bench/results/results.txt](bench/results/results.txt):

| Fixture | Operation | C median | Rust median | Rust/C |
| --- | ---: | ---: | ---: | ---: |
| Flat RGBA | Encode | 1.123 ms | 4.391 ms | 3.91x |
| Flat RGBA | Decode | 1.401 ms | 5.714 ms | 4.08x |
| Gradient RGB | Encode | 6.770 ms | 8.685 ms | 1.28x |
| Gradient RGB | Decode | 3.913 ms | 8.086 ms | 2.07x |
| Noise RGBA | Encode | 4.325 ms | 8.782 ms | 2.03x |
| Noise RGBA | Decode | 4.137 ms | 9.432 ms | 2.28x |

These results were collected on one machine and are not universal performance
claims. The fair benchmark design is documented in
[bench/methodology.md](bench/methodology.md).

## Current Performance Interpretation

The port prioritizes memory safety, explicit validation and auditable
equivalence over initial optimization.

The Rust implementation is currently slower than the pinned C reference,
especially for run-heavy data. Likely optimization areas include output-buffer
initialization, per-pixel bounds checks, state-machine structure and reducing
individual `Vec::push` operations.

These optimizations were intentionally deferred until after correctness was
established through deterministic differential testing and fuzzing.

The safe Rust port produced byte-for-byte compatible encodings and equivalent
decoding results across the deterministic and fuzz-generated valid inputs
tested.

The Rust implementation is currently approximately 1.28x-4.08x slower than the
pinned C implementation across the selected benchmark workloads.

## Reproducing The Results

Run the complete verification suite:

```bash
./scripts/verify.sh
```

Run the benchmark:

```bash
cargo run --release --manifest-path bench/Cargo.toml
```

Run the five-minute fuzz targets locally with nightly Rust:

```bash
cargo +nightly fuzz run differential -- -max_total_time=300
cargo +nightly fuzz run decode_arbitrary -- -max_total_time=300
```

## Repository Layout

- `src/`: safe Rust production encoder, decoder, errors and public types.
- `tests/differential/`: deterministic C/Rust integration harness.
- `tools/c-reference/`: small C reference executable used by tests.
- `fuzz/`: cargo-fuzz package and fuzz-only C reference wrapper.
- `bench/`: standalone benchmark package, methodology and recorded results.
- `reference/qoi/`: pinned upstream C implementation as a submodule.
- `SPEC.md`: implemented QOI behavior and strict-validation rules.
- `DECISIONS.md`: architectural decisions and compatibility rationale.

## Engineering Decisions

The design record is in [DECISIONS.md](DECISIONS.md). The implemented behavior
is in [SPEC.md](SPEC.md). Together they document why the production codec is
safe Rust, why outputs are owned `Vec<u8>` values, how source and output channel
counts are represented, and where malformed-input validation is stricter than C.

## Attribution And License

This project is a Rust port of
[QOI - The Quite OK Image Format](https://github.com/phoboslab/qoi), originally
created by Dominic Szablewski and licensed under the MIT License.

The Rust port is licensed under the terms in [LICENSE](LICENSE).

## Project Goals

`qoi-rs` is a small, auditable Rust port of the pinned QOI C reference
implementation.

The project aims to:

- Preserve the reference encoder's deterministic output.
- Decode valid QOI streams equivalently to the reference implementation.
- Use safe Rust throughout the production library.
- Reject malformed inputs without out-of-bounds reads or unchecked arithmetic.
- Make compatibility decisions explicit and testable.
- Maintain reproducible differential, fuzzing and benchmark evidence.

## Non-Goals

The project does not currently aim to:

- Be the fastest available QOI implementation.
- Provide streaming I/O APIs.
- Support `no_std`.
- Replace more mature general-purpose Rust QOI crates.
- Preserve the C decoder's permissive behavior for malformed streams.

## Project Status

The QOI encoder and decoder are feature-complete for the implemented QOI 1.0
format operations.

Current status:

- All six QOI chunk operations are supported.
- The encoder is compared byte-for-byte with the pinned C encoder.
- The decoder is cross-checked against C-produced streams.
- The production library forbids unsafe Rust.
- Strict malformed-input checks are intentional and documented.
- The public API should still be considered experimental until version 1.0.

## Project Origin

This project began as a time-bounded C-to-Rust porting exercise in August 2026.
It is now maintained as an independent Rust port and codec-engineering project.
