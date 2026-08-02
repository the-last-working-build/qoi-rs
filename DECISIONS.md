# Architectural Decisions

## D001 — Safe Rust core

The encoder and decoder will be implemented entirely in safe Rust. The original
C implementation is retained only as a separately compiled reference executable
for differential testing. The released Rust library does not link to or invoke
the C implementation.

## D002 — Owned output buffers

The C implementation returns heap-allocated pointers. The Rust API returns
`Vec<u8>` through `Result`, transferring ownership through Rust's type system
instead of exposing allocation and deallocation functions.

## D003 — Strict structural validation

The Rust decoder validates the exact eight-byte QOI end marker and checks all
multi-byte chunks before reading their operands.

The pinned C decoder reserves the final eight bytes but does not verify their
contents, and may read beyond the logical chunk region for malformed input.
It may also ignore a final run that exceeds the declared pixel count or unused
chunk bytes before the end marker.

This is an intentional divergence for malformed input. It does not affect
behavioral equivalence for valid QOI streams.

## D004 — Preserve source and output channel counts

`ImageDesc::channels` records the channel count declared by the QOI header.

`DecodedImage::output_channels` records the actual channel count of the
returned pixel buffer. These may differ when the caller requests RGB output
from an RGBA source or RGBA output from an RGB source.

This mirrors the distinction between `desc->channels` and the `channels`
argument in the C reference API.

## D005 — Reference-oriented project scope

The project is maintained as an auditable port of a pinned QOI reference
implementation rather than as a drop-in replacement for every existing Rust
QOI library.

Correctness evidence, explicit compatibility decisions and implementation
clarity take priority over API breadth.
