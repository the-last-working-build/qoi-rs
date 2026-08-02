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
