#!/bin/sh
# usage: update.sh vX.Y.Z  — rewrites portcode.rb with the version and the sha256 of each published asset.
set -eu
v="$1"; ver="${v#v}"; cd "$(dirname "$0")"
sed -i.bak "s/version \"[^\"]*\"/version \"$ver\"/; s#/download/v[0-9.]*/#/download/$v/#g" portcode.rb
for pair in AARCH64_APPLE_DARWIN:aarch64-apple-darwin X86_64_APPLE_DARWIN:x86_64-apple-darwin AARCH64_LINUX:aarch64-unknown-linux-gnu X86_64_LINUX:x86_64-unknown-linux-gnu; do
  key="${pair%%:*}"; t="${pair#*:}"
  sum="$(curl -fsSL "https://github.com/LakoMoor/portcode/releases/download/$v/portcode-$t.tar.gz.sha256" | cut -d' ' -f1)"
  sed -i.bak "s/REPLACE_$key/$sum/" portcode.rb
done
rm -f portcode.rb.bak; echo "updated portcode.rb for $v"
