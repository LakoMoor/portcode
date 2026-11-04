#!/bin/sh
# Real screenshots of PortCode itself. The model in the session replay is SCRIPTED (portcode-mock-llm);
# everything the tools do (pin, survey, bash, parity, ledger) is real. Output: docs/screenshots/*.svg
set -eu
cd "$(dirname "$0")/.."
PC="$PWD/target/release/portcode"; MOCK="$PWD/target/release/portcode-mock-llm"
OUT="$PWD/docs/screenshots"; mkdir -p "$OUT"
export PORTCODE_FORCE_COLOR=1
shot() { name="$1"; title="$2"; cmd="$3"; shift 3; "$@" 2>&1 | "$PC" port render "$OUT/$name.svg" --title "$title" --command "$cmd" >/dev/null; }

W=$(mktemp -d); cp -R examples/wordstat/original examples/wordstat/inputs examples/wordstat/tests "$W/"; mkdir -p "$W/port"; cp -R examples/wordstat/port/target "$W/port/" 2>/dev/null || true
"$MOCK" examples/wordstat/demo-session.json 18080 >/dev/null & MP=$!; sleep 1
trap 'kill $MP 2>/dev/null; rm -rf "$W"' EXIT
( cd "$W" && PORTCODE_BASE_URL=http://127.0.0.1:18080/v1 "$PC" run -a port -m mock/mock-model -y "Port the wordstat CLI to Rust, following rustport." 2>&1 ) \
  | "$PC" port render "$OUT/session-port-agent.svg" --title "portcode run -a port  (scripted model replay · real tools)" --command 'portcode run -a port "Port the wordstat CLI to Rust, following rustport."' >/dev/null
shot help      "portcode --help"       "portcode --help"       "$PC" --help
shot agents    "portcode agents"       "portcode agents"       "$PC" agents
shot providers "portcode providers"    "portcode providers"    "$PC" providers
shot skill     "portcode skill list"   "portcode skill list"   "$PC" skill list
shot ledger    "portcode port ledger"  "portcode port ledger"  sh -c "cd examples/wordstat && $PC port ledger"
# --- full-screen TUI (real UI code rendered headlessly) ---
P="$PC"; L="--cwd-label ~/projects/wordstat"
mockrun() { port="$1"; shift; "$MOCK" examples/wordstat/demo-session.json "$port" >/dev/null & mp=$!; sleep 1
  ( cd "$W" && rm -rf .portcode && PORTCODE_BASE_URL="http://127.0.0.1:$port/v1" "$P" -a port -m mock/mock-model snapshot "$@" $L ); kill $mp 2>/dev/null || true; }
# deep mode needs Capstone + Unicorn (installed once into the engines folder next to the binary)
"$PC" engines install core >/dev/null 2>&1 || true
"$PC" snapshot "$OUT/tui-home.svg" --size 110x32 $L >/dev/null
"$PC" snapshot "$OUT/tui-slash.svg" --size 110x32 --typed "/" $L >/dev/null
"$PC" snapshot "$OUT/tui-provider.svg" --size 110x32 --typed "/provider " $L >/dev/null
"$PC" snapshot "$OUT/tui-first-run.svg" --size 110x32 --first-run $L >/dev/null
mockrun 18081 "$OUT/tui-session.svg" --size 132x56 --prompt "Port the wordstat CLI to Rust, following rustport." >/dev/null
mockrun 18082 "$OUT/tui-permission.svg" --size 132x56 --until-permission --prompt "Port the wordstat CLI to Rust, following rustport." >/dev/null
ls "$OUT"
"$MOCK" examples/wordstat/demo-deep.json 18083 >/dev/null & mp=$!; sleep 1
( cd "$W" && rm -rf .portcode && PORTCODE_BASE_URL="http://127.0.0.1:18083/v1" "$P" --deep -a reverse -m mock/mock-model snapshot "$OUT/tui-deep.svg" --size 132x44 --prompt "What does the original accept on the command line?" $L >/dev/null ); kill $mp 2>/dev/null || true
# workbench: real functions/Ghidra/Capstone output for the wordstat binary, with two inference records in the ledger
( cd "$W" && rm -rf .portcode \
  && "$P" tool evidence_add '{"kind":"inference","subject":"cli.options","confidence":"medium","tool":"decompile","args":"main","statement":"main reads --top and --min-len; --top defaults to 10 (iVar = 10 before the argument loop)"}' >/dev/null \
  && "$P" tool evidence_add '{"kind":"unknown","subject":"cli.locale","confidence":"low","tool":"decompile","args":"main","statement":"does main call setlocale? not seen in the first 40 lines"}' >/dev/null \
  && PORTCODE_BASE_URL="http://127.0.0.1:1/v1" "$P" -a reverse -m mock/mock-model snapshot "$OUT/tui-workbench.svg" --size 150x42 --workbench auto --wb-tab pseudo $L >/dev/null \
  && PORTCODE_BASE_URL="http://127.0.0.1:1/v1" "$P" -a reverse -m mock/mock-model snapshot "$OUT/tui-workbench-asm.svg" --size 150x42 --workbench auto --wb-tab disasm $L >/dev/null )
# /create: the guided start (real UI; the source is inspected on disk)
( cd "$W" && PORTCODE_BASE_URL="http://127.0.0.1:1/v1" "$P" -a port -m mock/mock-model snapshot "$OUT/tui-create.svg" --size 124x42 --create "website:https://example.com" $L >/dev/null \
  && PORTCODE_BASE_URL="http://127.0.0.1:1/v1" "$P" -a port -m mock/mock-model snapshot "$OUT/tui-create-look.svg" --size 124x42 --create "look-app:original/wordstat" $L >/dev/null )
# interactive demo on the site: the events of a real (scripted-model) run
"$MOCK" examples/wordstat/demo-session.json 18091 >/dev/null & mp=$!; sleep 1
( cd "$W" && rm -rf .portcode && PORTCODE_BASE_URL="http://127.0.0.1:18091/v1" "$P" run -a port -m mock/mock-model -y --json "Port the wordstat CLI to Rust, following rustport." 2>/dev/null > "$W/events.jsonl" ); kill $mp 2>/dev/null || true
python3 scripts/build_demo.py "$W/events.jsonl" "Port the wordstat CLI to Rust, following rustport."
