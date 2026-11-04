#!/bin/sh
# Regenerates every artifact of this example from scratch with the real PortCode CLI:
# build original -> pin -> goldens -> port build -> screenshots -> parity -> fuzz -> evidence -> site.
set -eu
cd "$(dirname "$0")"
PC="${PC:-../../target/release/portcode}"
rm -rf .portcode/evidence docs/screenshots/* docs/site tests/golden/* /tmp/ws_port_out
cc -O2 -Wall -o original/wordstat original/wordstat.c
./tests/run_cases.sh original/wordstat tests/golden
(cd port && cargo build --release -q)
WS=port/target/release/wordstat

$PC port pin original/wordstat --version 1.0                                              # E-0001
$PC port shot orig-default   -s cli.default  --statement "original: report for inputs/sample.txt (exit 0)" -- original/wordstat inputs/sample.txt      # E-0002
$PC port shot port-default   -s cli.default  --statement "port: identical report for inputs/sample.txt (exit 0)" -- $WS inputs/sample.txt              # E-0003
$PC port shot orig-minlen5   -s cli.options  --statement "original: --min-len 5 --top 4" -- original/wordstat --min-len 5 --top 4 inputs/sample.txt     # E-0004
$PC port shot port-minlen5   -s cli.options  --statement "port: identical output for --min-len 5 --top 4" -- $WS --min-len 5 --top 4 inputs/sample.txt   # E-0005
$PC port shot orig-missing   -s cli.errors   --statement "original: missing file -> stderr message, exit 1" -- original/wordstat inputs/does-not-exist.txt # E-0006
$PC port shot port-missing   -s cli.errors   --statement "port: same stderr message and exit code 1" -- $WS inputs/does-not-exist.txt                    # E-0007

./tests/run_cases.sh $WS /tmp/ws_port_out
PAR=$($PC port parity tests/golden /tmp/ws_port_out | tr '\n' ' ')
$PC tool evidence_add "{\"kind\":\"observation\",\"subject\":\"parity.golden\",\"confidence\":\"high\",\"tool\":\"portcode port parity\",\"args\":\"tests/golden /tmp/ws_port_out\",\"statement\":\"$PAR\"}" >/dev/null   # E-0008
FUZZ=$(python3 -I tests/fuzz.py original/wordstat $WS 400)
$PC tool evidence_add "{\"kind\":\"observation\",\"subject\":\"parity.fuzz\",\"confidence\":\"high\",\"tool\":\"tests/fuzz.py\",\"args\":\"400 random files, seed 1234, NUL/0xFF bytes, 250-300 byte words, odd flags\",\"statement\":\"$FUZZ (stdout, stderr and exit code byte-identical)\"}" >/dev/null    # E-0009
$PC tool evidence_add '{"kind":"inference","subject":"cli.options","confidence":"medium","tool":"read original/wordstat.c","path":"original/wordstat.c","depends_on":["E-0009"],"statement":"A word is kept to at most 255 bytes (char buf[256], wordstat.c:31); longer runs still count once. Read from source; the fuzz includes 250-300 byte words and agrees."}' >/dev/null   # E-0010
$PC tool evidence_add '{"kind":"unknown","subject":"cli.locale","confidence":"low","tool":"read original/wordstat.c","statement":"The original calls isalnum()/tolower(), which are locale-dependent for bytes >= 128; all goldens were captured in the C locale. The port assumes ASCII only."}' >/dev/null  # E-0011

(cd port && cargo clippy --all-targets -q -- -D warnings && cargo test -q 2>&1 | tail -2)
$PC port site --title "wordstat — C to Rust port" --repo-url https://github.com/LakoMoor/portcode
echo "ledger:"; $PC port ledger
