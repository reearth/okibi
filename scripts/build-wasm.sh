#!/usr/bin/env bash
# Builds the wasm package the services import.
#
# Two targets, because the package has two audiences that cannot share a
# loader. `bundler` is what wrangler bundles into a Worker, which is where
# projection actually runs. `nodejs` is what vitest can import directly, which
# is the only way anything here gets tested outside a browser — and an
# untested binding is where a projection quietly disagrees with the planner's.
#
# The output is not committed: it is a build product of crates/okibi-wasm, and
# a committed copy is a copy that can be stale while looking authoritative.
set -euo pipefail

cd "$(dirname "$0")/.."

for target in bundler nodejs; do
  wasm-pack build crates/okibi-wasm \
    --target "$target" \
    --out-dir "../../packages/okibi/$target" \
    --out-name okibi \
    "$@"

  # wasm-pack writes a package.json per target describing that target alone.
  # The one this package publishes is authored, and lists both.
  rm -f "packages/okibi/$target/package.json" "packages/okibi/$target/.gitignore"
done
