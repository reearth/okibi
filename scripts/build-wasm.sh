#!/usr/bin/env bash
# Builds the wasm package the writer imports.
#
# The output is not committed: it is a build product of crates/okibi-wasm, and
# a committed copy is a copy that can be stale while looking authoritative.
# Anything that consumes @reearth/okibi runs this first.
set -euo pipefail

cd "$(dirname "$0")/.."

exec wasm-pack build crates/okibi-wasm \
  --target bundler \
  --out-dir ../../packages/okibi/pkg \
  --out-name okibi \
  "$@"
