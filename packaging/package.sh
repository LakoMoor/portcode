#!/bin/sh
# Build a release archive for one target: packaging/package.sh [target]  ->  dist/portcode-<target>.tar.gz(+.sha256)
set -eu
cd "$(dirname "$0")/.."
target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
cargo build --release --locked -p portcode-cli --target "$target"
out="dist"; mkdir -p "$out"; stage="$(mktemp -d)"
cp "target/$target/release/portcode" LICENSE NOTICE README.md "$stage/"
tar -czf "$out/portcode-$target.tar.gz" -C "$stage" portcode LICENSE NOTICE README.md
( cd "$out" && { shasum -a 256 "portcode-$target.tar.gz" 2>/dev/null || sha256sum "portcode-$target.tar.gz"; } > "portcode-$target.tar.gz.sha256" )
rm -rf "$stage"; ls -la "$out/portcode-$target.tar.gz"*
