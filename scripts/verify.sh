#!/usr/bin/env bash
set -euo pipefail

git submodule update --init --recursive

sha256sum -c SOURCE_HASHES.txt

cargo fmt --check
cargo fmt --manifest-path fuzz/Cargo.toml --check
cargo fmt --manifest-path bench/Cargo.toml --check

cargo clippy --all-targets -- -D warnings
cargo clippy --release --manifest-path bench/Cargo.toml -- -D warnings

cargo test --locked
cargo build --release --locked

echo "All verification checks passed."
