# PORT.md — living port plan (update as you go)

## 1. Source
- Origin: <repo/path/commit>   Language(s): <…>   LOC: <…>   License: <…>
- Platform targets for the port: <desktop / web(wasm) / mobile>
- Fidelity goal: [ ] bit-exact  [ ] behavior-exact (tolerance)  [ ] idiomatic rewrite (feature parity)

## 2. Scope
- In scope: …
- Out of scope / dropped: …   (every drop needs a reason)

## 3. Module map  (source → Rust)
| Source module | LOC | Rust crate::module | Strategy (rewrite / bind / wrap) | Status |
|---|---|---|---|---|

## 4. Dependency map
| Source dep | Rust replacement | Why / risk |
|---|---|---|

## 5. Hard problems
(global state, UB reliance, threading model, GC/ownership cycles, float determinism, shader dialect, platform APIs…)
| Problem | Chosen approach | Alternatives |
|---|---|---|

## 6. Golden tests
| Case | How the original produces it | Output file | Tolerance | Port status |
|---|---|---|---|---|

## 7. Milestones
- [ ] M0 builds + golden harness runs against ORIGINAL
- [ ] M1 core logic ported, headless golden tests green
- [ ] M2 window + wgpu renders first frame
- [ ] M3 feature parity, visual diff within tolerance
- [ ] M4 perf ≥ original (or documented), clippy clean, README

## 8. Decision log
- <date> — decision — reason
