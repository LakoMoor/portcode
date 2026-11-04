#!/bin/sh
# usage: run_cases.sh <wordstat-binary> <out-dir>   (run from examples/wordstat)
# Each case is recorded as: command line, stdout, stderr (path-normalised), exit code.
bin="$1"; out="$2"; mkdir -p "$out"
run() { name="$1"; shift
  { echo "\$ wordstat $*"; echo "--- stdout"; "$bin" "$@" 2>/tmp/ws_err.$$; code=$?; echo "--- stderr"; cat /tmp/ws_err.$$; echo "--- exit $code"; } > "$out/$name.txt"
  rm -f /tmp/ws_err.$$; }
run default      inputs/sample.txt
run top3         --top 3 inputs/sample.txt
run minlen5      --min-len 5 --top 4 inputs/sample.txt
run empty        inputs/empty.txt
run missing      inputs/does-not-exist.txt
run usage
