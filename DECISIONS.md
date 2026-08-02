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

This is an intentional divergence for malformed input. It does not affect
behavioral equivalence for valid QOI streams.
