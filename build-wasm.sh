#!/usr/bin/env bash
# Build the Tacit language machine (compiler + interpreter) to WASM and stage
# the deployable files for the browser page.
set -euo pipefail
cd "$(dirname "$0")/wasm"

echo "== building tacit-wasm (wasm32-unknown-unknown) =="
cargo build --release --target wasm32-unknown-unknown

echo "== staging tacit_wasm.wasm next to index.html =="
cp target/wasm32-unknown-unknown/release/tacit_wasm.wasm tacit_wasm.wasm

echo "build-wasm: ok -> wasm/tacit_wasm.wasm"
