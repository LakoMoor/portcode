# PORT.md — wordstat (C → Rust)

A deliberately small port that exercises the whole PortCode workflow on a real original: pin, golden harness, differential fuzz, parity, screenshots and the generated site. Every claim cites an evidence id from the ledger below (`E-xxxx`).

## 1. Source
- Origin: `original/wordstat.c` (C, 83 LOC, written for this example, MIT). Pinned build: see E-0001 (sha256 of `original/wordstat`, compiled with `cc -O2 -Wall`).
- Mode: **source port**, clean-room irrelevant (own code). Fidelity: **bit-exact** stdout, stderr and exit code.
- Targets: CLI on macOS / Linux.

## 2. Scope
- In scope: argument parsing (`--top`, `--min-len`, file), word counting, sorting, report format, error paths.
- Dropped: none. Known non-goal: non-C locales (E-0011).

## 3. Module map
| Source | LOC | Rust | Strategy | Evidence | Status |
|---|---|---|---|---|---|
| `main` arg loop | 15 | `wordstat_core::run` | rewrite | E-0002, E-0006 | done |
| tokenizer + counting | 25 | `wordstat_core::count` | rewrite | E-0004, E-0010 | done |
| `qsort(cmp)` + report | 15 | `wordstat_core::run` | rewrite (`sort_by` count desc, word asc) | E-0002 | done |
| `atoi` | libc | `wordstat_core::atoi` | rewrite (saturating) | E-0009 | done |

## 4. Dependency map
| Source dep | Rust replacement | Why / risk |
|---|---|---|
| libc `isalnum/tolower` | `u8::is_ascii_alphanumeric/to_ascii_lowercase` | identical in the C locale; see E-0011 |
| `printf("%.3f")` | `format!("{:.3}")` | both round the exact binary value; checked by fuzz |

## 5. Hard problems
| Problem | Approach | Alternatives |
|---|---|---|
| Original quirks (negative `--top`, value-less flag becomes the path, 255-byte word cap) | reproduce, unit-test each | "fix" them — rejected, fidelity goal is bit-exact |

## 6. Golden cases (`tests/golden/*.txt`, produced by `tests/run_cases.sh original/wordstat`)
| Case | Command | Evidence | Port status |
|---|---|---|---|
| default | `wordstat inputs/sample.txt` | E-0002 / E-0003 | PASS |
| top3 | `--top 3` | E-0008 | PASS |
| minlen5 | `--min-len 5 --top 4` | E-0004 / E-0005 | PASS |
| empty | empty file | E-0008 | PASS |
| missing | nonexistent file → stderr, exit 1 | E-0006 / E-0007 | PASS |
| usage | no args → usage, exit 2 | E-0008 | PASS |

## 7. Milestones
- [x] M0 original builds; golden harness captures 6 cases
- [x] M1 core ported, headless goldens green (E-0008)
- [x] M2 real-content slice: the shipped binary runs on real files
- [x] M3 parity: 6/6 goldens + 400/400 fuzz (E-0008, E-0009); screenshots attached to every feature
- [x] M4 `cargo clippy -D warnings` clean, site generated

## 8. Open unknowns
- E-0011: behaviour under non-C locales (`isalnum` on bytes ≥ 128).

## 9. Decision log
- Quirks of the original are kept rather than fixed (bit-exact goal).
- Core is a pure function over `(args, read)` so goldens run without touching the filesystem.
