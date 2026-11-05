# Command line

`portcode` with no command opens the app. These are all the commands and global options, taken from the binary:

{{cli}}

## Common uses

```sh
portcode                              # the app, in this folder
portcode -m ollama/qwen3:14b          # a local model
portcode -a port --deep               # the port agent, deep mode
portcode --resume                     # pick an earlier session
portcode run "list the TODOs"         # one prompt, no screen
portcode run -c "now fix the first"   # continue the latest session
portcode run -y "run the tests"       # approve everything for this run
echo "summarise README.md" | portcode run
```

## `run`

Runs one prompt without the screen and prints the answer. Options: `-y` approves every permission request (only in a sandbox or a trusted project), `-c` continues the latest session, `--json` emits JSON lines. The prompt can come from the arguments or from stdin.

## `sessions`

`portcode sessions [list | show <n|id> | rm <n|id>]`. See [Sessions](sessions.html).

## `port`

The porting workbench on the command line: `survey`, `pin`, `parity <golden> <port>`, `scaffold <name>`, `shot`, `site`, `render`, `page`, `ledger`. They do what the tools of the same names do, without a model. See [The port workflow](port-workflow.html).

## `tool`

`portcode tool <name> '<json>'` runs any built-in tool directly. See [Tools](tools.html).

## `engines`

`portcode engines list|install <id…>|path`. `install core` installs Capstone, Unicorn and LIEF; `install all` installs everything installable. See [Engines](engines.html).

## `auth`, `providers`, `models`, `doctor`, `eval`

`portcode auth set <provider>` stores a key. `portcode providers` shows configured and built-in providers. `portcode models` lists what a provider serves. `portcode doctor` checks the setup. `portcode eval` probes a model.

## `serve`, `remote`, `mcp`

`portcode serve` runs a session you can drive from a browser, `portcode remote new|list|revoke` manages pairing keys (see [Remote control](remote.html)), and `portcode mcp` serves the tools over MCP (see [Claude Code and Codex](subscriptions.html)).
