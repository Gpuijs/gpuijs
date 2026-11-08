#!/bin/sh
# Builds the release host for both Mac architectures into npm/darwin-*, then publishes those
# packages and js/ to npm. Bump the version in js/package.json, its optionalDependencies and
# npm/*/package.json together first. `scripts/publish.sh --dry-run` packs without publishing.
set -e
cd "$(dirname "$0")/.."
for arch in arm64:aarch64 x64:x86_64; do
  cargo build --release --target "${arch#*:}-apple-darwin"
  cp "target/${arch#*:}-apple-darwin/release/gpuijs" "npm/darwin-${arch%%:*}/gpuijs"
done
for dir in npm/darwin-arm64 npm/darwin-x64 js; do cp LICENSE "$dir/"; (cd "$dir" && npm publish --access public "$@"); done
