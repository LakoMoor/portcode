# Rust CLI port

Completed dependency-free source port of `original/main.c` into `port/src/main.rs`.
No GPU, unsafe code, C bindings, external crates, or network access. Original source
and installed skill were left unchanged. The C input is an original synthetic fixture created for this integration test.
The fixture and Rust port are distributed under the repository Apache-2.0 license.

## Build and run

```sh
cargo build --offline --manifest-path port/Cargo.toml
printf 'right tick left tick stop tick state\n' | port/target/debug/rustport-cli
python3 tests/check.py
```

Commands: `left`, `right`, `stop`, `tick`, `state`, `save PATH`, `load PATH`.
Whitespace separates commands; every successful command prints `t x v rng`.
Exit codes: 0 EOF, 2 missing path, 3 file open failure, 4 invalid save, 5 unknown command.

## Implementation and evidence

- Survey: one C entry point, no assets/dependencies/build system. State comprises
  signed tick/position/velocity and unsigned RNG; input and save files are the only
  I/O boundaries. Rewritten as scanner, state/tick logic, and command/file dispatcher.
- Before any Rust implementation, compiled C with local `cc -std=c11 -Wall -Wextra
  -O2` and ran 13 deterministic cases. Frozen inputs, stdout, stderr, statuses,
  save bytes, fixtures, and source SHA-256 are in `tests/golden/original.json`.
  No original hook or patch was necessary: stdin is already deterministic.
- `tests/check.py` rebuilds C and compares both implementations to those frozen
  goldens, then performs two cross-language save/load continuations, 120 seeded
  300-command differential traces, 11 additional save parser cases and four token
  cases. All pass byte-for-byte. Includes direction reversal, stopping, multiple
  ticks, uint32 wrap, signs, numeric suffixes, malformed/truncated saves, absent
  paths/files, command widths, path widths, embedded NUL and all error exits.
- Debug/release builds, cargo check, fmt check, clippy with warnings denied, and
  cargo test pass offline. Cargo test has zero unit tests; substantive coverage is
  the Python process-level oracle suite.
- Explicit wrapping multiply/add preserves the uint32 LCG in debug and release.
  Scanner preserves scanf field widths and numeric prefix parsing. Unix filenames
  retain raw bytes. Saves preserve the four decimal fields and final newline.

## Limits and performance

Measured on this host: 100,000 ticks including startup, pipe input/output and Python
capture took C 0.0146 s versus Rust release 0.0175 s in the final smoke run. Earlier
runs varied substantially (Rust up to 0.2407 s); buffering was added during checks.
These single-run process timings are noisy; no throughput parity claim.

Measured parity is bounded by the listed cases and current host C library. C signed
arithmetic overflow and out-of-range scanf conversions do not provide portable
semantics; Rust explicitly wraps signed state arithmetic and numeric accumulation.
Extreme decimal strings beyond host conversion range are not claimed equivalent.
Terminal line-buffering behavior, signals/broken pipes, concurrent file mutation,
non-Unix filename encodings, and special-device I/O failures were not validated.
Rust buffers stdout and flushes on normal command-loop exit. The port uses no new
global state; no skill exceptions were needed. Nothing from the supplied CLI scope
was dropped.
