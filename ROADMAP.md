# Roadmap

## 0.1.x — Correctness and API hardening

- Add rustdoc documentation for every public type and function.
- Add examples for RGB and RGBA encoding and decoding.
- Add tests using a broader external corpus of valid QOI files.
- Document exact allocation limits and failure behavior.
- Decide whether `inspect_header` belongs in the permanent public API.
- Add Windows and macOS CI coverage.

## 0.2.x — Performance investigation

- Profile encode and decode with `perf` or a sampling profiler.
- Measure bounds-check and allocation costs.
- Batch output writes where it improves generated code.
- Investigate decode-to-preallocated-buffer APIs.
- Preserve byte-for-byte encoder compatibility after every optimization.
- Repeat differential fuzzing after structural changes.

## Possible future work

- Strict and reference-compatible decoder modes.
- Caller-provided output buffers.
- Streaming APIs.
- Optional `no_std` plus `alloc` support.
- Integration with the Rust `image` ecosystem.

## Non-goals without evidence

- Claiming universal performance superiority.
- Adding unsafe code solely for benchmark improvements.
- Expanding the API before the current behavior is stable.
