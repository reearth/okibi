#!/usr/bin/env bash
# Builds the wasm package the services import.
#
# Three entry points, because the package has three audiences that cannot share
# a loader:
#
#   bundler/  what a bundler consumes, and what the workerd entry is built on
#   nodejs/   what vitest can import directly, so the binding is tested at all
#   workerd/  what a Worker actually runs
#
# The last one is not optional. On workerd, `import ... from "*.wasm"` yields
# an uninstantiated `WebAssembly.Module` rather than the instantiated exports
# the bundler entry assumes, so importing `bundler/` from a Worker fails with
# "__wbindgen_start is not a function" — at import time, which means the whole
# Worker, not just the tile that needed a quadkey.
#
# The output is not committed: it is a build product of crates/okibi-wasm, and
# a committed copy is a copy that can be stale while looking authoritative.
set -euo pipefail

cd "$(dirname "$0")/.."

PKG=packages/okibi

for target in bundler nodejs; do
  wasm-pack build crates/okibi-wasm \
    --target "$target" \
    --out-dir "../../$PKG/$target" \
    --out-name okibi \
    "$@"

  # wasm-pack writes a package.json per target describing that target alone.
  # The one this package publishes is authored, and lists all of them.
  rm -f "$PKG/$target/package.json" "$PKG/$target/.gitignore"
done

# The pricing tables travel with the package. `plan` needs one, and a service
# holding its own copy would be costing plans against whatever it copied and
# whenever it copied it — prices move for the vendor's reasons, and keeping
# them current is okibi's job rather than each service's.
rm -rf "$PKG/pricing"
mkdir -p "$PKG/pricing"
cp pricing/*.json "$PKG/pricing/"

# The export list is read from the bundler entry rather than written out here,
# so that adding an export to the crate does not silently fail to reach a
# Worker.
exported=$(awk '/^export \{/ { f = 1; next } f && /^\} from/ { exit } f' \
  "$PKG/bundler/okibi.js" | tr -s ' \n' ' ' | sed 's/^ *//; s/ *$//')
if [ -z "$exported" ]; then
  echo "error: could not read the export list from $PKG/bundler/okibi.js" >&2
  exit 1
fi

rm -rf "$PKG/workerd"
mkdir -p "$PKG/workerd"

# The import object key must equal the wasm module's own import descriptor,
# which wasm-bindgen emits as "./okibi_bg.js" wherever this file lives. It is
# not a specifier resolved from here.
cat > "$PKG/workerd/okibi.js" <<EOF
/* @ts-self-types="./okibi.d.ts" */
import * as glue from "../bundler/okibi_bg.js";
import wasmModule from "../bundler/okibi_bg.wasm";

// Synchronous instantiation is allowed at module scope on workerd; the same
// call inside a request handler would be rejected.
const instance = new WebAssembly.Instance(wasmModule, { "./okibi_bg.js": glue });
glue.__wbg_set_wasm(instance.exports);
instance.exports.__wbindgen_start();

export {
    $exported
} from "../bundler/okibi_bg.js";
EOF

cat > "$PKG/workerd/okibi.d.ts" <<'EOF'
/* tslint:disable */
/* eslint-disable */
export * from "../bundler/okibi.js";
EOF
