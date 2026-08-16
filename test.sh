#!/usr/bin/env bash
set -euo pipefail

cargo_args=()
if [[ $# -gt 0 ]]; then
    cargo_args+=(--release)
fi

echo "=== rustfmt ==="
cargo fmt --check

echo "=== Compile ==="
cargo check "${cargo_args[@]}"

echo "=== Clippy ==="
cargo clippy --all-targets "${cargo_args[@]}" -- -D warnings

echo "=== Tests ==="
cargo test "${cargo_args[@]}"

LOX=$(realpath "$(cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')")/debug/rox

echo "=== Lox test suite ==="
(
  cd ./mothership/ || exit 1
  dart tool/bin/test.dart clox --interpreter "$LOX"
)

echo ""
echo "All checks passed."
