#!/usr/bin/env bash
# Compatibility entrypoint; use python3 install.py directly on Windows.
set -eu
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$here/install.py" "$@"
