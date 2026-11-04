# Evidence — how a port stays honest

Every fact about the original lives in one append-only ledger (`.portcode/evidence/ledger.jsonl`; PortCode tools `evidence_add`, `evidence_list`; other agents: `scripts/workbench.py ledger`). The ledger is the single source for `docs/PORT.md` citations and the generated site.

## Three kinds — pick by *how you know*
| Kind | Means | Examples | Never use for |
|---|---|---|---|
| `observation` | You saw it happen or read the bytes | stdout of a run, a hash, a screenshot, a save file's bytes | anything from reading code or decompiling |
| `inference` | Derived; may be wrong | "the brake force is `k*v²`" from pseudo-code; a format guess | runtime behaviour you did not run |
| `unknown` | A named gap that blocks the port | "checksum algorithm of save files" | vague worries |

An `unknown` is closed by recording an observation and resolving it (`Ledger::resolve`); history is kept.

## Confidence
`high` = reproduced twice or hashed; `medium` = one clean observation / solid inference; `low` = hunch. Claim only as much as the lowest link in the chain (`depends_on`).

## A good record
- **subject**: dotted feature path (`save.header`, `physics.brake`, `cli.args`).
- **statement**: one falsifiable sentence with numbers (`exit code 2 and prints "usage:" when no args`).
- **source**: tool + args + path + sha256, plus the pinned build, so someone else can reproduce it.
- **attachments**: screenshots under `docs/screenshots/` that show it.

## Pinning
`pin` records sha256/size/format/version of the exact original. Findings do not carry across versions; when the target updates, re-pin and re-run goldens. Pinning proves file identity only.

## Citing
In `docs/PORT.md`, write `(E-0007)` after each rule you implemented. A rule without an id is a rumour.
