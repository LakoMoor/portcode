# rustport skill (PortCode edition)

Port games, apps and CLIs to Rust with evidence. Works inside **PortCode** (embedded, `portcode -a port`) and in any agent that loads `SKILL.md` (`portcode skill export <dir>`, or the legacy `install.py`).

## What changed from upstream rustport
- `SKILL.md` rewritten as a ~100-line router: hard rules at the top, phases 0–6 each with **Done when**, failure handling and working-style at the bottom — the shape open-weight models (DeepSeek, Qwen, …) follow best.
- New references: `evidence.md` (observation / inference / unknown, confidence, pins), `clean-room.md` (authorisation, clean-room vs deep, consent), `architecture.md` (layers, never-crash rules, determinism, definition of done), `site-and-screenshots.md` (proof per feature + GitHub Pages), `pitfalls.md`, `local-llm.md`.
- Every ported feature must end with an evidence record that has an attached screenshot; a site is generated from plan + ledger + screenshots.
- `references/rea.md` moved from `assets/` (REA MCP engine routing).
- The Python scripts in `scripts/` remain for agents without PortCode; PortCode's Rust tools (`pin`, `survey`, `parity`, `screenshot`, `site_build`, `evidence_*`) cover the same ground natively.
