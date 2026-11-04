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
`temperature, top_p, top_k, max_tokens, context_window, parallel_tool_calls, echo_reasoning, text_tool_fallback, strip_think, prompt_style (compact|full|qwen38), tool_output_limit`. Built-in families by substring: `deepseek*`, `qwen*` (smaller output/context for `flash`, `mini`, `4b/7b/8b`), generic open families. **These are starting points, not measured claims.** Newer releases may want different values — override under `"models"`.

### Small context windows and short outputs

Many local servers run with a 4k to 16k window and cut replies early. PortCode accounts for it:
- It never asks for more output than half the window, nor more than fits next to the prompt (vLLM rejects `prompt + max_tokens > window`).
- Tool results are capped at about an eighth of the window (`result_chars`), and old turns are compacted to stay under `window - output - 256` tokens.
- A reply that stops at the output limit (`finish_reason: length`) is continued ("continue exactly where you stopped") and the pieces are joined. A **tool call** cut by the limit is never run, even if its JSON could be repaired: the model is told to make it smaller and gets a larger output budget (up to 3x).
- `write` accepts `append: true`, so a long file is written in parts.
- If the instructions alone take more than a third of the window, PortCode says so once. Set the real server window in `portcode.json`: `"models": {"<id>": {"context_window": 8192, "max_tokens": 1024}}`.

## Prompt design for open models
- Rules are numbered one-liners, most important first; a short style block last.
- One worked example in the system prompt; one `Example arguments:` per tool.
- Stable prefix: nothing time-dependent before the final `date:` line (prefix caches in llama.cpp/vLLM/SGLang reuse the KV).
- Tool errors tell the model the fix (valid tool names, required parameter + example, "read before edit").
- Large outputs are clipped head+tail with an explicit "narrow the request" hint; old tool output is trimmed, then the conversation is summarised near 75 % of `context_window`.
- Skill = short router + on-demand references (`skill` tool with `file`).

## Check your model
`portcode eval -m <provider/model>` runs: chat, a tool call with valid JSON, and a read→edit task in a temp dir. Failing tool-call → try `parallel_tool_calls:false`, a bigger model, or a server flag that enables the model's tool parser (vLLM `--enable-auto-tool-choice`, llama.cpp `--jinja`).

## Qwen3.8 (27B, Flash-Next)

PortCode recognises `qwen3.8-27b`, `Qwen3.8-Flash-Next` and similar ids and switches to a profile built from the model cards ([Qwen3.8-Flash-Next](https://huggingface.co/Qwen/Qwen3.8-Flash-Next), [Unsloth's Qwen3.8-27B guide](https://unsloth.ai/docs/models/qwen3.8-27b)):

| | thinking (default) | non-thinking |
|---|---|---|
| temperature | 1.0 | 0.7 |
| top_p / top_k | 0.95 / 20 | 0.8 / 20 |
| presence_penalty | 0.0 | 1.5 |

What changes for these models:
- **Prompt.** A shorter rule set (`prompt_style: "qwen38"`): think once, don't redo earlier reasoning, tool calls only through the tool interface, independent reads in parallel. No worked example, the model does not need one.
- **Thinking control.** Sent as `chat_template_kwargs`: `reasoning_effort` (`xhigh`, `medium`, `low`) and `preserve_thinking`. PortCode defaults to `medium` and keeps preserved thinking on. The card's own default is `xhigh`; it also notes that a lower effort does not always shorten a multi-turn agent task, so try both on your hardware. `none` turns thinking off and applies the non-thinking sampling above.
- **Output budget.** Thinking tokens count against `max_tokens`, so the default is 16384.
- **Tool calls.** Parallel calls are on. Your server must run the Qwen tool-call parser (vLLM: `--tool-call-parser qwen3_xml --reasoning-parser qwen3`; llama.cpp: `--jinja`).

Settings, in `portcode.json`:

```json
{
  "prompt_style": "auto",
  "models": {
    "qwen3.8-27b": { "reasoning_effort": "low", "context_window": 131072 }
  }
}
```

`prompt_style` is `auto` (picked from the model id), `compact`, `full` or `qwen38`, and applies to every model; a per-model entry wins. In the app, `/style` and `/effort` change them for the current session. The ids in the cards and the sampling numbers come from the sources above; I could not verify them on a running model, so run `portcode eval` on yours.
