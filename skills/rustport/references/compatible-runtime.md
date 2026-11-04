# Compatible game runtimes and long-running reconstructions

Read this when the user wants an existing game/engine reimplemented in Rust while
retaining its content and behavior: the class of project illustrated by Horizon
and openOMSI. These projects illustrate techniques, not proof that every original
feature is already supported, nor starter code to copy into unrelated engines.

## Define compatibility, not resemblance

Pin the original build, platform, and hashes in `reference.lock.json`. Identify the
installation/content root and output/save root separately. Run against copies or a
scratch profile; keep the supplied installation read-only. Distribute new code and
small synthetic fixtures, not the player's game data or decompiled sources.

The deliverable loads original content and implements measured original rules.
Do not silently replace an unavailable map, model, script, simulation or save file
with a procedural demo and call the result a port. Synthetic fixtures are useful
for unit tests; label them, and keep real-content acceptance separate. If content
or the reference executable is unavailable, continue independent work and state
which integration and behavioral checks remain unavailable.

## Executable evidence ledger

For nontrivial, long-running work use [workbench.md](workbench.md) for a local SQLite behavior ledger with `origin`, `target`, `evidence`, confidence, runtime-wired flag and `compared`/`verified` states. Pin the owned original binary with SHA-256 in a **local ignored** `reference.lock.json` and use [scenarios.md](scenarios.md) for explicit deterministic probes. The trace comparator rejects empty or malformed results and audits changed proof artifacts. A passing test certifies **only the bounded trace and current original build**, not the whole game or independent authenticity of the oracle. Do not infer original-side runtime behavior solely from static analysis.

<!-- rustport-next/workbench -->

## Native-function recovery and seeded differential trials

Use [native-lab.md](native-lab.md) for Ghidra headless function recovery and a bounded optional Unicorn x86-64 emulator. Static function locations are *inferred*, emulator output proves only the extracted code under supplied CPU state, and differential fuzzing validates only sampled inputs. The game itself must be observed separately to establish a real reference oracle. `scripts/differential_fuzz.py` emits traces suitable for strict workbench comparison. Do not alter original assets or publish proprietary binary outputs.

<!-- rustport-next/native-lab -->

## Maintain a compatibility ledger

Create `docs/COMPATIBILITY.md` or a machine-readable equivalent. Every tracked
behavior records original symbol/format/version, Rust location, evidence source,
scenario/fixture, limitations, and next action. Track implementation, runtime
wiring, real-content verification, and differential verification separately;
a passing isolated test does not prove that the player-facing path uses the code.

For a reported bug: find its ledger row, read the original evidence and Rust path,
reproduce it, fix the real call path, then update the evidence and gaps. Explicitly
check shared versus alternate paths (window/offscreen, client/server, loaded/new
world) where the architecture has them. Keep the ledger proportional to the work;
do not invent a mapping for source that has not been inspected.

## Organize around observable boundaries

Separate formats/content decoding, deterministic simulation, scripting, world
streaming, presentation, platform services, and application orchestration when
those boundaries exist in the target. Do not create a fixed collection of empty
crates or force a voxel architecture onto a vehicle simulator. Keep renderer and
window dependencies out of parsers and simulation.

Inventory the original content graph: references, archive lookup, case rules,
encodings, units, coordinate systems, versions, model/texture/material bindings,
and fallback behavior. Report unsupported constructs with file and location.
Implement a content checker that can walk a real installation and summarize
loaded, missing, unsupported and malformed resources without launching the game.
Never label parsing success as execution compatibility.

If the game is driven by scripts, recover its language/VM semantics, variable and
stack behavior, initialization, triggers and update order; connect script outputs
to physics, animation, audio and UI. Do not replace a content script with hardcoded
example behavior. Preserve meaningful simulation ordering and RNG draws before
introducing parallel execution. Set and measure loading, frame-time and memory
budgets on representative scenarios rather than optimizing an empty scene.

## First playable vertical slice

Keep core logic headless-testable, but bring up a thin real runtime early to expose
integration failures. Pick the smallest real-content scenario exercising the
important boundaries. Examples, adapted to the target:

- A voxel game: read an original save, load a chunk, draw it, change a block, save,
  and reload the change. Compare rules and persisted state with the original.
- A vehicle simulator: load one original vehicle and a small map section, execute
  its scripts, start the engine and drive; compare state traces and rendered views.

Use the ordinary entrypoint and actual subsystem wiring. A separate demo path is
not acceptance. Track what is missing even if this slice is playable. Defer new
launcher features, multiplayer, VR or graphics enhancements unless requested or
required by the selected slice.

## Reference laboratory and agent control

For each subsystem build an original-side oracle and a Rust probe with the same
serialized inputs. Record seed, tick count, timestep semantics, input/event order,
content/version identifiers, and output schema. Re-run the oracle when its build,
fixture or instrumentation changes. Cache only against those identities.

Compare state by tick and report the first differing field and values; images,
audio and performance need appropriate separate metrics. Empty fixture sets,
missing reference data, skipped tests and unavailable oracles are not passes.
Coverage is the set of tested behaviors, never a blanket claim about the game.

Provide diagnostic commands suited to the target: load, step, input, inspect,
screenshot, save, quit. Route them through the real runtime, with deterministic
headless/offscreen scenarios where possible; do not build a separate fake game for
the harness. Restrict control to the local test process. Replays should include
initial state, content IDs and timestamped inputs so failures can be reproduced.

## Continue across sessions

At each completed slice update the ledger and `docs/PORT.md` with the exact commands
run, evidence/results, deviations, blockers and next concrete acceptance scenario.
Resume from that state instead of re-surveying and rebuilding the scaffold. A large
codebase is a reason to choose a narrower next slice, not to abandon the authorized
project or demand approval again merely because of its LOC count.
