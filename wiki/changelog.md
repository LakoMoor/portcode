# Changelog

What changed in each release. Newest first. Every release ships the same 13 files on [GitHub Releases](https://github.com/LakoMoor/portcode/releases).

## v0.1.26 — 9 Nov 2026

- The site has a changelog page.
- The desktop app bundles IBM Plex Sans and IBM Plex Mono, so it looks the same on every system.

## v0.1.25 — 9 Nov 2026

- Kuna replaces Ghidra as the decompiler. It installs on first use, with a sha256 check, and the `decompile` tool lists functions, decompiles one, and searches strings and xrefs.
- Steering: type while the agent works and your message joins the conversation before its next step.
- `portcode gui` opens the new desktop app (`portcode-gui`, built on GPUI Fast).

## v0.1.24 — 8 Nov 2026

- At the step limit the agent keeps going while todos are open.
- `ctrl+o` folds old tool output.
- `/export` writes the session as a Markdown report.
- `/notify` rings the terminal when a task ends.

## v0.1.23 — 7 Nov 2026

- The Windows installer no longer breaks `PATH`.
- The todo list is saved with the session.
- Parity shows as "N of M cases match" with a percentage.

## v0.1.22 — 7 Nov 2026

- `/settings` screen and a redesigned `/create`.

## v0.1.21 — 6 Nov 2026

- Update check runs every minute while a session is open.

## v0.1.20 — 6 Nov 2026

- `/context` sets the context window of the current model.

## v0.1.19 — 6 Nov 2026

- Checklist progress bar in the sidebar.
- The agent is asked for a todo list when there is none.

## v0.1.18 — 6 Nov 2026

- The agent keeps going when a reply names a next step but no todo list exists.

## v0.1.17 — 6 Nov 2026

- Loops are caught by identical output, not only identical calls.
- Compaction works without a user message.
- The todo list survives restarts.

## v0.1.16 — 6 Nov 2026

- Auto-compact when the model loops.
- `A` in the permission dialog means allow-all.
- No spinning tool after you stop a turn.

## v0.1.15 — 6 Nov 2026

- Context window is read from the server.
- Automatic reconnect.
- Qwen XML tool calls are understood.

## v0.1.14 and earlier — 5–6 Nov 2026

- v0.1.14: programs are detected three levels deep; `/create` offers the current folder.
- v0.1.13: restart after `/update`; program detection and a team option in `/create`.
- v0.1.12: `/connect` for OpenAI-compatible servers; welcome screen on first start in a folder.
- v0.1.11: prompt history is restored with a resumed session.
- v0.1.10: Ollama is the default provider; sessions remember agent, model and deep mode.
- v0.1.9: non-gated agents keep going while todos are open.
