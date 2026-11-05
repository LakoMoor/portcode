# Tools

Everything PortCode can do is a tool. The app, the agent and the command line share one list, so anything the agent can do you can also run yourself:

```sh
portcode tool read '{"path": "src/main.rs", "limit": 40}'
portcode tool venv '{"action": "status"}'
```

Tools never panic, never prompt on stdin and never hang: they have deadlines, capped output, and errors that tell the model how to fix the call. Each agent sees only the tools its role needs; `/tools` in the app prints the groups.

## Reference

The table is generated from the tool definitions (first sentence of each description).

{{tools}}

## Groups

| Group | Tools |
|---|---|
| Files | `read`, `ls`, `glob`, `grep`, `write`, `edit` |
| Run | `bash`, `venv`, `todowrite`, `webfetch`, `skill`, `task`, `team`, `blocked` |
| Proof | `survey`, `pin`, `evidence_add`, `evidence_list`, `parity`, `screenshot`, `scaffold`, `site_build` |
| Deep | `re_status`, `re_install`, `disasm`, `emulate`, `decompile`, `functions`, `hexdump`, `unity`, `dotnet`, `unity_assets` |
| More | tools from MCP servers listed in `portcode.json` |

The deep tools look inside the original and are available only in [deep mode](modes.html).

## MCP

Add MCP servers under `"mcp"` in `portcode.json` and their tools join the list for agents that allow MCP tools:

```json
{ "mcp": { "rea": { "command": ["npx", "-y", "rea-agents@latest", "mcp"] } } }
```

PortCode can also be an MCP server itself: `portcode mcp` serves its tools to Claude Code or Codex. See [Claude Code and Codex](subscriptions.html).

## Skills

A skill is a folder of instructions the agent loads on demand with the `skill` tool. PortCode ships `rustport`, the porting method. `portcode skill list`, `portcode skill show rustport` and `portcode skill export <dir>` manage them.
