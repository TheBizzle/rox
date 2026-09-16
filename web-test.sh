#!/usr/bin/env bash
set -euo pipefail

echo "=== Add Wasm target ==="
rustup target add wasm32-unknown-unknown

echo "=== Install Wasm converter ==="
cargo install wasm-pack

echo "=== Build library for Wasm ==="
cd library/
wasm-pack build --release --target nodejs --out-dir pkg
cd ../

echo "=== Build NodeJS/Wasm harness ==="
cd web/
npm install
npm run build

echo "=== Run test suite in Wasm ==="
npm run test

echo "=== Run benchmarks in Wasm ==="
npm run bench

cd ../

echo ""
echo "All checks passed."
