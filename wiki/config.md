# Configuration

PortCode reads `portcode.json`. `portcode init` writes a commented starting file in the current folder. Comments (`//`) are allowed.

## Where it is read from

Later layers win:

1. built-in defaults and provider presets
2. `~/.config/portcode/portcode.json` (your global file)
3. `./portcode.json`, or `./.portcode/portcode.json`
4. environment variables: `PORTCODE_MODEL`, `PORTCODE_BASE_URL`, `PORTCODE_API_KEY`

Commit `portcode.json`. Keep keys in environment variables or in `portcode auth set <provider>` (stored in `~/.config/portcode/auth.json`, mode 0600). Never commit a key.

## An example

```json
{
  "model": "ollama/qwen3:14b",
  "small_model": "ollama/qwen3:4b",
  "prompt_style": "auto",
  "venv": true,
  "suggest": true,
  "team": { "enabled": false, "parallel": 3 },
  "provider": {
    "ollama":   { "base_url": "http://localhost:11434/v1" }
  },
  "models": { "qwen3:14b": { "context_window": 32768, "parallel_tool_calls": false } },
  "permission": { "edit": "ask", "bash": { "*": "ask", "cargo test*": "allow" } },
  "agent": { "build": { "steps": 80 } },
  "mcp": { "rea": { "command": ["npx", "-y", "rea-agents@latest", "mcp"] } }
}
```

## Every setting

| Key | Type | Default | Meaning |
|---|---|---|---|
| `model` | string | `ollama/qwen3:14b` | `provider/model` to use |
| `small_model` | string | the main model | cheaper model for context compaction |
| `default_agent` | string | `build` | agent to start with |
| `provider` | map | presets | endpoints: `base_url`, `api_key`, `api_key_env`, `headers`. See [Providers](providers.html) |
| `models` | map | built-in profiles | per-model overrides keyed by model id (the part after `provider/`): sampling, `context_window`, `parallel_tool_calls`, `prompt_style`, `reasoning_effort`… The context size is taken from the server when it reports one; `/context 32k` (or `128k`, `auto`) in the app sets it for the current model and is remembered. |
| `prompt_style` | `auto` `compact` `full` `qwen38` | `auto` | prompt flavour for every model; a per-model entry wins |
| `permission` | map | ask for edits and commands | rules per tool, see [Permissions](permissions.html) |
| `agent` | map | | per-agent `model`, `steps`, `prompt_append` |
| `mcp` | map | | MCP servers: `command` (array), `env`, `enabled` |
| `max_steps` | number | per agent | upper bound on steps in one run |
| `mode` | `clean-room` `deep` | `clean-room` | see [Clean-room and deep mode](modes.html) |
| `cli_permission` | string | `acceptEdits` | Claude Code's permission mode: `acceptEdits`, `plan`, `bypassPermissions`, `default`. Read-only agents always use `plan`. See [Claude Code and Codex](subscriptions.html) |
| `team` | `{enabled, parallel}` | off, 3 | see [Team mode](team.html) |
| `venv` | bool | `true` | see [Python environments](python.html) |
| `suggest` | bool | `true` | the grey first-message suggestion, see [A tour](tour.html#the-first-message-suggestion) |
| `allowed_dirs` | array | `[]` | extra folders file tools may touch without asking |

## Remembered choices

A few choices you make in the app are remembered in a small state file in your config folder rather than in `portcode.json`: the last model (`/model`, `/provider`) and the theme (`/theme`). `-m` and `PORTCODE_MODEL` still win over the remembered model, and `PORTCODE_THEME` over the theme.

## Environment variables

| Variable | Effect |
|---|---|
| `PORTCODE_MODEL` | model, as `provider/model` |
| `PORTCODE_BASE_URL` | endpoint of the active provider |
| `PORTCODE_API_KEY` | key for the active provider |
| `PORTCODE_THEME` | `light` or `dark` |
| `PORTCODE_NO_UPDATE_CHECK` | `1` turns off the daily update check |
| `PORTCODE_INSTALL_DIR` | where `install.sh` puts the binary |
| `VLLM_API_KEY`, `DEEPSEEK_API_KEY` and the other provider variables | provider keys, named in `api_key_env` |
