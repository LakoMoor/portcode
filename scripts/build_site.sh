#!/bin/sh
# Rebuilds the GitHub Pages site in docs/: guide pages from markdown, the example port site, 404.
set -eu
cd "$(dirname "$0")/.."
PC="$PWD/target/release/portcode"
for p in GETTING_STARTED:"Getting started" ARCHITECTURE:Architecture PROVIDERS:"Providers, models and open-weight tuning" OPENCODE_MAPPING:"OpenCode → PortCode mapping" ENGINES:"Engines and modes" REMOTE:"Remote control" ROADMAP:Roadmap; do
  "$PC" port page "docs/${p%%:*}.md" "docs/guide/${p%%:*}.html" --title "${p#*:}" >/dev/null
done
python3 scripts/build_ru.py >/dev/null
"$PC" port page README.ru.md docs/ru.html --title "PortCode по-русски" --css style.css --home ./ >/dev/null
"$PC" port page /dev/stdin docs/404.html --title "Page not found" --css /portcode/style.css --home /portcode/ >/dev/null <<'MD'
# Page not found

That page does not exist. Go back to the [home page](https://lakomoor.github.io/portcode/).
MD
(cd examples/wordstat && ./demo.sh >/dev/null && rm -rf ../../docs/ports/wordstat && mkdir -p ../../docs/ports && cp -R docs/site ../../docs/ports/wordstat)
echo "site ready in docs/ — open docs/index.html"
