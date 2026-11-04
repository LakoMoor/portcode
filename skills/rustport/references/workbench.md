# rustport Workbench — deterministic evidence and long-running ports

`workbench.py` uses only Python's standard library. It does not automatically decompile, debug or execute a game. The optional `oracle_runner.py` **will execute explicitly supplied programs** only with `--allow-execute-original`; treat untrusted games as potentially hostile and use an actual isolated VM.

## 0. Identify what exists

```bash
python3 scripts/workbench.py doctor
python3 scripts/workbench.py inspect --binary '/path/to/Game.exe' --output local-inspection.json
python3 scripts/workbench.py inventory --source '/path/to/installed-game' --output ./evidence/content.json
```

Never upload original game files, private save files, extracted proprietary assets or decompiler output into a public repository. In the *port's* root, add `.rustport/`, `reference.lock.json`, `port-ledger.sqlite*`, captured images and local evidence to `.gitignore`. Do not use demo scaffolds as game-compatibility evidence.

## 1. Start without replacing a working Rust workspace

```bash
python3 scripts/workbench.py init --project ./port --name 'My Game' --source-version '1.0'
python3 scripts/workbench.py pin --original '/path/to/Game.exe' --output ./port/reference.lock.json --version '1.0'
```

`init` writes evidence notes, a local SQLite database and example agent instructions when missing. It does not replace your existing `Cargo.toml`, application logic or original installation. The lock file records the exact file hash and machine-local path; **pinning only proves file identity**.

## 2. Register independently investigated behavior

```bash
python3 scripts/workbench.py ledger --db ./port/port-ledger.sqlite add \
  --id vehicle.brake --subsystem physics \
  --origin 'Game.exe#sha256:...:0x004a1234' \
  --target 'crates/game-sim/src/vehicle/brakes.rs::step' \
  --evidence 'docs/TRACES.md' --confidence observed

python3 scripts/workbench.py ledger --db ./port/port-ledger.sqlite set \
  --id vehicle.brake --status implemented --wired 1
```

`wired=1` is a *developer claim*, not instrumentation proof: test the actual player-facing runtime (including network, saves, fresh/load paths) separately. Agents should trace inputs and observable effects through the active path, not mark an uncalled function complete.

## 3. Capture and compare an explicitly authorized scenario

See [scenarios.md](scenarios.md). A scenario is a **local** JSON file containing `original_argv`, `port_argv`, optional `stdin_file`, `timeout_seconds` and static environment settings. The runner does not invoke a shell; it creates distinct scratch HOME/config/data dirs, but **it is not a security sandbox**. Only run trusted, explicitly approved executables; use a VM or sandbox for hostile binaries. Keep source data read-only and use target-specific CLI flags to redirect save/output paths.

```bash
python3 scripts/oracle_runner.py ./scenario.json --output ./capture-run01 \
  --allow-execute-original --compare

python3 scripts/workbench.py parity ./capture-run01/original.jsonl ./capture-run01/port.jsonl \
  --db ./port/port-ledger.sqlite --id vehicle.brake \
  --reference-lock ./port/reference.lock.json --report ./capture-run01/diff.json

python3 scripts/workbench.py ledger --db ./port/port-ledger.sqlite audit
```

The comparator rejects empty JSON or JSONL, malformed/duplicate keys, non-finite numeric values, incompatible types, missing fields, order changes and mismatched values. Exact comparison is the default. Any ignored pointer or numeric tolerance must be documented with the reason; no global tolerance loosening. A passing comparison without a reference lock records `compared`, not `verified`. The `verified` label is **scoped to these traces and locked input binary**, never the entire game, and cannot guarantee an independently trustworthy oracle.

## 4. Analyze the next gap

1. Read the ledger row and inspect its source and Rust locations.
2. Reproduce the bug in the original if possible; save a new immutable scenario and evidence ID.
3. Implement the original's behavior, including RNG calls, float widths, iteration, triggering, errors, networking and persistence.
4. Run `cargo fmt --check`, Clippy, tests, content checker, original-vs-port trace and actual runtime end-to-end tests.
5. Update evidence; never silently reset the coverage counters or count fixture-only rendering as real-content support.
6. Hand off with **tested behavior**, the first mismatch, unresolved risks and the precise next command.

## 5. Limits

GameFrame has dedicated tested tooling for Ghidra headless export and Unicorn emulation. `rustport` does not bundle those, replace them, or certify itself superior. Integrate installed, version-pinned external tools on a per-project basis via `references/reverse-methods.md`; do not download/run toolchain installers without explicit approval.
