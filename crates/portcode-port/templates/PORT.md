# PORT.md — {{name}}

Living plan. Every claim about the original cites an evidence id (`E-0001`) from `.portcode/evidence/ledger.jsonl`.

## 1. Source
- Origin: <repo/path/commit or shipped artifact>   Pinned build: <sha256 + version, see `portcode port pin`>
- Language(s): <…>   LOC: <…>   License / authorization: <…>
- Mode: [ ] source port  [ ] compatible runtime  [ ] clean-room (observe only)  [ ] deep (look inside)
- Targets: <desktop / web(wasm) / cli>   Fidelity: [ ] bit-exact  [ ] within tolerance  [ ] feature parity

## 2. Scope
- In scope: …
- Dropped (each needs a reason): …

## 3. Module map
| Source module | LOC | Rust module | Strategy (rewrite / bind / drop) | Evidence | Status |
|---|---|---|---|---|---|

## 4. Dependency map
| Source dep | Rust replacement | Why / risk |
|---|---|---|

## 5. Hard problems
| Problem | Approach | Alternatives |
|---|---|---|

## 6. Golden cases
| Case | How the original produces it | File | Tolerance | Port status |
|---|---|---|---|---|

## 7. Milestones
- [ ] M0 original builds/runs; golden harness captures cases
- [ ] M1 core ported, headless goldens green
- [ ] M2 integration slice with real content
- [ ] M3 parity within stated tolerance, screenshots attached to every feature
- [ ] M4 perf measured, clippy clean, site published

## 8. Open unknowns
(mirrors `portcode port ledger unknowns`)

## 9. Decision log
- <date> — decision — reason
