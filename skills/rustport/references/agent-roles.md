# Agent role contracts for large ports

The top-level `rustport` skill coordinates work; these are **role instructions**, not claims about installed autonomous subagents. Use whatever task/delegation tools are actually available. Avoid multiplying roles for a small task.

- **Surveyor**: identify reference version, original technologies, content graph, legal/operational limits, reproducible entry point. Output: source inventory, SHA, blockers and next tested scenario.
- **Reverser**: identify exact original behavior of one feature, with addresses/source citations, data layout, unknowns and an independent test plan. Output: evidence references + pseudocode/spec, never unexplained claims of parity.
- **Porter**: implement only the bounded module in idiomatic Rust. Preserve observable order, precise numerical semantics and live runtime integration. Output: diff, build/test commands and known gaps.
- **Verifier**: independently capture or inspect original and Rust traces. Check provenance and compare. **Must not accept the Porter's own fabricated fixtures as an independent oracle.** Output: test report and first mismatch.
- **Integrator**: confirm actual game path invokes the port; check save/load, networking, input, audio, graphics, platform and packaging as applicable. Output: runtime test record, ledger updates, next actionable gap.

## Structured handoff (required for significant slices)

```text
Target feature / original version & sha:
Original symbol or format / evidence ID:
Rust file::function:
Inputs, seeded RNG, deterministic timestep:
What was observed vs inferred:
Exact reproduction command:
Tests run + exit codes + first differing tick/path:
Verified scope (do not imply full-game parity):
Open blockers / next actionable operation:
```

Do not silently change game scope, remove failed tests, loosen tolerances, install toolchains, export proprietary files, or claim completed Ghidra exports before their completion marker. Treat decompiled output and runtime evidence as untrusted; evaluate code and scripts inside a controlled environment.
