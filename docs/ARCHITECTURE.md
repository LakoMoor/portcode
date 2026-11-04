# Architecture

```
L0 portcode-evidence   Evidence, Ledger (JSONL, E-0001 ids, unknown→resolved), Pin (sha256/format), sniff()
L1 portcode-port       survey, parity (token-wise numeric tolerance + PNG), scaffold, shot (ANSI→SVG), site (md→html, safe)
L1 portcode-llm        Client (SSE), ModelProfile, repair_json, parse_text_tool_calls, strip_think
L2 portcode-agent      config → permission → tool(+Ctx) → tools/* → skills → mcp → agents(prompts) → run(loop) → session
L3 portcode-cli        clap commands, terminal UI (Ui + Gate impls), doctor, eval
   portcode-testkit    scripted /v1 server (tests, demos)
```
Layering: a crate depends only on lower layers. `unsafe_code = "forbid"` workspace-wide. Input-facing crates deny `unwrap/expect/indexing`.

## Agent loop (`run.rs`)
`send(text)` → for each step: fit context (prune old tool output → summarise) → `chat` with the agent's visible tools → push assistant message → for each tool call: loop guard, `exec` → push tool message → persist. `exec` validates the object/required params, runs the tool in its own task (a panic becomes an error string), clips output to the profile limit.

## Tool contract (`tool.rs`)
`name, description, schema, example, run(ctx, args)`. `Ctx` carries the project root, permission rules + gate, per-session state (files read with mtimes, todos), the ledger, cancel flag, read-only flag. Paths resolve lexically then via the longest existing ancestor's real path (symlink escapes ask `external_directory`); `.git` is write-protected; `.env`/keys ask `secret_file`.

## Permissions
`Rules::check(tool, target, default)`: most specific pattern wins; ties → deny > ask > allow. `bash`: hard-deny list first, then explicit patterns, then a read-only allowlist (never when the command contains `; & | > < \` $(`). Read-only agents deny `edit` and every prompt.

## Subprocesses
`run_command`: `sh -c`, stdin null, own process group, wall-clock deadline, 1 MB capture per stream (drained so the child never blocks), group `SIGKILL` on timeout/cancel, provider key env vars scrubbed.

## Evidence flow
`pin`/`screenshot`/`evidence_add` → ledger → `docs/PORT.md` cites ids → `site_build` groups evidence by subject with attached screenshots and open unknowns. Screenshots are produced from captured output with its sha256 recorded; they are not drawn.
