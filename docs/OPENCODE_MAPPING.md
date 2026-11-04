# OpenCode → PortCode mapping

PortCode is a clean Rust rewrite of OpenCode's *design*, not a translation of its 20+ TypeScript/Effect packages. Status: ✅ done · ◐ partial · ⏳ not yet.

| OpenCode | PortCode | |
|---|---|---|
| `core/agent` — `build`, `plan`, sub-agents | `portcode-agent::agents` — build, plan, port, reverse, explore, general | ✅ |
| `core/tool/*` — read, write, edit, glob, grep, shell, todo, webfetch, skill, subagent | `portcode-agent::tools` — same set (+ `ls`) | ✅ |
| `apply_patch`, `question`, `schedule`, `websearch`, `image-generate`, `artifact` | — | ⏳ |
| permission service (ask/allow/deny, patterns, "always") | `permission.rs` + `Ctx::permit` | ✅ |
| provider layer (many SDKs) | `portcode-llm` — one OpenAI-compatible client, presets for local/self-hosted | ◐ (no Anthropic/Google native protocols) |
| config (`opencode.json`, markdown agents) | `portcode.json`, layered; markdown agents | ◐ (JSON only) |
| sessions in SQLite/Drizzle | JSON-lines files, resumable | ◐ |
| skills (`SKILL.md`) | `skills.rs`, bundled `rustport`, discovery in `.portcode/.claude/.agents` | ✅ |
| MCP client | `mcp.rs` stdio JSON-RPC (`tools/list`, `tools/call`) | ◐ (no remote/HTTP, resources, prompts) |
| compaction | tool-output pruning + model summary near 75 % window | ✅ |
| TUI (`packages/tui`), desktop, web app, console | line-based terminal UI with colour panels | ◐ (no full-screen TUI/desktop/web) |
| server / SDK / plugins / LSP / formatters / GitHub integration / share | — | ⏳ |
| i18n READMEs | EN + RU | ◐ |

## PortCode-only
Evidence ledger + pins, `parity`, differential harness hooks, terminal screenshots → SVG, static site generator, scaffold, `port`/`reverse` agents, rustport skill embedded, open-model repair layer, model profiles, `portcode eval`, scripted mock server.
