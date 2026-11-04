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
| TUI (`packages/tui`) | full-screen ratatui UI: home, transcript with tool blocks, prompt box, sidebar (context · evidence · todo), slash popup, permission dialog; `--plain` line mode | ◐ (no diff viewer, model/session pickers, themes) |
| desktop, web app, console | — | ⏳ |
| server / SDK / plugins / LSP / formatters / GitHub integration / share | — | ⏳ |
| i18n READMEs | EN + RU | ◐ |

## Tested against OpenCode's behaviour
`crates/portcode-cli/src/tui/opencode.rs` pins the parts of the TUI people know from OpenCode: enter / alt+enter, history on ↑↓, readline keys, ctrl+c and esc, tab and shift+tab between primary agents, the `/` menu (filter, ↑↓, tab, enter), `/new` `/models` `/agents` `/help` `/exit`, the allow once / always / reject dialog, the home screen, the sidebar and small terminals. Not matched on purpose: OpenCode's leader key (`ctrl+x`) chords, `/undo`, `/share`, `/themes`, `/sessions` (PortCode has `/resume`) and the diff viewer.

## PortCode-only
Evidence ledger + pins, `parity`, differential harness hooks, terminal screenshots → SVG, static site generator, scaffold, `port`/`reverse` agents, rustport skill embedded, open-model repair layer, model profiles, `portcode eval`, scripted mock server.
