# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.0] - 2026-08-02

### Added

- Safe Rust QOI encoder and decoder.
- Support for RGB, RGBA, INDEX, DIFF, LUMA and RUN chunks.
- Strict malformed-input validation.
- Byte-for-byte differential encoder testing against pinned C.
- C/Rust cross-decoding tests.
- Differential and arbitrary-input fuzz targets.
- Reproducible C-versus-Rust benchmarks.
- One-command verification workflow.
