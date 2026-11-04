# Providers, models and open-weight tuning

## Configuration
Layers (later wins): built-in presets < `~/.config/portcode/portcode.json` < `./portcode.json` < `./.portcode/portcode.json` < env (`PORTCODE_MODEL`, `PORTCODE_BASE_URL`, `PORTCODE_API_KEY`). `//` comment lines are allowed. Model reference: `provider/model` (a bare id uses `lakomoor`).

```jsonc
{
  "model": "lakomoor/deepseek-chat",
  "small_model": "ollama/qwen3:4b",          // used for context compaction
  "provider": {
    "lakomoor": { "base_url": "https://lakomoor.com/v1", "api_key_env": "LAKOMOOR_API_KEY" },
    "mybox":    { "base_url": "http://10.0.0.5:8000/v1", "headers": { "X-Team": "rev" } }
  },
  "models": { "qwen3:14b": { "context_window": 32768, "parallel_tool_calls": false } },
  "permission": { "edit": "ask", "bash": { "*": "ask", "cargo test*": "allow" }, "webfetch": "deny" },
  "mcp": { "rea": { "command": ["npx", "-y", "rea-agents@latest", "mcp"] } }
}
```
Keys: env var (`api_key_env`), or `portcode auth set <provider>` (`~/.config/portcode/auth.json`, mode 0600). Never commit keys.

## What the client absorbs
| Quirk | Handling |
|---|---|
| `reasoning_content` / `reasoning` deltas | shown dim, stored, not echoed back unless `echo_reasoning` |
| inline `<think>…</think>` (even split across chunks) | routed to reasoning, stripped from text |
| tool calls written as text (`<tool_call>{…}</tool_call>`, fenced JSON) | promoted to real calls (known tool names only) |
| truncated / trailing-comma / fenced tool-argument JSON | repaired, else the model gets an error with the tool's example |
| numbers sent as strings | accepted |
| server rejects `stream_options`, `top_k`, `tool_choice`… (HTTP 400) | one automatic retry with a minimal body |
| 429/5xx, connection errors | backoff retry (3 attempts) |
| stream stalls | idle timeout (120 s) with a clear error |
| empty reply / cut-off reply | up to 2 nudges, then stop |
| same tool call 3× in a row | blocked; stops after repeats |

## Profiles (`ModelProfile`)
`temperature, top_p, top_k, max_tokens, context_window, parallel_tool_calls, echo_reasoning, text_tool_fallback, strip_think, prompt_style (compact|full), tool_output_limit`. Built-in families by substring: `deepseek*`, `qwen*` (smaller output/context for `flash`, `mini`, `4b/7b/8b`), generic open families. **These are starting points, not measured claims.** Newer releases may want different values — override under `"models"`.

## Prompt design for open models
- Rules are numbered one-liners, most important first; a short style block last.
- One worked example in the system prompt; one `Example arguments:` per tool.
- Stable prefix: nothing time-dependent before the final `date:` line (prefix caches in llama.cpp/vLLM/SGLang reuse the KV).
- Tool errors tell the model the fix (valid tool names, required parameter + example, "read before edit").
- Large outputs are clipped head+tail with an explicit "narrow the request" hint; old tool output is trimmed, then the conversation is summarised near 75 % of `context_window`.
- Skill = short router + on-demand references (`skill` tool with `file`).

## Check your model
`portcode eval -m <provider/model>` runs: chat, a tool call with valid JSON, and a read→edit task in a temp dir. Failing tool-call → try `parallel_tool_calls:false`, a bigger model, or a server flag that enables the model's tool parser (vLLM `--enable-auto-tool-choice`, llama.cpp `--jinja`).
