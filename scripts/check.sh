#!/usr/bin/env bash
# The product owns its check flags and dependencies. Works from any directory.
set -euo pipefail
finance_core_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$finance_core_root"
cargo test --locked
cargo fmt -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
