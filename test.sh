#!/usr/bin/env bash
set -euo pipefail

cargo_args=()
if [[ $# -gt 0 ]]; then
    cargo_args+=(--release)
    project_version=release
else
    project_version=debug
fi

echo "=== rustfmt ==="
cargo fmt --check

echo "=== Compile ==="
cargo check "${cargo_args[@]}"

echo "=== Clippy ==="
cargo clippy --all-targets "${cargo_args[@]}" -- -D warnings

echo "=== Compile ==="
cargo build "${cargo_args[@]}"

echo "=== Tests ==="
cargo test "${cargo_args[@]}"

LOX=$(realpath "$(cargo metadata --format-version 1 --no-deps | jq -r '.target_directory')")/$project_version/rox

echo "=== Lox test suite ==="
(
  cd ./mothership/ || exit 1
  dart tool/bin/test.dart clox --interpreter "$LOX"
)

echo "=== Lox test suite (roundtripped) ==="
(
  ROUNDTRIP_WRAPPER=$(mktemp)
  trap 'rm -f "$ROUNDTRIP_WRAPPER"' EXIT

  cat > "$ROUNDTRIP_WRAPPER" <<EOF
#!/bin/sh
exec "$LOX" --roundtrip "\$@"
EOF
  chmod +x "$ROUNDTRIP_WRAPPER"

  cd ./mothership/ || exit 1
  dart tool/bin/test.dart clox --interpreter "$ROUNDTRIP_WRAPPER"
)

echo "=== Lox benchmark suite (roundtripped) ==="
(
  for file in ./mothership/test/benchmark/*.lox; do
    [ -f "$file" ] || continue
    "$LOX" --roundtrip "$file"
  done
)

echo ""
echo "All checks passed."
