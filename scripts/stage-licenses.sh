#!/usr/bin/env bash
# Copies the licence files into each publishable package.
#
# npm ships one directory, and a package that says "MIT OR Apache-2.0" without
# carrying either text is making a claim the tarball cannot back up.
set -euo pipefail

cd "$(dirname "$0")/.."

for package in packages/okibi packages/okibi-writer; do
  cp LICENSE-MIT LICENSE-APACHE "$package/"
done
