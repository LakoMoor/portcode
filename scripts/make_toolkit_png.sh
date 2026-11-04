#!/bin/sh
# Renders the deep-mode toolkit cards from docs/index.html to docs/screenshots/deep-toolkit.png (needs Google Chrome).
set -eu
cd "$(dirname "$0")/.."
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
python3 - "$PWD" "$T/tk.html" <<'PY'
import sys
root, out = sys.argv[1:3]
s = open(root + "/docs/index.html").read()
a = s.index('<div class="tk"'); b = s.index('<p class="cap">', a)
open(out, "w").write('<!doctype html><meta charset=utf-8><base href="file://%s/docs/"><link rel=stylesheet href=style.css><link rel=stylesheet href=toolkit.css><body><div class=wrap style="padding:20px 22px 12px">%s</div>' % (root, s[a:b]))
PY
"$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 --window-size=1100,1096 --screenshot="$PWD/docs/screenshots/deep-toolkit.png" "file://$T/tk.html" >/dev/null 2>&1
echo "wrote docs/screenshots/deep-toolkit.png"
