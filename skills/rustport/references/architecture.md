# Architecture for a port that survives growth

Distilled from layered Rust engines. Apply proportionally: a 300-line CLI needs two crates, not eight.

## Layers (a crate may only depend downward)
```
L0 formats / evidence   pure data + parsers. No I/O, no clock, no globals. Fuzz-friendly.
L1 core / rules         deterministic simulation or logic. Seeded RNG, fixed timestep, BTreeMap where order matters.
L2 services             files, process, network, audio — behind traits.
L3 app / runtime        wiring, config, CLI, MCP.
L4 ui / render          window, wgpu, egui — the only layer that may touch them.
```
Keep L0–L1 headless so goldens run in CI without a GPU. Prefer a workspace over modules once you pass ~3k LOC.

## Rules that prevent most rework
1. **Never crash on hostile input.** Original save files, assets and network data are the main input. No `unwrap`/`expect`/`panic!`/`todo!`/indexing on input-derived values; checked arithmetic; bounded recursion; capped allocations. Add `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing))]` to input-facing crates. Every crash fix gets a regression test.
2. **Everything user-visible is a command/tool** with a name, typed params and a result — the CLI, GUI and tests call the same function. No logic in `main.rs` or UI code.
3. **Headless first.** Capability works without a window before it gets one.
4. **Safe Rust only** (`unsafe_code = "forbid"`). Bind C libs through a vetted wrapper crate and log it in `docs/PORT.md`.
5. **Data over code.** Tables of constants, formats and recipes are typed data, not string `match`es.
6. **Extend through traits**, not by adding arms to a growing enum.
7. **Subprocesses**: stdin closed, deadline, capped output, kill the whole process group, errors with a plain-English fix. Never inherit stdin; never leave a child behind.
8. **Honest effects**: declare what a command touches (reads, writes, executes foreign code, network).
9. **No secrets or personal paths** in tests, logs or the repo. Use obvious fakes (`sk-test`).

## Determinism checklist
Seeded RNG with the original's algorithm; same f32/f64 as source; same operation order (no `mul_add`, no fast-math); fixed timestep with accumulator; no `HashMap` iteration where order matters; stable sorts where the original sorted stably.

## Definition of done for a slice
`cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test` · golden `parity` PASS · evidence ids cited · tick in `docs/PORT.md`.
