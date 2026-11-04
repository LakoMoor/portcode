# Claude Code and Codex on your subscription

If you pay for Claude (Pro, Max) or ChatGPT (Plus, Pro) you can run PortCode on that plan instead of an API key. Nothing is shared with PortCode: it starts the official `claude` or `codex` program on your machine, and that program uses the login you already made. PortCode never reads or stores their tokens.

There are two directions, and you can use both.

## 1. PortCode runs on your subscription

```sh
portcode -m claude-code/sonnet      # Claude Code, any alias or full model id
portcode -m claude-code/default     # whatever your plan's default is
portcode -m codex/default           # Codex CLI
```

Or inside the app: `/provider claude-code sonnet`.

Before the first run, install the CLI and sign in once:

| | install | sign in |
|---|---|---|
| Claude Code | see claude.com/claude-code | run `claude` and log in |
| Codex | `npm i -g @openai/codex` | `codex login` |

`portcode doctor` shows whether each one was found.

What happens on a turn:
- The CLI does the work with its own tools (reading files, editing, running commands) and its own agent loop.
- PortCode gives it its extra tools over MCP: `survey`, `pin`, `evidence_add`, `evidence_list`, `parity`, `screenshot`, `scaffold`, `site_build`, and in `--deep` mode `disasm`, `emulate`, `decompile`, `functions`, `hexdump`, `unity`, `dotnet`, `unity_assets`.
- PortCode adds its rules to the CLI's system prompt: record every fact in the ledger as observation, inference or unknown, pin the original first, and never claim parity without a `parity` PASS. The `port`, `reverse` and `plan` agents each bring their own instructions.
- Everything the CLI does shows up in the PortCode screen as tool blocks, and the evidence goes into the same `.portcode/evidence` ledger as always.
- The next message in the same session continues the same CLI conversation (Claude Code only).

What does **not** apply to these models, because the CLI runs its own loop: the finish gate, the step limit, PortCode's permission prompts, `/style` and `/effort`, and context compaction (the CLI compacts by itself).

Permissions. Claude Code cannot ask you anything in this mode, so it needs a rule up front. Set it in `portcode.json`:

```json
{ "cli_permission": "acceptEdits" }
```

`acceptEdits` (default) lets it edit files and refuses shell commands it was not pre-approved for. `plan` is read-only (the `plan` agent always uses it). `bypassPermissions` lets it run anything without asking, so use it only in a project and on a machine you trust. Codex runs in its `workspace-write` sandbox, or `read-only` for the `plan` agent.

## 2. Claude Code or Codex use PortCode's tools

Use the tools from your usual Claude Code or Codex session, without starting PortCode's own screen:

```sh
portcode mcp --print-config     # shows the two commands below with your install path
claude mcp add portcode -- portcode mcp
codex mcp add portcode -- portcode mcp
```

Add `--deep` after `mcp` to also offer the reverse-engineering tools (only on software you own or may examine). The server offers no file or shell tools, since both CLIs have their own.

## Good to know

- These are your plan's usage limits, shared with whatever else you do in Claude or ChatGPT. A long port can use a lot of it.
- The Claude Code path was tested end to end against the real CLI. The Codex path follows OpenAI's documented `codex exec --json` and MCP configuration, and is covered by tests on recorded-shape data, but it has not been run against a real Codex install yet. If something breaks, `portcode -m codex/default` prints the CLI's own error.
- Use only the official CLIs and your own account. Check your provider's terms before running unattended jobs.
