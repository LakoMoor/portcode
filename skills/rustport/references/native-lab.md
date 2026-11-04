# Native Lab: executable-analysis workflow (Ghidra + Unicorn)

Native Lab is an **optional** layer of rustport for native executable and library analysis. It runs beside the pre-existing REA integration, source-port workflow, compatibility ledger and dual-process reference oracles. It **does not automatically port an executable** or assert whole-game compatibility.

## Safety and data policy

1. Use only binaries you are authorized to inspect. Preserve original assets and license notices. Never submit extracted game binaries, proprietary pseudocode, or machine code to a public repository. Store exports in `.rustport/native/`, excluded from Git by `workbench init`.
2. **Ghidra analysis is not execution of the game**, but parsing untrusted bytes is still a risk; isolate unknown samples inside a VM. `native_lab analyze` requires `--allow-analysis` even if installed.
3. **Unicorn is CPU emulation, not a secure isolation boundary**. Run untrusted samples in a VM. The bundled runner accepts only bounded x86-64 raw-function bytes with no mapped OS, system calls, or external libraries. It is NOT an EXE, DLL, Linux or Windows loader, not a game runtime, and not a replacement for an independent original-side oracle.
4. Exact ABI, relocations, globals, indirect calls, instructions and memory semantics must be established from the original before a function slice is meaningful. The x86-64 example below is intentionally synthetic.
5. Do not automatically install Ghidra, Unicorn or REA, and never change an agent's MCP configuration without explicit user approval.

## Tool routing

| Input / problem | Recommended approach | What it proves |
|---|---|---|
| PE/ELF/Mach-O, native EXE, DLL | Ghidra headless -> function and call-edge export | Static inferred locations and calls |
| One short self-contained native function | Extract verified file offsets -> Unicorn probe | Behavior of **extracted machine-code slice under chosen emulation state** |
| Running original game or native module | REA observation / debugger / original test harness | Original behavior for exactly tested inputs |
| Available Java source or bytecode | Bytecode/decompiler + direct JVM oracle | JVM observed behavior |
| Available Lua / Source / Garry's Mod files | Lua parser / real runtime and scenario harness | Observed script + engine behavior |
| Rust implementation | Cargo test + same-input differential parity | Bounded parity under declared evidence policy |

## 0. Check optional tools

```sh
python3 scripts/native_lab.py doctor
python3 scripts/workbench.py doctor
```

Install Ghidra from its official releases **separately** in a test environment. Set `GHIDRA_HOME` to its installation directory, or pass the path of `support/analyzeHeadless` as `--headless`. `native_lab.py` uses the standard `-import`, `-scriptPath`, and `-postScript RustportExport.java` API and exports a JSON report with function addresses and call edges. Pseudocode is **off by default**, limited by `--decompile-limit` to reduce accidental redistribution and massive exports. This script was contract-tested with a synthetic `analyzeHeadless` stand-in; the user must run an actual Ghidra integration test on their machine before treating the toolchain as validated.

## 1. Analyze the binary without launching it

```sh
python3 scripts/workbench.py pin --original '/games/Original.exe' --output .rustport/reference.lock.json
python3 scripts/native_lab.py analyze \
  --binary '/games/Original.exe' \
  --output .rustport/native/ghidra-functions.json \
  --max-functions 2500 --max-edges 200 \
  --timeout 1800 --allow-analysis
```

Review `functions_truncated` and `call_edges_truncated` in the JSON. Addresses may be relocation-dependent, and function recovery is heuristic. The export contains **inferred** evidence, not original-side runtime traces.

## 2. Import recovered symbols into the compatibility ledger

```sh
python3 scripts/workbench.py init --project ./port --name MyGame
python3 scripts/native_lab.py ingest \
  --export .rustport/native/ghidra-functions.json \
  --db ./port/port-ledger.sqlite \
  --subsystem executable
python3 scripts/workbench.py ledger --db ./port/port-ledger.sqlite stats
python3 scripts/workbench.py ledger --db ./port/port-ledger.sqlite list --status unknown
```

The import is repeatable. IDs are `native.<source_hash12>.<entry>`. Each recovered function remains `unknown` with `inferred` confidence. No `implemented`, `wired` or `verified` status is fabricated.

## 3. Extract a function by FILE OFFSET, not virtual address

A call graph reports virtual addresses. Convert VA/RVA into a file offset via the binary's section mappings first. Extract only known bounded ranges; inspect relocation and memory dependencies. **Do not feed an arbitrary executable directly into the emulator.**

```sh
python3 scripts/native_lab.py slice \
  --binary '/games/Original.exe' \
  --offset 0x12340 --length 0x40 \
  --output .rustport/native/func.bin --allow-slice
```

Writes a `.provenance.json` with original SHA-256, file offset, function slice hash, length and explicit caveat.

## 4. Emulate a self-contained x86-64 SysV function (optional Unicorn 2.x)

Install Unicorn in an isolated environment (`python -m pip install 'unicorn>=2.1,<3'`), not globally unless intended. This demonstrates only a *synthetic* five-byte function (`lea rax,[rdi+rsi] ; ret`) with System V register arguments. The code bytes are original to this example and may be shared.

