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
ls "$OUT"
