# Using REA when the original has no source

Site: https://rea.tools - Repo: https://github.com/morluto/rea (MIT; MCP server + CLI, runs locally). Full tool catalog: `docs/mcp-contracts.md` in the repo; the list below is a starting subset.

REA lets an agent read decompiled code, instructions and call relationships of shipped software, observe it while
running, and check recovered rules against the original. Targets: native executables/libraries, JavaScript/Electron
apps, browsers/web apps, Android APKs (also managed .NET, firmware).

## Setup (needs user approval - it writes agent config)
Requires Node.js 22.x/24.x/26.x + npm. Works with Claude Code, Codex, Cursor, Gemini CLI, Grok Build, any local-MCP agent. CLI without MCP: `npm i -g rea-agents` then `rea --help`, or `npx -y rea-agents@latest analyze-javascript-application <dir> --json`. Update: `rea update`.
```bash
npx -y rea-agents@latest doctor --json            # diagnose, read-only
npx -y rea-agents@latest setup --dry-run --json   # show the plan to the user
npx -y rea-agents@latest setup --yes              # only after approval; then restart the agent
```
Also load the `reverse-engineer-anything` skill; it routes by target type.

## Routing
| Target | Start with |
|---|---|
| ASAR / JS tree | `analyze_javascript_application` |
| APK | `inspect_android_package` |
| native exe/lib/.app | `open_binary` -> `binary_overview`, `search_procedures`, `procedure_pseudo_code`, `get_call_graph` |
| .NET assembly | `inspect_managed_artifact` |
| open browser / Electron | `list_browser_targets` / `list_electron_targets`, then `capture_*_scenario` |

## Porting workflow with REA
1. **Inventory**: strings, resources, exported/entry functions, imports (graphics API, audio, network) -> fills the survey.
2. **Feature trace**: `trace_feature` / `trace_application_feature` from a seed (a string, an input handler) to the rules it implements.
3. **Rule recovery**: write each recovered rule as a short spec in `docs/PORT.md` (inputs, formula, constants, edge cases) with Evidence IDs.
4. **Golden data**: capture scenarios from the *running* original (`capture_process_scenario`, `capture_browser_scenario`, screenshots) -> `tests/golden/`. Static analysis alone is not a golden.
5. **Verify**: compare the Rust port against the captured scenarios with `scripts/parity.sh`; use `compare_*` tools to diff original versions if the target updates.

## Rules
- Separate observed / inferred / unknown. Never claim execution behavior from static analysis.
- Version-pin: record hash, version, platform of the inspected build.
- Reconstruct, don't copy: reimplement logic in idiomatic Rust; recovered constants and rules are fine, decompiler output is not a deliverable.
- Native decompilation uses Hopper/Ghidra/IDA/Kuna; commercial ones (IDA, Hopper, JEB) must be licensed by the user - REA grants none. Static JS analysis needs no native engine.
- Respect licenses and law (REA's stated scope: lawful research, interoperability, preservation, software you own or are authorized to examine); only analyze what you're permitted to. Leave user data untouched when running the original.

## Provider and runtime checks

An MCP server can have a different environment from the shell running `rea doctor`.
Use the connected server's `binary_session` availability to determine whether its
selected provider can analyze the target; a shell diagnostic is not proof the MCP
provider is absent. Record disagreement rather than replacing working configuration.
For large binaries, let the initial analysis finish and inspect provider progress
where available; avoid launching duplicate analyses while it is pending.

On a host that cannot execute the target's OS/architecture, static recovery can
continue, but runtime goldens remain unavailable. Record the exact failed launch
and missing runtime (for example Windows PE on macOS without Wine or a Windows VM).
Do not equate successful `open_binary` with successful execution or decompilation.

When dependencies prevent running the original, a stubbed/instrumented harness
may isolate a subsystem but is not the original application. List every stub and
repair, label its outputs as harness-derived, and keep original runtime acceptance
blocked. Do not publish stub-derived goldens as unmodified-original evidence.

## Long analyses and transport limits

A tool-call timeout can terminate the provider's temporary analysis database.
Check the actual timeout/error and whether that database survives before retrying;
waiting longer in prose does not change the transport deadline. In Codex the
per-server setting is `mcp_servers.<server-id>.tool_timeout_sec`; use an explicitly
chosen per-run override when the test authorizes a longer bounded analysis, rather
than silently modifying persisted configuration. For example, with server `rea`:
`codex -c mcp_servers.rea.tool_timeout_sec=1200 exec ...`. This must be set before
the run; it is not a way to recover a database already cancelled. Other hosts have
their own settings. Source: https://learn.chatgpt.com/docs/extend/mcp .

Keep session/status output focused on provider identity, activity and the requested
capability. Do not repeatedly print the whole catalog; it can dominate context.

## Static recovery when execution remains unavailable

If the requested scope permits it, implement a small rule recovered from specific
instructions or decompilation as **static-inferred, unverified**. Record the exact
binary/hash, addresses, evidence and uncertain ABI/caller assumptions. Test the
recovered rules, but keep those expectation tests separate from original-side
differential tests. Mark unavailable oracle checks as not run, not successful.
Do not fabricate a frontend or claim measured compatibility from this milestone.
Fresh HOME/XDG paths and process-group deadlines reduce accidental data access but
are not an OS security sandbox; record the actual execution boundary and failures.