```sh
mkdir -p .rustport/native
python3 -c 'from pathlib import Path; Path(".rustport/native/add.bin").write_bytes(bytes.fromhex("488d0437c3"))'
cat > .rustport/native/cases.json <<'JSON'
[
  {"id":"sum-3-4", "registers":{"rdi":3,"rsi":4},"expected":7},
  {"id":"zero", "registers":{"rdi":0,"rsi":0},"expected":0}
]
JSON
python3 scripts/native_lab.py emulate \
  --code .rustport/native/add.bin --cases .rustport/native/cases.json \
  --output .rustport/native/emulated.jsonl \
  --instruction-limit 500 --timeout-ms 1000 --allow-emulation
```

The runner maps only a 64 KiB executable page and a 64 KiB stack page; it rejects reads outside these mappings, invalid interrupts/syscalls, out-of-function code execution, returns that do not reach a sentinel and over-budget loops. It provides neither Windows ABI setup/shadow-space guarantees nor arbitrary memory structures or relocated imports. Its JSONL output contains explicit `id`, input registers and return value; provenance states `unicorn-emulated-raw-function-not-original-game-execution`.

## 5. Independently check the game and Rust

**Do not equate the Unicorn result with a real-game oracle.** To establish runtime parity, capture independently from the original binary or game environment using the earlier `oracle_runner.py` procedure. Feed identical structured inputs to a Rust probe, verify schema/version and compare with `workbench parity`. Original instrumentation should record per-tick state, RNG, timing, UI events and content IDs as relevant. Maintain call-graph evidence separately from behavior traces.

## 6. Troubleshooting / limits

- *Ghidra unavailable*: `doctor`; pass absolute `support/analyzeHeadless`; still can use REA or plain source work.
- *Ghidra produced no JSON*: the wrapper fails without marking any result successful. Check logs, Ghidra JDK and the script compiler inside a VM.
- *Function calls outside the extract*: expected failure under this constrained emulator; isolate another function, stub dependencies consciously, or use an original process debugger instead.
- *Legacy 32-bit code*: Unicorn wrapper currently supports **x86-64 only**; use REA, native debugger or extend with explicit x86-32 ABI handling and tests.
- *Game needs assets/OS APIs*: Unicorn alone is not suitable for whole-game execution; use a full emulator/VM with a reference probe.
- *Ghidra and Unicorn integration*: actual toolchain testing depends on local installation; synthetic CLI tests cannot certify real third-party integrations.

Primary sources: [Ghidra analyzeHeadless documentation](https://github.com/NationalSecurityAgency/ghidra/blob/master/Ghidra/RuntimeScripts/support/analyzeHeadlessREADME.md) and [Unicorn Python tutorial](https://www.unicorn-engine.org/docs/tutorial.html).

## 7. Function prioritization / call graph

After a successful Ghidra export:

```sh
python3 scripts/native_lab.py rank --export .rustport/native/ghidra-functions.json --limit 40
python3 scripts/native_lab.py graph --export .rustport/native/ghidra-functions.json \
  --output .rustport/native/calls.dot
# Optional if graphviz is installed: dot -Tsvg .rustport/native/calls.dot -o .rustport/native/calls.svg
```

The ranking is based on **static fan-in/fan-out only**; it is a triage hint, not proof of runtime relevance or a critical path. Ghidra can miss indirect, virtual and dynamic calls. Never treat a high rank as a measured coverage number.

## 8. Differential fuzzing / replay

This runs an original-side harness and a Rust harness (both must accept one JSON object on stdin and emit one JSON object on stdout). Commands are **argv arrays** without a shell; executable and script paths should be absolute, because each process runs in a separate temporary working directory. The harnesses are your own; the tool does not teach an unknown game to speak JSON.

Create a `harness.json` in your work project:

```json
{
  "schema": 1,
  "original": ["/usr/bin/python3", "/abs/path/original_probe.py"],
  "port": ["/usr/bin/python3", "/abs/path/rust_probe.py"],
  "fields": {"velocity": {"min": -500, "max": 500}, "ticks": {"min": 0, "max": 80}}
}
```

Run only in an environment you trust; use a VM for untrusted original software:

```sh
python3 scripts/differential_fuzz.py harness.json \
  --output .rustport/fuzz/001 --seed 42 --iterations 100 \
  --timeout 2 --minimize --allow-execute-original
```

The tool records `original.jsonl`, `port.jsonl`, `cases.json`, and `report.json`. Status `DIFF` exits 1, `PASS` exits 0, invalid inputs and unavailable reference return 2. At the first discrepancy it tries a small set of numeric input reductions. This is **not a complete property-based test generator**, nor proof of semantic equivalence beyond tested cases. `--minimize` is a bounded heuristic, not a mathematically minimal counterexample.

The dedicated GitHub Actions workflow `.github/workflows/native-lab.yml` installs Unicorn and executes a real synthetic emulation case on Ubuntu after you push the changes. Unlike the current local synthetic interface tests, that optional CI job can verify actual Unicorn bindings. Real Ghidra compilation remains to be tested on a machine with Ghidra installed.
